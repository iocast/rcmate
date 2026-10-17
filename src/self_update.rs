//! `rcmate self-update`: replaces the running binary with the latest GitHub release.
//!
//! Every release asset ships with an Ed25519 signature (`<asset>.sig`, made by
//! `.github/workflows/release.yml`); the download is only installed when it verifies
//! against [`PUBLIC_KEY`].

use color_eyre::Result;
use color_eyre::eyre::{Context, eyre};
use crossterm::style::Stylize;
use ed25519_dalek::{Signature, VerifyingKey};
use signature::Verifier;
use semver::Version;
use serde::Deserialize;
use std::io::{self, Write};

const REPO: &str = "iocast/rcmate";

/// Release signing key as 64 hex digits (the raw 32-byte Ed25519 public key), compiled in
/// from the `RCMATE_UPDATE_PUBLIC_KEY` Actions variable. Its private half is the
/// `RCMATE_UPDATE_PRIVATE_KEY` Actions secret. Builds without it cannot self-update.
const PUBLIC_KEY: Option<&str> = option_env!("RCMATE_UPDATE_PUBLIC_KEY");

#[derive(Deserialize, Debug)]
struct Release {
    tag_name: String,
    body: Option<String>,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize, Debug)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

impl Release {
    fn version(&self) -> Option<Version> {
        Version::parse(self.tag_name.trim_start_matches('v')).ok()
    }
}

/// Name of the release asset built for this platform by `.github/workflows/release.yml`.
fn asset_name() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("rcmate-linux"),
        // The macOS asset is x86_64 only; Apple Silicon runs it through Rosetta.
        ("macos", _) => Ok("rcmate-macos"),
        ("windows", "x86_64") => Ok("rcmate-windows.exe"),
        (os, arch) => Err(eyre!("No prebuilt release for {os}/{arch}; build from source instead")),
    }
}

fn client() -> Result<reqwest::Client> {
    // The GitHub API rejects requests without a User-Agent.
    reqwest::Client::builder()
        .user_agent(concat!("rcmate/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("Failed to build HTTP client")
}

/// Published (non-draft, non-prerelease) releases newer than `current`, newest first.
async fn newer_releases(client: &reqwest::Client, current: &Version) -> Result<Vec<Release>> {
    let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=100");
    let releases: Vec<Release> = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("Failed to reach GitHub")?
        .error_for_status()
        .context("GitHub returned an error")?
        .json()
        .await
        .context("Unexpected response from GitHub")?;

    let mut newer: Vec<Release> = releases
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter(|r| r.version().is_some_and(|v| v > *current))
        .collect();
    newer.sort_by(|a, b| b.version().cmp(&a.version()));
    Ok(newer)
}

async fn download(client: &reqwest::Client, asset: &Asset) -> Result<Vec<u8>> {
    let bytes = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .with_context(|| format!("Downloading {} failed", asset.name))?
        .bytes()
        .await
        .with_context(|| format!("Downloading {} failed", asset.name))?;
    if bytes.len() as u64 != asset.size {
        return Err(eyre!(
            "Downloaded {} bytes of {}, expected {}; not replacing the binary",
            bytes.len(),
            asset.name,
            asset.size
        ));
    }
    Ok(bytes.to_vec())
}

/// Checks `bin` against its Ed25519 `signature`. The signed message is `"<asset> <tag>\n"`
/// followed by the binary, so an older signed binary cannot be passed off as a newer
/// release or as another platform's asset.
fn verify(public_key: &[u8; 32], bin: &[u8], signature: &[u8], asset_and_tag: &str) -> Result<()> {
    let public_key = VerifyingKey::from_bytes(public_key)
        .map_err(|_| eyre!("The built-in update key is invalid; not replacing the binary"))?;
    let signature = Signature::from_slice(signature)
        .map_err(|_| eyre!("Malformed release signature; not replacing the binary"))?;
    let mut message = format!("{asset_and_tag}\n").into_bytes();
    message.extend_from_slice(bin);
    public_key
        .verify(&message, &signature)
        .map_err(|_| eyre!("Signature check failed; not replacing the binary"))
}

fn decode_hex(hex: &str) -> Option<[u8; 32]> {
    let mut bytes = [0u8; 32];
    if hex.len() != 64 {
        return None;
    }
    for (byte, pair) in bytes.iter_mut().zip(hex.as_bytes().chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(bytes)
}

fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt} [y/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

pub async fn run() -> Result<()> {
    let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
    let public_key = PUBLIC_KEY.map(str::trim).and_then(decode_hex).ok_or_else(|| {
        eyre!("This build has no update key (e.g. built from source); update it the way you installed it")
    })?;
    let asset_name = asset_name()?;
    let client = client()?;

    println!("Checking {} for updates...", format!("github.com/{REPO}").dark_grey());
    let newer = newer_releases(&client, &current).await?;
    let Some(latest) = newer.first() else {
        println!("rcmate {} is up to date.", current.to_string().yellow().bold());
        return Ok(());
    };
    let latest_version = latest.version().expect("filtered on parseable versions");

    println!();
    println!(
        "{}{} -> {}",
        "Update available: ".white().bold(),
        current.to_string().dark_grey(),
        latest_version.to_string().yellow().bold()
    );

    // Release notes of every skipped version, so nothing between current and latest is hidden.
    for release in &newer {
        println!();
        println!("{}", format!("## {}", release.tag_name).cyan().bold());
        match release.body.as_deref().map(str::trim) {
            Some(body) if !body.is_empty() => println!("{body}"),
            _ => println!("{}", "(no release notes)".dark_grey()),
        }
    }
    println!();

    let asset = latest
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .ok_or_else(|| eyre!("Release {} has no {asset_name} asset", latest.tag_name))?;
    let sig_name = format!("{asset_name}.sig");
    let sig_asset = latest
        .assets
        .iter()
        .find(|a| a.name == sig_name)
        .ok_or_else(|| eyre!("Release {} has no {sig_name} signature; not updating", latest.tag_name))?;

    let exe = std::env::current_exe().context("Cannot locate the running rcmate binary")?;
    println!(
        "{}{}",
        "Binary to replace: ".white().bold(),
        exe.display().to_string().dark_grey()
    );
    if !confirm(&format!("Update rcmate to {latest_version}?"))? {
        println!("Update cancelled.");
        return Ok(());
    }

    println!("Downloading {} ({} KiB)...", asset.name, asset.size / 1024);
    let bytes = download(&client, asset).await?;
    let signature = download(&client, sig_asset).await?;
    verify(&public_key, &bytes, &signature, &format!("{asset_name} {}", latest.tag_name))?;
    println!("{}", "Signature verified.".green());

    // Stage next to the target so the final swap stays on one filesystem.
    let dir = exe
        .parent()
        .ok_or_else(|| eyre!("Binary path has no parent directory"))?;
    let staged = tempfile::Builder::new()
        .prefix(".rcmate-update")
        .tempfile_in(dir)
        .with_context(|| format!("Cannot write to {} (try with sudo?)", dir.display()))?;
    std::fs::write(staged.path(), &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(staged.path(), std::fs::Permissions::from_mode(0o755))?;
    }
    self_replace::self_replace(staged.path()).context("Failed to replace the rcmate binary")?;

    println!(
        "{} rcmate {}",
        "Updated to".green().bold(),
        latest_version.to_string().yellow().bold()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // A throwaway OpenSSL key, not the release key; its private half was discarded.
    // SIGNATURE is `openssl pkeyutl -sign -rawin` over "rcmate-linux v9.9.9\n" + PAYLOAD,
    // the same command the release workflow runs.
    const TEST_KEY_HEX: &str = "812de974efadda3f563acbf92d3ee74106d919613e3483408c1503c00b76c0b8";
    const PAYLOAD: &[u8] = b"rcmate test payload";
    const SIGNATURE: &str = "29d342cc6a9433dc7022077d911880008f8cc4df7e81c0631bcc0fcf6d4a387d937cb1dfdb206fdec592640e34188306f7d2b1168b3aa75d094de6219a2e3905";

    fn test_key() -> [u8; 32] {
        decode_hex(TEST_KEY_HEX).unwrap()
    }

    fn signature() -> Vec<u8> {
        (0..SIGNATURE.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&SIGNATURE[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn accepts_signed_payload() {
        verify(&test_key(), PAYLOAD, &signature(), "rcmate-linux v9.9.9").unwrap();
    }

    #[test]
    fn rejects_tampered_payload() {
        let tampered = b"rcmate test payloaX";
        assert!(verify(&test_key(), tampered, &signature(), "rcmate-linux v9.9.9").is_err());
    }

    #[test]
    fn rejects_other_asset_or_version() {
        assert!(verify(&test_key(), PAYLOAD, &signature(), "rcmate-macos v9.9.9").is_err());
        assert!(verify(&test_key(), PAYLOAD, &signature(), "rcmate-linux v10.0.0").is_err());
    }

    #[test]
    fn rejects_garbage_signature() {
        assert!(verify(&test_key(), PAYLOAD, b"not a signature", "rcmate-linux v9.9.9").is_err());
    }

    #[test]
    fn decodes_hex_keys_only() {
        assert!(decode_hex(TEST_KEY_HEX).is_some());
        assert!(decode_hex("TODO").is_none());
        assert!(decode_hex(&"zz".repeat(32)).is_none());
    }
}
