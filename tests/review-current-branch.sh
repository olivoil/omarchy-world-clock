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
cp -- scripts/review-current-branch.sh "$repo/scripts/review-current-branch.sh"

printf '%s\n' '#!/usr/bin/env bash' \
  'printf "%s\n" build >>"$TEST_LOG"' \
  >"$repo/scripts/build-plugin-backend.sh"
printf '%s\n' '#!/usr/bin/env bash' \
  'printf "%s\n" ci >>"$TEST_LOG"' \
  >"$repo/scripts/ci.sh"
printf '%s\n' '#!/usr/bin/env bash' \
  'printf "install %s\n" "$*" >>"$TEST_LOG"' \
  >"$repo/scripts/install-review-preview.sh"
chmod 0755 "$repo/scripts/"*.sh

git -C "$repo" init -q -b feature/current

cat >"$mock_bin/omarchy" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-} ${2:-}" in
  'plugin list')
    if [[ ${PRODUCTION_PRESENT:-true} == true ]]; then
      printf '%s\n' '[{"id":"io.github.olivoil.world-clock"}]'
    else
      printf '%s\n' '[]'
    fi
    ;;
  'plugin update' | 'plugin add')
    printf 'omarchy %s\n' "$*" >>"$TEST_LOG"
    ;;
  *) exit 2 ;;
esac
MOCK
chmod 0755 "$mock_bin/omarchy"

(
  cd "$repo"
  PATH="$mock_bin:$PATH" TEST_LOG="$log_file" \
    bash scripts/review-current-branch.sh --after omarchy.clock
)

cat >"$test_root/expected" <<'EXPECTED'
build
ci
omarchy plugin update io.github.olivoil.world-clock --yes
install --prune-other-reviews --after omarchy.clock
EXPECTED
cmp "$test_root/expected" "$log_file"

: >"$log_file"
(
  cd "$repo"
  PATH="$mock_bin:$PATH" TEST_LOG="$log_file" PRODUCTION_PRESENT=false \
    bash scripts/review-current-branch.sh
)

cat >"$test_root/expected" <<'EXPECTED'
build
ci
omarchy plugin add https://github.com/olivoil/omarchy-world-clock.git --enable --yes
install --prune-other-reviews
EXPECTED
cmp "$test_root/expected" "$log_file"

printf '%s\n' 'Current-branch review workflow test passed.'
