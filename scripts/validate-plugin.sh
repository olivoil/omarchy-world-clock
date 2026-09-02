#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(git -C "$(dirname "${BASH_SOURCE[0]}")/.." rev-parse --show-toplevel)
OMARCHY_ROOT=${OMARCHY_PATH:-/usr/share/omarchy}

if ! command -v omarchy >/dev/null 2>&1; then
  printf 'omarchy is required to validate the plugin manifest.\n' >&2
  exit 1
fi
if [[ -x /usr/lib/qt6/bin/qmllint ]]; then
  QMLLINT=/usr/lib/qt6/bin/qmllint
elif command -v qmllint >/dev/null 2>&1; then
  QMLLINT=$(command -v qmllint)
else
  printf 'qmllint is required to validate the Quattro entry point.\n' >&2
  exit 1
fi
if [[ ! -d "$OMARCHY_ROOT/shell" ]]; then
  printf 'Could not find the Omarchy shell at %s/shell.\n' "$OMARCHY_ROOT" >&2
  exit 1
fi

cd "$REPO_ROOT"
omarchy plugin validate .

# Quickshell's `qs` import is its source-tree namespace. Recreate that import
# root for the standalone Qt linter without copying or modifying Omarchy.
PLUGIN_LINT_ROOT=$(mktemp -d)
trap 'rm -r -- "$PLUGIN_LINT_ROOT"' EXIT
ln -s "$OMARCHY_ROOT/shell" "$PLUGIN_LINT_ROOT/qs"
# Quickshell's Process exit status and the bar host's dynamic methods are not
# represented completely in their QML type metadata. Suppress those two known
# false positives, then fail on every remaining warning.
"$QMLLINT" -W 0 \
  --signal-handler-parameters disable \
  --missing-property disable \
  -I "$PLUGIN_LINT_ROOT" \
  quattro/WorldClock.qml \
  quattro/Panel.qml \
  quattro/SolarArc.qml \
  quattro/WorldClockKeyCatcher.qml \
  quattro/Globe.qml

printf 'Quattro plugin validation passed.\n'
