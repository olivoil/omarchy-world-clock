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
LEGACY_PREFIX=${OMARCHY_WORLD_CLOCK_LEGACY_HOME:-"$HOME/.local/share/omarchy-world-clock-rs"}
LEGACY_WRAPPER_PATH=${OMARCHY_WORLD_CLOCK_LEGACY_WRAPPER:-"$BIN_DIR/omarchy-world-clock-rs"}

PURGE=false
for arg in "$@"; do
  case "$arg" in
    --purge)
      PURGE=true
      ;;
    *)
      printf 'Unknown argument: %s\nUsage: %s [--purge]\n' "$arg" "${BASH_SOURCE[0]}" >&2
      exit 2
      ;;
  esac
done

try_uninstall_waybar() {
  local label=$1
  shift
  local -a command=("$@")

  if "${command[@]}" uninstall-waybar \
    --waybar-config "$WAYBAR_CONFIG" \
    --waybar-style "$WAYBAR_STYLE"; then
    return 0
  fi

  printf 'Failed to uninstall Waybar via %s; trying next fallback.\n' "$label" >&2
  return 1
}

if [[ -x "$WRAPPER_PATH" ]] && try_uninstall_waybar wrapper "$WRAPPER_PATH"; then
  :
elif [[ -x "$INSTALLED_BINARY" ]] && try_uninstall_waybar installed-binary "$INSTALLED_BINARY"; then
  :
elif command -v cargo >/dev/null 2>&1 \
  && try_uninstall_waybar cargo cargo run --manifest-path "$REPO_ROOT/Cargo.toml" --; then
  :
else
  printf 'Unable to uninstall Waybar: wrapper, installed binary, and cargo fallback all failed.\n' >&2
  exit 1
fi

rm -f "$WRAPPER_PATH"
rm -rf "$PREFIX"
rm -f "$LEGACY_WRAPPER_PATH"
rm -rf "$LEGACY_PREFIX"

if [[ "$PURGE" == true ]]; then
  rm -rf "$CONFIG_DIR"
fi

if command -v omarchy-restart-waybar >/dev/null 2>&1; then
  omarchy-restart-waybar || true
else
  pkill -SIGUSR2 waybar || true
fi

printf 'Uninstalled Omarchy World Clock.\n'
