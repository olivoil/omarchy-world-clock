# Omarchy World Clock

Omarchy World Clock adds a small world-clock entry point next to Omarchy's
center Waybar clock and opens a multi-timezone popup for planning across
places.

The implementation is now Rust + GTK4 + `gtk4-layer-shell`. The old Python +
GTK3 app has been removed.

## Screenshots

<p>
  <strong>Rose Pine - Read View</strong><br>
  <img src="docs/screenshots/rose-pine-read.png" alt="Omarchy World Clock read view on the Rose Pine theme" width="900">
</p>

<p>
  <strong>Matte Black - Add Location</strong><br>
  <img src="docs/screenshots/matte-black-add.png" alt="Omarchy World Clock add location screen on the Matte Black theme" width="900">
</p>

<p>
  <strong>Kanagawa - Edit Mode</strong><br>
  <img src="docs/screenshots/kanagawa-edit.png" alt="Omarchy World Clock edit mode on the Kanagawa theme" width="900">
</p>

## What It Does

- Adds a compact world icon next to Omarchy's center Waybar clock.
- Toggles the popup on left click and opens `omarchy-tz-select` on right click.
- Opens a popup with live clocks for a user-managed timezone list.
- Supports manual reference-time conversion across the visible clock cards.
- Lets you add and remove timezones.
- Searches local timezone data first, then can use Open-Meteo geocoding for
  unresolved city/place searches.
- Follows the system time format.
- Adapts popup colors to the active Omarchy theme palette.
- Stores state in `~/.config/omarchy-world-clock/config.json`.

## Install

Recommended install, no Rust toolchain required:

```bash
curl -fsSL https://raw.githubusercontent.com/olivoil/omarchy-world-clock/master/install.sh | bash
```

From a local checkout:

```bash
./install.sh
```

This:

- downloads the latest prebuilt release binary
- installs it under `~/.local/share/omarchy-world-clock`
- writes `~/.local/bin/omarchy-world-clock`
- patches `~/.config/waybar/config.jsonc`
- patches `~/.config/waybar/style.css`
- restarts Waybar

Install a specific release:

```bash
OMARCHY_WORLD_CLOCK_VERSION=v0.1.0 ./install.sh
```

Build from source instead:

```bash
./install.sh --build-from-source
```

## Uninstall

```bash
./uninstall.sh
```

To also remove saved user state:

```bash
./uninstall.sh --purge
```

## Build And Run

Source builds require Rust/Cargo plus GTK4 development packages.

Build:

```bash
cargo build
```

Run the Waybar payload directly:

```bash
cargo run -- module
```

Open the popup:

```bash
cargo run -- popup
```

Toggle the popup:

```bash
cargo run -- toggle
```

Run tests:

```bash
cargo test
```

Run the local PR checks:

```bash
scripts/ci.sh
```

This mirrors the checks this project would normally put in a GitHub Action:
formatting, Clippy, Rust tests, and the shell installer tests.

Sign off the current commit after the local checks pass:

```bash
scripts/signoff.sh
```

This requires GitHub CLI auth and Basecamp's signoff extension:

```bash
gh auth login
gh extension install basecamp/gh-signoff
```

To make signoff a required merge check on GitHub, run `gh signoff install`
from the default branch.

If branch protection requires partial signoffs, pass the names through:

```bash
scripts/signoff.sh tests lint security
```

## Runtime Notes

This repo assumes an Omarchy-like environment with:

- Hyprland
- Waybar
- GTK4
- `gtk4-layer-shell`

Release installs do not require Rust or Cargo on the user's machine. They still
need the GTK runtime libraries. On Arch/Omarchy:

```bash
sudo pacman -S gtk4 gtk4-layer-shell
```

The supported CLI surface is:

- `omarchy-world-clock module`
- `omarchy-world-clock toggle`
- `omarchy-world-clock popup`
- `omarchy-world-clock install-waybar`
- `omarchy-world-clock uninstall-waybar`
- `omarchy-world-clock restart-waybar`

## Configuration

State lives in:

```text
~/.config/omarchy-world-clock/config.json
```

Example:

```json
{
  "version": 4,
  "timezones": [
    {
      "timezone": "America/Cancun",
      "label": "Home",
      "latitude": 21.1619,
      "longitude": -86.8515
    },
    {
      "timezone": "Europe/Paris",
      "label": "Rennes",
      "latitude": 48.1173,
      "longitude": -1.6778
    }
  ]
}
```

Optional privacy setting:

```json
{
  "disable_open_meteo_geolocation": true
}
```

When this is true, search uses only local timezone names, aliases, and bundled
timezone data. Existing coordinates already saved in the config still work.

Legacy `locked`, `sort_mode`, and `time_format` keys are ignored when old config
files are loaded and are not written back.

## Third-Party Services

Omarchy World Clock calls Open-Meteo's Geocoding API only for unresolved
city/place searches, and only when `disable_open_meteo_geolocation` is not set
to `true`. The app does not use a project API key; requests are made directly
from the user's machine.

Open-Meteo's free API is for non-commercial use with published rate limits, and
its API data is licensed under CC BY 4.0. Remote search results are attributed
inline in the popup with a link to Open-Meteo, as required by their licence.

Privacy note: the typed search text is sent to Open-Meteo for these remote
lookups. Open-Meteo's terms say free API logs may include IP addresses and
request details for technical reasons and troubleshooting, with log deletion
after 90 days. See Open-Meteo's [Terms & Privacy](https://open-meteo.com/en/terms)
and [Licence](https://open-meteo.com/en/licence).

## Docs

- Product behavior spec: [docs/specs.md](docs/specs.md)
- Maintainer release process: [docs/release.md](docs/release.md)

## Maintainer Release

Releases are published from a local machine, not GitHub Actions. See
[docs/release.md](docs/release.md) for the full checklist.

```bash
scripts/release.sh --description "Adds prebuilt release installs and local release publishing."
```

If you omit the tag, the script uses `v<package.version>` from `Cargo.toml`:

```bash
scripts/release.sh
```

The release script:

- requires a clean git worktree
- releases from the remote default branch, `master`, by default
- uses `Cargo.toml` as the version source of truth
- rejects explicit tags that do not match `v<package.version>`
- writes release notes from `--description` plus the commits since the previous tag
- accepts `--notes-file path/to/notes.md` when you want full manual release notes
- runs `scripts/ci.sh` before release builds unless `--skip-tests` is passed
- builds `target/release/omarchy-world-clock`
- packages `omarchy-world-clock-<rust-host-target>.tar.gz`
- creates and pushes the git tag if it does not already exist
- creates or updates the GitHub release with the archive and `.sha256`

To check the release before publishing:

```bash
scripts/release.sh --dry-run --description "Short summary of what changed."
```

Dry runs build the package, generate the notes, check tag/release state, and
print the publish actions without creating tags or touching the GitHub release.

Normal release flow:

```bash
git checkout master
git pull --ff-only
# update Cargo.toml version, commit, and push
scripts/release.sh --description "Short summary of what changed."
```

Prerequisites for maintainers:

- Rust/Cargo
- Rustfmt and Clippy
- `gh` authenticated for `olivoil/omarchy-world-clock`
- `git`, `tar`, and `sha256sum`
