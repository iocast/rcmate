+++
title = "CLI arguments"
description = "Override config paths, the rclone binary and logging from the command line."
weight = 4
+++

| Argument | Description | Default |
|----------|-------------|---------|
| `--config <PATH>` | Path to the rcmate TOML config file | `~/.config/rcmate/config.toml` |
| `--rclone <BIN>` | Path or name of the rclone binary | `rclone` |
| `--rclone-config <PATH>` | Path to the rclone configuration file | |
| `--workdir <PATH>` | Working directory for rclone (useful for bisync) | |
| `--log-path <PATH>` | Path to the log file or directory | |
| `--log-level <LEVEL>` | `trace`, `debug`, `info`, `warn`, `error` | `info` |
| `-v`, `--version` | Print version information and exit | |

## Example

```bash
rcmate --config ~/.config/rcmate/config.toml --rclone-config ~/.config/rclone/rclone.conf --log-level debug
```
