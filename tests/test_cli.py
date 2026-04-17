from __future__ import annotations

import unittest

from omarchy_world_clock.cli import format_tooltip_clock_rows


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


if __name__ == "__main__":
    unittest.main()
