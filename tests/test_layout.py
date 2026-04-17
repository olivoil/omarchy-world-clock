from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from omarchy_world_clock.layout import (
    load_waybar_height,
    load_window_gap,
    parse_hypr_int,
    parse_jsonc_int,
    popup_top_margin,
)


class LayoutTests(unittest.TestCase):
    def test_parse_jsonc_int_ignores_line_comments(self) -> None:
        config = """
{
  // "height": 99,
  "height": 26,
  "spacing": 0
}
"""
        self.assertEqual(parse_jsonc_int(config, "height"), 26)

    def test_parse_hypr_int_ignores_comments(self) -> None:
        config = """
general {
    # gaps_out = 99
    gaps_out = 10
}
"""
        self.assertEqual(parse_hypr_int(config, "gaps_out"), 10)

    def test_load_window_gap_falls_back_to_omarchy_default_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            base = Path(tmpdir)
            user_config = base / "looknfeel.conf"
            default_config = base / "default-looknfeel.conf"

            user_config.write_text(
                "general {\n    # gaps_out = 0\n}\n",
                encoding="utf-8",
            )
            default_config.write_text(
                "general {\n    gaps_out = 10\n}\n",
                encoding="utf-8",
            )

            self.assertEqual(load_window_gap([user_config, default_config]), 10)

    def test_load_waybar_height_uses_configured_height(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = Path(tmpdir) / "config.jsonc"
            config_path.write_text('{\n  "height": 30\n}\n', encoding="utf-8")

            self.assertEqual(load_waybar_height(config_path), 30)

    def test_popup_top_margin_matches_waybar_height_plus_gap(self) -> None:
        self.assertEqual(popup_top_margin(26, 10), 28)


if __name__ == "__main__":
    unittest.main()
