#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf '%s\n' \
    'Usage: scripts/install-review-preview.sh [--after <plugin-id>] [--prune-other-reviews]'
}

after_id=io.github.olivoil.world-clock
prune_other_reviews=false
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
  --prune-other-reviews)
    prune_other_reviews=true
    shift
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
# Leave room within Linux NAME_MAX for the plugin prefix, hash suffix, and
# temporary-directory suffix. Normalization produces ASCII, so this character
# bound is also a byte bound.
normalized_slug=${normalized_slug:0:180}
branch_hash=$(printf '%s' "$branch" | git hash-object --stdin)
slug="${normalized_slug}-${branch_hash:0:10}"

plugin_id="io.github.olivoil.world-clock-review-${slug}"
canonical_id=io.github.olivoil.world-clock
review_prefix="${canonical_id}-review-"
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
restore_previous() {
  rm -rf -- "$destination"
  if [[ $had_previous == true ]]; then
    mv -- "$previous" "$destination"
  fi
}
rollback_preview() {
  restore_previous
  if ! omarchy-shell shell rescanPlugins >/dev/null; then
    printf '%s\n' 'Warning: could not rescan plugins after restoring the previous review copy.' >&2
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
  rollback_preview
  printf '%s\n' 'Could not rescan plugins; restored the previous review copy.' >&2
  exit 1
fi

if omarchy plugin list --json | jq -e --arg id "$after_id" \
  'any(.[]; .id == $id and .enabled == true)' >/dev/null; then
  placement=(--after "$after_id")
else
  placement=(--section center)
fi
if ! omarchy plugin enable "$plugin_id" "${placement[@]}"; then
  rollback_preview
  printf '%s\n' 'Could not enable the plugin; restored the previous review copy.' >&2
  exit 1
fi

printf 'Installed %s\n' "$plugin_id"
printf 'Review branch: %s\n' "$branch"
printf 'Review config: %s\n' "$review_config"

if [[ $prune_other_reviews == true ]]; then
  proc_root=${OMARCHY_REVIEW_PROC_ROOT:-/proc}

  branch_for_review() {
    local review_id=$1
    local review_manifest="$plugins_dir/$review_id/manifest.json"
    [[ -f $review_manifest ]] || return 1
    jq -er \
      '.description
       | capture("^Local review build for branch (?<branch>.+)\\.$")
       | .branch' \
      "$review_manifest"
  }

  live_process_branches=()
  collect_live_process_branches() {
    local process_dir process_cwd process_branch known_branch
    local -A seen_process_cwds=()
    for process_dir in "$proc_root"/[0-9]*; do
      [[ -L $process_dir/cwd || -e $process_dir/cwd ]] || continue
      process_cwd=$(readlink "$process_dir/cwd" 2>/dev/null) || continue
      [[ -d $process_cwd ]] || continue
      [[ -z ${seen_process_cwds[$process_cwd]+present} ]] || continue
      seen_process_cwds[$process_cwd]=true
      process_branch=$(git -C "$process_cwd" branch --show-current 2>/dev/null) \
        || continue
      [[ -n $process_branch ]] || continue
      for known_branch in "${live_process_branches[@]}"; do
        [[ $known_branch != "$process_branch" ]] || continue 2
      done
      live_process_branches+=("$process_branch")
    done
  }

  branch_is_live() {
    local wanted_branch=$1
    local live_branch
    for live_branch in "${live_process_branches[@]}"; do
      [[ $live_branch != "$wanted_branch" ]] || return 0
    done
    return 1
  }

  collect_live_process_branches

  printf 'Retained %s (production)\n' "$canonical_id"
  printf 'Retained %s (current branch)\n' "$plugin_id"

  prune_failed=false
  mapfile -t review_ids < <(
    omarchy plugin list --json |
      jq -r --arg prefix "$review_prefix" \
        '.[] | select(.id | startswith($prefix)) | .id'
  )
  for review_id in "${review_ids[@]}"; do
    [[ $review_id != "$plugin_id" ]] || continue

    if ! review_branch=$(branch_for_review "$review_id"); then
      printf 'Retained %s (could not verify its branch)\n' "$review_id"
      continue
    fi
    if branch_is_live "$review_branch"; then
      printf 'Retained %s (live branch: %s)\n' "$review_id" "$review_branch"
      continue
    fi

    if omarchy plugin remove "$review_id" --yes; then
      printf 'Removed %s\n' "$review_id"
    else
      printf 'Could not remove %s\n' "$review_id" >&2
      prune_failed=true
    fi
  done

  [[ $prune_failed == false ]] || exit 1
fi
