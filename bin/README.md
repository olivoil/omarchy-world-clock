# Bundled backend

`omarchy-world-clock-backend` is the headless x86-64 Rust backend used by the
QML plugin. It is committed so `omarchy plugin add` installs a complete plugin
without a package manager, compiler, or post-install hook.

Rebuild it from the checked-in source and lockfile with:

```bash
scripts/build-plugin-backend.sh
```

Verify that a clean rebuild matches the committed executable with:

```bash
scripts/build-plugin-backend.sh --check
```

The executable targets x86-64 Linux as a static PIE built with musl, so it does
not inherit glibc or shared-library requirements from the maintainer's rolling
Arch installation. `BUILDINFO` records the protocol, source version, pinned
build image, toolchain, size, linkage, and input/output checksums;
`SHA256SUMS` provides the directly verifiable executable checksum.
