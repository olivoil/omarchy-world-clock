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
- Edit mode preserves the read layout and exposes pin/unpin and remove actions.
  Location names retain their read-mode styling until clicked or selected with
  the keyboard, then become inline inputs; `Enter` saves and `Escape` cancels.
- Add mode is a map-first surface. The rotating globe fills the panel, with
  compact Back and Search controls floating above it.

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
- submitting an empty inline name removes the custom label and restores that
  friendly timezone name
- renaming a location preserves its timezone and saved coordinates
- renaming a pinned location updates the pin to the new label
- a rename cannot duplicate another normalized label in the same timezone
- invalid coordinates are discarded
- a pin must match one configured location
- removing the pinned location clears the pin
- `disable_open_meteo_geolocation` defaults to `false` and is only persisted
  when true

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

## Search and add

Search accepts:

- exact IANA timezone identifiers
- bundled city/place aliases derived from timezone data
- unambiguous timezone abbreviations
- optional remote city/place lookup when local search has no result

Rules:

- local search always runs first
- the search field stays hidden until Search or `Enter` is selected, or a
  printable key is typed; that first key becomes the first query character
- closing search returns keyboard focus to the globe surface
- remote search requires at least three normalized characters
- remote search is skipped when `disable_open_meteo_geolocation` is true
- remote timezones are canonicalized and validated
- results are de-duplicated by canonical timezone and normalized place label;
  legacy timezone-link aliases for the same canonical place are coalesced to
  the highest-scoring, human-friendly result
- the upper-right search capsule and its result surface share a width fitted
  to the widest title or subtitle, constrained to 38–75% of the space between
  the back control and right edge so short results preserve the map and long
  results remain readable
- the close action lives inside the search field; closing it restores the
  standalone Search control
- while a query is present, the result coordinates replace every configured
  and featured marker; the camera fits the full result set instead of chasing
  a single hovered result
- search results include live time, day, notation, and relative-offset context;
  locally resolved places also include bundled coordinates, allowing the globe
  to focus the result set without a remote request
- Open-Meteo results show one quiet, clickable attribution in the lower-left
  map corner while those remote results are visible
- selecting a dropdown result adds it immediately; Enter chooses the first
  valid result
- selecting the same result on the map centers it and opens an anchored detail
  card with an explicit `Add` action instead of mutating configuration
- dismissing a detail card closes any active search and returns focus to the
  globe; the next printable key opens a fresh search through the global
  type-to-search handler
- a successful addition returns to the main clock view, where the new clock is
  visible as confirmation
- a duplicate or capacity violation produces an inline error
- remote failure leaves local search usable

## Globe and map lookup

- Add mode projects a bundled 8192×4096 equirectangular world texture, derived
  from Natural Earth 1:10m country and minor-island geometry, onto an
  orthographic sphere using a precompiled Qt shader package.
- The globe occupies the full add surface. Search results and contextual
  feedback float over it instead of reducing its viewport.
- The globe opens tightly framed on the local region, with its edge outside
  the viewport so the spherical form emerges through interaction rather than
  dominating the initial view.
- Drag rotates it. Mouse-wheel angle deltas and trackpad pixel deltas both
  zoom it across an extended range; zooming out reveals the complete sphere.
- Configured places and a ranked catalogue of more than 300 capitals and
  major cities appear as front-hemisphere markers with live local times.
- Major world cities remain visible at regional scale. More capitals and
  agglomerations fade in as the globe zooms closer, using Natural Earth's
  label priority and zoom guidance.
- Configured markers take label priority. Featured labels are collision-aware;
  eligible cities whose labels do not fit remain as quiet location dots.
- Map labels use a larger bold place name over a smaller regular-weight time,
  with adaptive high-contrast text for the active light or dark theme. Search
  and configured labels remain fully opaque, while browsing markers receive
  only a subtle horizon fade.
- Clicking a featured or search marker centers it and opens a compact detail
  card with its local time, day/relative offset, timezone, and `Add` action.
- Saved coordinates are preferred for marker placement. Bundled timezone
  coordinates are used when saved coordinates are absent.
- If the shader package cannot load, the viewport falls back to the bundled
  flat map while preserving click-to-select.
- Clicking bare land invokes the backend only after the click.
- The backend resolves land coordinates through the embedded compact timezone
  grid; ocean or invalid coordinates return no location.
- Map lookup itself never calls a remote service.
- A resolved location is persisted only from the detail card and only when the
  capacity rule allows it.

## Rename, pin, and remove

- In edit mode, every configured location label is editable inline. `Enter`
  saves the trimmed label and `Escape` cancels the draft. Clearing a label
  restores the friendly timezone name.
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
- Renaming persists, including for a pinned location and places that share a
  timezone.
- Time conversion works from the summary and every visible location.
- Local search and map lookup work offline.
- Remote search is attributed and can be disabled.
- Multiple names in one timezone remain distinct.
- Artifact regeneration, Rust tests, manifest validation, and QML lint pass.
