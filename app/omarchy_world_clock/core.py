from __future__ import annotations

from datetime import datetime, timedelta, timezone
from zoneinfo import ZoneInfo, available_timezones

TIME_ONLY_FORMATS = ("%H:%M", "%H:%M:%S")
DATETIME_FORMATS = (
    "%Y-%m-%d %H:%M",
    "%Y-%m-%d %H:%M:%S",
    "%Y/%m/%d %H:%M",
    "%Y/%m/%d %H:%M:%S",
)


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


def parse_manual_reference(
    raw_value: str,
    timezone_name: str,
    reference_utc: datetime,
) -> datetime:
    value = raw_value.strip()
    if not value:
        raise ValueError("Enter HH:MM or YYYY-MM-DD HH:MM.")

    zone = ZoneInfo(timezone_name)
    base = zoned_datetime(reference_utc, timezone_name)

    for fmt in DATETIME_FORMATS:
        try:
            parsed = datetime.strptime(value, fmt)
        except ValueError:
            continue
        return parsed.replace(tzinfo=zone).astimezone(timezone.utc)

    for fmt in TIME_ONLY_FORMATS:
        try:
            parsed = datetime.strptime(value, fmt)
        except ValueError:
            continue
        candidate = datetime(
            base.year,
            base.month,
            base.day,
            parsed.hour,
            parsed.minute,
            parsed.second,
            tzinfo=zone,
        )
        return candidate.astimezone(timezone.utc)

    raise ValueError("Use HH:MM or YYYY-MM-DD HH:MM.")
