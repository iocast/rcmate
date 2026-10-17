# Contributing to rcmate

## Building and Releasing

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

Once the tag is pushed, the workflow will validate the versions, compile the binaries for all target platforms, sign them, and automatically create a GitHub Release with the attached artifacts.

### Release Signing

The workflow signs every binary with an Ed25519 key and publishes the signature next to it (`rcmate-linux.sig`, `rcmate-macos.sig`, `rcmate-windows.exe.sig`). The signature covers the line `"<asset> <tag>\n"` followed by the binary, so `rcmate self-update` (which verifies with [`ed25519-dalek`](https://crates.io/crates/ed25519-dalek)) rejects a binary that was tampered with, belongs to another platform, or comes from another release.

The workflow fails before publishing if the `RCMATE_UPDATE_PRIVATE_KEY` secret is missing or does not match the `RCMATE_UPDATE_PUBLIC_KEY` variable. The build compiles that variable in as the key `self-update` trusts; a build without it (e.g. `cargo install` from source) cannot self-update and says so.

**Getting OpenSSL** (version 3 or newer; check with `openssl version`): on Linux and macOS it is usually installed already; otherwise use the package manager (e.g. `apt install openssl`, `brew install openssl`).

**Setting up the key** (once; run it outside the repository so the private key is never committed):

1. Create the **private key**. This writes it to the file `rcmate-update.pem` in the current directory:
   ```bash
   openssl genpkey -algorithm ed25519 -out rcmate-update.pem
   ```
2. Print the **public key** that belongs to it, as 64 hex digits. This only reads `rcmate-update.pem` and creates no file:
   ```bash
   openssl pkey -in rcmate-update.pem -pubout -outform DER | tail -c 32 | od -An -tx1 | tr -d ' \n'; echo
   ```
3. Under **Settings → Secrets and variables → Actions**, store:
   - the public key (the 64 hex digits from step 2) as the repository **variable** `RCMATE_UPDATE_PUBLIC_KEY`,
   - the private key (the full content of `rcmate-update.pem`, including the `-----BEGIN PRIVATE KEY-----` and `-----END PRIVATE KEY-----` lines) as the repository **secret** `RCMATE_UPDATE_PRIVATE_KEY`.
4. Keep a backup of `rcmate-update.pem` somewhere safe (e.g. a password manager) and delete the file. GitHub never shows a secret again, and installed copies only trust this key: if it is lost or replaced, copies built with the old public key cannot verify updates anymore.

### Manual Trigger

You can also trigger the workflow manually from the GitHub UI:

1. Navigate to the **Actions** tab in your repository.
2. Select the **Build and Release** workflow from the left sidebar.
3. Click the **Run workflow** dropdown and select **Run workflow**.

_(Note: Manual triggers still execute the version validation step. Ensure your `Cargo.toml` and `CHANGELOG.md` versions are properly aligned before triggering manually.)_
