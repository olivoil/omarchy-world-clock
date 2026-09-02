# Omarchy World Clock

A native Omarchy Quattro world clock. Open the bar widget for current time and weather across your places, add new timezones, pin the ones you want always visible, or scrub and enter times to convert from one timezone to the others.

This repository is the installable root of the native Quattro plugin
`io.github.olivoil.world-clock`. It is listed in the independent community
[Omarchy Plugin Marketplace](https://omarchyplugins.com/plugin.html?id=io.github.olivoil.world-clock).

<p>
  <img src="preview.png" alt="Omarchy World Clock live panel, interactive time scrubber, globe, and add-location flow" width="960">
</p>

## Install

On Omarchy Quattro:

```bash
omarchy plugin add https://github.com/olivoil/omarchy-world-clock.git --enable
```

The interactive installer asks where to place the widget. 
You can move it after installing at any time:

```bash
# Place it immediately after Omarchy's clock.
omarchy bar move io.github.olivoil.world-clock --after omarchy.clock

# Or place it in a particular section.
omarchy bar move io.github.olivoil.world-clock --section left
```

Use `center` or `right` instead of `left` as needed.

### Update

```bash
omarchy plugin update io.github.olivoil.world-clock
omarchy restart shell
```

The restart ensures the updated QML frontend and bundled backend are loaded
together. Omarchy releases affected by its
[stale plugin hot-reload bug](https://github.com/basecamp/omarchy/issues/6981)
can otherwise keep the previous QML component cached until the shell restarts.
Saved places and widget settings are preserved.

### Upgrading from the old AUR release

Versions through `0.3.x` used `omarchy-world-clock-bin` for a separate backend.
If that release already installed the Quattro plugin, update it with the
command above. If it did not, use the normal `omarchy plugin add ... --enable`
command instead.

The new plugin ignores the old `/usr/bin/omarchy-world-clock` executable and bundles its own, so
the AUR package is no longer required.

## Use

- **Left click** the world icon to open or close World Clock.
- **Right click** it to open Omarchy's system-timezone selector and set your local timezone.
- Use the **pencil** to rename, pin, or remove locations. Click a location name
  to edit it, then press **Enter** to save it or **Escape** to cancel. Submit an
  empty name to restore the location's default place name.
- Pin any combination of places. The first three stay visible as compact
  location codes and times beside the bar icon; additional pins collapse into
  a `+N` summary and remain available in the panel.
- Drag the time ruler beneath its fixed center marker to compare every place at
  that moment: pull right for earlier or left for later, then release to keep
  the selection. Two-finger trackpad scrolling and the mouse wheel work too.
- With no field focused, start typing a number to replace the local summary
  time, or start typing a location name to open Add mode and search for it.
- Cards automatically become compact only when the large layout would not fit.
- Select a displayed time and enter another time to convert the same instant
  across every visible place.
- Use **plus** to open the full-panel globe. Start typing or select **search**
  to find a city or timezone, or rotate the globe and choose a place directly.
  Choosing a place that is already saved offers **Add another** and an optional
  personal label for the new card.
- Use **refresh** to return a converted view to live time.

## Features

- Any number of saved places, with automatic large or compact cards plus a
  bounded, scrollable panel for unusually long lists.
- An interactive, DST-aware time scrubber with a fixed playhead, direct ruler
  dragging, trackpad and wheel input, plus explicit previous- and next-day
  overflow context.
- Optional current temperature and conditions for every place with a known
  coordinate.
- Multiple named places in the same timezone, such as Boston and New York.
- Multiple independent cards for the same place, such as Boston for a person
  and Boston for an office.
- Personal labels for saved locations, such as a person's name or "Home".
  The actual place remains visible beside a personal label.
- Multiple pinned place clocks in the bar, kept in pin order and identified by
  compact codes such as `TOK` or `NY`, with excess pins summarized as `+N`.
- A compact bar tooltip showing up to twelve time-sorted places, pairing any
  personal label with its actual place, plus a count for additional locations.
- Manual time conversion with DST-aware IANA timezone handling.
- Local timezone and alias search that works offline.
- Optional Open-Meteo city search for queries not resolved locally.
- A progressively loaded, high-resolution full-panel globe with detailed
  Natural Earth 1:10m coastlines, type-to-search, live major-city suggestions,
  drag-to-rotate, extended mouse-wheel or trackpad zoom, and offline
  click-to-add.
- Automatic 12/24-hour display matching the Omarchy clock or system locale.
- Automatic temperature units matching Omarchy Weather's effective preference,
  including its configured home location, then the system locale.
- Persistent state in `~/.config/omarchy-world-clock/config.json`.

## Why QML and Rust?

QML is the frontend because Quattro is a Quickshell/QML shell. It is what lets
World Clock use the same panel, bar, focus, keyboard, and theme primitives as
built-in Omarchy widgets. This frontend is native shell code rather than a
separate desktop window.

Rust is used as a small headless backend for arbitrary IANA timezone conversion, DST edge cases, config
migration and atomic writes, place search, HTTP fallback, and coordinate to
timezone lookup.

See [Architecture](docs/architecture.md) for the component boundary, JSON
protocol, packaging, etc.

## Configuration

State is written to:

```text
~/.config/omarchy-world-clock/config.json
```

Example:

```json
{
  "version": 8,
  "pinned_locations": [
    {
      "id": 2
    },
    {
      "id": 3
    }
  ],
  "timezones": [
    {
      "id": 1,
      "timezone": "America/Cancun",
      "place": "Cancun",
      "label": "Home",
      "latitude": 21.1619,
      "longitude": -86.8515
    },
    {
      "id": 2,
      "timezone": "Europe/Paris",
      "place": "Rennes",
      "latitude": 48.1173,
      "longitude": -1.6778
    },
    {
      "id": 3,
      "timezone": "Asia/Tokyo",
      "place": "Tokyo",
      "label": "Akiko",
      "latitude": 35.6764,
      "longitude": 139.65
    }
  ]
}
```

Each saved card has its own stable `id`. `place` is the geographic name used
when the card was added; optional `label` is the personal name shown most
prominently. Pins refer to card IDs, so duplicate places and duplicate personal
labels remain independent.

To prevent all Open-Meteo requests, add:

```json
{
  "disable_open_meteo_geolocation": true
}
```

This disables both remote city search and current weather. Existing coordinates
remain usable for the map.

The widget's native `Show current weather` setting controls weather separately.
Turning it off suppresses weather requests, conditions, and attribution while
leaving optional Open-Meteo city search available.

## Privacy and third-party data

Local timezone searches and map clicks do not call a remote service. In live
read mode, World Clock sends the saved coordinates for visible places directly
from the user's machine to Open-Meteo in bounded batches. Results are cached
for 15 minutes while the panel stays loaded and are attributed beside the
weather display. When a typed place query has no local result, World Clock may
also send that query to Open-Meteo's Geocoding API.

Set `disable_open_meteo_geolocation` to `true` to disable both requests. Weather
is hidden in converted-time views because it always represents current
conditions.

See Open-Meteo's [Terms & Privacy](https://open-meteo.com/en/terms) and
[Licence](https://open-meteo.com/en/license).

## Uninstall

```bash
omarchy plugin remove io.github.olivoil.world-clock
```

This removes the plugin but preserves saved places. Delete
`~/.config/omarchy-world-clock/config.json` separately only if you also want to
discard that state.

## Development

Build and test the source backend:

```bash
cargo build --locked --bin omarchy-world-clock-backend
cargo test --locked
```

Exercise the JSON protocol:

```bash
cargo run --locked --bin omarchy-world-clock-backend -- module
cargo run --locked --bin omarchy-world-clock-backend -- snapshot
cargo run --locked --bin omarchy-world-clock-backend -- weather
```

Rebuild checked-in artifacts after relevant source or dependency changes:

```bash
scripts/build-timezone-grid.sh       # only when map source/data changes
scripts/build-plugin-backend.sh      # whenever backend inputs change
```

For a local branch review on an Omarchy workstation, install an isolated,
visually marked copy without replacing the normal plugin:

```bash
scripts/install-review-preview.sh
```

The review copy gets a collision-resistant branch-derived plugin ID, a distinct
icon color, a branch header in its tooltip, and its own seeded config file.
Re-running the command updates that branch's copy in place.

Artifact reproduction requires Podman or Docker. The scripts use a
digest-pinned official Rust/Alpine image and produce a static x86-64 Linux
backend; users installing the plugin need neither the container engine nor a
Rust toolchain.

Run every release check, including byte-for-byte artifact reproduction and
Quattro validation when Omarchy tooling is available:

```bash
scripts/ci.sh
```

The same source and reproducibility checks run in GitHub Actions on pull
requests and changes to `master`; QML validation additionally runs on the
maintainer's Omarchy system.

The committed backend is intentionally part of the plugin distribution. See
[`bin/README.md`](bin/README.md) for its provenance and verification files.

## Support policy

New releases target Omarchy Quattro on x86-64 Linux. Omarchy 3/Waybar is
not part of the new plugin architecture; users who need it can remain on the
older AUR release.

## Documentation

- [Architecture and packaging](docs/architecture.md)
- [Product behavior specification](docs/specs.md)
- [Maintainer release process](docs/release.md)
- [Timezone map data and attribution](data/README.md)

## License

The application source is MIT licensed. The derived timezone map database is
licensed under ODbL 1.0; see [`data/ODbL-1.0.txt`](data/ODbL-1.0.txt). The
plugin manifest expresses the combined distribution as `MIT AND ODbL-1.0`.

Omarchy World Clock is an unofficial project and is not affiliated with
Basecamp or the Omarchy project.
