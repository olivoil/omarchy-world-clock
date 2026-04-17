#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
PREFIX=${OMARCHY_WORLD_CLOCK_RS_HOME:-"$HOME/.local/share/omarchy-world-clock-rs"}
BIN_DIR=${OMARCHY_WORLD_CLOCK_RS_BIN_DIR:-"$HOME/.local/bin"}
WRAPPER_PATH="$BIN_DIR/omarchy-world-clock-rs"

mkdir -p "$PREFIX" "$BIN_DIR"
cargo install --path "$REPO_ROOT/rust" --root "$PREFIX" --force

cat >"$WRAPPER_PATH" <<EOF
#!/usr/bin/env bash
set -euo pipefail
exec "$PREFIX/bin/omarchy-world-clock-rs" "\$@"
EOF
chmod +x "$WRAPPER_PATH"

cat <<EOF
Installed Omarchy World Clock Rust preview.
Wrapper: $WRAPPER_PATH

This install does not patch Waybar automatically.
Opt-in module example:

  "custom/world-clock-rs": {
    "exec": "$WRAPPER_PATH module",
    "return-type": "json",
    "interval": 2,
    "format": "{}",
    "tooltip": true,
    "on-click": "$WRAPPER_PATH toggle",
    "on-click-right": "omarchy-launch-floating-terminal-with-presentation omarchy-tz-select"
  }
EOF
