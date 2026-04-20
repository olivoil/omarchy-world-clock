# Omarchy World Clock Specification

This document describes the intended product behavior of Omarchy World Clock.
Where implementation and documentation differ, this document should be updated
or the implementation should be fixed so the project has one clear product
surface.

## Product Summary

Omarchy World Clock adds a world-clock entry point next to Omarchy's center
Waybar clock. Clicking the icon opens a lightweight popup that shows live clocks
for a user-managed list of places.

The popup supports:

- viewing the current time across configured places
- switching between live time and a manually entered reference instant
- adding places through local timezone search and optional remote geocoding
- removing configured places
- displaying times with the system time format

The popup does not include row locking, row sorting controls, drag reordering,
or user-selectable time format controls.

## Waybar Module

- A small world icon appears next to the center Waybar clock.
- Left click toggles the popup open and closed.
- Right click launches the Omarchy timezone selector terminal helper.
- The module tooltip is a compact text table with no title row.
- The tooltip lists configured non-local entries in the same time order used by
  the popup read view.
- If no additional timezones are configured, the tooltip shows
  `No additional timezones yet.`
- If the popup is open, the module exposes an `active` class; otherwise it is
  `inactive`.

## Popup

- The popup is a top-overlay panel intended for Wayland/layer-shell use.
- The popup can be dismissed by clicking outside it, losing focus, or pressing
  `Escape`.
- The popup adapts colors to the active Omarchy theme palette.
- The popup has three interaction states:
  - read mode
  - edit mode
  - add mode
- Read mode shows the summary clock, a relative timeline, and clock cards for
  configured non-local entries.
- Edit mode keeps the read layout but exposes card remove buttons when removal
  is valid.
- Add mode shows a search entry, search results, and a map with configured
  place markers.

## Configuration and Persistence

State lives in `~/.config/omarchy-world-clock/config.json`.

Persisted settings:

- configured timezone entries
- optional display labels
- optional latitude and longitude for map placement
- optional Open-Meteo geocoding opt-out

Expected config shape:

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

Persistence rules:

- timezone names are canonicalized before being stored
- duplicate timezone entries are not allowed
- saved order is preserved
- empty labels are allowed and display as friendly timezone names
- invalid coordinates are dropped
- `disable_open_meteo_geolocation` defaults to `false`
- `disable_open_meteo_geolocation` is only persisted when true

## Default Behavior

- On first load, the detected local timezone is inserted unless it already
  exists.
- Older configs are migrated forward transparently.
- If the user later removes the local timezone entry, it is not automatically
  re-added after migration has already run.
- If there are no configured non-local entries, the popup opens directly to the
  add screen.

## Clock Ordering

- Visible clock cards are ordered by wall-clock time at the current reference
  instant.
- Equal-time cards fall back to display label ordering.
- No sort, manual reorder, lock, pin, or pinned-section ordering controls are
  exposed.

## Time Display and Manual Reference Mode

The app normally runs in live mode:

- all visible clocks update every second
- the reference instant is `now`

The user can click into the summary clock or any visible clock card and type a
reference instant. When the value parses successfully:

- the app leaves live mode
- the edited clock becomes the source of truth for the reference instant
- every visible clock updates to show that same instant in its own timezone

Accepted manual input forms:

- `HH:MM`
- compact `830` or `0830`
- decimal half-hour shorthand like `8.5`
- meridiem shorthand like `3pm`, `8 am`, `12am`
- full datetime `YYYY-MM-DD HH:MM`

Time-only input is interpreted in the edited clock's timezone using that
timezone's current local date at the current reference instant.

If parsing succeeds:

- the edited text is normalized to the app's display format
- all visible clocks update to the new converted instant

If parsing fails:

- the edited field shows an error style
- a short error message appears in the popup status area
- no global reference change is committed

The refresh button returns the app to live mode and restores `now`.

## Time Format Behavior

The only user-facing display format is the system time format.

Detection order:

- follow the Waybar clock format when detectable
- fall back to locale-based detection
- default to `24h` if system detection remains ambiguous

Examples:

- system 24-hour format displays `21:26`
- system 12-hour format displays `9:26 PM`

## Add Place Flow

The add panel supports:

- exact timezone identifiers
- bundled city/place aliases derived from timezone data
- timezone abbreviations when unambiguous
- optional remote place lookup fallback for unresolved queries

Search behavior:

- local timezone resolution/autocomplete runs first
- remote place search runs only when local search finds no results
- remote place search runs only for normalized queries with at least three
  characters
- remote place search is skipped when `disable_open_meteo_geolocation` is true
- remote results are canonicalized to valid supported timezones
- duplicate visible results are collapsed by canonical timezone
- Open-Meteo-sourced results show inline attribution next to the result metadata

Add behavior:

- selecting a visible result adds that timezone to the list
- pressing Enter adds the first matching result or the exact timezone
- adding a timezone that already exists shows an error instead of duplicating it
- after a successful add, the panel stays ready for another search

## Map Behavior

- The add screen shows a world map with markers for configured places.
- Saved latitude and longitude are preferred for marker placement.
- Bundled tzdata coordinates are used when available.
- Local timezone alias data is used as a fallback when it has coordinates.
- The map must not call remote services just to backfill missing marker
  coordinates.
- Clicking a map marker adds the corresponding timezone when it is not already
  configured and capacity allows.
- Hovering a marker shows the place name, time, timezone metadata, and relative
  offset.

## Open-Meteo Use

Open-Meteo geocoding is enabled by default because it is only used for explicit
place search. It can be disabled by setting:

```json
{
  "disable_open_meteo_geolocation": true
}
```

When enabled, the app may send the user's city/place search text to:

```text
https://geocoding-api.open-meteo.com/v1/search
```

Open-Meteo requirements reflected in the product:

- use is limited to non-commercial/free API terms unless the project changes to
  a paid API arrangement
- single-user app usage is expected to stay far below the free API rate limits
- Open-Meteo-sourced results include an inline Open-Meteo link
- README privacy notes must disclose the remote lookup and opt-out

## Remove Behavior

- The remove button deletes that entry from the stored config.
- Removal takes effect immediately.
- The popup refreshes immediately after removal.
- The UI must not allow removing the final configured entry.

## Empty State

If there are no configured non-local entries:

- the popup opens directly to the `Add a Location` screen
- no empty-state filler is shown before the add flow
- the Waybar tooltip shows `No additional timezones yet.`

## Performance and Feel

- Popup open should feel immediate.
- Live updates should be visually stable.
- Editing one clock must not destroy and recreate the focused widget mid-edit.
- Remote search failures should fail quietly and leave local search usable.
- The app should remain suitable as a small always-running desktop helper.

## Acceptance Checklist

A correct implementation satisfies these behaviors:

- Waybar integration is idempotent and reversible.
- The popup can be toggled from Waybar.
- The tooltip reflects current configured non-local entries.
- Time conversion works from the summary clock and every visible clock card.
- Display format follows the system format only.
- Lock, sort, drag, and time-format settings are absent from the UI.
- Local timezone search works without network access.
- Open-Meteo geocoding can be disabled with
  `disable_open_meteo_geolocation`.
- Open-Meteo results are attributed inline when shown.
