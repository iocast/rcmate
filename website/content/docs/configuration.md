+++
title = "Configuration"
description = "Define sync pairs, paths, filters and per-pair options in a TOML file."
weight = 2
+++

rcmate uses a TOML file for configuration. The default location is `~/.config/rcmate/config.toml`.

## Example

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

## Sync modes

Each pair has a `type`: `sync`, `bisync`, `copy` or `move`.

## Options per type

Which options apply depends on the pair's `type`, mirroring what the rclone rc API accepts for each operation:

| Option | bisync | sync | copy | move |
|--------|:------:|:----:|:----:|:----:|
| `dry_run` | ✓ | ✓ | ✓ | ✓ |
| `create_empty_src_dirs` | ✓ | ✓ | ✓ | ✓ |
| `delete_empty_src_dirs` | | | | ✓ |
| `resync`, `resync_mode`, `force`, `check_access` | ✓ | | | |

The Options popup only shows the options that apply to the highlighted pair, and only those are sent to rclone. Options not listed for a type are still kept in the config, so changing a pair's type back does not lose them.

## Single-file pairs

Applicability also depends on the paths. A pair whose source or destination is a single file runs against the parent directories with an include filter for that file name — and, for `type = "sync"`, as two opposite `copy --update` jobs.

There are no source directories to mirror in that case, so `create_empty_src_dirs` does not apply: the popup does not offer it, it is not sent to rclone, and it is not written to the config file for that pair.
