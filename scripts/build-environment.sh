#!/usr/bin/env bash

# Platform-specific digest for the official Rust 1.97 Alpine image. Pinning the
# complete build userspace makes committed artifacts reproducible across hosts.
readonly BUILD_CONTAINER_IMAGE='docker.io/library/rust@sha256:83b16fd204613a557527e81ef4f87fd513229187c49635e3c7f96b08dfa43c33'

build_container_engine() {
  if [[ -n ${OMARCHY_WORLD_CLOCK_CONTAINER_ENGINE:-} ]]; then
    if command -v "$OMARCHY_WORLD_CLOCK_CONTAINER_ENGINE" >/dev/null 2>&1 \
      && [[ $OMARCHY_WORLD_CLOCK_CONTAINER_ENGINE =~ ^(podman|docker)$ ]]; then
      printf '%s\n' "$OMARCHY_WORLD_CLOCK_CONTAINER_ENGINE"
      return 0
    fi
    printf 'OMARCHY_WORLD_CLOCK_CONTAINER_ENGINE must name an installed podman or docker command.\n' >&2
    return 1
  fi
  if command -v podman >/dev/null 2>&1; then
    printf 'podman\n'
  elif command -v docker >/dev/null 2>&1; then
    printf 'docker\n'
  else
    printf 'A container engine (podman or docker) is required to reproduce bundled artifacts.\n' >&2
    return 1
  fi
}

run_build_container() {
  local engine
  local -a user_args=()
  engine=$(build_container_engine) || return 1
  if [[ $engine == docker ]]; then
    user_args=(--user "$(id -u):$(id -g)")
  fi
  mkdir -p "$REPO_ROOT/target/container-cargo-home" \
    "$REPO_ROOT/target/container-target" \
    "$REPO_ROOT/target/plugin-backend-dist"
  "$engine" run --rm \
    "${user_args[@]}" \
    --volume "$REPO_ROOT:/work" \
    --workdir /work \
    --env CARGO_HOME=/work/target/container-cargo-home \
    --env CARGO_TARGET_DIR=/work/target/container-target \
    --env CFLAGS=-ffile-prefix-map=/work=. \
    --env RUSTFLAGS=--remap-path-prefix=/work=. \
    --env RUSTUP_TOOLCHAIN=1.97.1-x86_64-unknown-linux-musl \
    --env SOURCE_DATE_EPOCH=1 \
    "$BUILD_CONTAINER_IMAGE" "$@"
}
