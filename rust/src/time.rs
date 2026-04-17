use chrono::{DateTime, Offset, TimeZone, Utc};
use chrono_tz::Tz;
use std::str::FromStr;

pub fn parse_timezone(timezone_name: &str) -> Option<Tz> {
    Tz::from_str(timezone_name).ok()
}

pub fn friendly_timezone_name(timezone_name: &str) -> String {
    if matches!(timezone_name, "UTC" | "Etc/UTC") {
        return "UTC".to_string();
    }

    timezone_name
        .split('/')
        .last()
        .unwrap_or(timezone_name)
        .replace('_', " ")
}

pub fn format_offset(offset_seconds: i32) -> String {
    let total_minutes = offset_seconds / 60;
    let sign = if total_minutes >= 0 { '+' } else { '-' };
    let absolute_minutes = total_minutes.abs();
    let hours = absolute_minutes / 60;
    let minutes = absolute_minutes % 60;
    format!("UTC{sign}{hours:02}:{minutes:02}")
}

pub fn zoned_datetime(reference_utc: DateTime<Utc>, timezone_name: &str) -> DateTime<Tz> {
    let timezone = parse_timezone(timezone_name).expect("timezone should be validated before use");
    reference_utc.with_timezone(&timezone)
}

pub fn format_display_time<T>(value: &DateTime<T>, time_format: &str) -> String
where
    T: TimeZone,
    T::Offset: std::fmt::Display,
{
    if time_format == "ampm" {
        let rendered = value.format("%I:%M %p").to_string();
        return rendered.strip_prefix('0').unwrap_or(&rendered).to_string();
    }

    value.format("%H:%M").to_string()
}

pub fn row_metadata<T>(value: &DateTime<T>) -> String
where
    T: TimeZone,
    T::Offset: std::fmt::Display,
{
    let abbreviation = value.format("%Z").to_string();
    let offset = format_offset(value.offset().fix().local_minus_utc());
    format!(
        "{}  ·  {}  ·  {}",
        value.format("%a %d %b"),
        abbreviation,
        offset
    )
}

#[cfg(test)]
mod tests {
    use super::{format_display_time, format_offset, friendly_timezone_name, zoned_datetime};
    use chrono::{TimeZone, Utc};

    #[test]
    fn formats_offsets() {
        assert_eq!(format_offset(5 * 3600 + 30 * 60), "UTC+05:30");
        assert_eq!(format_offset(-5 * 3600), "UTC-05:00");
    }

    #[test]
    fn supports_ampm_display() {
        let value = Utc.with_ymd_and_hms(2026, 4, 16, 15, 5, 0).unwrap();
        assert_eq!(format_display_time(&value, "ampm"), "3:05 PM");
    }

    #[test]
    fn renders_friendly_timezone_name() {
        assert_eq!(friendly_timezone_name("America/Cancun"), "Cancun");
        assert_eq!(friendly_timezone_name("UTC"), "UTC");
    }

    #[test]
    fn converts_to_zoned_datetime() {
        let utc = Utc.with_ymd_and_hms(2026, 4, 17, 12, 0, 0).unwrap();
        let tokyo = zoned_datetime(utc, "Asia/Tokyo");
        assert_eq!(
            tokyo.format("%Y-%m-%d %H:%M").to_string(),
            "2026-04-17 21:00"
        );
    }
}
