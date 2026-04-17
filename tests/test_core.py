from __future__ import annotations

import contextlib
import io
import json
import tempfile
import unittest
from datetime import datetime, timezone
from pathlib import Path
from unittest.mock import patch

from omarchy_world_clock.configuration import (
    ConfigManager,
    RemotePlaceSearch,
    TimezoneEntry,
    TimezoneResolver,
    TimezoneSearchResult,
    detect_system_time_format,
    ordered_timezones,
    timezone_link_aliases,
)
from omarchy_world_clock.core import (
    format_display_time,
    format_offset,
    parse_manual_reference,
    parse_manual_reference_details,
    zoned_datetime,
)


class CoreTests(unittest.TestCase):
    def test_parse_time_only_uses_current_date_in_zone(self) -> None:
        reference = datetime(2026, 4, 16, 18, 0, tzinfo=timezone.utc)
        parsed = parse_manual_reference("09:30", "America/New_York", reference)
        zoned = zoned_datetime(parsed, "America/New_York")
        self.assertEqual(zoned.strftime("%Y-%m-%d %H:%M"), "2026-04-16 09:30")

    def test_parse_time_only_accepts_compact_three_digits(self) -> None:
        reference = datetime(2026, 4, 16, 18, 0, tzinfo=timezone.utc)
        parsed = parse_manual_reference("830", "America/New_York", reference)
        zoned = zoned_datetime(parsed, "America/New_York")
        self.assertEqual(zoned.strftime("%Y-%m-%d %H:%M"), "2026-04-16 08:30")

    def test_parse_time_only_accepts_compact_four_digits(self) -> None:
        reference = datetime(2026, 4, 16, 18, 0, tzinfo=timezone.utc)
        parsed = parse_manual_reference("0830", "America/New_York", reference)
        zoned = zoned_datetime(parsed, "America/New_York")
        self.assertEqual(zoned.strftime("%Y-%m-%d %H:%M"), "2026-04-16 08:30")

    def test_parse_time_only_accepts_decimal_half_hour(self) -> None:
        reference = datetime(2026, 4, 16, 18, 0, tzinfo=timezone.utc)
        parsed = parse_manual_reference_details("8.5", "America/New_York", reference)
        zoned = zoned_datetime(parsed.reference_utc, "America/New_York")
        self.assertEqual(parsed.normalized_text, "08:30")
        self.assertEqual(zoned.strftime("%Y-%m-%d %H:%M"), "2026-04-16 08:30")

    def test_parse_time_only_accepts_meridiem_hour(self) -> None:
        reference = datetime(2026, 4, 16, 18, 0, tzinfo=timezone.utc)
        parsed = parse_manual_reference_details("3pm", "America/New_York", reference)
        zoned = zoned_datetime(parsed.reference_utc, "America/New_York")
        self.assertEqual(parsed.normalized_text, "15:00")
        self.assertEqual(zoned.strftime("%Y-%m-%d %H:%M"), "2026-04-16 15:00")

    def test_parse_time_only_accepts_meridiem_with_space(self) -> None:
        reference = datetime(2026, 4, 16, 18, 0, tzinfo=timezone.utc)
        parsed = parse_manual_reference_details("8 am", "America/New_York", reference)
        zoned = zoned_datetime(parsed.reference_utc, "America/New_York")
        self.assertEqual(parsed.normalized_text, "08:00")
        self.assertEqual(zoned.strftime("%Y-%m-%d %H:%M"), "2026-04-16 08:00")

    def test_parse_time_only_accepts_meridiem_midnight(self) -> None:
        reference = datetime(2026, 4, 16, 18, 0, tzinfo=timezone.utc)
        parsed = parse_manual_reference_details("12am", "America/New_York", reference)
        zoned = zoned_datetime(parsed.reference_utc, "America/New_York")
        self.assertEqual(parsed.normalized_text, "00:00")
        self.assertEqual(zoned.strftime("%Y-%m-%d %H:%M"), "2026-04-16 00:00")

    def test_parse_full_datetime(self) -> None:
        reference = datetime(2026, 4, 16, 18, 0, tzinfo=timezone.utc)
        parsed = parse_manual_reference("2026-04-18 21:15", "Asia/Tokyo", reference)
        zoned = zoned_datetime(parsed, "Asia/Tokyo")
        self.assertEqual(zoned.strftime("%Y-%m-%d %H:%M"), "2026-04-18 21:15")

    def test_format_offset(self) -> None:
        reference = datetime(2026, 1, 8, 12, 0, tzinfo=timezone.utc)
        zoned = zoned_datetime(reference, "Asia/Kolkata")
        self.assertEqual(format_offset(zoned.utcoffset()), "UTC+05:30")

    def test_format_display_time_supports_ampm(self) -> None:
        value = datetime(2026, 4, 16, 15, 5, tzinfo=timezone.utc)
        self.assertEqual(format_display_time(value, "ampm"), "3:05 PM")

    def test_timezone_resolver_city_alias(self) -> None:
        resolver = TimezoneResolver(["Asia/Tokyo", "Europe/Paris"])
        self.assertEqual(resolver.resolve("Tokyo"), "Asia/Tokyo")
        self.assertEqual(resolver.resolve("Europe/Paris"), "Europe/Paris")
        self.assertIsNone(resolver.resolve("America"))

    def test_timezone_resolver_search_matches_city_and_abbreviation(self) -> None:
        resolver = TimezoneResolver(["Asia/Kolkata", "America/Vancouver"])

        city_results = resolver.search("Vancouver")
        self.assertTrue(city_results)
        self.assertEqual(city_results[0].timezone, "America/Vancouver")

        abbreviation_results = resolver.search("IST")
        self.assertTrue(abbreviation_results)
        self.assertEqual(abbreviation_results[0].timezone, "Asia/Kolkata")

    def test_timezone_resolver_city_aliases(self) -> None:
        resolver = TimezoneResolver(["Asia/Kolkata", "America/Chicago"])

        self.assertEqual(resolver.resolve("New Delhi"), "Asia/Kolkata")
        self.assertEqual(resolver.resolve("Faridabad"), "Asia/Kolkata")
        self.assertEqual(resolver.resolve("Austin"), "America/Chicago")

        alias_results = resolver.search("New Delhi")
        self.assertTrue(alias_results)
        self.assertEqual(alias_results[0].title, "New Delhi")
        self.assertEqual(alias_results[0].timezone, "Asia/Kolkata")

    def test_timezone_resolver_normalized_timezone_exact_match(self) -> None:
        resolver = TimezoneResolver(["America/New_York", "America/Chicago"])

        self.assertEqual(resolver.resolve("America New York"), "America/New_York")
        self.assertEqual(resolver.resolve("America Chicago"), "America/Chicago")

    def test_timezone_resolver_tzdata_aliases(self) -> None:
        if not timezone_link_aliases():
            self.skipTest("tzdata link aliases unavailable")

        resolver = TimezoneResolver(["Asia/Kolkata", "America/New_York"])

        self.assertEqual(resolver.resolve("Calcutta"), "Asia/Kolkata")
        self.assertEqual(resolver.resolve("Asia/Calcutta"), "Asia/Kolkata")
        self.assertEqual(resolver.resolve("US Eastern"), "America/New_York")

        alias_results = resolver.search("Calcutta")
        self.assertTrue(alias_results)
        self.assertEqual(alias_results[0].title, "Calcutta")
        self.assertEqual(alias_results[0].timezone, "Asia/Kolkata")

    def test_remote_place_search_maps_places_to_canonical_timezones(self) -> None:
        payload = {
            "results": [
                {
                    "name": "Bengaluru",
                    "admin1": "Karnataka",
                    "country": "India",
                    "timezone": "Asia/Calcutta",
                },
                {
                    "name": "Mumbai",
                    "admin1": "Maharashtra",
                    "country": "India",
                    "timezone": "Asia/Kolkata",
                },
                {
                    "name": "Broken",
                    "country": "Nowhere",
                    "timezone": "Mars/OlympusMons",
                },
            ]
        }
        search = RemotePlaceSearch(["Asia/Kolkata"])

        with patch(
            "omarchy_world_clock.configuration.urllib.request.urlopen",
            return_value=contextlib.nullcontext(io.StringIO(json.dumps(payload))),
        ) as urlopen:
            first_results = search.search("Bangalore")
            second_results = search.search("bangalore")

        self.assertEqual(
            first_results,
            [
                TimezoneSearchResult(
                    timezone="Asia/Kolkata",
                    title="Bengaluru, Karnataka, India",
                    subtitle="Asia/Kolkata  ·  Karnataka, India",
                )
            ],
        )
        self.assertEqual(second_results, first_results)
        self.assertEqual(urlopen.call_count, 1)

    def test_config_round_trip(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "config.json"
            with patch("omarchy_world_clock.configuration.detect_local_timezone", return_value="UTC"):
                manager = ConfigManager(path)
                config = manager.load()
                self.assertEqual(config.timezones, [TimezoneEntry(timezone="UTC", label="")])
                self.assertEqual(config.sort_mode, "manual")
                self.assertEqual(config.time_format, "system")

                manager.add_timezone("Asia/Tokyo", label="Tokyo")
                loaded = manager.load()
                self.assertEqual(
                    loaded.timezones,
                    [
                        TimezoneEntry(timezone="UTC", label=""),
                        TimezoneEntry(timezone="Asia/Tokyo", label="Tokyo"),
                    ],
                )
                self.assertEqual(loaded.time_format, "system")

                manager.remove_timezone("UTC")
                self.assertEqual(
                    manager.load().timezones,
                    [TimezoneEntry(timezone="Asia/Tokyo", label="Tokyo")],
                )

    def test_config_loads_legacy_timezone_list(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "config.json"
            path.write_text('{"timezones": ["UTC", "Asia/Tokyo"]}\n', encoding="utf-8")

            with patch("omarchy_world_clock.configuration.detect_local_timezone", return_value="UTC"):
                manager = ConfigManager(path)
                loaded = manager.load()

                self.assertEqual(
                    loaded.timezones,
                    [
                        TimezoneEntry(timezone="UTC", label=""),
                        TimezoneEntry(timezone="Asia/Tokyo", label=""),
                    ],
                )
                self.assertEqual(loaded.sort_mode, "manual")
                self.assertEqual(loaded.time_format, "system")

    def test_config_canonicalizes_timezone_aliases(self) -> None:
        if not timezone_link_aliases():
            self.skipTest("tzdata link aliases unavailable")

        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "config.json"
            path.write_text('{"version": 2, "timezones": []}\n', encoding="utf-8")
            manager = ConfigManager(path)

            manager.add_timezone("Asia/Calcutta", label="Calcutta")
            manager.add_timezone("US/Eastern", label="New York")

            loaded = manager.load()
            self.assertEqual(
                loaded.timezones,
                [
                    TimezoneEntry(timezone="Asia/Kolkata", label="Calcutta"),
                    TimezoneEntry(timezone="America/New_York", label="New York"),
                ],
            )

    def test_config_preserves_label_order_sort_mode_time_format_and_locked_entries(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "config.json"
            path.write_text('{"version": 2, "timezones": []}\n', encoding="utf-8")
            manager = ConfigManager(path)

            manager.add_timezone("America/Chicago", label="Austin")
            manager.add_timezone("Asia/Kolkata", label="New Delhi")
            manager.move_timezone("Asia/Kolkata", -1)
            manager.set_sort_mode("alpha")
            manager.set_time_format("ampm")
            manager.set_timezone_locked("America/Chicago", True)

            loaded = manager.load()

            self.assertEqual(loaded.sort_mode, "alpha")
            self.assertEqual(loaded.time_format, "ampm")
            self.assertEqual(
                loaded.timezones,
                [
                    TimezoneEntry(timezone="America/Chicago", label="Austin", locked=True),
                    TimezoneEntry(timezone="Asia/Kolkata", label="New Delhi"),
                ],
            )

    def test_config_keeps_locked_entries_first(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "config.json"
            path.write_text(
                json.dumps(
                    {
                        "version": 3,
                        "timezones": [
                            {"timezone": "Europe/Paris", "label": "Paris"},
                            {"timezone": "America/Cancun", "label": "Home", "locked": True},
                        ],
                        "sort_mode": "manual",
                        "time_format": "system",
                    }
                )
                + "\n",
                encoding="utf-8",
            )

            loaded = ConfigManager(path).load()

            self.assertEqual(
                loaded.timezones,
                [
                    TimezoneEntry(timezone="America/Cancun", label="Home", locked=True),
                    TimezoneEntry(timezone="Europe/Paris", label="Paris"),
                ],
            )

    def test_config_migrates_legacy_local_timezone_once(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "config.json"
            path.write_text(
                '{"timezones": [{"timezone": "Europe/Paris", "label": "Paris"}], "sort_mode": "manual"}\n',
                encoding="utf-8",
            )

            with patch(
                "omarchy_world_clock.configuration.detect_local_timezone",
                return_value="America/Cancun",
            ):
                manager = ConfigManager(path)
                loaded = manager.load()

                self.assertEqual(
                    loaded.timezones,
                    [
                        TimezoneEntry(timezone="America/Cancun", label=""),
                        TimezoneEntry(timezone="Europe/Paris", label="Paris"),
                    ],
                )

                manager.remove_timezone("America/Cancun")
                self.assertEqual(
                    manager.load().timezones,
                    [TimezoneEntry(timezone="Europe/Paris", label="Paris")],
                )

    def test_ordered_timezones_sorts_all_entries(self) -> None:
        reference = datetime(2026, 4, 17, 12, 0, tzinfo=timezone.utc)
        entries = [
            TimezoneEntry(timezone="UTC", label="Home"),
            TimezoneEntry(timezone="Asia/Tokyo", label="Tokyo"),
            TimezoneEntry(timezone="America/New_York", label="New York"),
        ]

        self.assertEqual(
            ordered_timezones(entries, "manual", reference),
            entries,
        )
        self.assertEqual(
            ordered_timezones(entries, "alpha", reference),
            [
                TimezoneEntry(timezone="UTC", label="Home"),
                TimezoneEntry(timezone="America/New_York", label="New York"),
                TimezoneEntry(timezone="Asia/Tokyo", label="Tokyo"),
            ],
        )
        self.assertEqual(
            ordered_timezones(entries, "time", reference),
            [
                TimezoneEntry(timezone="America/New_York", label="New York"),
                TimezoneEntry(timezone="UTC", label="Home"),
                TimezoneEntry(timezone="Asia/Tokyo", label="Tokyo"),
            ],
        )

    def test_ordered_timezones_can_keep_locked_entries_first(self) -> None:
        reference = datetime(2026, 4, 17, 12, 0, tzinfo=timezone.utc)
        entries = [
            TimezoneEntry(timezone="UTC", label="Home"),
            TimezoneEntry(timezone="Asia/Tokyo", label="Tokyo", locked=True),
            TimezoneEntry(timezone="America/New_York", label="New York"),
        ]

        self.assertEqual(
            ordered_timezones(entries, "alpha", reference),
            [
                TimezoneEntry(timezone="Asia/Tokyo", label="Tokyo", locked=True),
                TimezoneEntry(timezone="UTC", label="Home"),
                TimezoneEntry(timezone="America/New_York", label="New York"),
            ],
        )
        self.assertEqual(
            ordered_timezones(entries, "time", reference),
            [
                TimezoneEntry(timezone="Asia/Tokyo", label="Tokyo", locked=True),
                TimezoneEntry(timezone="America/New_York", label="New York"),
                TimezoneEntry(timezone="UTC", label="Home"),
            ],
        )

    def test_detect_system_time_format_from_waybar_clock(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "config.jsonc"
            path.write_text(
                '{\n  "clock": {\n    "format": "{:L%A %I:%M %p}"\n  }\n}\n',
                encoding="utf-8",
            )

            detect_system_time_format.cache_clear()
            detected = detect_system_time_format((path,))

        self.assertEqual(detected, "ampm")


if __name__ == "__main__":
    unittest.main()
