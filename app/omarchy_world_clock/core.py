from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
import re
from zoneinfo import ZoneInfo, available_timezones

TIME_ONLY_FORMATS = ("%H:%M", "%H:%M:%S")
DATETIME_FORMATS = (
    "%Y-%m-%d %H:%M",
    "%Y-%m-%d %H:%M:%S",
    "%Y/%m/%d %H:%M",
    "%Y/%m/%d %H:%M:%S",
)
DECIMAL_HOUR_FRACTIONS = {
    "0": 0,
    "00": 0,
    "25": 15,
    "5": 30,
    "50": 30,
    "75": 45,
}
MANUAL_REFERENCE_ERROR = "Use HH:MM, 830, 8.5, 3pm, or YYYY-MM-DD HH:MM."


@dataclass(frozen=True)
class ParsedManualReference:
    reference_utc: datetime
    normalized_text: str


def all_timezones() -> list[str]:
    return sorted(available_timezones())


def friendly_timezone_name(timezone_name: str) -> str:
    if timezone_name in {"UTC", "Etc/UTC"}:
        return "UTC"
    return timezone_name.split("/")[-1].replace("_", " ")


def timezone_context_label(timezone_name: str) -> str:
    parts = [part.replace("_", " ") for part in timezone_name.split("/")]
    if len(parts) <= 1:
        return ""
    return " / ".join(parts[:-1])


def format_offset(offset: timedelta | None) -> str:
    if offset is None:
        return "UTC"
    total_minutes = int(offset.total_seconds() // 60)
    sign = "+" if total_minutes >= 0 else "-"
    total_minutes = abs(total_minutes)
    hours, minutes = divmod(total_minutes, 60)
    return f"UTC{sign}{hours:02d}:{minutes:02d}"


def zoned_datetime(reference_utc: datetime, timezone_name: str) -> datetime:
    if reference_utc.tzinfo is None:
        reference_utc = reference_utc.replace(tzinfo=timezone.utc)
    return reference_utc.astimezone(ZoneInfo(timezone_name))


def _normalized_time_text(hour: int, minute: int, second: int = 0) -> str:
    if second:
        return f"{hour:02d}:{minute:02d}:{second:02d}"
    return f"{hour:02d}:{minute:02d}"


def _parse_compact_time(value: str) -> tuple[int, int, int, str] | None:
    if not value.isdigit() or len(value) > 4:
        return None

    if len(value) <= 2:
        hour = int(value)
        minute = 0
    else:
        hour = int(value[:-2])
        minute = int(value[-2:])

    if hour > 23 or minute > 59:
        return None
    return hour, minute, 0, _normalized_time_text(hour, minute)


def _parse_decimal_hour(value: str) -> tuple[int, int, int, str] | None:
    hour_text, separator, fraction_text = value.partition(".")
    if separator != "." or not hour_text.isdigit() or not fraction_text:
        return None

    hour = int(hour_text)
    minute = DECIMAL_HOUR_FRACTIONS.get(fraction_text)
    if hour > 23 or minute is None:
        return None
    return hour, minute, 0, _normalized_time_text(hour, minute)


def _parse_meridiem_time(value: str) -> tuple[int, int, int, str] | None:
    cleaned = re.sub(r"\s+", "", value).lower().replace(".", "")
    match = re.fullmatch(r"(\d{1,4})(?::(\d{2}))?([ap]m?)", cleaned)
    if match is None:
        return None

    digits, minute_text, meridiem = match.groups()
    if minute_text is None:
        if len(digits) <= 2:
            hour_text = digits
            minute = 0
        else:
            hour_text = digits[:-2]
            minute = int(digits[-2:])
    else:
        hour_text = digits
        minute = int(minute_text)

    hour = int(hour_text)
    if not 1 <= hour <= 12 or minute > 59:
        return None

    if meridiem.startswith("p") and hour != 12:
        hour += 12
    if meridiem.startswith("a") and hour == 12:
        hour = 0
    return hour, minute, 0, _normalized_time_text(hour, minute)


def _parse_time_only(value: str) -> tuple[int, int, int, str] | None:
    for fmt in TIME_ONLY_FORMATS:
        try:
            parsed = datetime.strptime(value, fmt)
        except ValueError:
            continue
        return (
            parsed.hour,
            parsed.minute,
            parsed.second,
            _normalized_time_text(parsed.hour, parsed.minute, parsed.second),
        )

    compact = _parse_compact_time(value)
    if compact is not None:
        return compact

    meridiem = _parse_meridiem_time(value)
    if meridiem is not None:
        return meridiem

    return _parse_decimal_hour(value)


def parse_manual_reference_details(
    raw_value: str,
    timezone_name: str,
    reference_utc: datetime,
) -> ParsedManualReference:
    value = raw_value.strip()
    if not value:
        raise ValueError(MANUAL_REFERENCE_ERROR)

    zone = ZoneInfo(timezone_name)
    base = zoned_datetime(reference_utc, timezone_name)

    for fmt in DATETIME_FORMATS:
        try:
            parsed = datetime.strptime(value, fmt)
        except ValueError:
            continue
        zoned = parsed.replace(tzinfo=zone)
        normalized_text = parsed.strftime(
            "%Y-%m-%d %H:%M:%S" if parsed.second else "%Y-%m-%d %H:%M"
        )
        return ParsedManualReference(
            reference_utc=zoned.astimezone(timezone.utc),
            normalized_text=normalized_text,
        )

    parsed_time = _parse_time_only(value)
    if parsed_time is None:
        raise ValueError(MANUAL_REFERENCE_ERROR)

    hour, minute, second, normalized_text = parsed_time
    candidate = datetime(
        base.year,
        base.month,
        base.day,
        hour,
        minute,
        second,
        tzinfo=zone,
    )
    return ParsedManualReference(
        reference_utc=candidate.astimezone(timezone.utc),
        normalized_text=normalized_text,
    )


def parse_manual_reference(
    raw_value: str,
    timezone_name: str,
    reference_utc: datetime,
) -> datetime:
    return parse_manual_reference_details(raw_value, timezone_name, reference_utc).reference_utc
