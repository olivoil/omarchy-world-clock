#!/usr/bin/env bash
set -euo pipefail

test_root=$(mktemp -d)
cleanup() {
  rm -rf -- "$test_root"
}
trap cleanup EXIT

repo="$test_root/repo"
mock_bin="$test_root/bin"
config_home="$test_root/config"
state_home="$test_root/state"
log_file="$test_root/calls.log"
mkdir -p "$repo/scripts" "$repo/quattro" "$repo/bin" "$mock_bin"

cp -- scripts/install-review-preview.sh "$repo/scripts/install-review-preview.sh"
printf '%s\n' '{"id":"base","name":"base","description":"","entryPoints":{"barWidget":"quattro/WorldClock.qml"},"barWidget":{"displayName":"base","description":""}}' >"$repo/manifest.json"
printf '%s\n' 'WorldClock {}' >"$repo/quattro/WorldClock.qml"
printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$repo/bin/omarchy-world-clock-backend"
chmod 0755 "$repo/bin/omarchy-world-clock-backend"

git -C "$repo" init -q -b feature/rollback

cat >"$mock_bin/omarchy" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
printf 'omarchy %s\n' "$*" >>"$TEST_LOG"
case "${1:-} ${2:-}" in
  'plugin validate') exit 0 ;;
  'plugin list') printf '%s\n' '[]' ;;
  'plugin enable') exit 42 ;;
  *) exit 2 ;;
esac
MOCK
chmod 0755 "$mock_bin/omarchy"

cat >"$mock_bin/omarchy-shell" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
printf 'omarchy-shell %s\n' "$*" >>"$TEST_LOG"
exit 0
MOCK
chmod 0755 "$mock_bin/omarchy-shell"

branch=feature/rollback
normalized_slug=feature-rollback
branch_hash=$(printf '%s' "$branch" | git hash-object --stdin)
slug="${normalized_slug}-${branch_hash:0:10}"
plugin_id="io.github.olivoil.world-clock-review-${slug}"
destination="$config_home/omarchy/plugins/$plugin_id"
mkdir -p "$destination"
printf '%s\n' 'last working preview' >"$destination/marker"

if (
  cd "$repo"
  PATH="$mock_bin:$PATH" \
    XDG_CONFIG_HOME="$config_home" \
    XDG_STATE_HOME="$state_home" \
    TEST_LOG="$log_file" \
    bash scripts/install-review-preview.sh >"$test_root/stdout" 2>"$test_root/stderr"
); then
  printf '%s\n' 'installer unexpectedly succeeded when plugin enable failed' >&2
  exit 1
fi

[[ $(<"$destination/marker") == 'last working preview' ]]
[[ ! -e $destination/quattro/ReviewWorldClock.qml ]]
[[ $(grep -c '^omarchy-shell shell rescanPlugins$' "$log_file") -eq 2 ]]
grep -F "omarchy plugin enable $plugin_id --section center" "$log_file" >/dev/null
grep -F 'Could not enable the plugin; restored the previous review copy.' "$test_root/stderr" >/dev/null

staging_root="$config_home/omarchy/.plugin-review-installs"
if [[ -d $staging_root ]] && find "$staging_root" -mindepth 1 -print -quit | grep -q .; then
  printf '%s\n' 'installer left temporary review files behind' >&2
  exit 1
fi

printf '%s\n' 'Review installer rollback test passed.'
