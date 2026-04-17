from __future__ import annotations

import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

from .configuration import ConfigManager, detect_local_timezone
from .core import friendly_timezone_name, zoned_datetime
from .waybar import (
    MODULE_MARKER_START,
    STYLE_MARKER_START,
    patch_config_text,
    patch_style_text,
    unpatch_config_text,
    unpatch_style_text,
    write_text,
)


def runtime_pid_path() -> Path:
    runtime_dir = os.environ.get("XDG_RUNTIME_DIR") or f"/tmp/omarchy-world-clock-{os.getuid()}"
    return Path(runtime_dir) / "omarchy-world-clock.pid"


def read_pid(pid_path: Path) -> int | None:
    try:
        return int(pid_path.read_text(encoding="utf-8").strip())
    except Exception:
        return None


def is_process_alive(pid: int | None) -> bool:
    if pid is None:
        return False
    try:
        os.kill(pid, 0)
    except OSError:
        return False
    return True


def popup_running(pid_path: Path) -> bool:
    pid = read_pid(pid_path)
    alive = is_process_alive(pid)
    if not alive and pid_path.exists():
        pid_path.unlink(missing_ok=True)
    return alive


def kill_popup(pid_path: Path) -> bool:
    pid = read_pid(pid_path)
    if not is_process_alive(pid):
        pid_path.unlink(missing_ok=True)
        return False

    os.kill(pid, signal.SIGTERM)
    for _ in range(20):
        if not is_process_alive(pid):
            pid_path.unlink(missing_ok=True)
            return True
        time.sleep(0.05)
    return True


def spawn_popup(pid_path: Path) -> None:
    env = os.environ.copy()
    subprocess.Popen(
        [sys.executable, "-m", "omarchy_world_clock.cli", "popup"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        stdin=subprocess.DEVNULL,
        start_new_session=True,
        env=env,
    )


def format_tooltip_clock_rows(rows: list[tuple[str, str]]) -> list[str]:
    if not rows:
        return []

    widest_label = max(len(label) for label, _time_value in rows)
    return [f"{label:<{widest_label}}  {time_value}" for label, time_value in rows]


def module_payload(pid_path: Path) -> dict[str, object]:
    config = ConfigManager().load()
    local_timezone = detect_local_timezone()
    now = datetime.now(timezone.utc)
    clock_rows = [
        (
            f"Local  {friendly_timezone_name(local_timezone)}",
            zoned_datetime(now, local_timezone).strftime("%H:%M"),
        )
    ]
    entries = [entry for entry in config.timezones if entry.timezone != local_timezone]
    if config.sort_mode == "alpha":
        entries.sort(key=lambda entry: (entry.display_label().casefold(), entry.timezone.casefold()))
    elif config.sort_mode == "time":
        entries.sort(
            key=lambda entry: (
                zoned_datetime(now, entry.timezone).replace(tzinfo=None),
                entry.display_label().casefold(),
            )
        )

    for entry in entries:
        zoned = zoned_datetime(now, entry.timezone)
        clock_rows.append((entry.display_label(), zoned.strftime("%H:%M")))

    tooltip_lines = ["World Clock", "", *format_tooltip_clock_rows(clock_rows)]
    if len(clock_rows) == 1:
        tooltip_lines.append("No extra timezones yet.")

    return {
        "text": "󰥔",
        "class": "active" if popup_running(pid_path) else "inactive",
        "tooltip": "\n".join(tooltip_lines),
    }


def backup_if_needed(path: Path, marker: str) -> None:
    if not path.exists():
        return
    contents = path.read_text(encoding="utf-8")
    if marker in contents:
        return
    backup_path = path.with_name(f"{path.name}.bak.{int(time.time())}")
    shutil.copy2(path, backup_path)


def install_waybar(args: argparse.Namespace) -> int:
    waybar_config = Path(args.waybar_config).expanduser()
    waybar_style = Path(args.waybar_style).expanduser()

    backup_if_needed(waybar_config, MODULE_MARKER_START)
    backup_if_needed(waybar_style, STYLE_MARKER_START)

    config_text = waybar_config.read_text(encoding="utf-8")
    style_text = waybar_style.read_text(encoding="utf-8")

    write_text(waybar_config, patch_config_text(config_text, args.command_path))
    write_text(waybar_style, patch_style_text(style_text))

    ConfigManager(Path(args.user_config).expanduser()).load()
    return 0


def uninstall_waybar(args: argparse.Namespace) -> int:
    waybar_config = Path(args.waybar_config).expanduser()
    waybar_style = Path(args.waybar_style).expanduser()

    if waybar_config.exists():
        write_text(
            waybar_config,
            unpatch_config_text(waybar_config.read_text(encoding="utf-8")),
        )
    if waybar_style.exists():
        write_text(
            waybar_style,
            unpatch_style_text(waybar_style.read_text(encoding="utf-8")),
        )
    return 0


def restart_waybar() -> None:
    command = shutil.which("omarchy-restart-waybar")
    if command:
        subprocess.run([command], check=False)
        return
    subprocess.run(["pkill", "-SIGUSR2", "waybar"], check=False)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="omarchy-world-clock")
    subparsers = parser.add_subparsers(dest="command", required=True)

    subparsers.add_parser("module")
    subparsers.add_parser("toggle")
    subparsers.add_parser("popup")

    install_parser = subparsers.add_parser("install-waybar")
    install_parser.add_argument("--waybar-config", required=True)
    install_parser.add_argument("--waybar-style", required=True)
    install_parser.add_argument("--command-path", required=True)
    install_parser.add_argument("--user-config", required=True)

    uninstall_parser = subparsers.add_parser("uninstall-waybar")
    uninstall_parser.add_argument("--waybar-config", required=True)
    uninstall_parser.add_argument("--waybar-style", required=True)

    subparsers.add_parser("restart-waybar")
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    pid_path = runtime_pid_path()

    if args.command == "module":
        print(json.dumps(module_payload(pid_path)))
        return 0
    if args.command == "toggle":
        if popup_running(pid_path):
            kill_popup(pid_path)
        else:
            spawn_popup(pid_path)
        return 0
    if args.command == "popup":
        if popup_running(pid_path):
            return 0
        from .popup import run_popup

        run_popup(pid_path)
        return 0
    if args.command == "install-waybar":
        return install_waybar(args)
    if args.command == "uninstall-waybar":
        return uninstall_waybar(args)
    if args.command == "restart-waybar":
        restart_waybar()
        return 0
    parser.error(f"Unknown command: {args.command}")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
