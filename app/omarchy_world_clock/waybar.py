from __future__ import annotations

import re
from pathlib import Path

MODULE_MARKER_START = "  // omarchy-world-clock:start"
MODULE_MARKER_END = "  // omarchy-world-clock:end"
STYLE_MARKER_START = "/* omarchy-world-clock:start */"
STYLE_MARKER_END = "/* omarchy-world-clock:end */"


def module_block(command_path: str) -> str:
    return "\n".join(
        [
            MODULE_MARKER_START,
            '  "custom/world-clock": {',
            f'    "exec": "{command_path} module",',
            '    "return-type": "json",',
            '    "interval": 2,',
            '    "format": "{}",',
            '    "tooltip": true,',
            f'    "on-click": "{command_path} toggle",',
            '    "on-click-right": "omarchy-launch-floating-terminal-with-presentation omarchy-tz-select"',
            "  },",
            MODULE_MARKER_END,
        ]
    )


def style_block() -> str:
    return "\n".join(
        [
            STYLE_MARKER_START,
            "#custom-world-clock {",
            "  min-width: 12px;",
            "  margin-left: 6px;",
            "  margin-right: 0;",
            "  font-size: 12px;",
            "  opacity: 0.72;",
            "}",
            "",
            "#custom-world-clock.active {",
            "  opacity: 1;",
            "}",
            STYLE_MARKER_END,
        ]
    )


def patch_modules_center(text: str, include_module: bool) -> str:
    pattern = re.compile(r'("modules-center"\s*:\s*\[)(.*?)(\])', re.DOTALL)
    match = pattern.search(text)
    if not match:
        raise ValueError("Could not find modules-center in Waybar config.")

    prefix, content, suffix = match.groups()
    tokens = re.findall(r'"[^"]+"', content)
    target = '"custom/world-clock"'

    if include_module and target not in tokens:
        if '"clock"' in tokens:
            index = tokens.index('"clock"') + 1
            tokens.insert(index, target)
        else:
            tokens.append(target)
    if not include_module:
        tokens = [token for token in tokens if token != target]

    multiline = "\n" in content
    if multiline:
        item_indent_match = re.search(r"\n([ \t]*)\"", content)
        item_indent = item_indent_match.group(1) if item_indent_match else "    "
        closing_indent_match = re.search(r"\n([ \t]*)\]$", match.group(0))
        closing_indent = closing_indent_match.group(1) if closing_indent_match else "  "
        rebuilt_content = "\n" + "\n".join(
            f"{item_indent}{token}{',' if index < len(tokens) - 1 else ''}"
            for index, token in enumerate(tokens)
        )
        rebuilt_content += f"\n{closing_indent}"
    else:
        rebuilt_content = ", ".join(tokens)

    return text[: match.start()] + prefix + rebuilt_content + suffix + text[match.end() :]


def patch_config_text(text: str, command_path: str) -> str:
    text = patch_modules_center(text, include_module=True)
    block = module_block(command_path)
    marker_pattern = re.compile(
        rf"{re.escape(MODULE_MARKER_START)}.*?{re.escape(MODULE_MARKER_END)}\n?",
        re.DOTALL,
    )
    if marker_pattern.search(text):
        return marker_pattern.sub(block + "\n", text)

    return re.sub(r"\n}\s*$", ",\n" + block + "\n}\n", text, count=1)


def unpatch_config_text(text: str) -> str:
    text = patch_modules_center(text, include_module=False)
    marker_pattern = re.compile(
        rf"\n?{re.escape(MODULE_MARKER_START)}.*?{re.escape(MODULE_MARKER_END)}\n?",
        re.DOTALL,
    )
    text = marker_pattern.sub("\n", text)
    text = re.sub(r",\s*\n\s*\n}", "\n}", text)
    text = re.sub(r",\s*\n}", "\n}", text)
    return text


def patch_style_text(text: str) -> str:
    block = style_block()
    marker_pattern = re.compile(
        rf"{re.escape(STYLE_MARKER_START)}.*?{re.escape(STYLE_MARKER_END)}\n?",
        re.DOTALL,
    )
    if marker_pattern.search(text):
        return marker_pattern.sub(block + "\n", text)

    if text and not text.endswith("\n"):
        text += "\n"
    return text + "\n" + block + "\n"


def unpatch_style_text(text: str) -> str:
    marker_pattern = re.compile(
        rf"\n?{re.escape(STYLE_MARKER_START)}.*?{re.escape(STYLE_MARKER_END)}\n?",
        re.DOTALL,
    )
    return marker_pattern.sub("\n", text).rstrip() + "\n"


def write_text(path: Path, contents: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(contents, encoding="utf-8")
