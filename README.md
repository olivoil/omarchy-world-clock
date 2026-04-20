# Omarchy World Clock

Omarchy World Clock adds a small world-clock entry point next to Omarchy's
center Waybar clock and opens a multi-timezone popup for planning across
places.

The implementation is now Rust + GTK4 + `gtk4-layer-shell`. The old Python +
GTK3 app has been removed.

## Screenshots

<img src="docs/screenshots/white-popup.png" alt="Omarchy World Clock on the white theme" width="900">

<img src="docs/screenshots/nord-popup.png" alt="Omarchy World Clock on the nord theme" width="900">

<img src="docs/screenshots/rose-pine-popup.png" alt="Omarchy World Clock on the rose-pine theme" width="900">

## What It Does

- Adds a compact world icon next to Omarchy's center Waybar clock.
- Opens a popup with live clocks for a user-managed timezone list.
- Supports manual reference-time conversion across the visible clock cards.
- Lets you add and remove timezones.
- Searches local timezone data first, then can use Open-Meteo geocoding for
  unresolved city/place searches.
- Follows the system time format.
- Stores state in `~/.config/omarchy-world-clock/config.json`.

## Install

```bash
./install.sh
```

This:

- installs the Rust binary under `~/.local/share/omarchy-world-clock`
- writes `~/.local/bin/omarchy-world-clock`
- patches `~/.config/waybar/config.jsonc`
- patches `~/.config/waybar/style.css`
- restarts Waybar

## Uninstall

```bash
./uninstall.sh
```

To also remove saved user state:

```bash
./uninstall.sh --purge
```

## Build And Run

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

## Runtime Notes

This repo assumes an Omarchy-like environment with:

- Hyprland
- Waybar
- Rust / Cargo
- GTK4
- `gtk4-layer-shell`

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
