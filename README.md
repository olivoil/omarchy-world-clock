# Omarchy World Clock

Omarchy World Clock adds a small world-clock entry point next to Omarchy's
center Waybar clock and opens a fast multi-timezone popup for planning across
places.

The current implementation is Python + GTK 3 + `GtkLayerShell`, built for the
Omarchy desktop environment on Hyprland. The next planned port target is
Rust + GTK4.

## Screenshots

<img src="docs/screenshots/white-popup.png" alt="Omarchy World Clock on the white theme" width="900">

<img src="docs/screenshots/nord-popup.png" alt="Omarchy World Clock on the nord theme" width="900">

<img src="docs/screenshots/rose-pine-popup.png" alt="Omarchy World Clock on the rose-pine theme" width="900">

## What It Does

- Adds a compact world icon next to Omarchy's center Waybar clock.
- Opens a centered popup with large digital clocks for a user-managed timezone
  list.
- Supports live time plus manual reference-time conversion across all rows.
- Lets you reorder, pin, label, add, and remove timezones.
- Supports `System`, forced `24h`, and forced `AM/PM` display modes.
- Uses local timezone search first, then falls back to remote place lookup.
- Stores state in `~/.config/omarchy-world-clock/config.json`.

## Current UX

- Left click on the Waybar icon toggles the popup.
- Right click launches Omarchy's timezone selector helper.
- The popup dismisses on `Escape`, focus loss, or outside click.
- In manual sort mode, unlocked rows can be drag-reordered.
- The popup is currently centered on screen by design.

## Why This Exists

Omarchy ships a strong single-clock center bar, but coordinating across timezones
usually means opening a browser tab, terminal helper, or another app. This
project keeps that workflow inside Omarchy's existing bar-and-popup model.

The goal is a small tool that feels native to Omarchy:

- always close to the clock
- fast to open and dismiss
- readable at a glance
- practical enough for real timezone planning

## Install

```bash
./install.sh
```

This does the following:

- copies the app into `~/.local/share/omarchy-world-clock/app`
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

## Runtime And Dev Notes

This repo currently assumes an Omarchy-like environment with:

- Hyprland
- Waybar
- Python 3
- PyGObject / GTK 3
- `GtkLayerShell`

There is no separate Python package manifest because the app is meant to run in
the desktop environment Omarchy already configures.

Useful commands:

```bash
PYTHONPATH=app python3 -m unittest discover -s tests
PYTHONPATH=app python3 -m omarchy_world_clock.cli module
PYTHONPATH=app python3 -m omarchy_world_clock.cli toggle
```

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
- Rust + GTK4 migration notes: [docs/porting-notes.md](docs/porting-notes.md)
- Copyable Codex handoff prompt for the Rust port:
  [docs/rust-gtk4-port-prompt.md](docs/rust-gtk4-port-prompt.md)

## Port Direction

The project is now at a good handoff point for the rewrite:

- the GTK3 version is functionally credible
- the product behavior is documented
- the next larger features are better suited to a more modern rendering path

The intended destination is:

- Rust
- GTK4
- `gtk4-layer-shell`

`Python + GTK4` remains a fallback half-step if the rewrite hits binding or UX
issues early, but it is not the primary plan.

## Likely Next Features

Once the Rust + GTK4 baseline reaches parity, the next product ideas worth
exploring are:

- a world map view
- hover or click affordances for local time by region
- adding places directly from the map
- a day/night curve or solar terminator layer behind the map

Those are intentionally out of scope for the first port milestone. The rewrite
should first preserve the current Waybar module, popup, timezone management, and
time-conversion workflow.
