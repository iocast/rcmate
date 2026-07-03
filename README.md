# rcmate

**Your Friendly Rclone Companion**

A Terminal User Interface (TUI) for automating, monitoring, and managing [rclone](https://rclone.org/) synchronization tasks.

[![License](https://img.shields.io/badge/license-Apache%202.0-blue?style=flat-square)](LICENSE-APACHE)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE-MIT)
[![GitHub Release](https://img.shields.io/github/v/release/iocast/rcmate?style=flat-square&color=blue)](https://github.com/iocast/rcmate/releases)
[![Build Status](https://img.shields.io/github/actions/workflow/status/iocast/rcmate/release.yml?style=flat-square&label=build)](https://github.com/iocast/rcmate/actions)


## ✨ Features

- **Interactive TUI**: Built with `ratatui` for a fast, keyboard-driven terminal experience.
- **Multiple Sync Modes**: Supports `sync`, `bisync`, `copy`, and `move` operations.
- **Real-time Progress**: Connects to the `rclone` RC (Remote Control) API to display live transfer stats and progress bars.
- **Smart File Handling**: Automatically detects and translates single-file operations into directory operations with includes.
- **Highly Configurable**: Define sync pairs, paths, and filters via a simple TOML configuration file.
- **CLI Overrides**: Easily override configuration paths, rclone binaries, and logging levels via command-line arguments.

## 📦 Installation & Building

### Pre-compiled Binaries
You can cross-compile `rcmate` for various platforms using Rust's `cargo`:

```zsh
# Windows (GNU)
cargo build --release --target x86_64-pc-windows-gnu

# Windows (MSVC)
cargo build --release --target x86_64-pc-windows-msvc

# Linux
cargo build --release --target x86_64-unknown-linux-gnu
```

### From Source

```zsh
git clone https://github.com/yourusername/rcmate.git
cd rcmate
cargo build --release
```

The compiled binary will be located in `target/release/rcmate` (or `rcmate.exe` on Windows).

## 🚀 Usage & Testing

Run the application using `cargo` or the compiled binary. The default configuration path is `~/.local/share/rcmate/config.toml`.

```powershell
# Run with specific rclone config, workdir, and app config
cargo run -- --rclone-config ~/.local/share/rcmate/rclone.conf --workdir ~/.local/share/rcmate/sync --config ~/.local/share/rcmate/config.toml

# Run with just the app config (uses default rclone settings)
cargo run -- --config ~/.local/share/rcmate/config.toml
```

### CLI Arguments

- `--config <PATH>`: Path to the rcmate TOML config file (Default: `~/.local/share/rcmate/config.toml`).
- `--rclone <BIN>`: Path or name of the rclone binary (Default: `rclone`).
- `--rclone-config <PATH>`: Path to the rclone configuration file.
- `--workdir <PATH>`: Working directory for rclone (useful for bisync).
- `--log-path <PATH>`: Path to the log file or directory.
- `--log-level <LEVEL>`: Log level (`trace`, `debug`, `info`, `warn`, `error`). Default: `info`.
- `-v, --version`: Print version information and exit.


## 🚀 Building and Releasing

Binaries for Linux, macOS, and Windows are automatically compiled and published using GitHub Actions.

### Triggering a Release
The build and release process is automatically triggered when you push a Git tag that starts with `v`.

**Prerequisites:**
Before creating a tag, ensure the version is consistent across your project files. The workflow will abort the build if it detects a mismatch:
- `Cargo.toml` (the `version` field)
- `CHANGELOG.md` (the latest version in an H2 header, e.g., `## 1.0.0`)

**Steps to Release:**
1. Update the version in `Cargo.toml` and `CHANGELOG.md`.
2. Commit and push your changes:
   ```bash
   git add Cargo.toml CHANGELOG.md
   git commit -m "Release v1.0.0"
   git push
   ```
3. Create and push a new Git tag matching the version:
   ```bash
   git tag v1.0.0
   git push origin v1.0.0
   ```

Once the tag is pushed, the workflow will validate the versions, compile the binaries for all target platforms, and automatically create a GitHub Release with the attached artifacts.

### Manual Trigger

You can also trigger the workflow manually from the GitHub UI:
1. Navigate to the **Actions** tab in your repository.
2. Select the **Build and Release** workflow from the left sidebar.
3. Click the **Run workflow** dropdown and select **Run workflow**.

*(Note: Manual triggers still execute the version validation step. Ensure your `Cargo.toml` and `CHANGELOG.md` versions are properly aligned before triggering manually.)*


## ⚙️ Configuration

`rcmate` uses a TOML file for configuration. Below is an example of the expected structure:

```toml
[general]
log_path = "~/.local/share/rcmate/logs"
log_level = "info"

[rclone]
bin = "rclone"
# config = "~/.config/rclone/rclone.conf"
# workdir = "~/.local/share/rcmate/workdir"

[[sync_pairs]]
name = "Documents Sync"
type = "bisync"
source = "/local/documents"
destination = "remote:documents"
# excludes = ["*.tmp", "node_modules/"]
# includes = ["*.pdf"]

[[sync_pairs]]
name = "Media Copy"
type = "copy"
source = "/local/media"
destination = "remote:media"
```

## 📖 TUI Keybindings

*(Update these based on your actual `views/` implementations)*
- `↓`: Move selection down
- `↑`: Move selection up
- `SPACE`: Toggle selection for sync
- `ALT+s`: Start synchronization for selected pairs
- `Shift + Alt + I`: Open About/Info dialog
- `q` / `Ctrl+C`: Quit application

## 📄 License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## 🤝 Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
