#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(git -C "$(dirname "${BASH_SOURCE[0]}")/.." rev-parse --show-toplevel)
SOURCE="$REPO_ROOT/assets/world-map.svg"
OUTPUT="$REPO_ROOT/assets/world-map.png"
WIDTH=8192
HEIGHT=4096

if ! command -v rsvg-convert >/dev/null 2>&1; then
  printf 'rsvg-convert is required to render the globe texture.\n' >&2
  exit 1
fi

if [[ ${1:-} == "--check" ]]; then
  CHECK_OUTPUT=$(mktemp --suffix=.world-map.png)
  trap 'rm -f -- "$CHECK_OUTPUT"' EXIT
  rsvg-convert --width "$WIDTH" --height "$HEIGHT" \
    --output "$CHECK_OUTPUT" "$SOURCE"
  if ! cmp -s "$CHECK_OUTPUT" "$OUTPUT"; then
    printf 'assets/world-map.png is out of date; run scripts/build-world-map.sh.\n' >&2
    exit 1
  fi
  exit 0
fi

rsvg-convert --width "$WIDTH" --height "$HEIGHT" \
  --output "$OUTPUT" "$SOURCE"
