#!/usr/bin/env bash
set -euo pipefail

test_root=$(mktemp -d)
cleanup() {
  rm -rf -- "$test_root"
}
trap cleanup EXIT

repo="$test_root/repo"
mock_bin="$test_root/bin"
log_file="$test_root/calls.log"
mkdir -p "$repo/scripts" "$mock_bin"
cp -- scripts/signoff.sh "$repo/scripts/signoff.sh"

printf '%s\n' '#!/usr/bin/env bash' \
  'printf "%s\n" ci >>"$TEST_LOG"' \
  >"$repo/scripts/ci.sh"
chmod 0755 "$repo/scripts/"*.sh

git -C "$repo" init -q -b feature/signoff
git -C "$repo" config user.name 'Signoff Test'
git -C "$repo" config user.email 'signoff-test@example.com'
printf '%s\n' 'committed' >"$repo/tracked"
git -C "$repo" add .
git -C "$repo" commit -qm initial

cat >"$mock_bin/gh" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
if [[ ${1:-} == signoff && ${2:-} == --help ]]; then
  exit 0
fi
printf 'gh %s\n' "$*" >>"$TEST_LOG"
MOCK
chmod 0755 "$mock_bin/gh"

printf '%s\n' 'dirty' >"$repo/untracked"
if (
  cd "$repo"
  PATH="$mock_bin:$PATH" TEST_LOG="$log_file" \
    bash scripts/signoff.sh tests >"$test_root/stdout" 2>"$test_root/stderr"
); then
  printf '%s\n' 'signoff unexpectedly succeeded with an unclean worktree' >&2
  exit 1
fi
grep -F \
  'Cannot sign off: commit or discard all staged, unstaged, and untracked changes first.' \
  "$test_root/stderr" >/dev/null
[[ ! -s $log_file ]]

rm -- "$repo/untracked"
(
  cd "$repo"
  PATH="$mock_bin:$PATH" TEST_LOG="$log_file" \
    bash scripts/signoff.sh tests lint
)

cat >"$test_root/expected" <<'EXPECTED'
ci
gh signoff tests lint
EXPECTED
cmp "$test_root/expected" "$log_file"

printf '%s\n' 'Signoff clean-worktree test passed.'
