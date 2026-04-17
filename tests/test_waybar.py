from __future__ import annotations

import unittest

from omarchy_world_clock.waybar import (
    patch_config_text,
    patch_style_text,
    unpatch_config_text,
    unpatch_style_text,
)


WAYBAR_CONFIG = """{
  "modules-center": ["clock", "custom/update"],
  "clock": {
    "format": "{:L%A %H:%M}"
  },
  "tray": {
    "icon-size": 12
  }
}
"""

WAYBAR_STYLE = """#clock {
  margin-left: 5px;
}
"""


class WaybarPatchTests(unittest.TestCase):
    def test_patch_config_inserts_module_once(self) -> None:
        patched = patch_config_text(WAYBAR_CONFIG, "~/.local/bin/omarchy-world-clock")
        self.assertIn('"modules-center": ["clock", "custom/world-clock", "custom/update"]', patched)
        self.assertIn('"custom/world-clock": {', patched)

        patched_twice = patch_config_text(patched, "~/.local/bin/omarchy-world-clock")
        self.assertEqual(patched, patched_twice)

    def test_unpatch_config_removes_module(self) -> None:
        patched = patch_config_text(WAYBAR_CONFIG, "~/.local/bin/omarchy-world-clock")
        unpatched = unpatch_config_text(patched)
        self.assertNotIn('"custom/world-clock"', unpatched)
        self.assertIn('"modules-center": ["clock", "custom/update"]', unpatched)

    def test_patch_style_is_idempotent(self) -> None:
        patched = patch_style_text(WAYBAR_STYLE)
        self.assertIn("#custom-world-clock", patched)
        self.assertEqual(patched, patch_style_text(patched))

        unpatched = unpatch_style_text(patched)
        self.assertNotIn("#custom-world-clock", unpatched)


if __name__ == "__main__":
    unittest.main()
