#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(CDPATH= cd -- "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
SOURCE="$REPO_ROOT/assets/globe.frag"
OUTPUT="$REPO_ROOT/assets/globe.frag.qsb"
QSB_BIN=${QSB:-/usr/lib/qt6/bin/qsb}

if [[ ! -x $QSB_BIN ]]; then
  QSB_BIN=$(command -v qsb || true)
fi
if [[ -z $QSB_BIN || ! -x $QSB_BIN ]]; then
  printf 'Qt Shader Baker (qsb) is required to build the globe shader.\n' >&2
  exit 1
fi

if [[ ${1:-} == "--check" ]]; then
  GENERATED=$(mktemp)
  trap 'rm -f -- "$GENERATED"' EXIT
  "$QSB_BIN" --qt6 --qsbversion 64 -O -o "$GENERATED" "$SOURCE"
  if ! cmp -s "$GENERATED" "$OUTPUT"; then
    printf 'assets/globe.frag.qsb is out of date; run scripts/build-globe-shader.sh.\n' >&2
    exit 1
  fi
  printf 'Globe shader is up to date.\n'
  exit 0
fi

"$QSB_BIN" --qt6 --qsbversion 64 -O -o "$OUTPUT" "$SOURCE"
printf 'Built %s.\n' "$OUTPUT"
