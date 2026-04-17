from __future__ import annotations

import json
import subprocess
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from functools import lru_cache
from pathlib import Path
import unicodedata
import urllib.parse
import urllib.request
from zoneinfo import TZPATH, ZoneInfo

from .core import all_timezones, friendly_timezone_name

DEFAULT_TIMEZONES: list[str] = []
DEFAULT_SORT_MODE = "manual"
VALID_SORT_MODES = {"manual", "alpha", "time"}
STANDARD_TZ_REGIONS = {
    "Africa",
    "America",
    "Antarctica",
    "Arctic",
    "Asia",
    "Atlantic",
    "Australia",
    "Europe",
    "Indian",
    "Pacific",
}
MANUAL_CITY_ALIASES: dict[str, str] = {
    "Austin": "America/Chicago",
    "Delhi": "Asia/Kolkata",
    "Faridabad": "Asia/Kolkata",
    "Gurgaon": "Asia/Kolkata",
    "Gurugram": "Asia/Kolkata",
    "New Delhi": "Asia/Kolkata",
    "Noida": "Asia/Kolkata",
}


@lru_cache(maxsize=1)
def timezone_link_aliases() -> dict[str, str]:
    links: dict[str, str] = {}
    tzdata_paths = [Path(raw_path) for raw_path in TZPATH]
    default_tzdata_path = Path("/usr/share/zoneinfo")
    if default_tzdata_path not in tzdata_paths:
        tzdata_paths.append(default_tzdata_path)

    for base_path in tzdata_paths:
        tzdata_path = base_path / "tzdata.zi"
        if not tzdata_path.exists():
            continue

        try:
            with tzdata_path.open("r", encoding="utf-8") as handle:
                for raw_line in handle:
                    if not raw_line.startswith("L "):
                        continue
                    _, target, alias, *_ = raw_line.split()
                    if "/" not in alias:
                        continue
                    links[alias] = target
        except OSError:
            continue

    def resolve(name: str) -> str:
        current = name
        seen: set[str] = set()
        while current in links and current not in seen:
            seen.add(current)
            current = links[current]
        return current

    return {alias: resolve(alias) for alias in links}


def canonical_timezone_name(timezone_name: str) -> str:
    candidate = timezone_name.strip()
    if not candidate:
        return candidate
    return timezone_link_aliases().get(candidate, candidate)


def canonical_timezone_names(zones: list[str]) -> list[str]:
    canonical: list[str] = []
    seen: set[str] = set()
    for timezone_name in zones:
        resolved = canonical_timezone_name(timezone_name)
        if not resolved or resolved in seen:
            continue
        seen.add(resolved)
        canonical.append(resolved)
    return canonical


def detect_local_timezone() -> str:
    try:
        result = subprocess.run(
            ["timedatectl", "show", "--property=Timezone", "--value"],
            check=True,
            capture_output=True,
            text=True,
        )
        timezone_name = result.stdout.strip()
        if timezone_name:
            timezone_name = canonical_timezone_name(timezone_name)
            ZoneInfo(timezone_name)
            return timezone_name
    except Exception:
        pass

    tzinfo = datetime.now().astimezone().tzinfo
    timezone_name = getattr(tzinfo, "key", None)
    if timezone_name:
        return canonical_timezone_name(timezone_name)
    return "UTC"


@dataclass
class AppConfig:
    timezones: list["TimezoneEntry"]
    sort_mode: str = DEFAULT_SORT_MODE


@dataclass
class TimezoneEntry:
    timezone: str
    label: str = ""

    def display_label(self) -> str:
        value = self.label.strip()
        return value or friendly_timezone_name(self.timezone)


@dataclass(frozen=True)
class TimezoneSearchResult:
    timezone: str
    title: str
    subtitle: str


@dataclass(frozen=True)
class AliasRecord:
    alias: str
    normalized_alias: str
    alias_words: tuple[str, ...]
    timezone: str


@dataclass(frozen=True)
class TimezoneRecord:
    timezone: str
    normalized_timezone: str
    city: str
    normalized_city: str
    search_words: tuple[str, ...]
    abbreviations: tuple[str, ...]
    abbreviations_folded: tuple[str, ...]
    search_blob: str


class ConfigManager:
    def __init__(self, path: Path | None = None) -> None:
        self.path = path or Path.home() / ".config" / "omarchy-world-clock" / "config.json"
        self.path.parent.mkdir(parents=True, exist_ok=True)

    def load(self) -> AppConfig:
        if not self.path.exists():
            config = AppConfig(
                timezones=[
                    TimezoneEntry(timezone=zone)
                    for zone in canonical_timezone_names(DEFAULT_TIMEZONES)
                ]
            )
            self.save(config)
            return config

        try:
            with self.path.open("r", encoding="utf-8") as handle:
                raw = json.load(handle)
        except Exception:
            config = AppConfig(
                timezones=[
                    TimezoneEntry(timezone=zone)
                    for zone in canonical_timezone_names(DEFAULT_TIMEZONES)
                ]
            )
            self.save(config)
            return config

        entries: list[TimezoneEntry] = []
        seen: set[str] = set()
        for raw_entry in raw.get("timezones", []):
            entry = self._parse_entry(raw_entry)
            if entry is None or entry.timezone in seen:
                continue
            seen.add(entry.timezone)
            entries.append(entry)

        sort_mode = raw.get("sort_mode", DEFAULT_SORT_MODE)
        if sort_mode not in VALID_SORT_MODES:
            sort_mode = DEFAULT_SORT_MODE

        config = AppConfig(timezones=entries, sort_mode=sort_mode)
        self.save(config)
        return config

    def save(self, config: AppConfig) -> None:
        payload = {
            "timezones": [
                {
                    "timezone": entry.timezone,
                    "label": entry.label.strip(),
                }
                for entry in config.timezones
            ],
            "sort_mode": config.sort_mode if config.sort_mode in VALID_SORT_MODES else DEFAULT_SORT_MODE,
        }
        with self.path.open("w", encoding="utf-8") as handle:
            json.dump(payload, handle, indent=2)
            handle.write("\n")

    def add_timezone(self, timezone_name: str, label: str = "") -> AppConfig:
        config = self.load()
        timezone_name = canonical_timezone_name(timezone_name)
        if timezone_name not in {entry.timezone for entry in config.timezones}:
            config.timezones.append(TimezoneEntry(timezone=timezone_name, label=label.strip()))
            self.save(config)
        return config

    def remove_timezone(self, timezone_name: str) -> AppConfig:
        config = self.load()
        timezone_name = canonical_timezone_name(timezone_name)
        config.timezones = [
            entry for entry in config.timezones if entry.timezone != timezone_name
        ]
        self.save(config)
        return config

    def move_timezone(self, timezone_name: str, offset: int) -> AppConfig:
        config = self.load()
        timezone_name = canonical_timezone_name(timezone_name)
        index = next(
            (position for position, entry in enumerate(config.timezones) if entry.timezone == timezone_name),
            None,
        )
        if index is None:
            return config

        target_index = max(0, min(len(config.timezones) - 1, index + offset))
        if target_index == index:
            return config

        entry = config.timezones.pop(index)
        config.timezones.insert(target_index, entry)
        self.save(config)
        return config

    def set_sort_mode(self, sort_mode: str) -> AppConfig:
        config = self.load()
        config.sort_mode = sort_mode if sort_mode in VALID_SORT_MODES else DEFAULT_SORT_MODE
        self.save(config)
        return config

    @staticmethod
    def _parse_entry(raw_entry: object) -> TimezoneEntry | None:
        if isinstance(raw_entry, str):
            timezone_name = raw_entry
            label = ""
        elif isinstance(raw_entry, dict):
            raw_timezone = raw_entry.get("timezone", "")
            raw_label = raw_entry.get("label", "")
            timezone_name = raw_timezone.strip() if isinstance(raw_timezone, str) else ""
            label = raw_label.strip() if isinstance(raw_label, str) else ""
        else:
            return None

        timezone_name = canonical_timezone_name(timezone_name)
        if not ConfigManager.is_valid_timezone(timezone_name):
            return None
        return TimezoneEntry(timezone=timezone_name, label=label)

    @staticmethod
    def is_valid_timezone(timezone_name: str) -> bool:
        try:
            ZoneInfo(canonical_timezone_name(timezone_name))
            return True
        except Exception:
            return False


class TimezoneResolver:
    def __init__(self, zones: list[str] | None = None) -> None:
        self.zones = canonical_timezone_names(zones or all_timezones())
        self.alias_records = self._build_alias_records()
        self.alias_lookup: dict[str, list[AliasRecord]] = {}
        self.direct_lookup = {zone.casefold(): zone for zone in self.zones}
        self.city_lookup: dict[str, list[str]] = {}
        self.normalized_timezone_lookup: dict[str, list[str]] = {}
        self.abbreviation_lookup: dict[str, list[str]] = {}
        self.records = [self._build_record(zone) for zone in self.zones]
        for alias in self.alias_records:
            self.alias_lookup.setdefault(alias.normalized_alias, []).append(alias)
        for record in self.records:
            self.normalized_timezone_lookup.setdefault(record.normalized_timezone, []).append(
                record.timezone
            )
            self.city_lookup.setdefault(record.normalized_city, []).append(record.timezone)
            for abbreviation in record.abbreviations_folded:
                self.abbreviation_lookup.setdefault(abbreviation, []).append(record.timezone)

    def resolve(self, raw_value: str) -> str | None:
        candidate = raw_value.strip()
        if not candidate:
            return None

        exact = self.direct_lookup.get(candidate.casefold())
        if exact:
            return exact

        normalized = self._normalize(candidate)
        exact_normalized = self.normalized_timezone_lookup.get(normalized, [])
        if len(exact_normalized) == 1:
            return exact_normalized[0]

        alias_matches = self.alias_lookup.get(normalized, [])
        if alias_matches:
            timezones = {alias.timezone for alias in alias_matches}
            if len(timezones) == 1:
                return next(iter(timezones))

        city_matches = self.city_lookup.get(normalized, [])
        if len(city_matches) == 1:
            return city_matches[0]

        abbreviation_matches = self.abbreviation_lookup.get(normalized, [])
        if len(abbreviation_matches) == 1:
            return abbreviation_matches[0]

        matches = self.search(candidate, limit=2)
        if len(matches) == 1:
            return matches[0].timezone
        return None

    def search(self, raw_value: str, limit: int = 8) -> list[TimezoneSearchResult]:
        query = self._normalize(raw_value)
        if not query:
            return []

        alias_scored: list[tuple[int, str, str, AliasRecord]] = []
        for alias in self.alias_records:
            score = self._score_alias(alias, query)
            if score is None:
                continue
            alias_scored.append((score, alias.alias, alias.timezone, alias))

        scored: list[tuple[int, str, str, TimezoneRecord]] = []
        for record in self.records:
            score = self._score_record(record, query)
            if score is None:
                continue
            scored.append((score, record.city, record.timezone, record))

        alias_scored.sort(key=lambda item: (-item[0], item[1], item[2]))
        scored.sort(key=lambda item: (-item[0], item[1], item[2]))
        results: list[TimezoneSearchResult] = []
        seen_timezones: set[str] = set()
        for _, _, _, alias in alias_scored:
            if alias.timezone in seen_timezones:
                continue
            seen_timezones.add(alias.timezone)
            record = self.direct_lookup_record(alias.timezone)
            abbreviation_text = " / ".join(record.abbreviations) if record.abbreviations else "Timezone"
            results.append(
                TimezoneSearchResult(
                    timezone=alias.timezone,
                    title=alias.alias,
                    subtitle=f"{alias.timezone}  ·  {abbreviation_text}",
                )
            )
            if len(results) >= limit:
                break

        for _, _, _, record in scored:
            if record.timezone in seen_timezones:
                continue
            seen_timezones.add(record.timezone)
            abbreviation_text = " / ".join(record.abbreviations) if record.abbreviations else "Timezone"
            results.append(
                TimezoneSearchResult(
                    timezone=record.timezone,
                    title=record.city,
                    subtitle=f"{record.timezone}  ·  {abbreviation_text}",
                )
            )
            if len(results) >= limit:
                break
        return results

    @staticmethod
    def _normalize(value: str) -> str:
        normalized = unicodedata.normalize("NFKD", value)
        without_marks = "".join(
            character for character in normalized if not unicodedata.combining(character)
        )
        translated = without_marks.translate(
            str.maketrans({
                "/": " ",
                "_": " ",
                "-": " ",
                ".": " ",
                ",": " ",
                ":": " ",
                "(": " ",
                ")": " ",
                "'": " ",
            })
        )
        return " ".join(translated.casefold().split())

    def _build_alias_records(self) -> list[AliasRecord]:
        aliases: dict[tuple[str, str], AliasRecord] = {}
        for alias, timezone_name in sorted(MANUAL_CITY_ALIASES.items()):
            self._add_alias_record(aliases, alias, timezone_name)

        for alias, timezone_name in sorted(timezone_link_aliases().items()):
            self._add_alias_record(aliases, alias, timezone_name)

            alias_region, _, alias_city = alias.partition("/")
            timezone_region, _, _ = timezone_name.partition("/")
            if (
                alias_region
                and alias_region == timezone_region
                and alias_region in STANDARD_TZ_REGIONS
                and self._is_city_alias_candidate(alias_city)
            ):
                self._add_alias_record(aliases, alias_city.replace("_", " "), timezone_name)

        return sorted(aliases.values(), key=lambda record: (record.alias, record.timezone))

    def _add_alias_record(
        self,
        aliases: dict[tuple[str, str], AliasRecord],
        alias: str,
        timezone_name: str,
    ) -> None:
        canonical_timezone = canonical_timezone_name(timezone_name)
        if canonical_timezone not in self.zones:
            return

        normalized_alias = self._normalize(alias)
        if not normalized_alias:
            return

        key = (alias, canonical_timezone)
        if key in aliases:
            return

        aliases[key] = AliasRecord(
            alias=alias,
            normalized_alias=normalized_alias,
            alias_words=tuple(dict.fromkeys(normalized_alias.split())),
            timezone=canonical_timezone,
        )

    @staticmethod
    def _is_city_alias_candidate(value: str) -> bool:
        letters = [character for character in value if character.isalpha()]
        if len(letters) < 4:
            return False
        return value.upper() != value

    def direct_lookup_record(self, timezone_name: str) -> TimezoneRecord:
        for record in self.records:
            if record.timezone == timezone_name:
                return record
        raise KeyError(timezone_name)

    def _build_record(self, timezone_name: str) -> TimezoneRecord:
        now_utc = datetime.now(timezone.utc)
        zone = ZoneInfo(timezone_name)
        abbreviations: list[str] = []
        seasonal_samples = (
            now_utc,
            datetime(now_utc.year, 1, 15, tzinfo=timezone.utc),
            datetime(now_utc.year, 7, 15, tzinfo=timezone.utc),
            now_utc + timedelta(days=182),
        )
        for moment in seasonal_samples:
            abbreviation = moment.astimezone(zone).tzname()
            if abbreviation and abbreviation not in abbreviations:
                abbreviations.append(abbreviation)

        city = timezone_name.split("/")[-1].replace("_", " ")
        search_blob = timezone_name.replace("_", " ").replace("-", " ")
        words = tuple(dict.fromkeys(self._normalize(search_blob).split()))
        return TimezoneRecord(
            timezone=timezone_name,
            normalized_timezone=self._normalize(timezone_name.replace("/", " ")),
            city=city,
            normalized_city=self._normalize(city),
            search_words=words,
            abbreviations=tuple(abbreviations),
            abbreviations_folded=tuple(abbreviation.casefold() for abbreviation in abbreviations),
            search_blob=self._normalize(search_blob),
        )

    def _score_record(self, record: TimezoneRecord, query: str) -> int | None:
        if query == record.timezone.casefold():
            return 1400
        if query == record.normalized_timezone:
            return 1360
        if query == record.normalized_city:
            return 1320
        if query in record.abbreviations_folded:
            return 1280 if len(record.abbreviations_folded) == 1 else 1260
        if record.normalized_timezone.startswith(query):
            return 1180
        if any(word.startswith(query) for word in record.search_words):
            return 1120
        if query in record.normalized_city:
            return 1060
        if query in record.normalized_timezone:
            return 1000
        if any(query in abbreviation for abbreviation in record.abbreviations_folded):
            return 960
        if query in record.search_blob:
            return 920
        return None

    def _score_alias(self, alias: AliasRecord, query: str) -> int | None:
        if query == alias.normalized_alias:
            return 1500
        if alias.normalized_alias.startswith(query):
            return 1440
        if any(word.startswith(query) for word in alias.alias_words):
            return 1400
        if query in alias.normalized_alias:
            return 1340
        return None


class RemotePlaceSearch:
    endpoint = "https://geocoding-api.open-meteo.com/v1/search"

    def __init__(self, zones: list[str] | None = None, timeout: float = 2.5) -> None:
        self.zones = set(canonical_timezone_names(zones or all_timezones()))
        self.timeout = timeout
        self.cache: dict[str, tuple[TimezoneSearchResult, ...]] = {}

    def search(self, raw_value: str, limit: int = 8) -> list[TimezoneSearchResult]:
        query = " ".join(raw_value.strip().split())
        query_key = TimezoneResolver._normalize(query)
        if len(query_key) < 3:
            return []

        cached = self.cache.get(query_key)
        if cached is None:
            cached = tuple(self._fetch(query))
            self.cache[query_key] = cached
        return list(cached[:limit])

    def _fetch(self, query: str) -> list[TimezoneSearchResult]:
        params = urllib.parse.urlencode(
            {
                "name": query,
                "count": 12,
                "format": "json",
            }
        )
        request = urllib.request.Request(
            f"{self.endpoint}?{params}",
            headers={
                "Accept": "application/json",
                "User-Agent": "omarchy-world-clock/1.0",
            },
        )

        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                payload = json.load(response)
        except Exception:
            return []

        raw_results = payload.get("results")
        if not isinstance(raw_results, list):
            return []

        results: list[TimezoneSearchResult] = []
        seen_timezones: set[str] = set()
        for item in raw_results:
            if not isinstance(item, dict):
                continue

            raw_timezone = item.get("timezone")
            if not isinstance(raw_timezone, str):
                continue

            timezone_name = canonical_timezone_name(raw_timezone.strip())
            if timezone_name not in self.zones or timezone_name in seen_timezones:
                continue

            title = self._format_title(item)
            if not title:
                continue

            subtitle_parts = [timezone_name]
            location_summary = self._format_location_summary(item)
            if location_summary:
                subtitle_parts.append(location_summary)

            seen_timezones.add(timezone_name)
            results.append(
                TimezoneSearchResult(
                    timezone=timezone_name,
                    title=title,
                    subtitle="  ·  ".join(subtitle_parts),
                )
            )
        return results

    @staticmethod
    def _format_title(item: dict[str, object]) -> str:
        return ", ".join(
            RemotePlaceSearch._unique_parts(
                item.get("name"),
                item.get("admin1"),
                item.get("country"),
            )
        )

    @staticmethod
    def _format_location_summary(item: dict[str, object]) -> str:
        parts = RemotePlaceSearch._unique_parts(item.get("admin1"), item.get("country"))
        return ", ".join(parts)

    @staticmethod
    def _unique_parts(*values: object) -> list[str]:
        parts: list[str] = []
        seen: set[str] = set()
        for value in values:
            if not isinstance(value, str):
                continue
            cleaned = " ".join(value.split())
            if not cleaned:
                continue
            folded = cleaned.casefold()
            if folded in seen:
                continue
            seen.add(folded)
            parts.append(cleaned)
        return parts
