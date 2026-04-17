# Omarchy World Clock

Adds a small clock icon next to Omarchy's center Waybar clock. Clicking the icon opens a popup with multiple digital clocks, one for your local timezone and any extra timezones you add.

## Features

- Keeps Omarchy's stock center clock and adds a small adjacent world-clock icon.
- Opens a top popup with large digital clocks for your local timezone plus selected zones.
- Lets you type `HH:MM`, shorthand like `830` or `8.5`, or `YYYY-MM-DD HH:MM` into any clock to convert that instant across every other timezone.
- Lets you add timezones with autocomplete, plus an online place lookup fallback for places that are not direct timezone names.
- Stores your selected timezones in `~/.config/omarchy-world-clock/config.json`.
- Installs and uninstalls by patching Waybar with reversible markers.

## Install

```bash
./install.sh
```

The installer copies the Python app into `~/.local/share/omarchy-world-clock`, writes a wrapper to `~/.local/bin/omarchy-world-clock`, patches `~/.config/waybar/config.jsonc` and `~/.config/waybar/style.css`, and restarts Waybar.

## Uninstall

```bash
./uninstall.sh
```

To remove the saved timezone list too:

```bash
./uninstall.sh --purge
```

## Development

Run the tests:

```bash
PYTHONPATH=app python3 -m unittest discover -s tests
```

Smoke-check the Waybar module payload:

```bash
PYTHONPATH=app python3 -m omarchy_world_clock.cli module
```
