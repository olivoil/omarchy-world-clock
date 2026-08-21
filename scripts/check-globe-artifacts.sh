#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(CDPATH= cd -- "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$REPO_ROOT"
source scripts/build-environment.sh

# Official Ubuntu 26.04 image, pinned by manifest digest. Its exact package
# versions reproduce the QSB and PNG committed by this repository.
readonly GLOBE_ARTIFACT_IMAGE='docker.io/library/ubuntu@sha256:2260313b31c8c011cd2eebe728008efac1b3982be73eb71348ea2648d2c0e09b'
readonly QSB_PACKAGE='qt6-shader-baker=6.10.2-1'
readonly SPIRV_PACKAGE='spirv-tools=2026.1-1'
readonly RSVG_PACKAGE='librsvg2-bin=2.61.3+dfsg-3'

engine=$(build_container_engine)
"$engine" run --rm \
  --volume "$REPO_ROOT:/work:ro" \
  --workdir /work \
  --env LANG=C.UTF-8 \
  "$GLOBE_ARTIFACT_IMAGE" \
  bash -c '
    set -euo pipefail
    apt-get update -qq
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      '"$QSB_PACKAGE"' '"$SPIRV_PACKAGE"' '"$RSVG_PACKAGE"' >/dev/null
    scripts/build-globe-shader.sh --check
    scripts/build-world-map.sh --check
  '
