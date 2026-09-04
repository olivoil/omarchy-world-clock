#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT"

run() {
  printf '\n==> %s\n' "$*"
  "$@"
}

run cargo fmt --all -- --check
run bash -n scripts/install-review-preview.sh
run bash -n scripts/install-integrations.sh
run bash -n bin/omarchy-world-clock
run bash -n skills/omarchy-world-clock/scripts/world-clock
run bash tests/install-review-preview.sh
run bash tests/install-integrations.sh
run cargo clippy --locked --all-targets -- -D warnings
run cargo test --locked
run node tests/timeline-hover-state.mjs
run node tests/time-rail.mjs
run node tests/weather-state.mjs
run node tests/weather-detail-logic.mjs
run node tests/weather-refresh.mjs
run node tests/integration-state.mjs
run bash tests/bundle.sh
run node scripts/build-world-map-source.mjs --check
run node scripts/build-featured-cities.mjs --check
run scripts/check-globe-artifacts.sh

OMARCHY_ROOT=${OMARCHY_PATH:-/usr/share/omarchy}
if command -v omarchy >/dev/null 2>&1 \
  && { [[ -x /usr/lib/qt6/bin/qmllint ]] || command -v qmllint >/dev/null 2>&1; } \
  && [[ -d "$OMARCHY_ROOT/shell" ]]; then
  run scripts/validate-plugin.sh
fi

printf '\nCI checks passed.\n'
