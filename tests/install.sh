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

assert_executable() {
  local path=$1
  if [[ ! -x "$path" ]]; then
    fail "expected $path to be executable"
  fi
}

write_binary_stub() {
  local path=$1
  local name=$2

  cat >"$path" <<EOF
#!/usr/bin/env bash
set -euo pipefail
printf '%s %s\n' "$name" "\$*" >> "\$TEST_LOG"
exit 0
EOF
  chmod +x "$path"
}

write_cargo_stub() {
  local path=$1

  cat >"$path" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'cargo %s\n' "$*" >> "$TEST_LOG"
root=
while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      root=$2
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
mkdir -p "$root/bin"
cat >"$root/bin/omarchy-world-clock" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
printf 'source-binary %s\n' "$*" >> "$TEST_LOG"
exit 0
STUB
chmod +x "$root/bin/omarchy-world-clock"
EOF
  chmod +x "$path"
}

make_sandbox() {
  local sandbox
  sandbox=$(mktemp -d)

  mkdir -p \
    "$sandbox/home/.config/waybar" \
    "$sandbox/bin" \
    "$sandbox/prefix" \
    "$sandbox/release-src" \
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

  printf '%s\n' "$sandbox"
}

run_install() {
  local sandbox=$1
  shift

  TEST_LOG="$sandbox/log"
  export TEST_LOG

  PATH="$sandbox/stubs:$ORIGINAL_PATH" \
    HOME="$sandbox/home" \
    OMARCHY_WORLD_CLOCK_HOME="$sandbox/prefix" \
    OMARCHY_WORLD_CLOCK_BIN_DIR="$sandbox/bin" \
    WAYBAR_CONFIG="$sandbox/home/.config/waybar/config.jsonc" \
    WAYBAR_STYLE="$sandbox/home/.config/waybar/style.css" \
    OMARCHY_WORLD_CLOCK_CONFIG="$sandbox/home/.config/omarchy-world-clock/config.json" \
    "$REPO_ROOT/install.sh" "$@"
}

test_installs_release_archive() {
  local sandbox archive
  sandbox=$(make_sandbox)
  trap 'rm -rf "$sandbox"' RETURN

  write_binary_stub "$sandbox/release-src/omarchy-world-clock" release-binary
  archive="$sandbox/omarchy-world-clock-x86_64-unknown-linux-gnu.tar.gz"
  tar -C "$sandbox/release-src" -czf "$archive" omarchy-world-clock

  OMARCHY_WORLD_CLOCK_DOWNLOAD_URL="file://$archive" run_install "$sandbox"

  assert_executable "$sandbox/prefix/bin/omarchy-world-clock"
  assert_executable "$sandbox/bin/omarchy-world-clock"
  assert_contains "$sandbox/log" "release-binary install-waybar"
  assert_contains "$sandbox/log" "release-binary restart-waybar"
}

test_can_build_from_source() {
  local sandbox
  sandbox=$(make_sandbox)
  trap 'rm -rf "$sandbox"' RETURN

  write_cargo_stub "$sandbox/stubs/cargo"

  run_install "$sandbox" --build-from-source

  assert_contains "$sandbox/log" "cargo install --path $REPO_ROOT --root $sandbox/prefix --force"
  assert_contains "$sandbox/log" "source-binary install-waybar"
  assert_contains "$sandbox/log" "source-binary restart-waybar"
}

test_installs_release_archive
test_can_build_from_source

printf 'install.sh tests passed\n'
