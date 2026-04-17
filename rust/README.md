# Omarchy World Clock Rust Preview

This directory contains the parallel Rust + GTK4 rewrite. The Python app in
`app/` remains the default implementation and install path. The Rust binary is
deliberately separate so both versions can be tested against the same config.

## Current Milestone

Implemented in this slice:

- shared config loading/saving at `~/.config/omarchy-world-clock/config.json`
- config normalization for version, legacy rows, duplicate removal, and locked rows first
- Waybar module JSON payload generation
- separate binary name: `omarchy-world-clock-rs`
- separate runtime pid file: `omarchy-world-clock-rs.pid`
- GTK4 + layer-shell popup with the panel centered under Waybar
- read-only timezone rows with live ticking clocks
- current `time_format` config is honored when rendering

Not implemented yet:

- manual reference-time conversion
- edit mode
- add/remove/lock/reorder UI
- Rust-side Waybar patch/unpatch commands

## Build

```bash
cargo build --manifest-path rust/Cargo.toml
```

Release build:

```bash
cargo build --release --manifest-path rust/Cargo.toml
```

## Run

Waybar payload preview:

```bash
cargo run --manifest-path rust/Cargo.toml -- module
```

Popup directly:

```bash
cargo run --manifest-path rust/Cargo.toml -- popup
```

Toggle the popup the same way Waybar will:

```bash
cargo run --manifest-path rust/Cargo.toml -- toggle
```

## Side-By-Side Install

Install the Rust binary without touching the Python install:

```bash
./rust/install.sh
```

This writes:

- binary payload under `~/.local/share/omarchy-world-clock-rs`
- wrapper at `~/.local/bin/omarchy-world-clock-rs`

It does not patch Waybar automatically.

## Opt-In Waybar Module

Add a separate custom module entry instead of replacing the existing Python one:

```jsonc
"custom/world-clock-rs": {
  "exec": "~/.local/bin/omarchy-world-clock-rs module",
  "return-type": "json",
  "interval": 2,
  "format": "{}",
  "tooltip": true,
  "on-click": "~/.local/bin/omarchy-world-clock-rs toggle",
  "on-click-right": "omarchy-launch-floating-terminal-with-presentation omarchy-tz-select"
}
```

Add the module next to the existing Python world clock only when you want to
compare them.

Suggested Waybar CSS:

```css
#custom-world-clock-rs {
  min-width: 12px;
  margin-left: 6px;
  margin-right: 0;
  font-size: 12px;
  opacity: 0.72;
}

#custom-world-clock-rs.active {
  opacity: 1;
}
```

## Shared Config

Both implementations read the same config file:

```text
~/.config/omarchy-world-clock/config.json
```

That is intentional. It keeps parity checks grounded in real user data while
the Rust version is still incomplete.
