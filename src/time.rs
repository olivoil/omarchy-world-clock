use chrono::{DateTime, NaiveDateTime, NaiveTime, Offset, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use std::str::FromStr;

pub const MANUAL_REFERENCE_ERROR: &str = "Use HH:MM, 830, 8.5, 3pm, or YYYY-MM-DD HH:MM.";

const TIME_ONLY_FORMATS: [&str; 2] = ["%H:%M", "%H:%M:%S"];
const DATETIME_FORMATS: [&str; 4] = [
    "%Y-%m-%d %H:%M",
    "%Y-%m-%d %H:%M:%S",
    "%Y/%m/%d %H:%M",
    "%Y/%m/%d %H:%M:%S",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedManualReference {
    pub reference_utc: DateTime<Utc>,
    pub normalized_text: String,
}

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

fn normalized_time_text(hour: u32, minute: u32, second: u32) -> String {
    if second > 0 {
        return format!("{hour:02}:{minute:02}:{second:02}");
    }
    format!("{hour:02}:{minute:02}")
}

fn parse_compact_time(value: &str) -> Option<(u32, u32, u32, String)> {
    if value.is_empty() || value.len() > 4 || !value.chars().all(|char| char.is_ascii_digit()) {
        return None;
    }

    let (hour, minute) = if value.len() <= 2 {
        (value.parse::<u32>().ok()?, 0)
    } else {
        let split_at = value.len() - 2;
        (
            value[..split_at].parse::<u32>().ok()?,
            value[split_at..].parse::<u32>().ok()?,
        )
    };

    if hour > 23 || minute > 59 {
        return None;
    }

    Some((hour, minute, 0, normalized_time_text(hour, minute, 0)))
}

fn parse_decimal_hour(value: &str) -> Option<(u32, u32, u32, String)> {
    let (hour_text, fraction_text) = value.split_once('.')?;
    if hour_text.is_empty()
        || fraction_text.is_empty()
        || !hour_text.chars().all(|char| char.is_ascii_digit())
    {
        return None;
    }

    let hour = hour_text.parse::<u32>().ok()?;
    let minute = match fraction_text {
        "0" | "00" => 0,
        "25" => 15,
        "5" | "50" => 30,
        "75" => 45,
        _ => return None,
    };

    if hour > 23 {
        return None;
    }

    Some((hour, minute, 0, normalized_time_text(hour, minute, 0)))
}

fn parse_meridiem_time(value: &str) -> Option<(u32, u32, u32, String)> {
    let cleaned = value
        .chars()
        .filter(|char| !char.is_whitespace() && *char != '.')
        .collect::<String>()
        .to_lowercase();
    let (digits, is_pm) = if let Some(raw) = cleaned.strip_suffix("am") {
        (raw, false)
    } else if let Some(raw) = cleaned.strip_suffix('a') {
        (raw, false)
    } else if let Some(raw) = cleaned.strip_suffix("pm") {
        (raw, true)
    } else if let Some(raw) = cleaned.strip_suffix('p') {
        (raw, true)
    } else {
        return None;
    };

    if digits.is_empty() {
        return None;
    }

    let (hour_text, minute) = if let Some((hour_text, minute_text)) = digits.split_once(':') {
        if hour_text.is_empty()
            || minute_text.len() != 2
            || !hour_text.chars().all(|char| char.is_ascii_digit())
            || !minute_text.chars().all(|char| char.is_ascii_digit())
        {
            return None;
        }
        (hour_text, minute_text.parse::<u32>().ok()?)
    } else if !digits.chars().all(|char| char.is_ascii_digit()) {
        return None;
    } else if digits.len() <= 2 {
        (digits, 0)
    } else {
        let split_at = digits.len() - 2;
        (&digits[..split_at], digits[split_at..].parse::<u32>().ok()?)
    };

    let mut hour = hour_text.parse::<u32>().ok()?;
    if !(1..=12).contains(&hour) || minute > 59 {
        return None;
    }

    if is_pm && hour != 12 {
        hour += 12;
    }
    if !is_pm && hour == 12 {
        hour = 0;
    }

    Some((hour, minute, 0, normalized_time_text(hour, minute, 0)))
}

fn parse_time_only(value: &str) -> Option<(u32, u32, u32, String)> {
    for format in TIME_ONLY_FORMATS {
        if let Ok(parsed) = NaiveTime::parse_from_str(value, format) {
            return Some((
                parsed.hour(),
                parsed.minute(),
                parsed.second(),
                normalized_time_text(parsed.hour(), parsed.minute(), parsed.second()),
            ));
        }
    }

    parse_compact_time(value)
        .or_else(|| parse_meridiem_time(value))
        .or_else(|| parse_decimal_hour(value))
}

fn localize_naive_datetime(timezone: Tz, value: NaiveDateTime) -> Option<DateTime<Utc>> {
    timezone
        .from_local_datetime(&value)
        .earliest()
        .or_else(|| timezone.from_local_datetime(&value).latest())
        .map(|datetime| datetime.with_timezone(&Utc))
}

pub fn parse_manual_reference_details(
    raw_value: &str,
    timezone_name: &str,
    reference_utc: DateTime<Utc>,
) -> Result<ParsedManualReference, &'static str> {
    let value = raw_value.trim();
    if value.is_empty() {
        return Err(MANUAL_REFERENCE_ERROR);
    }

    let timezone = parse_timezone(timezone_name).ok_or(MANUAL_REFERENCE_ERROR)?;
    let base = zoned_datetime(reference_utc, timezone_name);

    for format in DATETIME_FORMATS {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(value, format) {
            let normalized_text = if parsed.second() == 0 {
                parsed.format("%Y-%m-%d %H:%M").to_string()
            } else {
                parsed.format("%Y-%m-%d %H:%M:%S").to_string()
            };
            let reference_utc =
                localize_naive_datetime(timezone, parsed).ok_or(MANUAL_REFERENCE_ERROR)?;
            return Ok(ParsedManualReference {
                reference_utc,
                normalized_text,
            });
        }
    }

    let (hour, minute, second, normalized_text) =
        parse_time_only(value).ok_or(MANUAL_REFERENCE_ERROR)?;
    let parsed = base
        .date_naive()
        .and_hms_opt(hour, minute, second)
        .ok_or(MANUAL_REFERENCE_ERROR)?;
    let reference_utc = localize_naive_datetime(timezone, parsed).ok_or(MANUAL_REFERENCE_ERROR)?;
    Ok(ParsedManualReference {
        reference_utc,
        normalized_text,
    })
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
    use super::{
        format_display_time, format_offset, friendly_timezone_name, parse_manual_reference_details,
        zoned_datetime, MANUAL_REFERENCE_ERROR,
    };
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

    #[test]
    fn parses_compact_manual_reference() {
        let reference = Utc.with_ymd_and_hms(2026, 4, 17, 12, 0, 0).unwrap();
        let parsed = parse_manual_reference_details("830", "America/Cancun", reference).unwrap();

        assert_eq!(parsed.normalized_text, "08:30");
        assert_eq!(
            zoned_datetime(parsed.reference_utc, "America/Cancun")
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-04-17 08:30"
        );
    }

    #[test]
    fn parses_meridiem_manual_reference() {
        let reference = Utc.with_ymd_and_hms(2026, 4, 17, 12, 0, 0).unwrap();
        let parsed = parse_manual_reference_details("3pm", "America/Cancun", reference).unwrap();

        assert_eq!(parsed.normalized_text, "15:00");
        assert_eq!(
            zoned_datetime(parsed.reference_utc, "America/Cancun")
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-04-17 15:00"
        );
    }

    #[test]
    fn parses_full_datetime_manual_reference() {
        let reference = Utc.with_ymd_and_hms(2026, 4, 17, 12, 0, 0).unwrap();
        let parsed =
            parse_manual_reference_details("2026-04-18 09:45", "Europe/Paris", reference).unwrap();

        assert_eq!(parsed.normalized_text, "2026-04-18 09:45");
        assert_eq!(
            zoned_datetime(parsed.reference_utc, "Europe/Paris")
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-04-18 09:45"
        );
    }

    #[test]
    fn rejects_invalid_manual_reference() {
        let reference = Utc.with_ymd_and_hms(2026, 4, 17, 12, 0, 0).unwrap();
        let error =
            parse_manual_reference_details("nonsense", "America/Cancun", reference).unwrap_err();
        assert_eq!(error, MANUAL_REFERENCE_ERROR);
    }
}
