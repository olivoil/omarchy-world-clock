# Natural Earth map source

`world-map.svg` is generated from the Natural Earth vector release v5.1.2:

- 1:10m Admin 0 Countries with boundary lakes
- 1:10m Minor Islands
- 1:10m Populated Places (the ranked globe city catalogue)

The pinned download URLs and SHA-256 checksums live in
`scripts/build-world-map-source.mjs` and `scripts/build-featured-cities.mjs`.
Natural Earth map data is public domain; its terms permit modification and
redistribution without required attribution:
<https://www.naturalearthdata.com/about/terms-of-use/>.

To refresh the derived assets:

```bash
node scripts/build-world-map-source.mjs
node scripts/build-featured-cities.mjs
scripts/build-world-map.sh
```

The first two commands download and verify their pinned GeoJSON inputs. They
emit the styled equirectangular SVG and the compact ranked city catalogue. The
last command renders the committed 8192×4096 PNG consumed by the globe shader.
