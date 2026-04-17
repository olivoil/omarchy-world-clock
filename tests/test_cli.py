from __future__ import annotations

import unittest
from datetime import datetime, timezone
from pathlib import Path
from unittest.mock import patch

from omarchy_world_clock.cli import format_tooltip_clock_rows, module_payload
from omarchy_world_clock.configuration import AppConfig, TimezoneEntry


class FixedDateTime:
    @staticmethod
    def now(_tz=None) -> datetime:
        return datetime(2026, 4, 17, 12, 0, tzinfo=timezone.utc)


class FixedEveningDateTime:
    @staticmethod
    def now(_tz=None) -> datetime:
        return datetime(2026, 4, 16, 20, 26, tzinfo=timezone.utc)


class CliTests(unittest.TestCase):
    def test_format_tooltip_clock_rows_aligns_to_widest_label(self) -> None:
        rows = [
            ("Local  Cancun", "22:03"),
            ("Vancouver", "20:03"),
            ("Paris", "05:03"),
            ("Los Angeles", "20:03"),
        ]

        formatted = format_tooltip_clock_rows(rows)

        self.assertEqual(
            formatted,
            [
                "Local  Cancun  22:03",
                "Vancouver      20:03",
                "Paris          05:03",
                "Los Angeles    20:03",
            ],
        )
        self.assertEqual({len(line) for line in formatted}, {len(formatted[0])})

    def test_module_payload_uses_world_icon(self) -> None:
        with (
            patch(
                "omarchy_world_clock.cli.ConfigManager.load",
                return_value=AppConfig(timezones=[], sort_mode="manual", time_format="24h"),
            ),
            patch("omarchy_world_clock.cli.detect_local_timezone", return_value="America/Cancun"),
            patch("omarchy_world_clock.cli.popup_running", return_value=False),
        ):
            payload = module_payload(Path("/tmp/omarchy-world-clock.pid"))

        self.assertEqual(payload["text"], "")
        self.assertEqual(payload["class"], "inactive")
        self.assertIn("World Clock", payload["tooltip"])

    def test_module_payload_shows_empty_state_when_no_timezones_are_configured(self) -> None:
        with (
            patch(
                "omarchy_world_clock.cli.ConfigManager.load",
                return_value=AppConfig(timezones=[], sort_mode="manual", time_format="system"),
            ),
            patch("omarchy_world_clock.cli.detect_local_timezone", return_value="UTC"),
            patch("omarchy_world_clock.cli.popup_running", return_value=False),
        ):
            payload = module_payload(Path("/tmp/omarchy-world-clock.pid"))

        self.assertEqual(payload["class"], "inactive")
        self.assertEqual(payload["tooltip"], "World Clock\n\nNo timezones yet.")

    def test_module_payload_marks_local_timezone_from_config(self) -> None:
        config = AppConfig(
            timezones=[
                TimezoneEntry(timezone="UTC", label="Home"),
                TimezoneEntry(timezone="Asia/Tokyo", label="Tokyo"),
            ],
            sort_mode="manual",
            time_format="24h",
        )

        with (
            patch("omarchy_world_clock.cli.ConfigManager.load", return_value=config),
            patch("omarchy_world_clock.cli.detect_local_timezone", return_value="UTC"),
            patch("omarchy_world_clock.cli.popup_running", return_value=True),
            patch("omarchy_world_clock.cli.datetime", FixedDateTime),
        ):
            payload = module_payload(Path("/tmp/omarchy-world-clock.pid"))

        self.assertEqual(payload["class"], "active")
        self.assertIn("Home  ·  Local", payload["tooltip"])
        self.assertIn("Tokyo", payload["tooltip"])

    def test_module_payload_keeps_locked_timezone_first_when_sorted(self) -> None:
        config = AppConfig(
            timezones=[
                TimezoneEntry(timezone="Asia/Tokyo", label="Tokyo", locked=True),
                TimezoneEntry(timezone="UTC", label="Home"),
                TimezoneEntry(timezone="America/New_York", label="New York"),
            ],
            sort_mode="time",
            time_format="24h",
        )

        with (
            patch("omarchy_world_clock.cli.ConfigManager.load", return_value=config),
            patch("omarchy_world_clock.cli.detect_local_timezone", return_value="UTC"),
            patch("omarchy_world_clock.cli.popup_running", return_value=False),
            patch("omarchy_world_clock.cli.datetime", FixedDateTime),
        ):
            payload = module_payload(Path("/tmp/omarchy-world-clock.pid"))

        tooltip = payload["tooltip"]
        self.assertLess(tooltip.index("Tokyo"), tooltip.index("Home  ·  Local"))
        self.assertLess(tooltip.index("Tokyo"), tooltip.index("New York"))

    def test_module_payload_uses_ampm_when_configured(self) -> None:
        config = AppConfig(
            timezones=[TimezoneEntry(timezone="UTC", label="Home")],
            sort_mode="manual",
            time_format="ampm",
        )

        with (
            patch("omarchy_world_clock.cli.ConfigManager.load", return_value=config),
            patch("omarchy_world_clock.cli.detect_local_timezone", return_value="UTC"),
            patch("omarchy_world_clock.cli.popup_running", return_value=False),
            patch("omarchy_world_clock.cli.datetime", FixedEveningDateTime),
        ):
            payload = module_payload(Path("/tmp/omarchy-world-clock.pid"))

        self.assertIn("8:26 PM", payload["tooltip"])


if __name__ == "__main__":
    unittest.main()
