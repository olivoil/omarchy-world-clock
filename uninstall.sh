#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PREFIX=${OMARCHY_WORLD_CLOCK_HOME:-"$HOME/.local/share/omarchy-world-clock"}
BIN_DIR=${OMARCHY_WORLD_CLOCK_BIN_DIR:-"$HOME/.local/bin"}
WAYBAR_CONFIG=${WAYBAR_CONFIG:-"$HOME/.config/waybar/config.jsonc"}
WAYBAR_STYLE=${WAYBAR_STYLE:-"$HOME/.config/waybar/style.css"}
CONFIG_DIR=${OMARCHY_WORLD_CLOCK_CONFIG_DIR:-"$HOME/.config/omarchy-world-clock"}
WRAPPER_PATH="$BIN_DIR/omarchy-world-clock"
INSTALLED_BINARY="$PREFIX/bin/omarchy-world-clock"

if [[ -x "$WRAPPER_PATH" ]]; then
  "$WRAPPER_PATH" uninstall-waybar \
    --waybar-config "$WAYBAR_CONFIG" \
    --waybar-style "$WAYBAR_STYLE"
elif [[ -x "$INSTALLED_BINARY" ]]; then
  "$INSTALLED_BINARY" uninstall-waybar \
    --waybar-config "$WAYBAR_CONFIG" \
    --waybar-style "$WAYBAR_STYLE"
else
  cargo run --manifest-path "$REPO_ROOT/rust/Cargo.toml" -- uninstall-waybar \
    --waybar-config "$WAYBAR_CONFIG" \
    --waybar-style "$WAYBAR_STYLE"
fi

rm -f "$WRAPPER_PATH"
rm -rf "$PREFIX"

if [[ "${1:-}" == "--purge" ]]; then
  rm -rf "$CONFIG_DIR"
fi

if command -v omarchy-restart-waybar >/dev/null 2>&1; then
  omarchy-restart-waybar || true
else
  pkill -SIGUSR2 waybar || true
fi

printf 'Uninstalled Omarchy World Clock.\n'
