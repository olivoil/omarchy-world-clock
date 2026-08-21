# Omarchy World Clock Specification

This document defines the intended behavior of the Quattro plugin. When it and
the implementation differ, either the implementation or this specification
must be corrected so there is one supported product surface.

## Product and platform

World Clock is an x86-64 Omarchy Quattro bar widget for viewing and converting
time across a user-managed list of places. New versions do not provide a
Waybar module or a separate GTK window.

The complete plugin is installed with:

```bash
omarchy plugin add https://github.com/olivoil/omarchy-world-clock.git --enable
```

QML is the native frontend. A relative, bundled Rust executable supplies its
timezone, search, map, and persistence model. The plugin must not depend on an
AUR package, `PATH` lookup, user-configured backend command, or install hook.

## Bar widget

- The plugin ID is `io.github.olivoil.world-clock`.
- The default section is `center`; placement remains user-controlled through
  `omarchy bar move`.
- Left click toggles the panel.
- Right click opens Omarchy's system-timezone selector.
- The widget participates in Quattro's `open`, `close`, `opened`, and sibling
  panel coordination contracts.
- It uses the normal compact status slot and native open-panel underline
  without recoloring the world icon.
- With no pin, it displays only the icon.
- With a pin, it displays the icon and that place's current time.
- The pinned time updates on minute boundaries.
- If the bundled backend cannot start or has an incompatible protocol, the
  widget is dimmed and its tooltip explains that the plugin is unavailable.

## Panel

- The panel renders inside the existing `omarchy-shell` Quickshell process
  using the shared `KeyboardPanel` behavior.
- It follows the active shell's background, foreground, accent, border,
  opacity, radius, spacing, and typography tokens.
- Outside click and `Escape` dismiss it.
- `Tab` and `Shift+Tab` can hand off to adjacent panels.
- Opening the panel is an in-process state change; only bounded backend
  commands start an external process.
- The panel has read, edit, and add modes.
- Read mode shows the summary clock, proportional relative timeline, and up to
  nine non-local location clocks in a borderless three-column layout.
- The current place uses an 18-pixel title at the default scale and sits close
  to its summary time, keeping place and time as the primary read-mode
  hierarchy.
- Live read mode shows current temperature and conditions as secondary context
  for every visible location with a known coordinate.
- Edit mode preserves the read layout and exposes pin/unpin and remove actions.
- Add mode shows local/remote search and an interactive world map.

## Configuration and persistence

State lives in `~/.config/omarchy-world-clock/config.json`.

```json
{
  "version": 6,
  "pinned_location": {
    "timezone": "Europe/Paris",
    "label": "Rennes"
  },
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

Persistence rules:

- writes use a temporary file and atomic replacement
- supported older config shapes migrate on load
- timezone names are canonicalized before storage
- distinct named places may share a timezone
- duplicate places are identified by canonical timezone and normalized label
- saved order is preserved
- an empty label displays as a friendly timezone name
- invalid coordinates are discarded
- a pin must match one configured location
- removing the pinned location clears the pin
- `disable_open_meteo_geolocation` defaults to `false` and is only persisted
  when true; it disables both remote place search and current weather

On first load, the detected local timezone is added unless already present. If
the user later removes it, it is not automatically re-added.

## Ordering and limits

- Visible locations are ordered by wall-clock time at the reference instant.
- Equal-time locations fall back to display-label ordering.
- Up to nine non-local locations may be configured.
- The UI never permits removing the final configured location.
- If there are no non-local locations, opening the panel goes directly to add
  mode.

## Live time and conversion

The default reference instant is `now`; display clocks refresh on minute
boundaries.

The user may focus the summary time or a location time and enter:

- `HH:MM`
- compact `830` or `0830`
- decimal half-hour shorthand such as `8.5`
- meridiem shorthand such as `3pm`, `8 am`, or `12am`
- `YYYY-MM-DD HH:MM`

Time-only input is interpreted in the edited place's timezone using that
timezone's current local date at the active reference instant. On success, the
input is normalized and every clock displays the same instant in its timezone.
On failure, only the edited source receives an error state and the current
reference instant is retained. Refresh returns to live time.

Pressing `Enter` in read mode focuses and selects the summary time for direct
replacement.

## Time format

The only user-facing format is the system format. Detection order is:

1. the `omarchy.clock` format in the Quattro shell configuration
2. the active locale
3. 24-hour format when ambiguous

No separate World Clock 12/24-hour preference is stored.

## Weather

- Weather uses Open-Meteo current conditions. A quiet `Open-Meteo` provider
  link sits beside the top-left return-to-live action at the smallest display
  type step and stays transparent at rest. Its padded hit target, tooltip,
  hover, and focus states preserve the link's context and affordance.
- The native `Show current weather` widget setting defaults to on. Turning it
  off prevents weather requests and removes conditions and their attribution
  row, while Open-Meteo place search remains available.
- The header identifies the provider from the first frame while data loads, so
  an asynchronous response never changes its geometry.
- All visible coordinates are fetched in one request when the panel opens.
- A successful response is reused for 15 minutes. While the panel remains
  open, a lightweight freshness check runs every 30 seconds so reopening the
  panel cannot restart the full cache interval.
- Temperature inherits an explicit `metric` or `imperial` unit from the
  `omarchy.weather` widget. In automatic mode, the configured Omarchy Weather
  location's country decides first; United States, Liberia, and Myanmar use
  Fahrenheit, and other known countries use Celsius. If that country is
  unavailable, the local timezone's country and then the system locale provide
  the fallback.
- Cards emphasize location names one type step above secondary metadata. They
  add only a day/night-aware icon and rounded temperature to the relative-time
  row; the icon is slightly larger than its temperature. Units appear on the
  summary weather value, while card temperatures omit the repeated unit letter
  to stay quiet.
- The summary timezone metadata, weather icon, and temperature share one
  centered row beneath the primary time.
- Weather remains secondary to the clock and does not change card ordering.
- Converted-time views hide weather because the conditions are current rather
  than historical or forecast data.
- Missing coordinates or individual conditions produce a quiet unavailable
  state without hiding the clock.
- A failed refresh keeps prior conditions visible with an update warning. An
  initial failure leaves the rest of the panel fully usable.
- `disable_open_meteo_geolocation` remains the master privacy opt-out and
  prevents both weather and geocoding requests.

## Search and add

Search accepts:

- exact IANA timezone identifiers
- bundled city/place aliases derived from timezone data
- unambiguous timezone abbreviations
- optional remote city/place lookup when local search has no result

Rules:

- local search always runs first
- remote search requires at least three normalized characters
- remote search is skipped when `disable_open_meteo_geolocation` is true
- remote timezones are canonicalized and validated
- results are de-duplicated by canonical timezone and normalized place label
- Open-Meteo results include visible attribution
- selecting a result adds it; Enter chooses the first valid result
- successful addition leaves the add view ready for another query
- a duplicate or capacity violation produces an inline error
- remote failure leaves local search usable

## Map

- Add mode displays a bundled world-map image and configured markers.
- Saved coordinates are preferred for marker placement.
- Bundled timezone coordinates are used when saved coordinates are absent.
- Clicking the map invokes the backend only after the click.
- The backend resolves land coordinates through the embedded compact timezone
  grid; ocean or invalid coordinates return no location.
- Map lookup itself never calls a remote service.
- A resolved location is addable only when it is not already present and the
  capacity rule allows it.

## Pin and remove

- Edit mode exposes `PIN` for non-local locations.
- Only one location can be pinned; choosing another replaces it.
- The pin records timezone and label so two places sharing a timezone remain
  independently addressable.
- `UNPIN`, removal of the pinned place, or config normalization of an invalid
  pin returns the bar widget to icon-only display.
- Add/remove/pin mutations refresh the panel and bar state immediately.

## Backend contract and failure behavior

- `module.protocol_version` must equal the frontend's supported protocol.
- snapshot payloads must have the supported `schema_version` and a summary.
- QML must never evaluate backend output as code.
- Commands and user values are passed as argument arrays rather than shell
  strings.
- Concurrent refreshes are coalesced, and stale process responses must not
  overwrite newer editing state.
- A failed backend command produces a short inline diagnostic and leaves the
  shell process running.
- A failed or unavailable remote request must not block normal panel use.

## Acceptance checklist

- A clean `omarchy plugin add ... --enable` install works without AUR, Rust, or
  a post-install step.
- The QML always resolves the backend from its plugin directory.
- Plugin and backend versions match and the module protocol handshake passes.
- The panel toggles and coordinates with built-in Quattro panels.
- Pinning persists and updates the bar display.
- Time conversion works from the summary and every visible location.
- Local search and map lookup work offline.
- Current conditions load independently, refresh as one batched request, and
  never block clock rendering or conversion.
- Remote search is attributed and can be disabled.
- Multiple names in one timezone remain distinct.
- Artifact regeneration, Rust tests, manifest validation, and QML lint pass.
