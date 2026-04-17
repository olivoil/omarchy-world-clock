from __future__ import annotations

import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from omarchy_world_clock.cli import format_tooltip_clock_rows, module_payload


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
        fake_config = SimpleNamespace(timezones=[], sort_mode="manual")
        with (
            patch("omarchy_world_clock.cli.ConfigManager") as config_manager,
            patch("omarchy_world_clock.cli.detect_local_timezone", return_value="America/Cancun"),
            patch("omarchy_world_clock.cli.friendly_timezone_name", return_value="Cancun"),
            patch("omarchy_world_clock.cli.zoned_datetime") as zoned_datetime,
            patch("omarchy_world_clock.cli.popup_running", return_value=False),
        ):
            config_manager.return_value.load.return_value = fake_config
            zoned_datetime.return_value.strftime.return_value = "08:26"

            payload = module_payload(Path("/tmp/omarchy-world-clock.pid"))

        self.assertEqual(payload["text"], "")
        self.assertEqual(payload["class"], "inactive")
        self.assertIn("World Clock", payload["tooltip"])


if __name__ == "__main__":
    unittest.main()
