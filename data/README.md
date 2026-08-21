# Timezone map data

`timezone-grid.bin` is a deterministic 0.1-degree, row-run-length-encoded
lookup grid generated from the reduced timezone-boundary data shipped by
[`tzf-rel` 0.0.2026-a](https://github.com/ringsaturn/tzf-rel). The generator is
checked in at `examples/generate_timezone_grid.rs`.

The grid deliberately omits `Etc/*` ocean zones. Its resolution is finer than
the World Clock panel's rendered map, while reducing the runtime payload from
the roughly 11 MiB embedded polygon database to about 211 KiB.

The source timezone-boundary database and this derived database are available
under the Open Database License 1.0. See `ODbL-1.0.txt`. Individual contents
may be subject to the Database Contents License; consult the upstream
[`tzf-rel` project](https://github.com/ringsaturn/tzf-rel) for its current
attribution and source information.

Regenerate the grid with:

```bash
scripts/build-timezone-grid.sh
```

Use `scripts/build-timezone-grid.sh --check` to regenerate into an ignored
build directory and verify byte-for-byte reproducibility without changing the
committed data. Both operations use the project's digest-pinned build
container and require Podman or Docker.

Then rebuild the bundled backend because the grid is embedded at compile time:

```bash
scripts/build-plugin-backend.sh
```

## Featured cities

`featured-cities.json` is a compact catalogue of ranked capitals and major
agglomerations generated from Natural Earth v5.1.2 Populated Places. It keeps
Natural Earth's label ordering and maps its recommended zoom levels onto the
globe's zoom range. The backend resolves each coordinate through the embedded
timezone grid, then supplies its live local time to QML.

Natural Earth data is public domain. Source URLs and checksums are pinned in
`scripts/build-featured-cities.mjs`; additional provenance is documented in
`assets/NATURAL_EARTH.md`.

Regenerate and verify the catalogue with:

```bash
node scripts/build-featured-cities.mjs
node scripts/build-featured-cities.mjs --check
```

Rebuild the bundled backend afterward because the catalogue is embedded at
compile time.
