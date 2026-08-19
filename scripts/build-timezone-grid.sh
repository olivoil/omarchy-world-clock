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
  printf 'Usage: scripts/build-timezone-grid.sh [--check]\n' >&2
  exit 2
fi

for command_name in cmp install sha256sum stat; do
  command -v "$command_name" >/dev/null 2>&1 || {
    printf 'Missing required command: %s\n' "$command_name" >&2
    exit 1
  }
done

candidate=target/plugin-backend-dist/timezone-grid.bin
mkdir -p "$(dirname "$candidate")"
run_build_container sh -c \
  'cargo run --release --locked --example generate_timezone_grid -- /work/target/plugin-backend-dist/timezone-grid.bin'

if [[ $mode == check ]]; then
  cmp --silent "$candidate" data/timezone-grid.bin || {
    printf 'Committed timezone grid is stale. Run scripts/build-timezone-grid.sh.\n' >&2
    exit 1
  }
  printf 'Timezone grid is reproducible and current (%s bytes, %s).\n' \
    "$(stat -c %s "$candidate")" \
    "$(sha256sum "$candidate" | awk '{ print $1 }')"
  exit 0
fi

install -Dm644 "$candidate" data/timezone-grid.bin
printf 'Updated data/timezone-grid.bin (%s bytes).\n' \
  "$(stat -c %s data/timezone-grid.bin)"
printf 'Rebuild the embedded backend with scripts/build-plugin-backend.sh.\n'
