#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT"

scripts/build-timezone-grid.sh --check
scripts/build-plugin-backend.sh --check

backend=bin/omarchy-world-clock-backend
[[ -x $backend ]]
[[ $("$backend" version) == "$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')" ]]
[[ $(stat -c %s "$backend") -le $((10 * 1024 * 1024)) ]]
file "$backend" | grep -q 'static-pie linked'
! readelf -d "$backend" | grep -q '(NEEDED)'
! readelf -l "$backend" | grep -q 'INTERP'

grep -qx "backend_version=$($backend version)" bin/BUILDINFO
grep -qx "binary_size_bytes=$(stat -c %s "$backend")" bin/BUILDINFO
grep -qx "binary_sha256=$(sha256sum "$backend" | awk '{ print $1 }')" bin/BUILDINFO
grep -qx "cargo_lock_sha256=$(sha256sum Cargo.lock | awk '{ print $1 }')" bin/BUILDINFO
grep -qx "timezone_grid_sha256=$(sha256sum data/timezone-grid.bin | awk '{ print $1 }')" bin/BUILDINFO

printf 'bundle tests passed\n'
