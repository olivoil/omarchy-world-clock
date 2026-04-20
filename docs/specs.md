# Omarchy World Clock Specification

This document describes the intended product behavior of Omarchy World Clock.
It is written as the source of truth for future implementation work, including
rewrites. Where the current implementation differs, this spec wins.

## Product Summary

Omarchy World Clock adds a world-clock entry point next to Omarchy's center
Waybar clock. Clicking the icon opens a lightweight popup that shows multiple
digital clocks for a user-managed list of timezones.

The popup supports:

- viewing current time across several timezones
- switching between live time and a manually entered reference instant
- adding and removing timezones
- choosing a display format that follows the system or forces `24h` or `AM/PM`

## User-Facing Components

### Waybar Module

- A small world icon appears next to the center Waybar clock.
- The icon uses the same command wrapper as the rest of the app.
- Left click toggles the popup open and closed.
- Right click launches the Omarchy timezone selector terminal helper.
- The module tooltip is a compact text table with no title row.
- The tooltip lists configured non-local rows in the same time order used by
  the popup read view.
- If the popup is open, the module exposes an `active` class; otherwise it is
  `inactive`.

### Popup

- The popup is a top-overlay panel intended for Wayland/layer-shell use.
- The popup is visually lightweight and fast to open.
- The popup can be dismissed by clicking outside it, losing focus, or pressing
  `Escape`.
- The popup has two top-level interaction states:
  - read-only mode
  - edit mode

## Row Model

Each row represents one timezone entry with:

- canonical timezone identifier
- optional user label

Each row displays:

- title: user label if present, otherwise a friendly timezone name
- context line: canonical timezone name
- metadata line: weekday, date, abbreviation, and UTC offset
- large editable time field

If a row's timezone matches the detected local timezone, its title is annotated
with `· Local`.

## Configuration and Persistence

The app stores user state in `~/.config/omarchy-world-clock/config.json`.

Persisted settings:

- timezone rows
- row labels
- time format preference

Expected config shape:

```json
{
  "version": 3,
  "timezones": [
    {
      "timezone": "America/Cancun",
      "label": "Home"
    },
    {
      "timezone": "Europe/Paris",
      "label": "Rennes"
    }
  ],
  "time_format": "system"
}
```

Persistence rules:

- timezone names are canonicalized before being stored
- duplicate timezone entries are not allowed
- unsupported `time_format` values fall back to `system`

## Default Behavior

- Default time format is `system`.
- On first load, the detected local timezone is inserted unless it already
  exists.
## Row Ordering Behavior

- Visible rows are ordered by wall-clock time at the current reference instant.
- Equal-time rows fall back to display label ordering.
- No sort, lock, pin, manual-order, or pinned-section ordering controls are exposed.

## Time Display and Manual Reference Mode

The app normally runs in live mode:

- all rows update every second
- the reference instant is `now`

The user can click into any row's time field and type a reference instant.
When the value parses successfully:

- the app leaves live mode
- the entered row becomes the source of truth for the reference instant
- every row updates to show that same instant in its own timezone

Accepted manual input forms:

- `HH:MM`
- compact `830` or `0830`
- decimal half-hour shorthand like `8.5`
- meridiem shorthand like `3pm`, `8 am`, `12am`
- full datetime `YYYY-MM-DD HH:MM`

Time-only input is interpreted in the edited row's timezone using that
timezone's current local date at the current reference instant.

If parsing succeeds:

- the row's text is normalized to the app's display format
- all rows update to the new converted instant

If parsing fails:

- the edited field shows an error style
- a short error message appears in the popup status area
- no global reference change is committed

The refresh button returns the app to live mode and restores `now`.

## Time Format Behavior

Supported display preferences:

- `system`
- `24h`
- `ampm`

Format semantics:

- `system` follows the Waybar clock format when detectable
- if Waybar format cannot be read, the app falls back to locale-based detection
- if system detection remains ambiguous, default to `24h`

Examples:

- `24h` -> `21:26`
- `ampm` -> `9:26 PM`

## Edit Mode

Edit mode reveals controls that are hidden in read-only mode.

Read-only mode:

- rows are visible
- times can still be edited to convert timezones
- add/remove controls are hidden

Edit mode:

- time format control is visible
- add timezone controls are visible
- per-row remove buttons are visible

## Add Timezone Flow

The add panel supports:

- exact timezone identifiers
- city/place aliases
- timezone abbreviations when unambiguous
- remote place lookup fallback for unresolved queries

Search behavior:

- local timezone resolution/autocomplete runs first
- remote place search runs only when local search finds nothing useful
- remote results are canonicalized to valid supported timezones
- duplicate visible results are collapsed by canonical timezone

Add behavior:

- selecting a visible result adds that timezone to the list
- adding a timezone that already exists shows an error instead of duplicating it
- after a successful add, the panel collapses and the list refreshes

## Remove Behavior

- The remove button deletes that row from the stored config.
- Removal takes effect immediately.
- The popup refreshes immediately after removal.

## Empty State

If there are no configured non-local rows:

- the popup opens directly to the `Add a Location` screen
- no empty-state filler is shown before the add flow
- the Waybar tooltip shows `No additional timezones yet.`

## Performance and Feel

- Popup open should feel immediate.
- Live updates should be visually stable.
- Editing one row must not destroy and recreate the focused widget mid-edit.
- The app should remain suitable as a small always-running desktop helper.

## Acceptance Checklist

A future implementation is correct when all of the following are true:

- Waybar integration is idempotent and reversible.
- The popup can be toggled from Waybar.
- The tooltip reflects current configured non-local rows.
- Time conversion works from any editable row.
- `system`, `24h`, and `ampm` formats all work.
- No sort, manual reorder, or pin/unpin controls are exposed.
