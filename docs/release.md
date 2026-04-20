# Release Process

Releases are published from a local machine with `scripts/release.sh`.

## Normal Flow

1. Start from the default branch.

   ```bash
   git checkout master
   git pull --ff-only
   ```

2. Update the package version in `Cargo.toml`.

   ```toml
   version = "0.1.1"
   ```

3. Commit and push the version bump and release changes.

   ```bash
   git add Cargo.toml Cargo.lock
   git commit -m "Release v0.1.1"
   git push
   ```

4. Publish the release.

   ```bash
   scripts/release.sh --description "Short summary of what changed."
   ```

The script reads `Cargo.toml` and releases `v<package.version>`. For example,
`version = "0.1.1"` publishes tag `v0.1.1`.

## Release Notes

By default, pass a succinct description:

```bash
scripts/release.sh --description "Adds prebuilt release installs."
```

The script uses that as the first paragraph and appends a commit list since the
previous version tag.

For fully manual notes:

```bash
scripts/release.sh --notes-file release-notes.md
```

## Safety Checks

The release script:

- requires a clean worktree
- requires releasing from `master` unless `--allow-non-default-branch` is passed
- requires local `master` to match `origin/master`
- rejects tags that do not match `v<package.version>`
- rejects existing tags that do not point at `HEAD`
- runs Rust tests and shell installer tests unless `--skip-tests` is passed

## Output

Each release uploads:

- `omarchy-world-clock-<target>.tar.gz`
- `omarchy-world-clock-<target>.tar.gz.sha256`

Users install the latest release without Rust:

```bash
curl -fsSL https://raw.githubusercontent.com/olivoil/omarchy-world-clock/main/install.sh | bash
```
