#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PREFIX=${OMARCHY_WORLD_CLOCK_HOME:-"$HOME/.local/share/omarchy-world-clock"}
BIN_DIR=${OMARCHY_WORLD_CLOCK_BIN_DIR:-"$HOME/.local/bin"}
WAYBAR_CONFIG=${WAYBAR_CONFIG:-"$HOME/.config/waybar/config.jsonc"}
WAYBAR_STYLE=${WAYBAR_STYLE:-"$HOME/.config/waybar/style.css"}
USER_CONFIG=${OMARCHY_WORLD_CLOCK_CONFIG:-"$HOME/.config/omarchy-world-clock/config.json"}
WRAPPER_PATH="$BIN_DIR/omarchy-world-clock"
LEGACY_PREFIX=${OMARCHY_WORLD_CLOCK_LEGACY_HOME:-"$HOME/.local/share/omarchy-world-clock-rs"}
LEGACY_WRAPPER_PATH=${OMARCHY_WORLD_CLOCK_LEGACY_WRAPPER:-"$BIN_DIR/omarchy-world-clock-rs"}

mkdir -p "$PREFIX" "$BIN_DIR"
cargo install --path "$REPO_ROOT" --root "$PREFIX" --force

cat >"$WRAPPER_PATH" <<EOF
#!/usr/bin/env bash
set -euo pipefail
exec "$PREFIX/bin/omarchy-world-clock" "\$@"
EOF
chmod +x "$WRAPPER_PATH"

"$WRAPPER_PATH" install-waybar \
  --waybar-config "$WAYBAR_CONFIG" \
  --waybar-style "$WAYBAR_STYLE" \
  --command-path "$WRAPPER_PATH" \
  --user-config "$USER_CONFIG"

rm -f "$LEGACY_WRAPPER_PATH"
rm -rf "$LEGACY_PREFIX"

"$WRAPPER_PATH" restart-waybar

printf 'Installed Omarchy World Clock.\nWaybar config: %s\nWaybar style: %s\nWrapper: %s\n' \
  "$WAYBAR_CONFIG" "$WAYBAR_STYLE" "$WRAPPER_PATH"
