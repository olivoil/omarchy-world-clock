# Omarchy World Clock

Omarchy World Clock adds a small world-clock entry point next to Omarchy's
center Waybar clock and opens a multi-timezone popup for planning across
places.

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

### AUR

Recommended on Arch/Omarchy via AUR, no Rust toolchain required:

```bash
paru -S omarchy-world-clock-bin
```

Then add the module to your current user's Waybar config:

```bash
omarchy-world-clock install-waybar
omarchy-world-clock restart-waybar
```

The AUR package installs only the system binary. `install-waybar` patches the
current user's Waybar config and creates the user config file.

To patch non-default paths:

```bash
omarchy-world-clock install-waybar \
  --waybar-config "$HOME/.config/waybar/config.jsonc" \
  --waybar-style "$HOME/.config/waybar/style.css" \
  --command-path "$(command -v omarchy-world-clock)" \
  --user-config "$HOME/.config/omarchy-world-clock/config.json"
```

### Manual Waybar Setup

If you prefer to edit Waybar yourself, add the module to `modules-center` in
`~/.config/waybar/config.jsonc`:

```jsonc
"modules-center": ["clock", "custom/world-clock"]
```

Then add this top-level module block. If you did not install from AUR, replace
`/usr/bin/omarchy-world-clock` with your installed binary path. Keep normal
JSON comma placement for where you insert the block.

```jsonc
"custom/world-clock": {
  "exec": "/usr/bin/omarchy-world-clock module",
  "return-type": "json",
  "interval": 2,
  "format": "{}",
  "tooltip": true,
  "on-click": "/usr/bin/omarchy-world-clock toggle",
  "on-click-right": "omarchy-launch-floating-terminal-with-presentation omarchy-tz-select"
}
```

Add the matching styles to `~/.config/waybar/style.css`:

```css
#custom-world-clock {
  min-width: 12px;
  margin-left: 6px;
  margin-right: 0;
  font-size: 12px;
  opacity: 0.72;
}

#custom-world-clock.active {
  opacity: 1;
}
```

Restart Waybar after editing:

```bash
omarchy-world-clock restart-waybar
```

### Script Install

Alternative install, no Rust toolchain required:

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

## Disclaimer

Omarchy World Clock is an unofficial project and is not affiliated with
Basecamp or the Omarchy project.
