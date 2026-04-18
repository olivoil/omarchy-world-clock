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
- Supports manual reference-time conversion across rows.
- Lets you add, remove, pin, and reorder timezones.
- Supports `System`, forced `24h`, and forced `AM/PM` display modes.
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
cargo build --manifest-path rust/Cargo.toml
```

Run the Waybar payload directly:

```bash
cargo run --manifest-path rust/Cargo.toml -- module
```

Open the popup:

```bash
cargo run --manifest-path rust/Cargo.toml -- popup
```

Toggle the popup:

```bash
cargo run --manifest-path rust/Cargo.toml -- toggle
```

Run tests:

```bash
cargo test --manifest-path rust/Cargo.toml
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
  "version": 3,
  "timezones": [
    {
      "timezone": "America/Cancun",
      "label": "Home",
      "locked": true
    },
    {
      "timezone": "Europe/Paris",
      "label": "Rennes",
      "locked": false
    }
  ],
  "sort_mode": "manual",
  "time_format": "system"
}
```

## Docs

- Product behavior spec: [docs/specs.md](docs/specs.md)
