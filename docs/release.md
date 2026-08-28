# Release Process

World Clock releases are complete Quattro plugin repository commits. The QML,
headless backend, map data, manifest, and provenance files must move together.
There is no separate binary archive or AUR update for new releases.

World Clock is a native third-party Omarchy Quattro plugin, installed by
`omarchy plugin`, with the root `manifest.json` as its plugin manifest. Its
community listing is maintained by the independent
[Omarchy Plugin Marketplace](https://github.com/HANCORE-linux/omarchy-plugin-marketplace),
not by a Codex or ChatGPT plugin marketplace.

The ownership boundary matters during a release:

- `manifest.json` owns the plugin name, version, author, description, license,
  kinds, and entry points. The marketplace uses its top-level `description` as
  the listing summary. Keep `barWidget.description` aligned with it so Omarchy
  and the marketplace present the same copy.
- The marketplace registry owns the category, tags, recorded validation
  snapshot, and any curated manual-installation override. Upstream manifest
  changes cannot remove that override.

`scripts/release.sh` is deliberately a **validation-only** command. It cannot
tag, push, create a release, or update the plugin directory. Publication stays
a separate, visible action after hands-on testing and explicit approval.

## Prepare a candidate

1. Make changes on a non-default branch. Do not publish the branch as a release
   yet.

2. If the timezone boundary dependency or grid format changed, regenerate the
   compact database:

   ```bash
   scripts/build-timezone-grid.sh
   ```

3. Set the same semantic version in `Cargo.toml` and `manifest.json`, then let
   Cargo update the root package entry in `Cargo.lock`:

   ```bash
   cargo check --bin omarchy-world-clock-backend
   git diff -- Cargo.lock
   ```

   This is the one intentionally unlocked Cargo command in the release flow.
   Return to locked commands after inspecting the lockfile diff.

4. Rebuild the checked-in executable after every backend source, data,
   dependency, profile, protocol, or version change:

   ```bash
   scripts/build-plugin-backend.sh
   ```

   This also refreshes `bin/SHA256SUMS` and `bin/BUILDINFO`.

5. Run the complete checks:

   ```bash
   scripts/ci.sh
   git diff --check
   ```

   CI verifies Rust formatting/lints/tests, protocol behavior, manifest and QML
   validity, map-data reproduction, exact static-binary reproduction, size,
   linkage, version alignment, and provenance. Artifact reproduction requires
   Podman or Docker; the toolchain/userspace image is pinned by digest.

6. Review the full diff, especially the executable size/checksum, manifest,
   license expression, and any generated-data change. Commit the candidate
   locally. No push or tag is required for local testing.

7. With a clean committed candidate, run:

   ```bash
   scripts/release.sh
   ```

   The script stops after validation and prints a reminder that nothing was
   published.

## Test through the real plugin manager

`omarchy plugin add` performs a Git clone, so it tests committed state rather
than uncommitted working-tree changes. Back up the current shell configuration
and installed checkout first:

```bash
candidate=$(git rev-parse --show-toplevel)
stamp=$(date +%Y%m%d-%H%M%S)
backup="$HOME/.local/state/omarchy-world-clock/test-backups/$stamp"
mkdir -p "$backup"
cp ~/.config/omarchy/shell.json "$backup/shell.json"
if [[ -d ~/.config/omarchy/plugins/io.github.olivoil.world-clock ]]; then
  cp -a ~/.config/omarchy/plugins/io.github.olivoil.world-clock "$backup/plugin"
fi
```

Then replace only the plugin checkout with the local candidate:

```bash
omarchy plugin remove io.github.olivoil.world-clock --yes
omarchy plugin add "$candidate" --enable --yes
omarchy bar move io.github.olivoil.world-clock --after omarchy.clock
omarchy restart shell
```

Saved places remain in `~/.config/omarchy-world-clock/config.json` and are not
removed by those commands.

Test at minimum:

- fresh panel open and native panel handoff
- existing config migration/load
- multiple pin and targeted unpin, including every coded bar time
- add/remove and two named places sharing one timezone
- local search, an Open-Meteo result, and the privacy opt-out
- map clicks on representative land and ocean locations
- time conversion around a DST transition
- plugin reload and shell restart
- the bundled version:

  ```bash
  ~/.config/omarchy/plugins/io.github.olivoil.world-clock/bin/omarchy-world-clock-backend version
  ```

To return to the currently published build before approval:

```bash
omarchy plugin remove io.github.olivoil.world-clock --yes
omarchy plugin add https://github.com/olivoil/omarchy-world-clock.git --enable
```

## Publish only after approval

After the tester explicitly approves the candidate:

1. Merge it into the default branch and push that branch.
2. Confirm a fresh public clone passes `omarchy plugin validate` and reports
   the intended bundled backend version.
3. Create and push the immutable `v<version>` tag.
4. Create the matching GitHub release; no separate binary asset is needed.
5. Verify a clean user install and `omarchy plugin update` from the public URL,
   followed by `omarchy restart shell` so affected Omarchy releases do not
   retain the previous QML component cache.
6. Open the marketplace's
   [plugin verification issue form](https://github.com/HANCORE-linux/omarchy-plugin-marketplace/issues/new?template=verify-plugin.yml),
   select **Verify and publish a newer upstream commit**, and supply the exact
   plugin ID, repository URL, and full 40-character SHA of the final default
   branch HEAD. Do this only after every intended release PR is merged; the
   guarded update rejects a target that is no longer current HEAD.
7. In that update issue, explicitly ask the marketplace maintainers to remove
   the curated `installation.mode: "manual"` override for
   `io.github.olivoil.world-clock`. Link the historical
   [World Clock submission #553](https://github.com/HANCORE-linux/omarchy-plugin-marketplace/issues/553)
   for context, but do not rely on a comment on that closed submission to start
   the current update workflow. Include the published version/commit and
   explain that the static backend is inside the repository, no setup hook
   runs, and this command produces a functioning plugin:

   ```bash
   omarchy plugin add https://github.com/olivoil/omarchy-world-clock.git --enable
   ```

   The verified-update workflow records the new commit but preserves curated
   installation overrides, so the maintainer must remove this field explicitly.
8. Check the Omarchy Plugins listing after the override is removed. It should
   show the standard plugin installation command rather than manual setup.

The marketplace's scheduled catalog refresh can pick up upstream manifest
copy, including a revised description, but it cannot promote a new validated
snapshot or remove the manual-installation override. A description-only change
does not need a separate marketplace issue; include the final description in
the release commit and use the single update issue above for the released
snapshot and installability request.

Example commands are intentionally not automated:

```bash
version=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')
git tag -s "v$version" -m "World Clock v$version"
git push origin "v$version"
gh release create "v$version" --verify-tag --generate-notes \
  --title "World Clock v$version"
```

Pushing the default branch is itself a publication event because normal plugin
installs and updates follow it. Do not perform that step merely to make a test
build available.

## Legacy AUR policy

`omarchy-world-clock-bin` remains the historical install path for Omarchy 3
users. It is frozen on the old architecture and is not part of new version
work. Never update it to a Quattro-only release or reintroduce an AUR dependency
into `manifest.json`, QML, README installation steps, or release checks.
