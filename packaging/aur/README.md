# AUR Packaging

The `omarchy-world-clock-bin` package files live in:

```text
packaging/aur/omarchy-world-clock-bin
```

This package installs the prebuilt GitHub release binary and project `LICENSE`
from the release archive. It intentionally does not patch user bar config
during package installation. The package post-install message tells users to
run the one-time `omarchy-world-clock install` command, which detects the
Omarchy 4 shell or Omarchy 3 Waybar. On Omarchy 4, that command installs the
repository's native Quattro plugin and migrates the old command widget.

## Initial Publish

Create or verify your AUR SSH access, then initialize the AUR Git repository:

```bash
git clone ssh://aur@aur.archlinux.org/omarchy-world-clock-bin.git aur-omarchy-world-clock-bin
cd aur-omarchy-world-clock-bin
```

Copy the package files from this repository into the AUR checkout:

```bash
cp /path/to/omarchy-world-clock/packaging/aur/omarchy-world-clock-bin/{PKGBUILD,.SRCINFO,omarchy-world-clock-bin.install} .
```

Validate locally:

```bash
makepkg --printsrcinfo > .SRCINFO
makepkg -f
```

Publish:

```bash
git add PKGBUILD .SRCINFO omarchy-world-clock-bin.install
git commit -m "Initial import"
git push
```

## Updating For A New Release

1. Publish the GitHub release asset first.
2. Update `pkgver` in `PKGBUILD` and reset `pkgrel=1`.
3. Update `sha256sums_x86_64`.
4. Regenerate `.SRCINFO`.
5. Run `makepkg -f`.
6. Commit and push the AUR repository.

Helpful commands:

```bash
updpkgsums
makepkg --printsrcinfo > .SRCINFO
makepkg -f
git diff
```
