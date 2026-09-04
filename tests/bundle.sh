#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT"

scripts/build-timezone-grid.sh --check
scripts/build-plugin-backend.sh --check

backend=bin/omarchy-world-clock-backend
agent_cli=bin/omarchy-world-clock
skill_helper=skills/omarchy-world-clock/scripts/world-clock
[[ -x $backend ]]
[[ -x $agent_cli ]]
[[ -x $skill_helper ]]
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
  "version": 8,
  "timezones": [{ "id": 1, "timezone": "UTC", "place": "Probe" }]
}
JSON
local_timezone=$(
  HOME="$bundle_sandbox/home" \
  OMARCHY_WORLD_CLOCK_CONFIG="$bundle_sandbox/probe.json" \
    "$backend" snapshot --at 2026-08-11T11:05:00Z | jq -er '.local_timezone'
)
jq -n --arg local "$local_timezone" '
  def location($id; $timezone; $place): {id: $id, timezone: $timezone, place: $place};
  [
    location(2; "America/Los_Angeles"; "Los Angeles"),
    location(3; "America/Denver"; "Denver"),
    location(4; "America/Chicago"; "Chicago"),
    location(5; "America/New_York"; "New York"),
    location(6; "Europe/London"; "London"),
    location(7; "Europe/Paris"; "Paris"),
    location(8; "Europe/Athens"; "Athens"),
    location(9; "Asia/Kolkata"; "Delhi"),
    location(10; "Asia/Tokyo"; "Tokyo"),
    location(11; "Australia/Sydney"; "Sydney"),
    location(12; "Pacific/Auckland"; "Auckland")
  ] | map(select(.timezone != $local)) as $remote |
  {
    add_timezone: $remote[9].timezone,
    config: {
      version: 8,
      timezones: ([location(1; $local; "Home")] + $remote[0:9])
    }
  }
' >"$bundle_sandbox/plan.json"
jq '.config' "$bundle_sandbox/plan.json" >"$bundle_sandbox/config.json"
add_timezone=$(jq -er '.add_timezone' "$bundle_sandbox/plan.json")
HOME="$bundle_sandbox/home" \
OMARCHY_WORLD_CLOCK_CONFIG="$bundle_sandbox/config.json" \
  "$backend" add "$add_timezone" --place-label "Tenth remote place"
jq -e --arg timezone "$add_timezone" \
  '(.timezones | length) == 11 and any(.timezones[]; .timezone == $timezone)' \
  "$bundle_sandbox/config.json" >/dev/null

HOME="$bundle_sandbox/home" \
OMARCHY_WORLD_CLOCK_CONFIG="$bundle_sandbox/config.json" \
  "$agent_cli" places --at 2026-08-11T11:05:00Z |
  jq -e '
    .api_version == 1 and
    .reference_utc == "2026-08-11T11:05:00Z" and
    any(.locations[]; .place == "Home")
  ' >/dev/null

HOME="$bundle_sandbox/home" \
OMARCHY_WORLD_CLOCK_CONFIG="$bundle_sandbox/config.json" \
  "$skill_helper" time --location Home --at 2026-08-11T11:05:00Z |
  jq -e '.api_version == 1 and .location.place == "Home"' >/dev/null

printf 'bundle tests passed\n'
