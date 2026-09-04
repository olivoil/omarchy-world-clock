#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/install-integrations.sh [options]

Install the optional desktop and agent integrations shipped with World Clock.

Options:
  --shortcut <chord>  Hyprland shortcut (default: SUPER + SHIFT + T)
  --no-shortcut       Do not add the Hyprland shortcut
  --no-agent          Do not link the CLI and agent skill
  --status            Print integration status as JSON without changing files
  --remove            Remove integrations owned by this plugin checkout
  -h, --help          Show this help
EOF
}

shortcut="SUPER + SHIFT + T"
install_shortcut=true
install_agent=true
mode=install
mode_option_count=0
while (( $# > 0 )); do
  case "$1" in
  --shortcut)
    [[ -n ${2:-} ]] || {
      printf '%s\n' '--shortcut requires a Hyprland chord' >&2
      exit 2
    }
    shortcut=$2
    shift 2
    ;;
  --no-shortcut)
    install_shortcut=false
    shift
    ;;
  --no-agent)
    install_agent=false
    shift
    ;;
  --remove)
    mode=remove
    mode_option_count=$((mode_option_count + 1))
    shift
    ;;
  --status)
    mode=status
    mode_option_count=$((mode_option_count + 1))
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
if (( mode_option_count > 1 )); then
  printf '%s\n' '--status and --remove cannot be combined' >&2
  exit 2
fi

plugin_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
manifest="$plugin_root/manifest.json"
skill_source="$plugin_root/skills/omarchy-world-clock"
cli_source="$plugin_root/bin/omarchy-world-clock"

for command_name in awk grep jq mktemp; do
  command -v "$command_name" >/dev/null 2>&1 || {
    printf 'Missing required command: %s\n' "$command_name" >&2
    exit 1
  }
done
[[ -f $manifest ]] || {
  printf 'Could not find the World Clock manifest at %s\n' "$manifest" >&2
  exit 1
}
plugin_id=$(jq -er '.id | select(type == "string" and length > 0)' "$manifest")

config_home=${XDG_CONFIG_HOME:-"$HOME/.config"}
bindings_file="$config_home/hypr/bindings.lua"
cli_destination="$HOME/.local/bin/omarchy-world-clock"
block_begin="-- BEGIN omarchy-world-clock integration (managed)"
block_end="-- END omarchy-world-clock integration (managed)"

skill_destinations=(
  "$HOME/.agents/skills/omarchy-world-clock"
  "$HOME/.claude/skills/omarchy-world-clock"
  "$HOME/.codex/skills/omarchy-world-clock"
  "$HOME/.pi/agent/skills/omarchy-world-clock"
  "$HOME/.gemini/config/skills/omarchy-world-clock"
  "$HOME/.hermes/skills/omarchy-world-clock"
)
if [[ -d $HOME/.hermes/profiles ]]; then
  while IFS= read -r -d '' profile; do
    skill_destinations+=("$profile/skills/omarchy-world-clock")
  done < <(find "$HOME/.hermes/profiles" -mindepth 1 -maxdepth 1 -type d -print0)
fi

same_link() {
  local destination=$1 source=$2 target
  [[ -L $destination ]] || return 1
  target=$(readlink -f -- "$destination" 2>/dev/null || true)
  [[ $target == "$source" ]]
}

remove_owned_link() {
  local destination=$1 source=$2
  if same_link "$destination" "$source"; then
    rm -- "$destination"
    printf 'Removed link %s\n' "$destination"
  elif [[ -e $destination || -L $destination ]]; then
    printf 'Retained unrelated path %s\n' "$destination"
  fi
}

strip_managed_block() {
  local source=$1 destination=$2
  awk -v begin="$block_begin" -v end="$block_end" '
    $0 == begin { managed = 1; next }
    $0 == end { managed = 0; next }
    !managed { print }
  ' "$source" >"$destination"
}

managed_block_count() {
  local marker=$1
  [[ -f $bindings_file ]] || {
    printf '0\n'
    return
  }
  grep -Fxc -- "$marker" "$bindings_file" || true
}

managed_shortcut_installed() {
  local begin_count end_count command_json managed_binding
  begin_count=$(managed_block_count "$block_begin")
  end_count=$(managed_block_count "$block_end")
  [[ $begin_count == 1 && $end_count == 1 ]] || return 1
  command_json=$(jq -Rn --arg value "omarchy-shell shell toggle $plugin_id" '$value')
  managed_binding=$(
    awk -v begin="$block_begin" -v end="$block_end" '
      $0 == begin { managed = 1; next }
      $0 == end { managed = 0; next }
      managed { print }
    ' "$bindings_file"
  )
  grep -Fq -- "\"World Clock\", $command_json)" <<<"$managed_binding"
}

print_status() {
  local supported=true reason= shortcut_installed=false cli_installed=false
  local skill_installed=false agent_installed=false installed=false
  local skill_links_installed=0 destination

  if [[ $plugin_id == *-review-* ]]; then
    supported=false
    reason=review_build
  fi
  if managed_shortcut_installed; then
    shortcut_installed=true
  fi
  if same_link "$cli_destination" "$cli_source"; then
    cli_installed=true
  fi
  # ~/.agents/skills is Omarchy's generic skill location and is installed
  # alongside every agent-specific mirror. Treat it as the canonical signal
  # that whichever harness is selected as default can discover the skill.
  if same_link "${skill_destinations[0]}" "$skill_source"; then
    skill_installed=true
  fi
  for destination in "${skill_destinations[@]}"; do
    if same_link "$destination" "$skill_source"; then
      skill_links_installed=$((skill_links_installed + 1))
    fi
  done
  if [[ $cli_installed == true && $skill_installed == true ]]; then
    agent_installed=true
  fi
  if [[ $shortcut_installed == true && $agent_installed == true ]]; then
    installed=true
  fi

  jq -cn \
    --argjson supported "$supported" \
    --arg reason "$reason" \
    --argjson installed "$installed" \
    --argjson shortcut_installed "$shortcut_installed" \
    --argjson agent_installed "$agent_installed" \
    --argjson cli_installed "$cli_installed" \
    --argjson skill_installed "$skill_installed" \
    --argjson skill_links_installed "$skill_links_installed" \
    --argjson skill_links_total "${#skill_destinations[@]}" \
    --arg default_shortcut "$shortcut" \
    '{
      status_version: 1,
      supported: $supported,
      reason: (if $reason == "" then null else $reason end),
      installed: $installed,
      shortcut_installed: $shortcut_installed,
      agent_installed: $agent_installed,
      cli_installed: $cli_installed,
      skill_installed: $skill_installed,
      skill_links_installed: $skill_links_installed,
      skill_links_total: $skill_links_total,
      default_shortcut: $default_shortcut
    }'
}

if [[ $mode == status ]]; then
  print_status
  exit 0
fi

if [[ $plugin_id == *-review-* ]]; then
  printf '%s\n' 'Refusing to manage persistent integrations from an isolated review build.' >&2
  printf '%s\n' 'Run this script from the canonical io.github.olivoil.world-clock install instead.' >&2
  exit 1
fi

hyprland_available=false
existing_errors=
if [[ $install_shortcut == true ]] && command -v hyprctl >/dev/null 2>&1 &&
  hyprctl version >/dev/null 2>&1; then
  hyprland_available=true
  existing_errors=$(hyprctl configerrors 2>/dev/null || true)
  if [[ $mode != remove && -n ${existing_errors//[[:space:]]/} ]]; then
    printf '%s\n' 'Hyprland already reports configuration errors; fix them before installing a shortcut:' >&2
    printf '%s\n' "$existing_errors" >&2
    exit 1
  fi
fi

validate_hyprland_change() {
  local backup=$1 existed=$2 errors
  if [[ $hyprland_available != true ]]; then
    printf '%s\n' 'Hyprland is not reachable; the shortcut will load at the next graphical login.'
    return
  fi
  if ! hyprctl reload >/dev/null 2>&1; then
    printf '%s\n' 'Hyprland reload failed; restoring the previous bindings file.' >&2
  else
    errors=$(hyprctl configerrors 2>/dev/null || true)
    if [[ -z ${errors//[[:space:]]/} ]]; then
      printf '%s\n' 'Hyprland reloaded with no configuration errors.'
      return
    fi
    if [[ $mode == remove && $errors == "$existing_errors" ]]; then
      printf '%s\n' 'World Clock was removed; pre-existing Hyprland errors remain:' >&2
      printf '%s\n' "$errors" >&2
      return
    fi
    printf '%s\n' 'The shortcut change introduced a Hyprland configuration error; restoring the previous bindings file:' >&2
    printf '%s\n' "$errors" >&2
  fi

  if [[ $existed == true ]]; then
    cp -p -- "$backup" "$bindings_file"
  else
    rm -f -- "$bindings_file"
  fi
  hyprctl reload >/dev/null 2>&1 || true
  exit 1
}

remove_shortcut() {
  [[ -f $bindings_file ]] || return
  local begin_count end_count temp backup
  begin_count=$(managed_block_count "$block_begin")
  end_count=$(managed_block_count "$block_end")
  if [[ $begin_count == 0 && $end_count == 0 ]]; then
    return
  fi
  if [[ $begin_count != 1 || $end_count != 1 ]]; then
    printf 'Refusing to edit malformed managed block in %s\n' "$bindings_file" >&2
    exit 1
  fi
  temp=$(mktemp "${bindings_file}.XXXXXX")
  backup=$(mktemp "${bindings_file}.backup.$(date +%Y%m%d%H%M%S).XXXXXX")
  cp -p -- "$bindings_file" "$backup"
  strip_managed_block "$bindings_file" "$temp"
  chmod --reference="$bindings_file" "$temp"
  mv -- "$temp" "$bindings_file"
  validate_hyprland_change "$backup" true
  printf 'Removed World Clock shortcut from %s (backup: %s)\n' "$bindings_file" "$backup"
}

if [[ $mode == remove ]]; then
  if [[ $install_shortcut == true ]]; then
    remove_shortcut
  fi
  if [[ $install_agent == true ]]; then
    remove_owned_link "$cli_destination" "$cli_source"
    for destination in "${skill_destinations[@]}"; do
      remove_owned_link "$destination" "$skill_source"
    done
  fi
  printf '%s\n' 'World Clock integrations removed.'
  exit 0
fi

if [[ $install_agent == true ]]; then
  [[ -x $cli_source ]] || {
    printf 'Missing executable World Clock CLI: %s\n' "$cli_source" >&2
    exit 1
  }
  [[ -f $skill_source/SKILL.md ]] || {
    printf 'Missing World Clock agent skill: %s\n' "$skill_source/SKILL.md" >&2
    exit 1
  }
  if [[ ( -e $cli_destination || -L $cli_destination ) ]] &&
    ! same_link "$cli_destination" "$cli_source"; then
    printf 'Refusing to replace existing command: %s\n' "$cli_destination" >&2
    exit 1
  fi
  if [[ ( -e ${skill_destinations[0]} || -L ${skill_destinations[0]} ) ]] &&
    ! same_link "${skill_destinations[0]}" "$skill_source"; then
    printf 'Refusing to replace existing generic skill: %s\n' \
      "${skill_destinations[0]}" >&2
    exit 1
  fi
fi

if [[ $install_shortcut == true ]]; then
  command -v omarchy >/dev/null 2>&1 || {
    printf '%s\n' 'omarchy is required to check and install the keyboard shortcut.' >&2
    exit 1
  }
  begin_count=$(managed_block_count "$block_begin")
  end_count=$(managed_block_count "$block_end")
  if [[ $begin_count != "$end_count" || $begin_count -gt 1 ]]; then
    printf 'Refusing to edit malformed managed block in %s\n' "$bindings_file" >&2
    exit 1
  fi

  normalized_shortcut=$(printf '%s' "$shortcut" | tr '[:lower:]' '[:upper:]' | tr -d ' +\t')
  collisions=$(
    omarchy menu keybindings --print 2>/dev/null |
      awk -F '→' -v wanted="$normalized_shortcut" '
        function normalized(value) {
          gsub(/[ +\t]/, "", value)
          return toupper(value)
        }
        NF > 1 && normalized($1) == wanted { print }
      '
  )
  collision_count=$(grep -c . <<<"$collisions" || true)
  shortcut_json=$(jq -Rn --arg value "$shortcut" '$value')
  command_json=$(jq -Rn --arg value "omarchy-shell shell toggle $plugin_id" '$value')
  expected_binding="o.bind($shortcut_json, \"World Clock\", $command_json)"
  managed_same=false
  if [[ $begin_count == 1 ]]; then
    managed_binding=$(
      awk -v begin="$block_begin" -v end="$block_end" '
        $0 == begin { managed = 1; next }
        $0 == end { managed = 0; next }
        managed { print }
      ' "$bindings_file"
    )
    if grep -Fxq -- "$expected_binding" <<<"$managed_binding"; then
      managed_same=true
    fi
  fi
  if (( collision_count > 0 )) && { [[ $managed_same != true ]] || (( collision_count > 1 )); }; then
    printf 'Shortcut %s is already in use:\n%s\n' "$shortcut" "$collisions" >&2
    printf '%s\n' 'Choose another with --shortcut or rerun with --no-shortcut.' >&2
    exit 1
  fi

  bindings_dir=$(dirname -- "$bindings_file")
  mkdir -p -- "$bindings_dir"
  existed=false
  if [[ -f $bindings_file ]]; then
    existed=true
    bindings_source=$bindings_file
  else
    bindings_source=/dev/null
  fi
  backup=$(mktemp "${bindings_file}.backup.$(date +%Y%m%d%H%M%S).XXXXXX")
  if [[ $existed == true ]]; then
    cp -p -- "$bindings_file" "$backup"
  fi
  filtered=$(mktemp "${bindings_file}.filtered.XXXXXX")
  next=$(mktemp "${bindings_file}.next.XXXXXX")
  strip_managed_block "$bindings_source" "$filtered"
  {
    cat "$filtered"
    printf '\n%s\n' "$block_begin"
    printf 'o.bind(%s, "World Clock", %s)\n' "$shortcut_json" "$command_json"
    printf '%s\n' "$block_end"
  } >"$next"
  if [[ $existed == true ]]; then
    chmod --reference="$bindings_file" "$next"
  else
    chmod 0644 "$next"
  fi
  mv -- "$next" "$bindings_file"
  rm -f -- "$filtered"
  validate_hyprland_change "$backup" "$existed"
  printf 'Installed %s shortcut in %s (backup: %s)\n' "$shortcut" "$bindings_file" "$backup"
fi

if [[ $install_agent == true ]]; then
  mkdir -p -- "$(dirname -- "$cli_destination")"
  ln -sfn -- "$cli_source" "$cli_destination"
  printf 'Installed CLI %s\n' "$cli_destination"
  for destination in "${skill_destinations[@]}"; do
    if [[ ( -e $destination || -L $destination ) ]] &&
      ! same_link "$destination" "$skill_source"; then
      printf 'Skipped unrelated skill path %s\n' "$destination" >&2
      continue
    fi
    mkdir -p -- "$(dirname -- "$destination")"
    ln -sfn -- "$skill_source" "$destination"
    printf 'Installed skill %s\n' "$destination"
  done
fi

printf '%s\n' 'World Clock integrations installed. Start a new agent session to load the skill.'
