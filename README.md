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

Run the application using `cargo` or the compiled binary. The default configuration path is `~/.config/rcmate/config.toml`.

```powershell
# Run with specific rclone config, workdir, and app config
cargo run -- --rclone-config ~/.config/rcmate/rclone.conf --workdir ~/.local/state/rcmate/sync --config ~/.config/rcmate/config.toml

# Run with just the app config (uses default rclone settings)
cargo run -- --config ~/.local/state/rcmate/config.toml
```

### CLI Arguments

- `--config <PATH>`: Path to the rcmate TOML config file (Default: `~/.config/rcmate/config.toml`).
- `--rclone <BIN>`: Path or name of the rclone binary (Default: `rclone`).
- `--rclone-config <PATH>`: Path to the rclone configuration file.
- `--workdir <PATH>`: Working directory for rclone (useful for bisync).
- `--log-path <PATH>`: Path to the log file or directory.
- `--log-level <LEVEL>`: Log level (`trace`, `debug`, `info`, `warn`, `error`). Default: `info`.
- `-v, --version`: Print version information and exit.

### Updating

```zsh
rcmate self-update
```

Checks the GitHub releases for a newer version, shows the release notes of every version since the installed one, and replaces the running binary only after you confirm. Pre-releases are ignored. If the binary sits in a system directory such as `/usr/local/bin`, run it with `sudo`.

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

_(Note: Manual triggers still execute the version validation step. Ensure your `Cargo.toml` and `CHANGELOG.md` versions are properly aligned before triggering manually.)_

## ⚙️ Configuration

`rcmate` uses a TOML file for configuration. Below is an example of the expected structure:

```toml
[general]
log_path = "~/.local/state/rcmate/logs"
log_level = "info"

[rclone]
bin = "rclone"
# config = "~/.config/rclone/rclone.conf"
# workdir = "~/.local/state/rcmate/workdir"

[[sync_pairs]]
name = "Documents Sync"
type = "bisync"
source = "/local/documents"
destination = "remote:documents"
# excludes = ["*.tmp", "node_modules/"]
# includes = ["*.pdf"]

# Options are per sync pair and are written back by the Options popup (`o`).
[sync_pairs.options]
dry_run = false
create_empty_src_dirs = false
resync = false
resync_mode = "none"   # none | path1 | path2 | newer | older | larger | smaller
force = false
check_access = false

[[sync_pairs]]
name = "Media Copy"
type = "copy"
source = "/local/media"
destination = "remote:media"
```

Which options apply depends on the pair's `type`, mirroring what the rclone rc
API accepts for each operation:

| Option                                           | bisync | sync | copy | move |
| ------------------------------------------------ | :----: | :--: | :--: | :--: |
| `dry_run`                                        |   ✓    |  ✓   |  ✓   |  ✓   |
| `create_empty_src_dirs`                          |   ✓    |  ✓   |  ✓   |  ✓   |
| `delete_empty_src_dirs`                          |        |      |      |  ✓   |
| `resync`, `resync_mode`, `force`, `check_access` |   ✓    |      |      |      |

The Options popup only shows the options that apply to the highlighted pair,
and only those are sent to rclone. Options not listed for a type are still
kept in the config, so changing a pair's type back does not lose them.

Applicability also depends on the paths. A pair whose source or destination is
a single file runs against the parent directories with an include filter for
that file name — and, for `type = "sync"`, as two opposite `copy --update`
jobs. There are no source directories to mirror in that case, so
`create_empty_src_dirs` does not apply: the popup does not offer it, it is not
sent to rclone, and it is not written to the config file for that pair.

### Choosing a sync type

| Type     | Direction  | Deletes                                         | Use for                                  |
| -------- | ---------- | ----------------------------------------------- | ---------------------------------------- |
| `sync`   | one-way    | on the destination, to match the source         | an exact mirror / backup                 |
| `copy`   | one-way    | never                                           | a backup that keeps deleted files        |
| `move`   | one-way    | on the source, after transferring               | moving files off a source                |
| `bisync` | two-way    | on either side, mirroring deletions             | keeping two places in step               |

**`sync` mirrors.** The destination is made identical to the source, so a file
deleted on the source is deleted from the destination on the next run. If the
source is a mount point that is not mounted, it looks like an empty directory
and `sync` empties the destination to match. Check that both sides are mounted,
or use an rclone remote (e.g. `nas:/home/user`), which fails when
unreachable instead of looking empty. Run once with `dry_run` first.

**Single file: `sync` vs. `bisync`.** For a single file, `sync` is not a
mirror: it runs two opposite `copy --update` jobs, so the newer side
overwrites the older one. If the file changed on both sides since the last
run, the older side's changes are lost silently. `bisync` detects that case
and keeps both versions as `<file>.conflict1` and `<file>.conflict2` for you
to merge. For files where every change matters, such as a KeePass database,
use `bisync`.

**`bisync` needs a resync on its first run** to build the baseline listings it
compares against (stored in the rclone `workdir`):

1. Open the pair's Options (`o`), enable `resync` (and `dry_run` to preview).
2. Run the pair (`CTRL+s`).
3. Disable `resync` again. Left on, every run re-baselines and skips conflict
   detection.

A resync is also needed after changing the pair's paths or filters; rclone
reports this, and the error popup (`ALT+e`) offers `o` to open the Options.

## 📖 TUI Keybindings

- `↑` / `↓`: Move selection up/down
- `SPACE`: Toggle selection for sync
- `a`: Toggle selection for all pairs
- `CTRL+s`: Start synchronization for selected pairs
- `CTRL+a`: Add a new sync pair
- `e`: Edit the highlighted sync pair
- `o`: Options for the highlighted sync pair
- `p`: Per-file progress for the highlighted pair
- `ALT+e`: Show the error of the highlighted pair
- `SHIFT+ALT+I`: Open About/Info dialog
- `q` / `CTRL+C`: Quit application

Both `CTRL+a` and `e` open the same form. `Tab` / `Shift+Tab` move between
fields, `Enter` saves the pair back to the config file, and `Esc` cancels.

## 📄 License

Licensed under either of

- Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license
  ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## 🤝 Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
