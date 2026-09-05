#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf 'Usage: scripts/review-current-branch.sh [--after <plugin-id>]\n'
}

case "${1:-}" in
-h | --help)
  usage
  exit 0
  ;;
esac

for command_name in git jq omarchy; do
  command -v "$command_name" >/dev/null 2>&1 || {
    printf 'Missing required command: %s\n' "$command_name" >&2
    exit 1
  }
done

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

canonical_id=io.github.olivoil.world-clock
production_repo=https://github.com/olivoil/omarchy-world-clock.git

scripts/build-plugin-backend.sh
scripts/ci.sh

if omarchy plugin list --json | jq -e --arg id "$canonical_id" \
  'any(.[]; .id == $id)' >/dev/null; then
  omarchy plugin update "$canonical_id" --yes
else
  omarchy plugin add "$production_repo" --enable --yes
fi

scripts/install-review-preview.sh --prune-other-reviews "$@"
