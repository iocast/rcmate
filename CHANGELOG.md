# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/)
and this project adheres to [Semantic Versioning](https://semver.org/).

<!-- next-header -->

## [Unreleased]

### Added
- Per sync pair options, persisted to the config file under `[sync_pairs.options]`. The Options popup (`o`) now edits the pair under the cursor rather than the whole selection, and offers only the options the pair's type accepts.
- `dry_run` option for every sync type. It is sent as bisync's dedicated `dryRun` parameter, and through the rc `_config` blob (`{"DryRun": true}`) for `sync`, `copy` and `move`, which have no parameter of their own for it.
- `create_empty_src_dirs` for all types and `delete_empty_src_dirs` for `move`, plus `check_access` for bisync. `create_empty_src_dirs` does not apply to a pair whose source or destination is a single file — such a pair runs against the parent directories with an include filter — so for those it is hidden from the popup, left out of the request, and not written to the config file.

### Changed
- Renamed the "Bisync Options" popup to "Options"; bisync options were previously runtime-only and lost on restart.

### Fixed
- `sync` sent a `delete_empty_src_dirs` parameter that `sync/sync` does not accept (only `sync/move` does), and `copy` sent `update`, which is not an rc parameter either — the file-mode two-way sync now passes `--update` correctly via `_config` (`UpdateOlder`), so the older side can no longer overwrite the newer one.

## [0.2.0] - 2026-07-26

### Added
- Introduced the `FileEntry` struct in `event.rs` to track progress in both directions for bisync operations. It replaces the previous direction-specific tracking with a unified per-file view, storing forward/reverse direction progress separately and enabling better handling of bidirectional syncs (bisync) and file-mode two-way syncs. The TUI progress view now shows direction-specific icons and statuses, improving clarity for users managing two-way operations.

### Fixed
- Two-way (bisync / file-mode two-way sync) file transfers reporting the wrong direction: `classify_direction` compared raw `srcFs`/`dstFs` against the configured source/destination without accounting for how rclone echoes paths back — trailing separators, `\` vs `/`, case, and the Windows extended-length path prefix (`\\?\` / `//?/`) on local paths. This caused genuine destination-to-source transfers to be silently misclassified as source-to-destination (and vice versa) in the progress view.
- File progress status text (e.g. `Transferred`) dropping its direction arrow once both sides of a two-way file had reported, even when only one direction actually moved data.
- Two-way files that were already in sync (checked, nothing to transfer) getting stuck showing the untouched side as "never reported" indefinitely — a bidirectional check now settles both sides once one side reports a no-op result, since a single bisync job only ever emits one report per file.

## [0.1.6] - 2026-07-03

### Added
- Initial release of **rcmate**, a Terminal User Interface (TUI) for automating and managing `rclone` tasks.
- Interactive TUI built with `ratatui` for a fast, keyboard-driven terminal experience.
- Support for multiple sync modes: `sync`, `bisync`, `copy`, and `move`.
- Real-time progress tracking and live transfer stats via the `rclone` Remote Control (RC) API.
- Smart file handling that automatically translates single-file operations into directory operations with includes.
- Highly configurable TOML-based setup for defining sync pairs, paths, and filters.
- CLI argument overrides for rclone binaries, configs, workdirs, and log levels.
- Automated cross-platform CI/CD pipeline (Linux, macOS, Windows) via GitHub Actions.
- Project is dual-licensed under MIT and Apache-2.0.
