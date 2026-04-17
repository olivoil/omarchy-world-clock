# Omarchy World Clock Porting Notes

This document is for future implementation work, including a possible rewrite.
`docs/specs.md` is the product source of truth. This file explains the current
architecture, compatibility requirements, and migration guidance.

## Current Stack

- Language: Python
- UI toolkit: GTK 3 via PyGObject
- Popup shell integration: `GtkLayerShell`
- Desktop target: Wayland, currently used on Hyprland
- Integration point: Waybar custom module

## Current Repository Layout

- [app/omarchy_world_clock/cli.py](/home/olivier/.t3/worktrees/omarchy-world-clock/t3code-71b369af/app/omarchy_world_clock/cli.py:1)
  Entry point, popup lifecycle, Waybar module JSON, install/uninstall commands.
- [app/omarchy_world_clock/popup.py](/home/olivier/.t3/worktrees/omarchy-world-clock/t3code-71b369af/app/omarchy_world_clock/popup.py:1)
  Native popup UI, edit mode, sorting controls, time editing, add/remove/lock,
  and drag/reorder UI.
- [app/omarchy_world_clock/configuration.py](/home/olivier/.t3/worktrees/omarchy-world-clock/t3code-71b369af/app/omarchy_world_clock/configuration.py:1)
  Config schema, migrations, timezone canonicalization, resolver, remote place
  search, sort semantics.
- [app/omarchy_world_clock/core.py](/home/olivier/.t3/worktrees/omarchy-world-clock/t3code-71b369af/app/omarchy_world_clock/core.py:1)
  Time formatting, offset formatting, timezone conversions, manual input
  parsing.
- [app/omarchy_world_clock/waybar.py](/home/olivier/.t3/worktrees/omarchy-world-clock/t3code-71b369af/app/omarchy_world_clock/waybar.py:1)
  Waybar patch/unpatch helpers.

## Current CLI Surface

The existing command surface should be treated as compatibility-sensitive:

- `omarchy-world-clock module`
- `omarchy-world-clock toggle`
- `omarchy-world-clock popup`
- `omarchy-world-clock install-waybar`
- `omarchy-world-clock uninstall-waybar`
- `omarchy-world-clock restart-waybar`

If a rewrite happens, preserving these commands will reduce migration risk.

## Persisted Data Model

Config file path:

- `~/.config/omarchy-world-clock/config.json`

Current schema:

- `version`
- `timezones[]`
- `sort_mode`
- `time_format`

Each timezone entry stores:

- `timezone`
- `label`
- `locked`

Important invariants already encoded in the current logic:

- timezone IDs are canonicalized before persistence
- duplicates are removed
- locked rows are normalized to the front
- old configs may contain plain timezone strings instead of structured objects
- config version `3` is current
- local-timezone one-time migration cutoff is version `2`

## Existing Behavior Worth Preserving

### Time Conversion Semantics

- Manual input edits the global reference instant, not just the edited row
- all rows then render that same instant in their own timezone
- time-only input is interpreted in the edited row's timezone
- successful parse exits live mode until the user clicks refresh

### Sorting Semantics

- sort applies to unlocked rows
- locked rows always remain above unlocked rows
- manual sort is the only mode where direct reordering is allowed

### Waybar Semantics

- patching is marker-based and idempotent
- unpatching is marker-based and reversible
- module payload includes tooltip rows in effective display order
- local timezone rows are labeled `· Local`

## Current Weak Area

The current implementation's main weak area is drag-and-drop reorder in
`popup.py`.

Important guidance:

- treat `docs/specs.md` as authoritative for drag behavior
- do not infer intended behavior from the current GTK 3 drag code
- the product intent is a snapped in-list reorder preview, not a floating ghost
- the final implementation should model reorder state explicitly instead of
  relying on opaque toolkit behavior

## Rewrite Guidance

If the app is rewritten, keep the following boundaries explicit:

### Recommended Separation

- pure domain logic
  - timezone parsing
  - formatting
  - config normalization
  - ordering rules
  - drag target calculation
- integration logic
  - Waybar patching
  - popup lifecycle
  - file paths
  - environment detection
- UI layer
  - rendering
  - input handling
  - drag presentation

This separation makes it possible to unit test the product rules without
depending on native GUI event delivery.

### Drag Model Recommendation

Represent reordering with explicit state:

- `drag_source_id` or source index
- `tentative_insert_index`
- `is_dragging`

Recommended flow:

1. Arm drag on pointer down.
2. Activate drag only after a movement threshold.
3. Hide or lift the source row out of normal layout.
4. Compute tentative insert position from pointer location against row
   midpoints or list slots.
5. Render a preview row in the tentative slot.
6. Commit reorder on release if the slot is valid and non-no-op.
7. Restore original row if cancelled or invalid.

This model should be implemented independently of toolkit-native DnD if the
toolkit's abstractions become a liability.

## If Rewriting in Rust

Rust may be a good long-term fit for an always-running desktop helper, but the
main benefit should be architectural control and correctness, not just raw
performance.

Potential benefits:

- stronger state modeling
- easier separation between core logic and UI
- lower long-term maintenance risk for always-on desktop software

Potential costs:

- longer initial implementation time
- more setup complexity around desktop integration
- possible friction if the chosen UI toolkit does not match Wayland/layer-shell
  needs cleanly

## Toolkit Guidance

### Python + GTK 3

- fastest to iterate in the current codebase
- already integrated with the rest of the app
- weakest part is the current drag implementation and native test loop

### Python or Rust + GTK 4

- cleaner modern event/controller model than GTK 3
- likely a better foundation for native drag behavior
- still a non-trivial migration because GTK 3 to GTK 4 is a real port

### Rust + egui

- may simplify explicit list-reorder interaction
- less tied to legacy GTK event behavior
- would require a fresh answer for popup shell integration and desktop fit

## Compatibility Targets for Any Rewrite

Any rewrite should aim to preserve:

- existing config file path
- existing config semantics
- existing CLI command names where possible
- Waybar marker-based install/uninstall behavior
- local timezone detection and `· Local` labeling
- manual input parsing semantics
- lock and sort invariants

## Testing Strategy for Future Work

The next implementation should have tests at three layers:

- pure unit tests
  - time parsing
  - ordering
  - config normalization
  - drag insert-index calculation
- integration tests
  - config round-trip
  - Waybar patch/unpatch idempotence
  - module payload formatting
- UI verification
  - popup open/close
  - edit-mode visibility
  - drag reorder happy path
  - locked row drag denial

The current test suite already covers much of the non-UI domain logic and is a
good baseline to preserve or port.

## Suggested Migration Order

If doing a rewrite, the least risky order is:

1. Port or reimplement `core.py` behavior.
2. Port config schema and normalization behavior.
3. Preserve CLI and Waybar payload behavior.
4. Rebuild popup UI around the spec.
5. Implement drag reorder last, using explicit drag state and acceptance
   scenarios from the spec.

