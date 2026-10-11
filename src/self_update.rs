//! `rcmate self-update`: replaces the running binary with the latest GitHub release.

use color_eyre::Result;
use color_eyre::eyre::{Context, eyre};
use crossterm::style::Stylize;
use semver::Version;
use serde::Deserialize;
use std::io::{self, Write};

const REPO: &str = "iocast/rcmate";

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

fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt} [y/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

pub async fn run() -> Result<()> {
    let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
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
    let bytes = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .context("Download failed")?
        .error_for_status()
        .context("Download failed")?
        .bytes()
        .await
        .context("Download failed")?;
    if bytes.len() as u64 != asset.size {
        return Err(eyre!(
            "Downloaded {} bytes, expected {}; not replacing the binary",
            bytes.len(),
            asset.size
        ));
    }

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
