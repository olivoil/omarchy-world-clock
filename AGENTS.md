# Repository agent instructions

## Local review plugins

When a user-visible World Clock branch is ready for local review on an Omarchy
workstation, run `scripts/install-review-preview.sh` before handoff unless the
user explicitly asks not to install it.

- Never overwrite or repurpose the canonical
  `io.github.olivoil.world-clock` install; keep it as the comparison baseline.
- Review builds must use the script's collision-resistant branch-derived plugin
  ID, visible review icon color, branch tooltip header, and branch-isolated
  config file.
- Before removing old World Clock review installs, audit live work rather than
  treating the existence of a Git worktree as proof that it is active. On T3
  Code workstations, processes whose `/proc/<pid>/cwd` points into a worktree
  are strong evidence that its branch is active.
- Preserve every preview associated with live work. Remove only inactive
  review IDs with `omarchy plugin remove <id> --yes`; do not delete the
  canonical install or backup directories created by Omarchy.
- Report which review IDs were installed, retained, and removed. A review
  config lives under
  `~/.local/state/omarchy-world-clock/reviews/` and must not replace the user's
  canonical `~/.config/omarchy-world-clock/config.json`.

Run the normal repository verification and rebuild the bundled backend before
installing a review copy, so the installed QML and backend protocol always
move together.
