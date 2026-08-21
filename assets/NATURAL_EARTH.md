# Natural Earth map source

`world-map.svg` is generated from the Natural Earth vector release v5.1.2:

- 1:10m Admin 0 Countries with boundary lakes
- 1:10m Minor Islands

The pinned download URLs and SHA-256 checksums live in
`scripts/build-world-map-source.mjs`. Natural Earth map data is public domain;
its terms permit modification and redistribution without required attribution:
<https://www.naturalearthdata.com/about/terms-of-use/>.

To refresh the derived assets:

```bash
node scripts/build-world-map-source.mjs
scripts/build-world-map.sh
```

The first command downloads and verifies the pinned GeoJSON inputs, then emits
the styled equirectangular SVG. The second renders the committed 8192×4096 PNG
consumed by the globe shader.
