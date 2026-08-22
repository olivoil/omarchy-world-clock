use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use std::io::Read;
use std::time::Duration;

// This bounds bytes exposed by reqwest's response reader, so the same ceiling
// applies to decoded content if response decompression is enabled later.
pub(crate) const MAX_OPEN_METEO_RESPONSE_BYTES: usize = 64 * 1024;

pub(crate) fn open_meteo_client(timeout: Duration) -> reqwest::Result<Client> {
    Client::builder()
        .timeout(timeout)
        // The API URLs are fixed and currently return successful responses
        // directly. Refusing redirects prevents a compromised provider from
        // turning this backend into a cross-origin or local-network requester.
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

pub(crate) fn read_json_response<T>(
    response: reqwest::blocking::Response,
    max_bytes: usize,
) -> Result<T>
where
    T: DeserializeOwned,
{
    let content_length = response.content_length();
    read_json_with_limit(response, content_length, max_bytes)
}

fn read_json_with_limit<T, R>(reader: R, content_length: Option<u64>, max_bytes: usize) -> Result<T>
where
    T: DeserializeOwned,
    R: Read,
{
    let max_bytes_u64 = u64::try_from(max_bytes).context("response byte limit is unsupported")?;
    if content_length.is_some_and(|length| length > max_bytes_u64) {
        bail!("remote response exceeds {max_bytes}-byte limit");
    }

    let read_limit = max_bytes_u64
        .checked_add(1)
        .context("response byte limit is unsupported")?;
    let mut body = Vec::new();
    reader
        .take(read_limit)
        .read_to_end(&mut body)
        .context("could not read remote response")?;
    if body.len() > max_bytes {
        bail!("remote response exceeds {max_bytes}-byte limit");
    }

    serde_json::from_slice(&body).context("remote response was not valid JSON")
}

#[cfg(test)]
pub(crate) fn serve_http_response_without_length(
    body: Vec<u8>,
) -> (String, std::thread::JoinHandle<()>) {
    use std::io::Write;
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local HTTP fixture");
    let address = listener.local_addr().expect("read local HTTP address");
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept local HTTP request");
        let mut request = [0_u8; 4_096];
        let _ = stream.read(&mut request);
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
            )
            .expect("write local HTTP response headers");
        // The bounded client intentionally closes before an oversized body is
        // complete, so a broken pipe is the expected server-side outcome.
        let _ = stream.write_all(&body);
    });

    (format!("http://{address}/"), server)
}

#[cfg(test)]
pub(crate) fn serve_http_redirect_to_response(
    body: Vec<u8>,
) -> (
    String,
    std::thread::JoinHandle<()>,
    std::sync::mpsc::Sender<()>,
    std::thread::JoinHandle<bool>,
) {
    use std::io::Write;
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Duration;

    let target_listener = TcpListener::bind("127.0.0.1:0").expect("bind redirect target");
    let target_address = target_listener
        .local_addr()
        .expect("read redirect target address");
    target_listener
        .set_nonblocking(true)
        .expect("make redirect target nonblocking");
    let (stop_tx, stop_rx) = mpsc::channel();
    let target_server = std::thread::spawn(move || loop {
        match target_listener.accept() {
            Ok((mut stream, _)) => {
                let mut request = [0_u8; 4_096];
                let _ = stream.read(&mut request);
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream
                    .write_all(headers.as_bytes())
                    .expect("write redirect target headers");
                stream.write_all(&body).expect("write redirect target body");
                return true;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                match stop_rx.try_recv() {
                    Ok(()) | Err(mpsc::TryRecvError::Disconnected) => return false,
                    Err(mpsc::TryRecvError::Empty) => {}
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("accept redirect target request: {error}"),
        }
    });

    let redirect_listener = TcpListener::bind("127.0.0.1:0").expect("bind redirect response");
    let redirect_address = redirect_listener
        .local_addr()
        .expect("read redirect response address");
    let redirect_server = std::thread::spawn(move || {
        let (mut stream, _) = redirect_listener
            .accept()
            .expect("accept redirect response request");
        let mut request = [0_u8; 4_096];
        let _ = stream.read(&mut request);
        let response = format!(
            "HTTP/1.1 302 Found\r\nLocation: http://{target_address}/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        stream
            .write_all(response.as_bytes())
            .expect("write redirect response");
    });

    (
        format!("http://{redirect_address}/"),
        redirect_server,
        stop_tx,
        target_server,
    )
}

#[cfg(test)]
mod tests {
    use super::read_json_with_limit;
    use serde::Deserialize;
    use std::cell::Cell;
    use std::io::{Cursor, Read};
    use std::rc::Rc;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Payload {
        value: String,
    }

    struct CountingReader {
        inner: Cursor<Vec<u8>>,
        bytes_read: Rc<Cell<usize>>,
    }

    impl CountingReader {
        fn new(bytes: Vec<u8>) -> (Self, Rc<Cell<usize>>) {
            let bytes_read = Rc::new(Cell::new(0));
            (
                Self {
                    inner: Cursor::new(bytes),
                    bytes_read: Rc::clone(&bytes_read),
                },
                bytes_read,
            )
        }
    }

    impl Read for CountingReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let count = self.inner.read(buffer)?;
            self.bytes_read.set(self.bytes_read.get() + count);
            Ok(count)
        }
    }

    struct PanicReader;

    impl Read for PanicReader {
        fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
            panic!("a response with an oversized declared length must not be read");
        }
    }

    #[test]
    fn unknown_length_response_stops_after_limit_plus_one_byte() {
        let limit = 64;
        let body = format!(r#"{{"value":"{}"}}"#, "x".repeat(4_096)).into_bytes();
        let (reader, bytes_read) = CountingReader::new(body);

        let error = read_json_with_limit::<Payload, _>(reader, None, limit).unwrap_err();

        assert!(error.to_string().contains("exceeds 64-byte limit"));
        assert_eq!(bytes_read.get(), limit + 1);
    }

    #[test]
    fn understated_declared_length_cannot_bypass_the_streaming_limit() {
        let limit = 64;
        let body = format!(r#"{{"value":"{}"}}"#, "x".repeat(4_096)).into_bytes();
        let (reader, bytes_read) = CountingReader::new(body);

        let error =
            read_json_with_limit::<Payload, _>(reader, Some(limit as u64), limit).unwrap_err();

        assert!(error.to_string().contains("exceeds 64-byte limit"));
        assert_eq!(bytes_read.get(), limit + 1);
    }

    #[test]
    fn oversized_declared_length_is_rejected_before_reading() {
        let error = read_json_with_limit::<Payload, _>(PanicReader, Some(65), 64).unwrap_err();

        assert!(error.to_string().contains("exceeds 64-byte limit"));
    }

    #[test]
    fn response_at_exact_limit_is_accepted() {
        let body = br#"{"value":"safe"}"#.to_vec();
        let limit = body.len();

        let payload = read_json_with_limit::<Payload, _>(Cursor::new(body), None, limit).unwrap();

        assert_eq!(payload.value, "safe");
    }

    #[test]
    fn malformed_bounded_response_is_rejected() {
        let error =
            read_json_with_limit::<Payload, _>(Cursor::new(br#"{"value": }"#.to_vec()), None, 64)
                .unwrap_err();

        assert!(error.to_string().contains("not valid JSON"));
    }
}
