from __future__ import annotations

import tempfile
import unittest
from datetime import datetime, timezone
from pathlib import Path

from omarchy_world_clock.configuration import ConfigManager, TimezoneEntry, TimezoneResolver
from omarchy_world_clock.core import format_offset, parse_manual_reference, zoned_datetime


class CoreTests(unittest.TestCase):
    def test_parse_time_only_uses_current_date_in_zone(self) -> None:
        reference = datetime(2026, 4, 16, 18, 0, tzinfo=timezone.utc)
        parsed = parse_manual_reference("09:30", "America/New_York", reference)
        zoned = zoned_datetime(parsed, "America/New_York")
        self.assertEqual(zoned.strftime("%Y-%m-%d %H:%M"), "2026-04-16 09:30")

    def test_parse_full_datetime(self) -> None:
        reference = datetime(2026, 4, 16, 18, 0, tzinfo=timezone.utc)
        parsed = parse_manual_reference("2026-04-18 21:15", "Asia/Tokyo", reference)
        zoned = zoned_datetime(parsed, "Asia/Tokyo")
        self.assertEqual(zoned.strftime("%Y-%m-%d %H:%M"), "2026-04-18 21:15")

    def test_format_offset(self) -> None:
        reference = datetime(2026, 1, 8, 12, 0, tzinfo=timezone.utc)
        zoned = zoned_datetime(reference, "Asia/Kolkata")
        self.assertEqual(format_offset(zoned.utcoffset()), "UTC+05:30")

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

    def test_config_round_trip(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "config.json"
            manager = ConfigManager(path)
            config = manager.load()
            self.assertEqual(config.timezones, [])
            self.assertEqual(config.sort_mode, "manual")

            manager.add_timezone("UTC", label="UTC")
            loaded = manager.load()
            self.assertEqual(loaded.timezones, [TimezoneEntry(timezone="UTC", label="UTC")])

            manager.remove_timezone("UTC")
            self.assertEqual(manager.load().timezones, [])

    def test_config_loads_legacy_timezone_list(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "config.json"
            path.write_text('{"timezones": ["UTC", "Asia/Tokyo"]}\n', encoding="utf-8")

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

    def test_config_preserves_label_order_and_sort_mode(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "config.json"
            manager = ConfigManager(path)

            manager.add_timezone("America/Chicago", label="Austin")
            manager.add_timezone("Asia/Kolkata", label="New Delhi")
            manager.move_timezone("Asia/Kolkata", -1)
            manager.set_sort_mode("alpha")

            loaded = manager.load()
            self.assertEqual(loaded.sort_mode, "alpha")
            self.assertEqual(
                loaded.timezones,
                [
                    TimezoneEntry(timezone="Asia/Kolkata", label="New Delhi"),
                    TimezoneEntry(timezone="America/Chicago", label="Austin"),
                ],
            )


if __name__ == "__main__":
    unittest.main()
