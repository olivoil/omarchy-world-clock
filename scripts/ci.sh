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
run bash tests/install.sh
run bash tests/uninstall.sh

printf '\nCI checks passed.\n'
