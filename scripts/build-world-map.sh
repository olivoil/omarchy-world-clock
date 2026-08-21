#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(CDPATH= cd -- "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
SOURCE="$REPO_ROOT/assets/world-map.svg"
DETAIL_OUTPUT="$REPO_ROOT/assets/world-map.png"
PREVIEW_OUTPUT="$REPO_ROOT/assets/world-map-preview.png"
DETAIL_WIDTH=8192
DETAIL_HEIGHT=4096
PREVIEW_WIDTH=2048
PREVIEW_HEIGHT=1024

if ! command -v rsvg-convert >/dev/null 2>&1; then
  printf 'rsvg-convert is required to render the globe texture.\n' >&2
  exit 1
fi

render_texture() {
  local output=$1
  local width=$2
  local height=$3
  rsvg-convert --width "$width" --height "$height" \
    --output "$output" "$SOURCE"
}

if [[ ${1:-} == "--check" ]]; then
  CHECK_DIR=$(mktemp -d)
  trap 'rm -r -- "$CHECK_DIR"' EXIT
  render_texture "$CHECK_DIR/world-map.png" "$DETAIL_WIDTH" "$DETAIL_HEIGHT"
  render_texture "$CHECK_DIR/world-map-preview.png" "$PREVIEW_WIDTH" "$PREVIEW_HEIGHT"

  status=0
  for filename in world-map.png world-map-preview.png; do
    if ! cmp -s "$CHECK_DIR/$filename" "$REPO_ROOT/assets/$filename"; then
      printf 'assets/%s is out of date; run scripts/build-world-map.sh.\n' \
        "$filename" >&2
      status=1
    fi
  done
  exit "$status"
fi

render_texture "$DETAIL_OUTPUT" "$DETAIL_WIDTH" "$DETAIL_HEIGHT"
render_texture "$PREVIEW_OUTPUT" "$PREVIEW_WIDTH" "$PREVIEW_HEIGHT"
