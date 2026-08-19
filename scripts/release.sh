#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT"

usage() {
  cat <<'EOF'
Usage: scripts/release.sh

Validates a clean, committed World Clock release candidate. This command is
intentionally publication-free: it never commits, tags, pushes, creates a
GitHub release, changes the AUR, or updates the plugin directory.
EOF
}

if (( $# > 0 )); then
  if [[ $1 == -h || $1 == --help ]] && (( $# == 1 )); then
    usage
    exit 0
  fi
  usage >&2
  exit 2
fi

for command_name in cargo git jq sha256sum; do
  command -v "$command_name" >/dev/null 2>&1 || {
    printf 'Missing required command: %s\n' "$command_name" >&2
    exit 1
  }
done

if [[ -n $(git status --porcelain) ]]; then
  printf 'Release candidate has uncommitted changes. Commit it locally before sign-off.\n' >&2
  exit 1
fi

package_version=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')
manifest_version=$(jq -r '.version' manifest.json)
backend_version=$(bin/omarchy-world-clock-backend version)

if [[ $package_version != "$manifest_version" || $package_version != "$backend_version" ]]; then
  printf 'Version mismatch: Cargo=%s manifest=%s backend=%s\n' \
    "$package_version" "$manifest_version" "$backend_version" >&2
  exit 1
fi

git diff --check
scripts/ci.sh

printf '\nRelease candidate v%s is internally consistent and fully validated.\n' \
  "$package_version"
printf 'No publication action was performed. Follow docs/release.md only after tester approval.\n'
