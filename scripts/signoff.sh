#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT"

usage() {
  cat <<EOF
Usage: scripts/signoff.sh [SIGNOFF_NAME...]

Runs the local CI checks, then signs off the current commit with Basecamp's
gh-signoff extension. Pass optional names when branch protection requires
partial signoffs.

Examples:
  scripts/signoff.sh
  scripts/signoff.sh tests lint security

Prerequisites:
  gh auth login
  gh extension install basecamp/gh-signoff
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if ! command -v gh >/dev/null 2>&1; then
  printf 'Missing required command: gh\n' >&2
  exit 1
fi

if ! gh signoff --help >/dev/null 2>&1; then
  printf 'Missing gh extension: basecamp/gh-signoff\n' >&2
  printf 'Install it with: gh extension install basecamp/gh-signoff\n' >&2
  exit 1
fi

scripts/ci.sh

printf '\n==> gh signoff %s\n' "$*"
gh signoff "$@"
