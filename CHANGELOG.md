# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/)
and this project adheres to [Semantic Versioning](https://semver.org/).

<!-- next-header -->

## [Unreleased] - ReleaseDate

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
