+++
title = "Building and releasing"
description = "How binaries are built and published with GitHub Actions."
weight = 5
+++

Binaries for Linux, macOS and Windows are automatically compiled and published using GitHub Actions.

## Triggering a release

The build and release process is triggered when you push a Git tag that starts with `v`.

**Prerequisites:** the version must be consistent across the project files. The workflow aborts the build if it detects a mismatch:

- `Cargo.toml` (the `version` field)
- `CHANGELOG.md` (the latest version in an H2 header, e.g. `## 1.0.0`)

**Steps:**

1. Update the version in `Cargo.toml` and `CHANGELOG.md`.
2. Commit and push your changes:
   ```bash
   git add Cargo.toml CHANGELOG.md
   git commit -m "Release v1.0.0"
   git push
   ```
3. Create and push a matching tag:
   ```bash
   git tag v1.0.0
   git push origin v1.0.0
   ```

Once the tag is pushed, the workflow validates the versions, compiles the binaries for all target platforms and creates a GitHub Release with the artifacts attached.

## Manual trigger

1. Open the **Actions** tab of the repository.
2. Select the **Build and Release** workflow.
3. Click **Run workflow**.

> Manual triggers still run the version validation. Make sure `Cargo.toml` and `CHANGELOG.md` are aligned first.
