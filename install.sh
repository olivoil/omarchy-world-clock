#!/usr/bin/env bash
set -euo pipefail

SCRIPT_PATH=${BASH_SOURCE[0]:-$0}
if [[ -f "$SCRIPT_PATH" ]]; then
  REPO_ROOT=$(cd "$(dirname "$SCRIPT_PATH")" && pwd)
else
  REPO_ROOT=$(pwd)
fi

PREFIX=${OMARCHY_WORLD_CLOCK_HOME:-"$HOME/.local/share/omarchy-world-clock"}
BIN_DIR=${OMARCHY_WORLD_CLOCK_BIN_DIR:-"$HOME/.local/bin"}
WAYBAR_CONFIG=${WAYBAR_CONFIG:-"$HOME/.config/waybar/config.jsonc"}
WAYBAR_STYLE=${WAYBAR_STYLE:-"$HOME/.config/waybar/style.css"}
SHELL_CONFIG=${OMARCHY_SHELL_CONFIG:-"$HOME/.config/omarchy/shell.json"}
USER_CONFIG=${OMARCHY_WORLD_CLOCK_CONFIG:-"$HOME/.config/omarchy-world-clock/config.json"}
WRAPPER_PATH="$BIN_DIR/omarchy-world-clock"
INSTALLED_BINARY="$PREFIX/bin/omarchy-world-clock"
LEGACY_PREFIX=${OMARCHY_WORLD_CLOCK_LEGACY_HOME:-"$HOME/.local/share/omarchy-world-clock-rs"}
LEGACY_WRAPPER_PATH=${OMARCHY_WORLD_CLOCK_LEGACY_WRAPPER:-"$BIN_DIR/omarchy-world-clock-rs"}
RELEASE_REPO=${OMARCHY_WORLD_CLOCK_RELEASE_REPO:-"olivoil/omarchy-world-clock"}
RELEASE_VERSION=${OMARCHY_WORLD_CLOCK_VERSION:-"latest"}
RELEASE_DOWNLOAD_URL=${OMARCHY_WORLD_CLOCK_DOWNLOAD_URL:-""}
PLUGIN_REVISION_OVERRIDE=${OMARCHY_WORLD_CLOCK_PLUGIN_REVISION:-""}
PLUGIN_ID=io.github.olivoil.world-clock
PLUGIN_PATH="$HOME/.config/omarchy/plugins/$PLUGIN_ID"
INSTALL_MODE=release
TARGET=${OMARCHY_WORLD_CLOCK_TARGET:-""}

usage() {
  cat <<EOF
Usage: install.sh [--from-release|--build-from-source]

Options:
  --from-release       Download the prebuilt GitHub release binary (default).
  --build-from-source  Build and install from this checkout with cargo.
  -h, --help           Show this help.

Environment:
  OMARCHY_WORLD_CLOCK_VERSION       Release tag to install, or "latest" (default).
  OMARCHY_WORLD_CLOCK_RELEASE_REPO  GitHub repo that owns releases (default: $RELEASE_REPO).
  OMARCHY_WORLD_CLOCK_DOWNLOAD_URL  Exact archive URL override.
  OMARCHY_WORLD_CLOCK_TARGET        Target asset override.
  OMARCHY_WORLD_CLOCK_PLUGIN_REVISION
                                      Exact plugin git tag or commit override.
  OMARCHY_WORLD_CLOCK_SKIP_RUNTIME_DEPS
                                      Set to 1 to skip Arch runtime package install.
EOF
}

for arg in "$@"; do
  case "$arg" in
    --from-release)
      INSTALL_MODE=release
      ;;
    --build-from-source)
      INSTALL_MODE=source
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'Unknown argument: %s\n\n' "$arg" >&2
      usage >&2
      exit 2
      ;;
  esac
done

detect_target() {
  local kernel machine
  kernel=$(uname -s)
  machine=$(uname -m)

  case "$kernel:$machine" in
    Linux:x86_64|Linux:amd64)
      printf 'x86_64-unknown-linux-gnu'
      ;;
    Linux:aarch64|Linux:arm64)
      printf 'aarch64-unknown-linux-gnu'
      ;;
    *)
      printf 'Unsupported platform: %s %s\n' "$kernel" "$machine" >&2
      printf 'Install from source with ./install.sh --build-from-source if this platform can build the app.\n' >&2
      exit 1
      ;;
  esac
}

download_file() {
  local url=$1
  local destination=$2

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 3 --connect-timeout 20 -o "$destination" "$url"
    return
  fi
  if command -v wget >/dev/null 2>&1; then
    wget -O "$destination" "$url"
    return
  fi

  printf 'Need curl or wget to download the release archive.\n' >&2
  exit 1
}

release_url() {
  local archive=$1

  if [[ -n "$RELEASE_DOWNLOAD_URL" ]]; then
    printf '%s' "$RELEASE_DOWNLOAD_URL"
  elif [[ "$RELEASE_VERSION" == "latest" ]]; then
    printf 'https://github.com/%s/releases/latest/download/%s' "$RELEASE_REPO" "$archive"
  else
    printf 'https://github.com/%s/releases/download/%s/%s' "$RELEASE_REPO" "$RELEASE_VERSION" "$archive"
  fi
}

install_from_release() {
  local target archive url tmp archive_path binary_path

  target=${TARGET:-$(detect_target)}
  archive="omarchy-world-clock-${target}.tar.gz"
  url=$(release_url "$archive")
  tmp=$(mktemp -d)
  archive_path="$tmp/$archive"
  binary_path="$tmp/omarchy-world-clock"

  printf 'Downloading %s\n' "$url"
  download_file "$url" "$archive_path"
  tar -xzf "$archive_path" -C "$tmp"

  if [[ ! -f "$binary_path" ]]; then
    printf 'Release archive did not contain omarchy-world-clock.\n' >&2
    exit 1
  fi

  install -m 755 "$binary_path" "$INSTALLED_BINARY"
  rm -rf "$tmp"
}

install_from_source() {
  if [[ ! -f "$REPO_ROOT/Cargo.toml" ]]; then
    printf 'Could not find Cargo.toml at %s.\n' "$REPO_ROOT" >&2
    printf 'Run --build-from-source from a checkout, or use the default release install.\n' >&2
    exit 1
  fi
  if ! command -v cargo >/dev/null 2>&1; then
    printf 'cargo is required for --build-from-source.\n' >&2
    exit 1
  fi

  cargo install --locked --path "$REPO_ROOT" --root "$PREFIX" --force
}

install_arch_runtime_dependencies() {
  if [[ "${OMARCHY_WORLD_CLOCK_SKIP_RUNTIME_DEPS:-0}" == "1" ]]; then
    return
  fi
  if ! command -v pacman >/dev/null 2>&1; then
    return
  fi

  local -a packages missing
  local missing_output
  packages=(gtk4 gtk4-layer-shell)

  missing_output=$(pacman -T "${packages[@]}" || true)

  if [[ -z "$missing_output" ]]; then
    return
  fi

  mapfile -t missing <<<"$missing_output"

  printf 'Installing missing runtime dependencies with pacman: %s\n' "${missing[*]}"
  if [[ ${EUID:-$(id -u)} -eq 0 ]]; then
    pacman -S --needed --noconfirm "${missing[@]}"
  elif command -v sudo >/dev/null 2>&1; then
    sudo pacman -S --needed --noconfirm "${missing[@]}"
  else
    printf '\nError: missing runtime dependencies: %s\n' "${missing[*]}" >&2
    printf 'Install them with:\n  sudo pacman -S --needed %s\n\n' "${missing[*]}" >&2
    exit 1
  fi
}

check_runtime_libraries() {
  if ! command -v ldd >/dev/null 2>&1; then
    return
  fi

  local missing
  missing=$(ldd "$INSTALLED_BINARY" 2>/dev/null | awk '/not found/ {print $1}' | sort -u || true)
  if [[ -z "$missing" ]]; then
    return
  fi

  printf '\nError: the installed binary is missing runtime libraries:\n%s\n' "$missing" >&2
  if command -v pacman >/dev/null 2>&1; then
    printf 'On Arch/Omarchy, install runtime dependencies with:\n  sudo pacman -S --needed gtk4 gtk4-layer-shell\n\n' >&2
  fi
  exit 1
}

resolve_plugin_revision() {
  local revision version

  if [[ -n "$PLUGIN_REVISION_OVERRIDE" ]]; then
    revision=$PLUGIN_REVISION_OVERRIDE
  elif [[ "$INSTALL_MODE" == "source" ]] &&
    revision=$(git -C "$REPO_ROOT" rev-parse --verify HEAD 2>/dev/null); then
    :
  elif [[ "$RELEASE_VERSION" != "latest" ]]; then
    revision=$RELEASE_VERSION
  else
    version=$("$WRAPPER_PATH" version)
    version=${version#v}
    revision="v$version"
  fi

  if [[ -z "$revision" || "$revision" == -* ]]; then
    printf 'Could not determine a valid World Clock plugin revision.\n' >&2
    exit 1
  fi
  printf '%s' "$revision"
}

pin_installed_plugin_revision() {
  local revision=$1
  local current target changed=0

  [[ -e "$PLUGIN_PATH" ]] || return 0
  if [[ ! -e "$PLUGIN_PATH/.git" ]]; then
    printf 'Cannot pin non-git World Clock plugin at %s to %s.\n' "$PLUGIN_PATH" "$revision" >&2
    exit 1
  fi

  current=$(git -C "$PLUGIN_PATH" rev-parse --verify HEAD)
  target=$(git -C "$PLUGIN_PATH" rev-parse --verify "${revision}^{commit}" 2>/dev/null || true)
  if [[ -z "$target" ]]; then
    git -C "$PLUGIN_PATH" fetch --quiet -- origin "$revision"
    target=$(git -C "$PLUGIN_PATH" rev-parse --verify FETCH_HEAD)
  fi
  if [[ "$current" != "$target" ]]; then
    git -C "$PLUGIN_PATH" checkout --quiet --detach "$target"
    changed=1
  fi

  omarchy plugin validate "$PLUGIN_PATH"
  if (( changed )) && command -v omarchy-shell >/dev/null 2>&1; then
    omarchy-shell shell rescanPlugins >/dev/null
  fi
}

mkdir -p "$PREFIX/bin" "$BIN_DIR"
install_arch_runtime_dependencies

case "$INSTALL_MODE" in
  release)
    install_from_release
    ;;
  source)
    install_from_source
    ;;
esac

cat >"$WRAPPER_PATH" <<EOF
#!/usr/bin/env bash
set -euo pipefail
exec "$INSTALLED_BINARY" "\$@"
EOF
chmod +x "$WRAPPER_PATH"

check_runtime_libraries

PLUGIN_REVISION=$(resolve_plugin_revision)
INSTALL_ARGS=(install \
  --shell-config "$SHELL_CONFIG" \
  --waybar-config "$WAYBAR_CONFIG" \
  --waybar-style "$WAYBAR_STYLE" \
  --command-path "$WRAPPER_PATH" \
  --user-config "$USER_CONFIG" \
  --plugin-revision "$PLUGIN_REVISION")
if [[ "$INSTALL_MODE" == "source" ]] && git -C "$REPO_ROOT" rev-parse --verify HEAD >/dev/null 2>&1; then
  INSTALL_ARGS+=(--plugin-url "$REPO_ROOT")
fi
"$WRAPPER_PATH" "${INSTALL_ARGS[@]}"
pin_installed_plugin_revision "$PLUGIN_REVISION"

rm -f "$LEGACY_WRAPPER_PATH"
rm -rf "$LEGACY_PREFIX"

if [[ -f "$SHELL_CONFIG" ]]; then
  printf 'Installed Omarchy World Clock.\nOmarchy shell config: %s\nWrapper: %s\n' \
    "$SHELL_CONFIG" "$WRAPPER_PATH"
else
  printf 'Installed Omarchy World Clock.\nWaybar config: %s\nWaybar style: %s\nWrapper: %s\n' \
    "$WAYBAR_CONFIG" "$WAYBAR_STYLE" "$WRAPPER_PATH"
fi
