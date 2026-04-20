#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT"

TAG=
DRAFT=false
SKIP_TESTS=false
ALLOW_NON_DEFAULT_BRANCH=false
DESCRIPTION=${OMARCHY_WORLD_CLOCK_RELEASE_DESCRIPTION:-}
NOTES_FILE=
TARGET=${OMARCHY_WORLD_CLOCK_TARGET:-}
DIST_DIR=${OMARCHY_WORLD_CLOCK_DIST_DIR:-"$REPO_ROOT/target/release-dist"}

usage() {
  cat <<EOF
Usage: scripts/release.sh [tag] [--draft] [--skip-tests]

Builds a local release binary, packages it as a GitHub release asset, creates
and pushes the tag if needed, then creates or updates the GitHub release.
If no tag is provided, the script uses v<package.version> from Cargo.toml.

Options:
  --description TEXT          First paragraph for generated release notes.
  --notes-file PATH           Use an exact Markdown release notes file.
  --allow-non-default-branch  Release from the current branch instead of origin/HEAD.
  --draft                     Create a draft release.
  --skip-tests                Skip release preflight tests.

Examples:
  scripts/release.sh
  scripts/release.sh --description "Adds prebuilt release installs."
  scripts/release.sh v0.1.0
  scripts/release.sh v0.1.0 --draft
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --draft)
      DRAFT=true
      shift
      ;;
    --skip-tests)
      SKIP_TESTS=true
      shift
      ;;
    --allow-non-default-branch)
      ALLOW_NON_DEFAULT_BRANCH=true
      shift
      ;;
    --description)
      if [[ $# -lt 2 ]]; then
        printf 'Missing value for --description.\n' >&2
        exit 2
      fi
      DESCRIPTION=$2
      shift 2
      ;;
    --notes-file)
      if [[ $# -lt 2 ]]; then
        printf 'Missing value for --notes-file.\n' >&2
        exit 2
      fi
      NOTES_FILE=$2
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    -*)
      printf 'Unknown argument: %s\n\n' "$1" >&2
      usage >&2
      exit 2
      ;;
    *)
      if [[ -n "$TAG" ]]; then
        printf 'Only one tag may be provided.\n\n' >&2
        usage >&2
        exit 2
      fi
      TAG=$1
      shift
      ;;
  esac
done

if [[ -n "$DESCRIPTION" && -n "$NOTES_FILE" ]]; then
  printf 'Use either --description or --notes-file, not both.\n' >&2
  exit 2
fi

if [[ -n "$NOTES_FILE" && ! -f "$NOTES_FILE" ]]; then
  printf 'Release notes file does not exist: %s\n' "$NOTES_FILE" >&2
  exit 2
fi

for command_name in awk cargo gh git grep head rustc sed sha256sum tar; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$command_name" >&2
    exit 1
  fi
done

if [[ "$(uname -s)" != "Linux" ]]; then
  printf 'Release assets must be built on Linux because the app targets Omarchy/Wayland.\n' >&2
  exit 1
fi

CARGO_VERSION=$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"version":"\([^"]*\)".*/\1/p' | head -n 1)
if [[ -z "$CARGO_VERSION" ]]; then
  printf 'Could not read package version from Cargo.toml.\n' >&2
  exit 1
fi

EXPECTED_TAG="v$CARGO_VERSION"
if [[ -z "$TAG" ]]; then
  TAG=$EXPECTED_TAG
elif [[ "$TAG" != "$EXPECTED_TAG" ]]; then
  printf 'Release tag %s does not match Cargo.toml version %s.\n' "$TAG" "$CARGO_VERSION" >&2
  printf 'Use %s or update Cargo.toml first.\n' "$EXPECTED_TAG" >&2
  exit 1
fi

if [[ -n "$(git status --porcelain)" ]]; then
  printf 'Working tree is dirty. Commit or stash changes before releasing.\n' >&2
  exit 1
fi

git fetch --tags origin

if [[ "$ALLOW_NON_DEFAULT_BRANCH" != true ]]; then
  remote_head=$(git symbolic-ref --short refs/remotes/origin/HEAD 2>/dev/null || true)
  release_branch=${OMARCHY_WORLD_CLOCK_RELEASE_BRANCH:-${remote_head#origin/}}
  release_branch=${release_branch:-master}
  current_branch=$(git branch --show-current)

  if [[ "$current_branch" != "$release_branch" ]]; then
    printf 'Release from %s. Current branch is %s.\n' "$release_branch" "${current_branch:-detached}" >&2
    printf 'Merge your changes first, or pass --allow-non-default-branch intentionally.\n' >&2
    exit 1
  fi

  head_commit=$(git rev-parse HEAD)
  remote_commit=$(git rev-parse "origin/$release_branch" 2>/dev/null || true)
  if [[ -z "$remote_commit" || "$head_commit" != "$remote_commit" ]]; then
    printf 'Local %s is not aligned with origin/%s.\n' "$release_branch" "$release_branch" >&2
    printf 'Push or pull before releasing so the release tag points at the public branch tip.\n' >&2
    exit 1
  fi
fi

if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
  tag_commit=$(git rev-list -n 1 "$TAG")
  head_commit=$(git rev-parse HEAD)
  if [[ "$tag_commit" != "$head_commit" ]]; then
    printf 'Tag %s already exists but does not point at HEAD.\n' "$TAG" >&2
    printf 'Check out that tag, or choose a new release tag.\n' >&2
    exit 1
  fi
fi

if [[ -z "$TARGET" ]]; then
  TARGET=$(rustc -vV | awk '/^host:/ {print $2}')
fi

ASSET="omarchy-world-clock-${TARGET}.tar.gz"
ARCHIVE="$DIST_DIR/$ASSET"
CHECKSUM="$ARCHIVE.sha256"
STAGING="$DIST_DIR/staging"
GENERATED_NOTES="$DIST_DIR/release-notes.md"

if [[ "$SKIP_TESTS" != true ]]; then
  cargo test --locked
  bash tests/install.sh
  bash tests/uninstall.sh
fi

cargo build --release --locked

rm -rf "$DIST_DIR"
mkdir -p "$STAGING"
install -m 755 "$REPO_ROOT/target/release/omarchy-world-clock" "$STAGING/omarchy-world-clock"
tar -C "$STAGING" -czf "$ARCHIVE" omarchy-world-clock
sha256sum "$ARCHIVE" >"$CHECKSUM"

if [[ -z "$NOTES_FILE" ]]; then
  previous_tag=$(git tag --merged HEAD --sort=-version:refname 'v[0-9]*' | grep -vx "$TAG" | head -n 1 || true)
  if [[ -n "$previous_tag" ]]; then
    commit_range="$previous_tag..HEAD"
    commits_title="Commits since $previous_tag"
  else
    commit_range="HEAD"
    commits_title="Commits"
  fi

  {
    printf '%s\n\n' "${DESCRIPTION:-Omarchy World Clock $TAG release.}"
    printf '## %s\n\n' "$commits_title"
    if git log --pretty=format:'- %s (%h)' "$commit_range" | grep .; then
      printf '\n'
    else
      printf 'No commits found.\n'
    fi
  } >"$GENERATED_NOTES"
  NOTES_FILE=$GENERATED_NOTES
fi

if ! git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
  git tag -a "$TAG" -m "$TAG"
fi

if ! git ls-remote --exit-code --tags origin "refs/tags/$TAG" >/dev/null 2>&1; then
  git push origin "$TAG"
fi

if gh release view "$TAG" >/dev/null 2>&1; then
  gh release edit "$TAG" --notes-file "$NOTES_FILE"
  gh release upload "$TAG" "$ARCHIVE" "$CHECKSUM" --clobber
else
  args=(release create "$TAG" "$ARCHIVE" "$CHECKSUM" --title "$TAG" --notes-file "$NOTES_FILE")
  if [[ "$DRAFT" == true ]]; then
    args+=(--draft)
  fi
  gh "${args[@]}"
fi

printf 'Published %s with asset %s\n' "$TAG" "$ASSET"
