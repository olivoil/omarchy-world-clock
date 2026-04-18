#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
ORIGINAL_PATH=$PATH

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  exit 1
}

assert_contains() {
  local file=$1
  local needle=$2
  if ! grep -F "$needle" "$file" >/dev/null 2>&1; then
    fail "expected '$needle' in $file"
  fi
}

assert_not_contains() {
  local file=$1
  local needle=$2
  if grep -F "$needle" "$file" >/dev/null 2>&1; then
    fail "did not expect '$needle' in $file"
  fi
}

assert_missing() {
  local path=$1
  if [[ -e "$path" ]]; then
    fail "expected $path to be removed"
  fi
}

write_stub() {
  local path=$1
  local name=$2
  local exit_code=$3

  cat >"$path" <<EOF
#!/usr/bin/env bash
set -euo pipefail
printf '%s %s\n' "$name" "\$*" >> "\$TEST_LOG"
exit $exit_code
EOF
  chmod +x "$path"
}

make_sandbox() {
  local sandbox
  sandbox=$(mktemp -d)

  mkdir -p \
    "$sandbox/home/.config/waybar" \
    "$sandbox/home/.config/omarchy-world-clock" \
    "$sandbox/bin" \
    "$sandbox/prefix/bin" \
    "$sandbox/stubs"

  cat >"$sandbox/home/.config/waybar/config.jsonc" <<'EOF'
{
  "modules-center": ["clock"]
}
EOF

  cat >"$sandbox/home/.config/waybar/style.css" <<'EOF'
#clock {
  color: white;
}
EOF

  write_stub "$sandbox/stubs/omarchy-restart-waybar" restart-waybar 0
  printf '%s\n' "$sandbox"
}

run_uninstall() {
  local sandbox=$1
  shift

  TEST_LOG="$sandbox/log"
  export TEST_LOG

  PATH="$sandbox/stubs:$ORIGINAL_PATH" \
    HOME="$sandbox/home" \
    OMARCHY_WORLD_CLOCK_HOME="$sandbox/prefix" \
    OMARCHY_WORLD_CLOCK_BIN_DIR="$sandbox/bin" \
    OMARCHY_WORLD_CLOCK_CONFIG_DIR="$sandbox/home/.config/omarchy-world-clock" \
    "$REPO_ROOT/uninstall.sh" "$@"
}

test_prefers_wrapper() {
  local sandbox
  sandbox=$(make_sandbox)
  trap 'rm -rf "$sandbox"' RETURN

  write_stub "$sandbox/bin/omarchy-world-clock" wrapper 0
  write_stub "$sandbox/prefix/bin/omarchy-world-clock" installed-binary 0
  write_stub "$sandbox/stubs/cargo" cargo 0

  run_uninstall "$sandbox"

  assert_contains "$sandbox/log" "wrapper uninstall-waybar"
  assert_not_contains "$sandbox/log" "installed-binary uninstall-waybar"
  assert_not_contains "$sandbox/log" "cargo run"
  assert_missing "$sandbox/bin/omarchy-world-clock"
  assert_missing "$sandbox/prefix"
}

test_falls_back_to_installed_binary() {
  local sandbox
  sandbox=$(make_sandbox)
  trap 'rm -rf "$sandbox"' RETURN

  write_stub "$sandbox/bin/omarchy-world-clock" wrapper 1
  write_stub "$sandbox/prefix/bin/omarchy-world-clock" installed-binary 0
  write_stub "$sandbox/stubs/cargo" cargo 0

  run_uninstall "$sandbox"

  assert_contains "$sandbox/log" "wrapper uninstall-waybar"
  assert_contains "$sandbox/log" "installed-binary uninstall-waybar"
  assert_not_contains "$sandbox/log" "cargo run"
  assert_missing "$sandbox/bin/omarchy-world-clock"
  assert_missing "$sandbox/prefix"
}

test_falls_back_to_cargo_and_purges_config() {
  local sandbox
  sandbox=$(make_sandbox)
  trap 'rm -rf "$sandbox"' RETURN

  write_stub "$sandbox/stubs/cargo" cargo 0

  run_uninstall "$sandbox" --purge

  assert_contains "$sandbox/log" "cargo run --manifest-path $REPO_ROOT/Cargo.toml -- uninstall-waybar"
  assert_contains "$sandbox/log" "restart-waybar "
  assert_missing "$sandbox/prefix"
  assert_missing "$sandbox/home/.config/omarchy-world-clock"
}

test_prefers_wrapper
test_falls_back_to_installed_binary
test_falls_back_to_cargo_and_purges_config

printf 'uninstall.sh tests passed\n'
