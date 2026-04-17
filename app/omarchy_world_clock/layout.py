from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path

DEFAULT_WAYBAR_HEIGHT = 26
DEFAULT_WINDOW_GAP = 10
DEFAULT_BORDER_SIZE = 2
POPUP_TOP_CONTENT_MARGIN = 8


def parse_jsonc_int(text: str, field_name: str) -> int | None:
    pattern = re.compile(rf'"{re.escape(field_name)}"\s*:\s*(\d+)\b')
    for raw_line in text.splitlines():
        line = re.sub(r"/\*.*?\*/", "", raw_line)
        line = line.split("//", 1)[0].strip()
        if not line:
            continue
        match = pattern.search(line)
        if match is not None:
            return int(match.group(1))
    return None


def parse_hypr_int(text: str, key: str) -> int | None:
    pattern = re.compile(rf"^{re.escape(key)}\s*=\s*(\d+)\b")
    for raw_line in text.splitlines():
        line = raw_line.split("#", 1)[0].strip()
        if not line:
            continue
        match = pattern.search(line)
        if match is not None:
            return int(match.group(1))
    return None


def parse_hyprctl_custom_int(raw_value: str) -> int | None:
    match = re.search(r"\d+", raw_value)
    if match is None:
        return None
    return int(match.group(0))


def load_hyprctl_option_int(option_name: str) -> int | None:
    try:
        result = subprocess.run(
            ["hyprctl", "-j", "getoption", option_name],
            check=True,
            capture_output=True,
            text=True,
        )
        payload = json.loads(result.stdout)
    except Exception:
        return None

    value = payload.get("int")
    if isinstance(value, int):
        return value

    custom = payload.get("custom")
    if isinstance(custom, str):
        return parse_hyprctl_custom_int(custom)
    return None


def load_waybar_height(path: Path | None = None) -> int:
    config_path = path or Path.home() / ".config" / "waybar" / "config.jsonc"
    try:
        value = parse_jsonc_int(config_path.read_text(encoding="utf-8"), "height")
    except OSError:
        value = None
    return value if value is not None else DEFAULT_WAYBAR_HEIGHT


def load_window_gap(paths: list[Path] | None = None) -> int:
    live_value = load_hyprctl_option_int("general:gaps_out")
    if live_value is not None:
        return live_value

    candidate_paths = paths or [
        Path.home() / ".config" / "hypr" / "looknfeel.conf",
        Path.home() / ".local" / "share" / "omarchy" / "default" / "hypr" / "looknfeel.conf",
    ]
    for path in candidate_paths:
        try:
            value = parse_hypr_int(path.read_text(encoding="utf-8"), "gaps_out")
        except OSError:
            continue
        if value is not None:
            return value
    return DEFAULT_WINDOW_GAP


def load_window_border_size(paths: list[Path] | None = None) -> int:
    live_value = load_hyprctl_option_int("general:border_size")
    if live_value is not None:
        return live_value

    candidate_paths = paths or [
        Path.home() / ".config" / "hypr" / "looknfeel.conf",
        Path.home() / ".local" / "share" / "omarchy" / "default" / "hypr" / "looknfeel.conf",
    ]
    for path in candidate_paths:
        try:
            value = parse_hypr_int(path.read_text(encoding="utf-8"), "border_size")
        except OSError:
            continue
        if value is not None:
            return value
    return DEFAULT_BORDER_SIZE


def popup_top_margin(
    window_gap: int,
    border_size: int,
    content_margin_top: int = POPUP_TOP_CONTENT_MARGIN,
) -> int:
    return max(0, window_gap + border_size - content_margin_top)
