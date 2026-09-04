#!/usr/bin/env bash
set -euo pipefail

test_root=$(mktemp -d)
cleanup() {
  rm -rf -- "$test_root"
}
trap cleanup EXIT

repo="$test_root/repo"
live_worktree="$test_root/live-worktree"
mock_bin="$test_root/bin"
proc_root="$test_root/proc"
config_home="$test_root/config"
state_home="$test_root/state"
log_file="$test_root/calls.log"
mkdir -p "$repo/scripts" "$repo/quattro" "$repo/bin" "$mock_bin" "$proc_root/123"

cp -- scripts/install-review-preview.sh "$repo/scripts/install-review-preview.sh"
printf '%s\n' '{"id":"base","name":"base","description":"","entryPoints":{"barWidget":"quattro/WorldClock.qml"},"barWidget":{"displayName":"base","description":""}}' >"$repo/manifest.json"
printf '%s\n' 'WorldClock {}' >"$repo/quattro/WorldClock.qml"
printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$repo/bin/omarchy-world-clock-backend"
chmod 0755 "$repo/bin/omarchy-world-clock-backend"

git -C "$repo" init -q -b feature/current
git -C "$repo" config user.name 'Review Test'
git -C "$repo" config user.email 'review-test@example.com'
git -C "$repo" add .
git -C "$repo" commit -qm initial
git -C "$repo" branch feature/live
git -C "$repo" worktree add -q "$live_worktree" feature/live
mkdir -p "$live_worktree/session"
ln -s "$live_worktree/session" "$proc_root/123/cwd"

current_branch=feature/current
current_slug=feature-current
current_hash=$(printf '%s' "$current_branch" | git hash-object --stdin)
current_id="io.github.olivoil.world-clock-review-${current_slug}-${current_hash:0:10}"
live_id=io.github.olivoil.world-clock-review-feature-live-1111111111
inactive_id=io.github.olivoil.world-clock-review-feature-inactive-2222222222
plugins_dir="$config_home/omarchy/plugins"
mkdir -p "$plugins_dir/$live_id" "$plugins_dir/$inactive_id"
printf '%s\n' \
  "{\"id\":\"$live_id\",\"description\":\"Local review build for branch feature/live.\"}" \
  >"$plugins_dir/$live_id/manifest.json"
printf '%s\n' \
  "{\"id\":\"$inactive_id\",\"description\":\"Local review build for branch feature/inactive.\"}" \
  >"$plugins_dir/$inactive_id/manifest.json"

cat >"$mock_bin/omarchy" <<MOCK
#!/usr/bin/env bash
set -euo pipefail
printf 'omarchy %s\\n' "\$*" >>"\$TEST_LOG"
case "\${1:-} \${2:-}" in
  'plugin validate') exit 0 ;;
  'plugin list')
    printf '%s\\n' '[
      {"id":"io.github.olivoil.world-clock","enabled":true},
      {"id":"$current_id","enabled":true},
      {"id":"$live_id","enabled":true},
      {"id":"$inactive_id","enabled":true}
    ]'
    ;;
  'plugin enable') exit 0 ;;
  'plugin remove') rm -rf -- "\$XDG_CONFIG_HOME/omarchy/plugins/\$3" ;;
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

(
  cd "$repo"
  PATH="$mock_bin:$PATH" \
    XDG_CONFIG_HOME="$config_home" \
    XDG_STATE_HOME="$state_home" \
    OMARCHY_REVIEW_PROC_ROOT="$proc_root" \
    TEST_LOG="$log_file" \
    bash scripts/install-review-preview.sh --prune-other-reviews \
      >"$test_root/stdout" 2>"$test_root/stderr"
)

[[ -d $plugins_dir/$current_id ]]
[[ -d $plugins_dir/$live_id ]]
[[ ! -e $plugins_dir/$inactive_id ]]
grep -F "omarchy plugin remove $inactive_id --yes" "$log_file" >/dev/null
if grep -F "omarchy plugin remove $live_id --yes" "$log_file" >/dev/null; then
  printf '%s\n' 'installer removed a review associated with live work' >&2
  exit 1
fi
grep -F "Retained io.github.olivoil.world-clock (production)" "$test_root/stdout" >/dev/null
grep -F "Retained $current_id (current branch)" "$test_root/stdout" >/dev/null
grep -F "Retained $live_id (live branch: feature/live)" "$test_root/stdout" >/dev/null
grep -F "Removed $inactive_id" "$test_root/stdout" >/dev/null

printf '%s\n' 'Review installer pruning test passed.'
