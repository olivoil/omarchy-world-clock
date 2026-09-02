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
- With one or more pins, it displays the icon followed by up to three
  `CODE time` groups in pin order. Additional pins collapse into `+N`, keeping
  the bar width bounded without changing the saved pin set.
- A code is derived from the first segment of the display label: multi-word
  names use up to three initials (`New York` becomes `NY`), while single-word
  names use their first three alphanumeric characters (`Tokyo` becomes `TOK`).
- Pinned times update on minute boundaries. A vertical bar stays icon-only.
- Its tooltip is a compact, time-sorted table of up to twelve visible non-local
  places; it omits both a title row and the local summary place, then appends a
  `+N more locations` line when more places exist.
- With no additional places, the tooltip says `No additional timezones yet.`
- If the bundled backend cannot start, returns invalid output, or has an
  incompatible protocol, the widget is dimmed and its tooltip gives the
  specific recovery. A protocol mismatch tells the user to restart the shell,
  because older Omarchy releases can retain cached plugin QML across updates.

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
- Read mode shows the summary clock, proportional relative timeline, and every
  non-local location clock. Comfortable density uses a borderless three-column
  layout; automatic density switches to a denser grid when those cards would
  exceed the panel's available height. Card density is entirely responsive,
  not a user preference.
- The panel height is bounded by the shell's available-card height and its
  normal 680-pixel cap. Lists that still exceed compact density scroll inside
  that bounded surface.
- The current place uses an 18-pixel title at the default scale and sits close
  to its summary time, keeping place and time as the primary read-mode
  hierarchy.
- Live read mode shows current temperature and conditions as secondary context
  for every visible location with a known coordinate.
- Edit mode preserves the read layout and exposes pin/unpin and remove actions.
  Location names retain their read-mode styling until clicked or selected with
  the keyboard, then become inline inputs; `Enter` saves and `Escape` cancels.
- Add mode is a map-first surface. The rotating globe fills the panel, with
  compact Back and Search controls floating above it.

## Configuration and persistence

State lives in `~/.config/omarchy-world-clock/config.json`.

```json
{
  "version": 7,
  "pinned_locations": [
    {
      "timezone": "Europe/Paris",
      "label": "Rennes"
    },
    {
      "timezone": "Asia/Tokyo",
      "label": "Tokyo"
    }
  ],
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
    },
    {
      "timezone": "Asia/Tokyo",
      "label": "Tokyo",
      "latitude": 35.6764,
      "longitude": 139.65
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
- pin order is preserved and duplicate pins are discarded
- an empty label displays as a friendly timezone name
- submitting an empty inline name removes the custom label and restores that
  friendly timezone name
- renaming a location preserves its timezone and saved coordinates
- renaming a pinned location updates its matching pin to the new label
- a rename cannot duplicate another normalized label in the same timezone
- invalid coordinates are discarded
- every pin must match one configured location
- removing a pinned location clears only its matching pin
- `disable_open_meteo_geolocation` defaults to `false` and is only persisted
  when true; it disables both remote place search and current weather

On first load, the detected local timezone is added unless already present. If
the user later removes it, it is not automatically re-added.

## Ordering and scale

- Visible locations are ordered by wall-clock time at the reference instant.
- Equal-time locations fall back to display-label ordering.
- Any number of non-local locations may be configured and every configured
  place remains present in snapshots, time conversion, weather, and the panel.
- The UI never permits removing the final configured location.
- If there are no non-local locations, opening the panel goes directly to add
  mode.

## Live time and conversion

The default reference instant is `now`; display clocks refresh on minute
boundaries.

The user may drag the time ruler and release to commit a selected instant. The
selection playhead and value bubble remain fixed at the center while hour ticks
move beneath them, so releasing never appears to move the chosen value back to
the center. The ruler follows direct manipulation: dragging it right brings
earlier time under the playhead, while dragging it left brings later time under
the playhead. Horizontal or vertical two-finger trackpad scrolling follows the
same earlier/later direction, and a mouse-wheel notch moves by one hour. Wheel
and trackpad selections commit after the gesture pauses. Merely hovering or
pressing without dragging does not change the selected instant.
Pointer movement only becomes a drag after it changes the selected 15-minute
slot. Releasing on a nonexistent local time during a clock change cancels the
preview and restores the preceding state instead of leaving the ruler pending.
`Escape` unwinds time selection before dismissing the panel: it first cancels
an in-progress preview, then returns an already locked selection to live time;
another `Escape` closes the live panel.
Closing the panel by any route also discards an in-progress preview or locked
selection, requests the current instant, and makes the next open start live.
Large versus compact cards is resolved automatically from the available panel
height, and incomplete final rows stay aligned with the first grid column.

Timezone markers use calendar-aware positions instead of cyclic wrapping.
Hovering a location card highlights its corresponding marker. When that
location shares the source marker, the fixed playhead itself brightens and
expands instead of drawing a second overlapping dot.
Locations on the following day that fall beyond the visible 24-hour ruler are
grouped just past its right edge; previous-day locations are grouped just
before the left edge. A single overflow marker names its timezone and exact
wall-clock distance (`→ NZDT · +17H`). Multiple markers summarize their count
and distance range (`→ 3 ZONES · +13–17H`). Fractional-hour differences retain
their minutes. This prevents a later calendar day from appearing before the
selected source time without widening the ruler for every location.

The user may also focus the summary time or a location time and enter:

- `HH:MM`
- compact `830` or `0830`
- decimal half-hour shorthand such as `8.5`
- meridiem shorthand such as `3pm`, `8 am`, or `12am`
- `YYYY-MM-DD HH:MM`

Time-only input is interpreted in the edited place's timezone using that
timezone's current local date at the active reference instant. On success, the
input is normalized and every clock displays the same instant in its timezone.
On failure, only the edited source receives an error state and the current
reference instant is retained. Refresh ends any active time edit, moves focus
back to the panel, and returns every clock to live time.

Live cards describe their date as `Yesterday`, `Today`, or `Tomorrow` relative
to the local summary place. While previewing or viewing a selected time, cards
instead use `Previous day`, `Same day`, or `Next day` relative to the selected
timeline source. The timeline selection includes that source's compact date and
remains visible after the selected time is locked.

While previewing or viewing a selected time, every location card also shows a
passive local-day ruler seated on its bottom edge. The common left-to-right
scale runs from 00:00 to 24:00. A clipped glow begins at apparent sunrise,
follows the sun's calculated elevation to its solar-noon peak, and returns to
the edge at sunset. Latitude, longitude, displayed local date, and that civil
day's time-zone offset transitions drive the NOAA-based profile locally,
without an Open-Meteo request. Daylight cycles crossing midnight split cleanly
at the ruler boundary. Square-root visual compression keeps low winter arcs
legible while preserving their lower peak relative to summer and tropical arcs.
Polar day retains its full-day elevation profile, polar night stays dim, and
missing coordinates fall back to a neutral edge. A half-height marker rises
from the edge at that card's
`local_minutes`; its restrained color and contrast interpolate from cool, muted
night through twilight to warm, bright daylight using the current solar
elevation.
The rulers disappear in live and edit modes, never replace the printed time,
and do not label or imply availability. They are not an additional
time-scrubbing target.

Pressing `Enter` in read mode focuses and selects the summary time for direct
replacement. Typing a decimal digit while no field has focus does the same and
uses that digit as the first character. Typing an alphabetic character instead
opens Add mode, focuses location search, and preserves that character as the
start of the query.

Core clock management remains reachable without moving through toolbar buttons:

- `F2` enters or leaves edit mode
- the arrow keys select the summary or a location card
- `Enter` edits the selected time in read mode or its name in edit mode
- `P` pins or unpins the selected clock in edit mode
- `Delete` or `X` removes the selected non-summary clock in edit mode
- `Ctrl+T` focuses the time ruler; `Left` and `Right` preview, `Enter` locks,
  and `Home` returns to live time
- `Escape` unwinds the active detail, search, edit, or time-preview state before
  closing the panel

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
- Visible coordinates are fetched in bounded batches when the panel opens, so
  long location lists do not create an unbounded request URL or response.
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
- the search field stays hidden until Search or `Enter` is selected, or a
  printable key is typed; that first key becomes the first query character
- typing a location from the main clock view opens search on a stationary
  whole-globe overview, reserving the next camera move for the result set;
  explicitly choosing Add still plays the location-arrival flight
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
  and featured marker; the first result is selected and focused instead of
  fitting distant matches into one globe view
- `Up` and `Down` move the selected dropdown row, keep it visible when the list
  scrolls, and move the globe to that result without moving focus from search
- search results include live time, day, notation, and relative-offset context;
  locally resolved places use their exact tzdata coordinate when available and
  a separate canonical-zone focus coordinate otherwise, allowing every valid
  result to move the globe without saving an approximate place coordinate
- Open-Meteo results show one quiet, clickable attribution in the lower-left
  map corner while those remote results are visible
- selecting a dropdown result, or pressing Enter to choose the selected
  result, centers it and opens an anchored detail card with an explicit `Add`
  action instead of mutating configuration
- keyboard selection moves focus to that `Add` action; pressing Enter or Space
  confirms it, including when the initial Enter had to wait for search results
- selecting the same result from its map marker follows the identical preview
  and confirmation flow
- dismissing a detail card closes any active search and returns focus to the
  globe; the next printable key opens a fresh search through the global
  type-to-search handler
- a successful addition returns to the main clock view, where the new clock is
  visible as confirmation
- a duplicate place produces an inline error
- remote failure leaves local search usable

## Globe and map lookup

- Add mode immediately projects a warm 2048×1024 preview derived from Natural
  Earth 1:10m country and minor-island geometry onto an orthographic sphere.
  It retains that preview while the bundled 8192×4096 detail texture loads
  asynchronously through the same precompiled Qt shader package.
- The detail texture stays available when moving between add and read modes in
  one panel session, then is released when the panel closes.
- The globe occupies the full add surface. Search results and contextual
  feedback float over it instead of reducing its viewport.
- When Add is opened explicitly, the completely visible globe immediately
  begins easing the destination in from beyond the near horizon. Rotation and
  zoom share one 1.1-second camera flight, so the globe arrives as a continuous
  movement at a comfortable regional scale. Place labels stay quiet during the
  flight and fade in on arrival. Direct type-to-search holds the same overview
  still instead, avoiding a competing camera move before results appear. The
  opening target prefers the local summary's
  saved coordinate, then its timezone representative, followed by the first
  pinned location by its full place identity, or otherwise the first configured
  place; UTC-only setups use Greenwich as their visual anchor.
- Drag rotates it. Mouse-wheel angle deltas and trackpad pixel deltas both
  zoom it across an extended range; zooming out reveals the complete sphere.
- Up to 48 configured places and a ranked catalogue of more than 300 capitals
  and major cities appear as front-hemisphere markers with live local times.
  The local summary and pinned places take priority in that saved-marker cap;
  every saved place remains available as a card and excluded from add targets.
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
- Focusing a search result may zoom closer when needed but never zooms out from
  the user's current view.
- Saved coordinates are preferred for marker placement. Bundled timezone
  coordinates are used when saved coordinates are absent.
- If the shader package cannot load, the viewport falls back to the bundled
  flat map while preserving click-to-select.
- Clicking bare land invokes the backend only after the click.
- The backend resolves land coordinates through the embedded compact timezone
  grid; ocean or invalid coordinates return no location.
- Map lookup itself never calls a remote service.
- A resolved location is persisted only from the detail card's explicit Add
  action.

## Rename, pin, and remove

- In edit mode, every configured location label is editable inline. `Enter`
  saves the trimmed label and `Escape` cancels the draft. Clearing a label
  restores the friendly timezone name.
- Edit mode exposes `PIN` for non-local locations.
- Any number of configured locations can be pinned; pinning another appends it
  without disturbing earlier pins, and pinning the same location is idempotent.
- Each pin records timezone and label so two places sharing a timezone remain
  independently addressable.
- `UNPIN` removes only the selected location. Removal or config normalization
  also discards only pins that no longer match configured locations.
- The bar returns to icon-only display after the final pin is removed.
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
- Multiple pins persist and update independently; the first three appear in
  the bar and any remainder appears as a `+N` summary.
- Renaming persists, including for a pinned location and places that share a
  timezone.
- Time conversion works from the summary and every visible location.
- More than nine saved places remain visible; automatic compact density keeps
  typical long lists together and the panel scrolls when compact rows still do
  not fit.
- Large versus compact card density is always resolved automatically.
- The time-rail playhead stays centered while dragging moves the hour ruler in
  the grabbed direction; trackpad and mouse-wheel scrubbing use the same
  earlier/later model, and hovering or pressing without dragging does not
  change the clocks.
- With no field focused in read mode, a decimal digit starts editing the local
  summary time and an alphabetic character starts an Add-mode location search.
- Preparing the time rail keeps its 288 source-time slots independent of the
  location count. Each location contributes only compact UTC-offset states,
  including any DST transition in the rail window; the panel derives visible
  card times locally instead of receiving a full clock copy in every slot.
- Next-day markers never wrap to the left of the selected source; out-of-range
  day markers appear beyond the appropriate ruler edge with their real offset,
  and multiple markers collapse into a count and distance range. Card hover
  follows the location identities carried by each marker, so equal wall-clock
  minutes on different dates do not highlight one another.
- Local search and map lookup work offline.
- Current conditions load independently, refresh in bounded batches, and
  never block clock rendering or conversion.
- Remote search is attributed and can be disabled.
- Multiple names in one timezone remain distinct.
- Artifact regeneration, Rust tests, manifest validation, and QML lint pass.
