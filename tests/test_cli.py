from __future__ import annotations

import unittest
from pathlib import Path
from unittest.mock import patch

from omarchy_world_clock.cli import format_tooltip_clock_rows, module_payload
from omarchy_world_clock.configuration import AppConfig, TimezoneEntry


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

    def test_module_payload_shows_empty_state_when_no_timezones_are_configured(self) -> None:
        with (
            patch(
                "omarchy_world_clock.cli.ConfigManager.load",
                return_value=AppConfig(timezones=[], sort_mode="manual"),
            ),
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
        )
        fixed_now = type(
            "FixedDateTime",
            (),
            {"now": staticmethod(lambda _tz=None: __import__("datetime").datetime(2026, 4, 17, 12, 0, tzinfo=__import__("datetime").timezone.utc))},
        )

        with (
            patch("omarchy_world_clock.cli.ConfigManager.load", return_value=config),
            patch("omarchy_world_clock.cli.detect_local_timezone", return_value="UTC"),
            patch("omarchy_world_clock.cli.popup_running", return_value=True),
            patch("omarchy_world_clock.cli.datetime", fixed_now),
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
        )
        fixed_now = type(
            "FixedDateTime",
            (),
            {"now": staticmethod(lambda _tz=None: __import__("datetime").datetime(2026, 4, 17, 12, 0, tzinfo=__import__("datetime").timezone.utc))},
        )

        with (
            patch("omarchy_world_clock.cli.ConfigManager.load", return_value=config),
            patch("omarchy_world_clock.cli.detect_local_timezone", return_value="UTC"),
            patch("omarchy_world_clock.cli.popup_running", return_value=False),
            patch("omarchy_world_clock.cli.datetime", fixed_now),
        ):
            payload = module_payload(Path("/tmp/omarchy-world-clock.pid"))

        tooltip = payload["tooltip"]
        self.assertLess(tooltip.index("Tokyo"), tooltip.index("Home  ·  Local"))
        self.assertLess(tooltip.index("Tokyo"), tooltip.index("New York"))


if __name__ == "__main__":
    unittest.main()
