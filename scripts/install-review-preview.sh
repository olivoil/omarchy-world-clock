#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf 'Usage: scripts/install-review-preview.sh [--after <plugin-id>]\n'
}

after_id=io.github.olivoil.world-clock
while (( $# > 0 )); do
  case "$1" in
  --after)
    [[ -n ${2:-} ]] || {
      printf '%s\n' '--after requires a plugin ID' >&2
      exit 2
    }
    after_id=$2
    shift 2
    ;;
  -h | --help)
    usage
    exit 0
    ;;
  *)
    usage >&2
    exit 2
    ;;
  esac
done

for command_name in git jq omarchy omarchy-shell rsync; do
  command -v "$command_name" >/dev/null 2>&1 || {
    printf 'Missing required command: %s\n' "$command_name" >&2
    exit 1
  }
done

repo_root=$(git rev-parse --show-toplevel)
branch=$(git -C "$repo_root" branch --show-current)
[[ -n $branch ]] || {
  printf '%s\n' 'A named Git branch is required for a review install.' >&2
  exit 1
}

normalized_slug=$(printf '%s' "$branch" |
  tr '[:upper:]/_' '[:lower:]--' |
  sed -E 's/[^a-z0-9.-]+/-/g; s/^-+//; s/-+$//; s/-+/-/g')
[[ -n $normalized_slug ]] || {
  printf 'Could not derive a review ID from branch: %s\n' "$branch" >&2
  exit 1
}
branch_hash=$(printf '%s' "$branch" | git hash-object --stdin)
slug="${normalized_slug}-${branch_hash:0:10}"

plugin_id="io.github.olivoil.world-clock-review-${slug}"
branch_leaf=${branch##*/}
display_name="World Clock · ${branch_leaf}"
config_home=${XDG_CONFIG_HOME:-"$HOME/.config"}
state_home=${XDG_STATE_HOME:-"$HOME/.local/state"}
plugins_dir="$config_home/omarchy/plugins"
staging_root="$config_home/omarchy/.plugin-review-installs"
destination="$plugins_dir/$plugin_id"
review_config="$state_home/omarchy-world-clock/reviews/$slug/config.json"
canonical_config="$config_home/omarchy-world-clock/config.json"

mkdir -p "$plugins_dir" "$staging_root" "$(dirname "$review_config")"
if [[ ! -f $review_config && -f $canonical_config ]]; then
  cp -p -- "$canonical_config" "$review_config"
fi

work_dir=$(mktemp -d "$staging_root/${slug}.XXXXXX")
stage="$work_dir/stage"
previous="$work_dir/previous"
had_previous=false
mkdir -- "$stage"
cleanup() {
  if [[ -d $work_dir ]]; then
    rm -rf -- "$work_dir"
  fi
}
trap cleanup EXIT

rsync -a \
  --exclude .git \
  --exclude target \
  --exclude '.t3' \
  "$repo_root/" "$stage/"

jq \
  --arg id "$plugin_id" \
  --arg name "$display_name" \
  --arg branch "$branch" \
  '.id = $id
   | .name = $name
   | .description = ("Local review build for branch " + $branch + ".")
   | .entryPoints.barWidget = "quattro/ReviewWorldClock.qml"
   | .barWidget.displayName = $name
   | .barWidget.description = ("Local review build for branch " + $branch + ".")' \
  "$stage/manifest.json" >"$stage/manifest.json.next"
mv -- "$stage/manifest.json.next" "$stage/manifest.json"

plugin_id_json=$(jq -Rn --arg value "$plugin_id" '$value')
branch_label_json=$(jq -Rn --arg value "Review branch: $branch" '$value')
{
  printf '%s\n' 'pragma ComponentBehavior: Bound'
  printf '\n%s\n' 'import QtQuick'
  printf '%s\n' 'import qs.Commons'
  printf '\n%s\n' 'WorldClock {'
  printf '  moduleName: %s\n' "$plugin_id_json"
  printf '  buildLabel: %s\n' "$branch_label_json"
  printf '%s\n' '  iconForeground: Color.urgent'
  printf '%s\n' '  backendExecutableName: "omarchy-world-clock-review-backend"'
  printf '%s\n' '}'
} >"$stage/quattro/ReviewWorldClock.qml"

{
  printf '%s\n' '#!/usr/bin/env bash'
  printf '%s\n' 'set -euo pipefail'
  printf '%s\n' 'review_state_home=${XDG_STATE_HOME:-"$HOME/.local/state"}'
  printf 'export OMARCHY_WORLD_CLOCK_CONFIG="$review_state_home/omarchy-world-clock/reviews/%s/config.json"\n' "$slug"
  printf '%s\n' 'exec "$(dirname "$0")/omarchy-world-clock-backend" "$@"'
} >"$stage/bin/omarchy-world-clock-review-backend"
chmod 0755 "$stage/bin/omarchy-world-clock-review-backend"

omarchy plugin validate "$stage"

if [[ -e $destination || -L $destination ]]; then
  mv -- "$destination" "$previous"
  had_previous=true
fi
mv -- "$stage" "$destination"

if ! omarchy-shell shell rescanPlugins >/dev/null; then
  rm -rf -- "$destination"
  if [[ $had_previous == true ]]; then mv -- "$previous" "$destination"; fi
  printf '%s\n' 'Could not rescan plugins; restored the previous review copy.' >&2
  exit 1
fi

if omarchy plugin list --json | jq -e --arg id "$after_id" \
  'any(.[]; .id == $id and .enabled == true)' >/dev/null; then
  omarchy plugin enable "$plugin_id" --after "$after_id"
else
  omarchy plugin enable "$plugin_id" --section center
fi

printf 'Installed %s\n' "$plugin_id"
printf 'Review branch: %s\n' "$branch"
printf 'Review config: %s\n' "$review_config"
