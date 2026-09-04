#!/usr/bin/env bash
set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
sandbox=$(mktemp -d)
trap 'rm -rf -- "$sandbox"' EXIT

stub_dir="$sandbox/stubs"
mkdir -p "$stub_dir"
cat >"$stub_dir/omarchy" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ $* == "menu keybindings --print" ]]; then
  printf '%s' "${TEST_KEYBINDINGS:-}"
  exit 0
fi
if [[ $* == "plugin list --json" ]]; then
  printf '%s\n' '[{"id":"io.github.olivoil.world-clock","enabled":true}]'
  exit 0
fi
exit 1
EOF
cat >"$stub_dir/hyprctl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
version) printf '%s\n' 'Hyprland test' ;;
configerrors)
  if [[ -n ${TEST_HYPR_RELOAD_MARKER:-} && -e $TEST_HYPR_RELOAD_MARKER ]]; then
    printf '%s' "${TEST_HYPR_ERRORS_AFTER_RELOAD:-}"
  else
    printf '%s' "${TEST_HYPR_ERRORS:-}"
  fi
  ;;
reload)
  if [[ -n ${TEST_HYPR_RELOAD_MARKER:-} ]]; then
    : >"$TEST_HYPR_RELOAD_MARKER"
  fi
  exit 0
  ;;
*) exit 1 ;;
esac
EOF
chmod 0755 "$stub_dir/omarchy" "$stub_dir/hyprctl"

home="$sandbox/home"
config_home="$home/config"
mkdir -p "$config_home/hypr"
cat >"$config_home/hypr/bindings.lua" <<'EOF'
-- Existing personal binding.
o.bind("SUPER + SHIFT + R", "SSH", "ssh example.test")
EOF

run_installer() {
  HOME="$home" XDG_CONFIG_HOME="$config_home" PATH="$stub_dir:$PATH" \
    "$repo_root/scripts/install-integrations.sh" "$@"
}

before_status_checksum=$(sha256sum "$config_home/hypr/bindings.lua")
run_installer --status |
  jq -e '
    .status_version == 1 and
    .supported == true and
    .installed == false and
    .shortcut_installed == false and
    .agent_installed == false and
    .default_shortcut == "SUPER + SHIFT + T"
  ' >/dev/null
[[ $(sha256sum "$config_home/hypr/bindings.lua") == "$before_status_checksum" ]]

run_installer >/dev/null
TEST_KEYBINDINGS='SUPER SHIFT + T                     → World Clock
' run_installer >/dev/null

run_installer --status |
  jq -e '
    .supported == true and
    .installed == true and
    .shortcut_installed == true and
    .agent_installed == true and
    .cli_installed == true and
    .skill_installed == true and
    .skill_links_installed == 6 and
    .skill_links_total == 6
  ' >/dev/null

bindings="$config_home/hypr/bindings.lua"
[[ $(grep -Fc -- '-- BEGIN omarchy-world-clock integration (managed)' "$bindings") == 1 ]]
[[ $(grep -Fc -- '-- END omarchy-world-clock integration (managed)' "$bindings") == 1 ]]
grep -Fq 'o.bind("SUPER + SHIFT + T", "World Clock", "omarchy-shell shell toggle io.github.olivoil.world-clock")' "$bindings"
grep -Fq 'o.bind("SUPER + SHIFT + R", "SSH", "ssh example.test")' "$bindings"

[[ $(readlink -f "$home/.local/bin/omarchy-world-clock") == "$repo_root/bin/omarchy-world-clock" ]]
for destination in \
  "$home/.agents/skills/omarchy-world-clock" \
  "$home/.claude/skills/omarchy-world-clock" \
  "$home/.codex/skills/omarchy-world-clock" \
  "$home/.pi/agent/skills/omarchy-world-clock" \
  "$home/.gemini/config/skills/omarchy-world-clock" \
  "$home/.hermes/skills/omarchy-world-clock"; do
  [[ $(readlink -f "$destination") == "$repo_root/skills/omarchy-world-clock" ]]
done

TEST_KEYBINDINGS= run_installer --shortcut 'SUPER + ALT + W' >/dev/null
grep -Fq 'o.bind("SUPER + ALT + W", "World Clock", "omarchy-shell shell toggle io.github.olivoil.world-clock")' "$bindings"
run_installer --status |
  jq -e '.installed == true and .shortcut_installed == true' >/dev/null

run_installer --remove >/dev/null
! grep -Fq -- '-- BEGIN omarchy-world-clock integration (managed)' "$bindings"
grep -Fq 'o.bind("SUPER + SHIFT + R", "SSH", "ssh example.test")' "$bindings"
[[ ! -e $home/.local/bin/omarchy-world-clock ]]
[[ ! -e $home/.codex/skills/omarchy-world-clock ]]
run_installer --status |
  jq -e '.installed == false and .shortcut_installed == false and .agent_installed == false' \
  >/dev/null

review_root="$sandbox/review-plugin"
mkdir -p "$review_root/scripts"
cp "$repo_root/scripts/install-integrations.sh" "$review_root/scripts/"
cat >"$review_root/manifest.json" <<'JSON'
{"id":"io.github.olivoil.world-clock-review-test-1234567890"}
JSON
HOME="$sandbox/review-home" XDG_CONFIG_HOME="$sandbox/review-config" \
  PATH="$stub_dir:$PATH" "$review_root/scripts/install-integrations.sh" --status |
  jq -e '
    .status_version == 1 and
    .supported == false and
    .reason == "review_build" and
    .installed == false
  ' >/dev/null
if HOME="$sandbox/review-home" XDG_CONFIG_HOME="$sandbox/review-config" \
  PATH="$stub_dir:$PATH" "$review_root/scripts/install-integrations.sh" \
  >"$sandbox/review.out" 2>"$sandbox/review.err"; then
  printf '%s\n' 'installer unexpectedly managed integrations from a review build' >&2
  exit 1
fi
grep -Fq 'Refusing to manage persistent integrations from an isolated review build' \
  "$sandbox/review.err"

collision_home="$sandbox/collision-home"
collision_config="$collision_home/config"
mkdir -p "$collision_config/hypr"
printf '%s\n' \
  'o.bind("SUPER + SHIFT + T", "World Clock", "omarchy-shell shell toggle io.github.olivoil.world-clock")' \
  >"$collision_config/hypr/bindings.lua"
if HOME="$collision_home" XDG_CONFIG_HOME="$collision_config" \
  TEST_KEYBINDINGS='SUPER SHIFT + T                     → Existing action
' \
  PATH="$stub_dir:$PATH" "$repo_root/scripts/install-integrations.sh" --no-agent \
  >"$sandbox/collision.out" 2>"$sandbox/collision.err"; then
  printf '%s\n' 'installer unexpectedly replaced an occupied shortcut' >&2
  exit 1
fi
grep -Fq 'already in use' "$sandbox/collision.err"
[[ $(cat "$collision_config/hypr/bindings.lua") == \
  'o.bind("SUPER + SHIFT + T", "World Clock", "omarchy-shell shell toggle io.github.olivoil.world-clock")' ]]

unrelated_home="$sandbox/unrelated-home"
mkdir -p "$unrelated_home/.local/bin"
ln -s /tmp/not-world-clock "$unrelated_home/.local/bin/omarchy-world-clock"
if HOME="$unrelated_home" XDG_CONFIG_HOME="$unrelated_home/config" \
  PATH="$stub_dir:$PATH" "$repo_root/scripts/install-integrations.sh" --no-shortcut \
  >"$sandbox/unrelated.out" 2>"$sandbox/unrelated.err"; then
  printf '%s\n' 'installer unexpectedly replaced an unrelated CLI link' >&2
  exit 1
fi
grep -Fq 'Refusing to replace existing command' "$sandbox/unrelated.err"
[[ $(readlink "$unrelated_home/.local/bin/omarchy-world-clock") == /tmp/not-world-clock ]]

generic_skill_home="$sandbox/generic-skill-home"
mkdir -p "$generic_skill_home/.agents/skills"
ln -s /tmp/not-world-clock-skill \
  "$generic_skill_home/.agents/skills/omarchy-world-clock"
if HOME="$generic_skill_home" XDG_CONFIG_HOME="$generic_skill_home/config" \
  PATH="$stub_dir:$PATH" "$repo_root/scripts/install-integrations.sh" --no-shortcut \
  >"$sandbox/generic-skill.out" 2>"$sandbox/generic-skill.err"; then
  printf '%s\n' 'installer unexpectedly replaced an unrelated generic skill' >&2
  exit 1
fi
grep -Fq 'Refusing to replace existing generic skill' "$sandbox/generic-skill.err"
[[ $(readlink "$generic_skill_home/.agents/skills/omarchy-world-clock") == \
  /tmp/not-world-clock-skill ]]

skill_home="$sandbox/skill-home"
mkdir -p "$skill_home/.codex/skills"
ln -s /tmp/not-world-clock-skill "$skill_home/.codex/skills/omarchy-world-clock"
HOME="$skill_home" XDG_CONFIG_HOME="$skill_home/config" \
  TEST_HYPR_ERRORS='pre-existing error ignored for agent-only setup' \
  PATH="$stub_dir:$PATH" "$repo_root/scripts/install-integrations.sh" --no-shortcut \
  >"$sandbox/skill.out" 2>"$sandbox/skill.err"
grep -Fq 'Skipped unrelated skill path' "$sandbox/skill.err"
[[ $(readlink "$skill_home/.codex/skills/omarchy-world-clock") == /tmp/not-world-clock-skill ]]
[[ $(readlink -f "$skill_home/.local/bin/omarchy-world-clock") == \
  "$repo_root/bin/omarchy-world-clock" ]]

rollback_home="$sandbox/rollback-home"
rollback_config="$rollback_home/config"
rollback_marker="$sandbox/reloaded"
mkdir -p "$rollback_config/hypr"
printf '%s\n' '-- rollback original' >"$rollback_config/hypr/bindings.lua"
if HOME="$rollback_home" XDG_CONFIG_HOME="$rollback_config" \
  TEST_HYPR_RELOAD_MARKER="$rollback_marker" \
  TEST_HYPR_ERRORS_AFTER_RELOAD='test error after reload' \
  PATH="$stub_dir:$PATH" "$repo_root/scripts/install-integrations.sh" --no-agent \
  >"$sandbox/rollback.out" 2>"$sandbox/rollback.err"; then
  printf '%s\n' 'installer unexpectedly retained an invalid Hyprland change' >&2
  exit 1
fi
grep -Fq 'restoring the previous bindings file' "$sandbox/rollback.err"
[[ $(cat "$rollback_config/hypr/bindings.lua") == '-- rollback original' ]]

printf '%s\n' 'integration installer tests passed'
