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

bundle_sandbox=$(mktemp -d)
trap 'rm -r -- "$bundle_sandbox"' EXIT
mkdir -p "$bundle_sandbox/home"
cat >"$bundle_sandbox/probe.json" <<'JSON'
{
  "version": 7,
  "timezones": [{ "timezone": "UTC", "label": "Probe" }]
}
JSON
local_timezone=$(
  HOME="$bundle_sandbox/home" \
  OMARCHY_WORLD_CLOCK_CONFIG="$bundle_sandbox/probe.json" \
    "$backend" snapshot --at 2026-08-11T11:05:00Z | jq -er '.local_timezone'
)
jq -n --arg local "$local_timezone" '
  def location($timezone; $label): {timezone: $timezone, label: $label};
  [
    location("America/Los_Angeles"; "Los Angeles"),
    location("America/Denver"; "Denver"),
    location("America/Chicago"; "Chicago"),
    location("America/New_York"; "New York"),
    location("Europe/London"; "London"),
    location("Europe/Paris"; "Paris"),
    location("Europe/Athens"; "Athens"),
    location("Asia/Kolkata"; "Delhi"),
    location("Asia/Tokyo"; "Tokyo"),
    location("Australia/Sydney"; "Sydney"),
    location("Pacific/Auckland"; "Auckland")
  ] | map(select(.timezone != $local)) as $remote |
  {
    add_timezone: $remote[9].timezone,
    config: {
      version: 7,
      timezones: ([location($local; "Home")] + $remote[0:9])
    }
  }
' >"$bundle_sandbox/plan.json"
jq '.config' "$bundle_sandbox/plan.json" >"$bundle_sandbox/config.json"
add_timezone=$(jq -er '.add_timezone' "$bundle_sandbox/plan.json")
HOME="$bundle_sandbox/home" \
OMARCHY_WORLD_CLOCK_CONFIG="$bundle_sandbox/config.json" \
  "$backend" add "$add_timezone" --label "Tenth remote place"
jq -e --arg timezone "$add_timezone" \
  '(.timezones | length) == 11 and any(.timezones[]; .timezone == $timezone)' \
  "$bundle_sandbox/config.json" >/dev/null

printf 'bundle tests passed\n'
