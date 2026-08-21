#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT"
source scripts/build-environment.sh

mode=write
if [[ ${1:-} == --check ]]; then
  mode=check
  shift
fi
if (( $# > 0 )); then
  printf 'Usage: scripts/build-plugin-backend.sh [--check]\n' >&2
  exit 2
fi

for command_name in awk cargo cmp file grep install jq readelf sha256sum stat uname; do
  command -v "$command_name" >/dev/null 2>&1 || {
    printf 'Missing required command: %s\n' "$command_name" >&2
    exit 1
  }
done

[[ $(uname -s) == Linux ]] || {
  printf 'The bundled backend must be built on Linux.\n' >&2
  exit 1
}
[[ $(uname -m) == x86_64 ]] || {
  printf 'The bundled backend currently supports x86_64 only.\n' >&2
  exit 1
}

package_version=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')
manifest_version=$(jq -r '.version' manifest.json)
[[ $package_version == "$manifest_version" ]] || {
  printf 'Cargo version %s does not match manifest version %s.\n' \
    "$package_version" "$manifest_version" >&2
  exit 1
}

candidate=target/plugin-backend-dist/omarchy-world-clock-backend
run_build_container sh -c '
  set -eu
  cargo build --release --locked --bin omarchy-world-clock-backend
  install -Dm755 "$CARGO_TARGET_DIR/release/omarchy-world-clock-backend" \
    /work/target/plugin-backend-dist/omarchy-world-clock-backend
  strip --strip-all /work/target/plugin-backend-dist/omarchy-world-clock-backend
  rustc --version > /work/target/plugin-backend-dist/rustc-version
'

file_output=$(file "$candidate")
[[ $file_output == *"ELF 64-bit"* \
  && $file_output == *"x86-64"* \
  && $file_output == *"static-pie linked"* ]] || {
  printf 'Unexpected bundled backend format: %s\n' "$file_output" >&2
  exit 1
}
[[ $(stat -c %s "$candidate") -le $((10 * 1024 * 1024)) ]] || {
  printf 'Bundled backend exceeds the 10 MiB size budget.\n' >&2
  exit 1
}
if readelf -d "$candidate" | grep -q '(NEEDED)'; then
  printf 'Bundled backend unexpectedly has a dynamic-library dependency:\n' >&2
  readelf -d "$candidate" >&2
  exit 1
fi
if readelf -l "$candidate" | grep -q 'INTERP'; then
  printf 'Bundled backend unexpectedly requests a dynamic interpreter:\n' >&2
  readelf -l "$candidate" >&2
  exit 1
fi

[[ $("$candidate" version) == "$package_version" ]] || {
  printf 'Bundled backend version does not match Cargo.toml.\n' >&2
  exit 1
}

runtime_dir=target/plugin-backend-dist/runtime-check
mkdir -p "$runtime_dir/home"
protocol_version=$(
  HOME="$runtime_dir/home" \
  OMARCHY_WORLD_CLOCK_CONFIG="$runtime_dir/config.json" \
    "$candidate" module | jq -er '.protocol_version'
)

expected_buildinfo=target/plugin-backend-dist/BUILDINFO
cat >"$expected_buildinfo" <<EOF
backend_version=$package_version
backend_protocol=$protocol_version
target=x86_64-unknown-linux-musl
linkage=static-pie
build_image=$BUILD_CONTAINER_IMAGE
rustc=$(cat target/plugin-backend-dist/rustc-version)
binary_size_bytes=$(stat -c %s "$candidate")
binary_sha256=$(sha256sum "$candidate" | awk '{ print $1 }')
cargo_lock_sha256=$(sha256sum Cargo.lock | awk '{ print $1 }')
timezone_grid_sha256=$(sha256sum data/timezone-grid.bin | awk '{ print $1 }')
featured_cities_sha256=$(sha256sum data/featured-cities.json | awk '{ print $1 }')
EOF

if [[ $mode == check ]]; then
  cmp --silent "$candidate" bin/omarchy-world-clock-backend || {
    printf 'Committed backend is stale. Run scripts/build-plugin-backend.sh.\n' >&2
    exit 1
  }
  expected_checksum=$(awk '$2 == "omarchy-world-clock-backend" { print $1 }' bin/SHA256SUMS)
  actual_checksum=$(sha256sum "$candidate" | awk '{ print $1 }')
  [[ -n $expected_checksum && $expected_checksum == "$actual_checksum" ]] || {
    printf 'bin/SHA256SUMS does not match the bundled backend.\n' >&2
    exit 1
  }
  cmp --silent "$expected_buildinfo" bin/BUILDINFO || {
    printf 'bin/BUILDINFO is stale. Run scripts/build-plugin-backend.sh.\n' >&2
    diff -u bin/BUILDINFO "$expected_buildinfo" >&2 || true
    exit 1
  }
  printf 'Bundled backend is reproducible and current (%s bytes).\n' \
    "$(stat -c %s "$candidate")"
  exit 0
fi

install -Dm755 "$candidate" bin/omarchy-world-clock-backend
(
  cd bin
  sha256sum omarchy-world-clock-backend >SHA256SUMS
)
install -m644 "$expected_buildinfo" bin/BUILDINFO

printf 'Updated bin/omarchy-world-clock-backend (%s bytes).\n' \
  "$(stat -c %s bin/omarchy-world-clock-backend)"
