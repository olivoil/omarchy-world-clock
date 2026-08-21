# Architecture and Packaging

## Decision

Omarchy World Clock is a Quattro-only plugin with two cooperating parts:

1. a Quickshell/QML frontend loaded by `omarchy-shell`
2. a bundled, headless Rust executable invoked through `Quickshell.Io.Process`

Both parts live in the same plugin repository and are installed or updated by
one standard Omarchy plugin command. There is no package-manager dependency,
post-install hook, GTK application, or Waybar integration in new releases.

```text
omarchy-shell
  └─ quattro/WorldClock.qml       bar lifecycle and module status
       └─ quattro/Panel.qml       native panel, input, rendering, processes
            ├─ quattro/WorldClockKeyCatcher.qml
            ├─ quattro/Globe.qml  projection, motion, high-resolution texture
            └─ bin/omarchy-world-clock-backend
                 ├─ timezone/DST and conversion logic
                 ├─ search and batched Open-Meteo weather requests
                 ├─ config migration and atomic persistence
                 └─ embedded data/timezone-grid.bin
```

## Why the frontend is QML

The switch to QML was for Quattro integration, not because timezone logic
requires QML. Quattro is implemented with Quickshell and discovers plugin entry
points from `manifest.json`. A QML entry point can use the shell's own:

- bar widget and active-panel contracts
- `KeyboardPanel` positioning and dismissal behavior
- theme, spacing, typography, border, and opacity tokens
- focus and keyboard navigation
- one-popout-at-a-time handoff
- hot reload and plugin lifecycle

A GTK window can imitate those details but cannot participate as directly in
the long-running shell. Keeping the user interface in QML is therefore the
right architecture and is effectively required for a truly native Quattro bar
plugin.

## Why the backend remains Rust

QML contains JavaScript, so a JavaScript-only implementation is technically
possible. It is not the better boundary for this plugin:

- the QML JavaScript runtime does not provide a dependable `Temporal` API or
  arbitrary IANA timezone conversion surface across Qt versions
- DST gaps, folds, canonical aliases, and conversions are already covered by
  tested Rust/`chrono-tz` code
- config schema migration and atomic file replacement are safer outside the
  visual component lifecycle
- offline place search, optional HTTPS geocoding, and binary map parsing are
  domain/backend work rather than rendering work
- relying on Node, Bun, or Deno would merely replace the AUR dependency with a
  different runtime dependency that Omarchy plugins do not guarantee

Rust was previously coupled to a GTK UI and system package. The refactor keeps
the useful domain engine while deleting that UI and packaging coupling. A
stripped backend is larger than a script but gives predictable behavior with
no user-installed language runtime.

QML JavaScript remains appropriate for ephemeral presentation state,
debouncing, focus handoff, and mapping JSON payloads onto controls.

## Backend protocol

The executable exposes a deliberately small command/JSON boundary:

| Command | Purpose | Output |
| --- | --- | --- |
| `module` | bar tooltip, pinned label/time, compatibility handshake | JSON |
| `snapshot` | complete read/edit model at now or `--at` | JSON |
| `convert` | parse a local input and return a converted snapshot | JSON |
| `weather` | fetch current conditions for visible coordinates at now or `--at` | JSON |
| `search` | local search with optional remote fallback and live display context | JSON array |
| `locate` | map coordinate to a timezone and display model | JSON/null |
| `add` | persist a location | no output |
| `rename` | change a matching location label | no output |
| `remove` | remove a matching location | no output |
| `pin` / `unpin` | mutate the pinned location | no output |
| `version` | report the source/package version | text |

`module.protocol_version` lets QML reject an incompatible executable before
using it. Snapshot payloads have their own `schema_version`. Protocol changes
must be implemented in backend tests and QML together, then shipped in the
same commit.

Weather is deliberately outside the snapshot path. Clock refreshes remain
local and deterministic, while the panel requests all current conditions in a
single HTTPS call and keeps the last successful response for 15 minutes. A
lightweight 30-second freshness check refreshes that response at its original
expiry even when the panel closes and reopens. A weather timeout therefore
cannot delay panel opening, minute ticks, or time conversion.

Snapshots carry the effective unit inherited from `omarchy.weather`. An
explicit `shell.json` unit wins; automatic mode resolves the configured Weather
location through the embedded timezone map and system tzdata country codes,
then tries the local timezone. The frontend applies Qt's system locale only
when those inputs are inconclusive. This keeps temperature formatting aligned
with Omarchy without adding duplicate World Clock settings or coupling the
panel to another widget's loaded state.

Weather visibility is a native bar-widget setting enforced by the QML process
boundary: when disabled, the frontend never starts the weather command and
removes weather attribution with the data. The setting is deliberately not
passed to the search command, so remote place lookup remains controlled only by
the app-level `disable_open_meteo_geolocation` privacy opt-out.

The executable is not a general user CLI and does not own shell integration.
Installation, enablement, placement, updates, and removal remain the job of
the official `omarchy plugin` and `omarchy bar` commands.

## Bundle layout

```text
manifest.json
quattro/WorldClock.qml
quattro/Panel.qml
quattro/Globe.qml
quattro/WorldClockKeyCatcher.qml
assets/world-map.png
assets/world-map.svg
assets/NATURAL_EARTH.md
assets/globe.frag
assets/globe.frag.qsb
bin/omarchy-world-clock-backend
bin/BUILDINFO
bin/SHA256SUMS
data/timezone-grid.bin
data/ODbL-1.0.txt
```

The QML resolves the executable relative to its own plugin checkout. It does
not consult `PATH` or a user setting, so an old AUR binary cannot silently
pair a new frontend with an old protocol.

Bundling is preferable to an install hook because Omarchy's plugin contract is
clone, validate, enable, and load. A hook would add mutation and privilege
questions, would not make updates atomic, and is unnecessary when the runtime
artifact fits safely in the repository.

## Artifact policy

The release bundle currently targets x86-64 GNU/Linux, matching the supported
Omarchy platform. `scripts/build-plugin-backend.sh` enforces:

- Cargo and manifest version equality
- a 10 MiB stripped-binary budget
- x86-64 ELF format
- a static PIE with no dynamic interpreter or shared-library dependencies
- a working runtime version and protocol handshake
- exact reproduction of the committed binary
- checksums for the binary, lockfile, and embedded map
- exact `BUILDINFO` provenance

The backend and timezone-grid artifact scripts run inside the same
digest-pinned official Rust/Alpine container.
`scripts/build-timezone-grid.sh --check` regenerates the compact map database
from the locked `tzf-rs` source and compares it byte for byte.

`scripts/check-globe-artifacts.sh` runs the shader and texture freshness checks
inside a digest-pinned official Ubuntu image with exact Qt Shader Tools,
SPIR-V Tools, and librsvg package versions. `scripts/build-globe-shader.sh
--check` compiles the globe fragment shader there and compares the generated
QSB package byte for byte. QML owns the transient projection, motion,
zoom-dependent city reveal, and collision-aware label placement. Rust supplies
live featured-city times and resolves the generated Natural Earth city
catalogue through the embedded timezone grid. The precompiled package avoids a
runtime shader-compiler dependency.

`scripts/build-world-map.sh --check` renders the committed SVG geography into
the 8192×4096 PNG used by the GPU and verifies it byte for byte. The larger
texture keeps coastlines and boundaries crisp through the globe's extended
zoom range without adding a runtime SVG-rendering dependency.

`scripts/build-world-map-source.mjs` regenerates that SVG from checksum-pinned
Natural Earth v5.1.2 country and minor-island GeoJSON, while
`scripts/build-featured-cities.mjs` regenerates the zoom-ranked city catalogue
from the corresponding populated-place data. CI runs both generators in check
mode before validating the derived shader and PNG. The 1:10m inputs retain
roughly 87 times as many geographic points as the original simplified source,
while the 8K derived PNG deliberately trades additional texture memory for
crisper coastlines at the extended zoom ceiling. Natural Earth publishes the
source map data in the public domain.

The full upstream polygon database made the backend unnecessarily large. The
derived 0.1-degree row-run-length grid preserves click-to-add accuracy at the
panel's rendered resolution while reducing that input to roughly 211 KiB.

## Security and trust

Omarchy warns that plugins are unsandboxed code inside a long-running shell.
World Clock keeps the binary out of that process: QML launches it for bounded
commands and parses its output. That reduces crash coupling but does not turn
the plugin into a security sandbox.

Committing the exact executable makes review and checksum verification
possible and lets the plugin manager install without running build or setup
code. Maintainers must never replace it without rebuilding from the checked-in
source and passing the reproducibility checks.

## Compatibility policy

The Quattro frontend and bundled backend form one release unit. New versions do
not carry the Omarchy 3 Waybar/GTK compatibility surface. The last separate AUR
package can remain available for users who intentionally stay on that older
desktop architecture.
