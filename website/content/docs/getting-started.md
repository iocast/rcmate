+++
title = "Getting started"
description = "Build rcmate from source and run it for the first time."
weight = 1
+++

## Requirements

- [rclone](https://rclone.org/) installed and on your `PATH` (or pass its location with `--rclone`)
- A Rust toolchain with `cargo` to build from source

Prebuilt binaries for Linux, macOS and Windows are published on the [releases page](https://github.com/iocast/rcmate/releases).

## Build from source

```bash
git clone https://github.com/iocast/rcmate.git
cd rcmate
cargo build --release
```

The compiled binary is located in `target/release/rcmate` (or `rcmate.exe` on Windows).

## Cross-compile

```bash
# Windows (GNU)
cargo build --release --target x86_64-pc-windows-gnu

# Windows (MSVC)
cargo build --release --target x86_64-pc-windows-msvc

# Linux
cargo build --release --target x86_64-unknown-linux-gnu
```

## Run

The default configuration path is `~/.config/rcmate/config.toml`.

```bash
# Run with specific rclone config, workdir and app config
cargo run -- --rclone-config ~/.config/rcmate/rclone.conf --workdir ~/.local/state/rcmate/sync --config ~/.config/rcmate/config.toml

# Run with just the app config (uses default rclone settings)
cargo run -- --config ~/.local/state/rcmate/config.toml
```

## Next steps

- Set up your sync pairs in the [configuration](@/docs/configuration.md)
- Learn the [keybindings](@/docs/keybindings.md)
- See all [CLI arguments](@/docs/cli.md)
