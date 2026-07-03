use std::{fmt, path::PathBuf};

use ratatui::style::Style;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq)]
pub enum SyncStatus {
    Idle,
    Syncing,
    Done,
    Error,
}

impl fmt::Display for SyncStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BisyncOptions {
    pub resync: bool,
    pub force: bool,
    pub resync_mode: String,
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

    #[serde(skip)]
    pub bisync_opts: BisyncOptions,
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
