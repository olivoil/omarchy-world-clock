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
  if ! grep -F -- "$needle" "$file" >/dev/null 2>&1; then
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
if [[ \${1:-} == version ]]; then
  printf '0.3.0\n'
fi
if [[ \${1:-} == install && -n \${TEST_PLUGIN_PATH:-} ]]; then
  mkdir -p "\$TEST_PLUGIN_PATH/.git"
  printf '{}\n' >"\$TEST_PLUGIN_PATH/manifest.json"
fi
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
if [[ ${1:-} == version ]]; then
  printf '0.3.0\n'
fi
exit 0
STUB
chmod +x "$root/bin/omarchy-world-clock"
EOF
  chmod +x "$path"
}

write_pacman_stub() {
  local path=$1

  cat >"$path" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'pacman %s\n' "$*" >> "$TEST_LOG"
case "$1" in
  -T)
    printf '%s\n' gtk4 gtk4-layer-shell
    exit 127
    ;;
  -S)
    exit 0
    ;;
  *)
    exit 2
    ;;
esac
EOF
  chmod +x "$path"
}

write_sudo_stub() {
  local path=$1

  cat >"$path" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'sudo %s\n' "$*" >> "$TEST_LOG"
exec "$@"
EOF
  chmod +x "$path"
}

write_git_pin_stub() {
  local path=$1

  cat >"$path" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'git %s\n' "$*" >> "$TEST_LOG"
case "$*" in
  *"rev-parse --verify HEAD")
    printf 'old-head\n'
    ;;
  *"rev-parse --verify FETCH_HEAD")
    printf 'target-head\n'
    ;;
  *"rev-parse --verify "*"^{commit}")
    exit 1
    ;;
esac
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
    TEST_PLUGIN_PATH="${TEST_PLUGIN_PATH:-}" \
    OMARCHY_WORLD_CLOCK_SKIP_RUNTIME_DEPS="${OMARCHY_WORLD_CLOCK_SKIP_RUNTIME_DEPS:-1}" \
    "$REPO_ROOT/install.sh" "$@"
}

test_installs_release_archive() {
  local sandbox archive
  sandbox=$(make_sandbox)
  trap 'rm -rf "$sandbox"' RETURN

  write_binary_stub "$sandbox/release-src/omarchy-world-clock" release-binary
  archive="$sandbox/omarchy-world-clock-x86_64-unknown-linux-gnu.tar.gz"
  tar -C "$sandbox/release-src" -czf "$archive" omarchy-world-clock

  OMARCHY_WORLD_CLOCK_VERSION=v0.2.1 \
    OMARCHY_WORLD_CLOCK_DOWNLOAD_URL="file://$archive" \
    run_install "$sandbox"

  assert_executable "$sandbox/prefix/bin/omarchy-world-clock"
  assert_executable "$sandbox/bin/omarchy-world-clock"
  assert_contains "$sandbox/log" "release-binary install"
  assert_contains "$sandbox/log" "--plugin-revision v0.2.1"
}

test_can_build_from_source() {
  local sandbox
  sandbox=$(make_sandbox)
  trap 'rm -rf "$sandbox"' RETURN

  write_cargo_stub "$sandbox/stubs/cargo"

  run_install "$sandbox" --build-from-source

  assert_contains "$sandbox/log" "cargo install --locked --path $REPO_ROOT --root $sandbox/prefix --force"
  assert_contains "$sandbox/log" "source-binary install"
  assert_contains "$sandbox/log" "--plugin-revision $(git -C "$REPO_ROOT" rev-parse HEAD)"
  assert_contains "$sandbox/log" "--plugin-url $REPO_ROOT"
}

test_installs_arch_runtime_dependencies() {
  local sandbox archive
  sandbox=$(make_sandbox)
  trap 'rm -rf "$sandbox"' RETURN

  write_pacman_stub "$sandbox/stubs/pacman"
  write_sudo_stub "$sandbox/stubs/sudo"
  write_binary_stub "$sandbox/release-src/omarchy-world-clock" release-binary
  archive="$sandbox/omarchy-world-clock-x86_64-unknown-linux-gnu.tar.gz"
  tar -C "$sandbox/release-src" -czf "$archive" omarchy-world-clock

  OMARCHY_WORLD_CLOCK_SKIP_RUNTIME_DEPS=0 \
    OMARCHY_WORLD_CLOCK_DOWNLOAD_URL="file://$archive" \
    run_install "$sandbox"

  assert_contains "$sandbox/log" "pacman -T gtk4 gtk4-layer-shell"
  assert_contains "$sandbox/log" "pacman -S --needed --noconfirm gtk4 gtk4-layer-shell"
  assert_contains "$sandbox/log" "release-binary install"
}

test_pins_plugin_after_installing_an_older_binary() {
  local sandbox archive plugin_path
  sandbox=$(make_sandbox)
  trap 'rm -rf "$sandbox"' RETURN
  plugin_path="$sandbox/home/.config/omarchy/plugins/io.github.olivoil.world-clock"

  write_binary_stub "$sandbox/release-src/omarchy-world-clock" release-binary
  write_git_pin_stub "$sandbox/stubs/git"
  write_binary_stub "$sandbox/stubs/omarchy" omarchy
  write_binary_stub "$sandbox/stubs/omarchy-shell" omarchy-shell
  archive="$sandbox/omarchy-world-clock-x86_64-unknown-linux-gnu.tar.gz"
  tar -C "$sandbox/release-src" -czf "$archive" omarchy-world-clock

  TEST_PLUGIN_PATH="$plugin_path" \
    OMARCHY_WORLD_CLOCK_VERSION=v0.2.1 \
    OMARCHY_WORLD_CLOCK_DOWNLOAD_URL="file://$archive" \
    run_install "$sandbox"

  assert_contains "$sandbox/log" "git -C $plugin_path fetch --quiet -- origin v0.2.1"
  assert_contains "$sandbox/log" "git -C $plugin_path checkout --quiet --detach target-head"
  assert_contains "$sandbox/log" "omarchy plugin validate $plugin_path"
  assert_contains "$sandbox/log" "omarchy-shell shell rescanPlugins"
}

test_installs_release_archive
test_can_build_from_source
test_installs_arch_runtime_dependencies
test_pins_plugin_after_installing_an_older_binary

printf 'install.sh tests passed\n'
