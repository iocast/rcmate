use std::{fmt, path::PathBuf};

use ratatui::style::Style;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncStatus {
    Idle,
    Queued,
    Syncing,
    Done,
    Error,
}

impl fmt::Display for SyncStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            SyncStatus::Queued => write!(f, "Queued"),
            Self::Syncing => write!(f, "Syncing"),
            Self::Done => write!(f, "Done"),
            Self::Error => write!(f, "Error"),
        }
    }
}

impl std::default::Default for SyncStatus {
    fn default() -> Self {
        SyncStatus::Idle
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SyncType {
    Sync,
    BiSync,
    Copy,
    Move,
}

impl fmt::Display for SyncType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Sync => write!(f, "sync"),
            Self::BiSync => write!(f, "bisync"),
            Self::Copy => write!(f, "copy"),
            Self::Move => write!(f, "move"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SyncState {
    pub percent: u16,
    pub style: Style,
    pub messages: Vec<String>,
}

// Added Default implementation for SyncState to prevent compilation errors
impl Default for SyncState {
    fn default() -> Self {
        Self {
            percent: 0,
            style: Style::default(),
            messages: Vec::new(),
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct SyncPairUi {
    #[serde(skip)]
    pub key: Uuid,
    #[serde(skip)]
    pub status: SyncStatus,
    #[serde(skip)]
    pub selected: bool,
    #[serde(skip)]
    pub sync_state: SyncState,

    pub sync_pair: SyncPairConfig,
}

fn default_resync_mode() -> String {
    "none".to_string()
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Per sync pair rclone options.
///
/// Which fields actually apply depends on the pair's [`SyncType`], and on
/// whether its paths point at a single file. The Options view only offers the
/// applicable ones (see `views::options::fields_for`) and the request building
/// only sends those.
///
/// Options that a *type* doesn't use are still stored, so switching a pair's
/// type back and forth doesn't lose what was configured before. Options that
/// don't apply to a file-mode pair are a different matter: they're kept out of
/// the config file entirely (see `App::save_config`), since a value in the
/// file that the popup never shows and the run never uses is just misleading.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SyncOptions {
    /// `--dry-run`. Sent as bisync's dedicated `dryRun` parameter, and via
    /// the generic `_config` blob (`{"DryRun": true}`) for the other
    /// operations, which have no rc parameter of their own for it.
    #[serde(default)]
    pub dry_run: bool,
    /// Doesn't apply to a pair that operates on a single file (see
    /// `SyncPairConfig::is_file_mode`). Skipped when off so such a pair
    /// carries no trace of it in the config file.
    #[serde(default, skip_serializing_if = "is_false")]
    pub create_empty_src_dirs: bool,
    /// `move` only.
    #[serde(default)]
    pub delete_empty_src_dirs: bool,
    /// `bisync` only.
    #[serde(default)]
    pub resync: bool,
    /// `bisync` only; one of `none`, `path1`, `path2`, `newer`, `older`,
    /// `larger`, `smaller`. `none` means "don't send resyncMode at all".
    #[serde(default = "default_resync_mode")]
    pub resync_mode: String,
    /// `bisync` only.
    #[serde(default)]
    pub force: bool,
    /// `bisync` only.
    #[serde(default)]
    pub check_access: bool,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            create_empty_src_dirs: false,
            delete_empty_src_dirs: false,
            resync: false,
            resync_mode: default_resync_mode(),
            force: false,
            check_access: false,
        }
    }
}

/// SyncPairConfig is the configuration for a single sync pair
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct SyncPairConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub sync_type: SyncType,
    pub source: String,
    pub destination: String,

    #[serde(default)]
    pub excludes: Option<Vec<String>>,
    #[serde(default)]
    pub includes: Option<Vec<String>>,
    #[serde(default)]
    pub filter: Option<String>,

    /// Kept last so it serializes after the scalar fields - TOML requires a
    /// nested table to come after the plain keys of its parent.
    #[serde(default)]
    pub options: SyncOptions,
}

// Default value function for Serde
fn default_log_level() -> String {
    "info".to_string()
}

/// General is the configuration for general settings
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GeneralConfig {
    pub log_path: Option<PathBuf>,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

/// RcloneConfig is the configuration for rclone
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RcloneConfig {
    pub bin: String,
    pub config: Option<PathBuf>,
    pub workdir: Option<PathBuf>,
}

/// Config is the configuration for the application
#[derive(Deserialize, Serialize, Debug)]
pub struct Config {
    pub general: GeneralConfig,
    pub rclone: RcloneConfig,
    pub sync_pairs: Vec<SyncPairConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                log_path: None,
                log_level: default_log_level(),
            },
            rclone: RcloneConfig {
                bin: "rclone".to_string(),
                config: None,
                workdir: None,
            },
            sync_pairs: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Configs written before options were persisted have no `[..options]`
    /// table; they must still load, with options falling back to defaults.
    #[test]
    fn sync_pair_without_options_gets_defaults() {
        let toml_str = r#"
[general]
log_level = "info"

[rclone]
bin = "rclone"

[[sync_pairs]]
name = "docs"
type = "bisync"
source = "/src"
destination = "/dst"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let opts = &config.sync_pairs[0].options;
        assert_eq!(*opts, SyncOptions::default());
        // "none" rather than an empty string, so the value is always one of
        // the modes the Options view cycles through.
        assert_eq!(opts.resync_mode, "none");
    }

    /// `options` is a nested table and TOML rejects plain keys after one, so
    /// it has to stay the last field of `SyncPairConfig`.
    #[test]
    fn options_survive_a_toml_round_trip() {
        let mut config = Config::default();
        config.sync_pairs.push(SyncPairConfig {
            name: "docs".to_string(),
            sync_type: SyncType::BiSync,
            source: "/src".to_string(),
            destination: "/dst".to_string(),
            excludes: Some(vec!["*.tmp".to_string()]),
            includes: None,
            filter: None,
            options: SyncOptions {
                dry_run: true,
                resync: true,
                resync_mode: "newer".to_string(),
                ..SyncOptions::default()
            },
        });

        let serialized = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(parsed.sync_pairs[0].options, config.sync_pairs[0].options);
    }

    /// `create_empty_src_dirs` doesn't apply to a file-mode pair, and
    /// `save_config` leaves it off for those - which only keeps it out of the
    /// file if serialization skips the disabled flag.
    #[test]
    fn a_disabled_create_empty_src_dirs_is_not_written() {
        let mut config = Config::default();
        config.sync_pairs.push(SyncPairConfig {
            name: "one file".to_string(),
            sync_type: SyncType::Sync,
            source: "/src/notes.txt".to_string(),
            destination: "/dst".to_string(),
            excludes: None,
            includes: None,
            filter: None,
            options: SyncOptions {
                dry_run: true,
                create_empty_src_dirs: false,
                ..SyncOptions::default()
            },
        });

        let serialized = toml::to_string_pretty(&config).unwrap();
        assert!(
            !serialized.contains("create_empty_src_dirs"),
            "{serialized}"
        );
        assert!(serialized.contains("dry_run"));
    }
}
