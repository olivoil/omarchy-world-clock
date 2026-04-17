#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PREFIX=${OMARCHY_WORLD_CLOCK_HOME:-"$HOME/.local/share/omarchy-world-clock"}
BIN_DIR=${OMARCHY_WORLD_CLOCK_BIN_DIR:-"$HOME/.local/bin"}
WAYBAR_CONFIG=${WAYBAR_CONFIG:-"$HOME/.config/waybar/config.jsonc"}
WAYBAR_STYLE=${WAYBAR_STYLE:-"$HOME/.config/waybar/style.css"}
CONFIG_DIR=${OMARCHY_WORLD_CLOCK_CONFIG_DIR:-"$HOME/.config/omarchy-world-clock"}
WRAPPER_PATH="$BIN_DIR/omarchy-world-clock"

PYTHONPATH="$REPO_ROOT/app${PYTHONPATH:+:$PYTHONPATH}" \
  python3 -m omarchy_world_clock.cli uninstall-waybar \
  --waybar-config "$WAYBAR_CONFIG" \
  --waybar-style "$WAYBAR_STYLE"

rm -f "$WRAPPER_PATH"
rm -rf "$PREFIX"

if [[ "${1:-}" == "--purge" ]]; then
  rm -rf "$CONFIG_DIR"
fi

PYTHONPATH="$REPO_ROOT/app${PYTHONPATH:+:$PYTHONPATH}" \
  python3 -m omarchy_world_clock.cli restart-waybar

printf 'Uninstalled Omarchy World Clock.\n'
