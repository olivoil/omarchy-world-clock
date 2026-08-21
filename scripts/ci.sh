#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT"

run() {
  printf '\n==> %s\n' "$*"
  "$@"
}

run cargo fmt --all -- --check
run cargo clippy --locked --all-targets -- -D warnings
run cargo test --locked
run bash tests/bundle.sh

if [[ -x /usr/lib/qt6/bin/qsb ]] || command -v qsb >/dev/null 2>&1; then
  run scripts/build-globe-shader.sh --check
fi

OMARCHY_ROOT=${OMARCHY_PATH:-/usr/share/omarchy}
if command -v omarchy >/dev/null 2>&1 \
  && { [[ -x /usr/lib/qt6/bin/qmllint ]] || command -v qmllint >/dev/null 2>&1; } \
  && [[ -d "$OMARCHY_ROOT/shell" ]]; then
  run scripts/validate-plugin.sh
fi

printf '\nCI checks passed.\n'
