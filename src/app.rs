use crate::config::{
    Config, GeneralConfig, RcloneConfig, SyncPairConfig, SyncPairUi, SyncState, SyncStatus,
    SyncType,
};
use crate::event::{
    AppEvent, Direction, ErrorState, Event, EventHandler, FileEntry, FileProgressState,
    FileTransferInfo, FinishedState, HasKey, ProgressState, TransferState,
};
use crate::rclone_request::{Builder, Configurable};
use crate::tui::{ActionHandler, View};
use crate::views::ViewAction;
use color_eyre::{Result, eyre::Context, eyre::eyre};
use ratatui::DefaultTerminal;
use reqwest;
use serde_json;
use std::collections::HashMap;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};
use tokio::process::Command;
use tokio::sync::{RwLock, mpsc};
use tokio::task::JoinSet;
use tracing::{error, info};
use uuid::Uuid;

pub fn expand_tilde(path: PathBuf) -> PathBuf {
    let path_str = path.to_string_lossy();
    if path_str.starts_with("~/") {
        if let Some(home_dir) = home::home_dir() {
            return home_dir.join(&path_str[2..]);
        }
    }
    path
}

pub struct App {
    should_quit: bool,
    pub(crate) events: EventHandler,
    pub(crate) sync_pairs_tbl_state: ratatui::widgets::TableState,
    pub(crate) active_view: View,
    pub(crate) popup: Option<View>,
    pub(crate) sync_pairs: Arc<RwLock<Vec<Arc<RwLock<SyncPairUi>>>>>,
    pub(crate) rclone: Arc<RwLock<RcloneConfig>>,
    pub(crate) general: Arc<RwLock<GeneralConfig>>,
    rcd_child: Option<tokio::process::Child>,
    pub(crate) config_path: PathBuf,
    pub(crate) rclone_version: Arc<RwLock<String>>,
    /// Per sync pair, the merged per-file progress (one row per filename, with
    /// forward/reverse direction tracked inside `FileEntry` for bisync).
    pub file_progress: Arc<RwLock<HashMap<Uuid, HashMap<String, FileEntry>>>>,
}

impl Default for App {
    fn default() -> Self {
        let c = Config::default();
        App {
            should_quit: false,
            events: EventHandler::new(),
            sync_pairs_tbl_state: ratatui::widgets::TableState::new(),
            active_view: View::Main(crate::views::main::MainView),
            popup: None,
            sync_pairs: Arc::new(RwLock::new(
                c.sync_pairs
                    .iter()
                    .map(|p| {
                        Arc::new(RwLock::new(SyncPairUi {
                            key: Uuid::new_v4(),
                            selected: false,
                            status: SyncStatus::default(),
                            sync_state: SyncState::default(),
                            sync_pair: p.clone(),
                        }))
                    })
                    .collect::<Vec<_>>(),
            )),
            rclone: Arc::new(RwLock::new(c.rclone)),
            general: Arc::new(RwLock::new(c.general)),
            rcd_child: None,
            config_path: PathBuf::from("config.toml"),
            rclone_version: Arc::new(RwLock::new("Checking...".to_string())),
            file_progress: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl App {
    async fn start_rcd(&mut self) -> Result<()> {
        let rclone_cfg = self.rclone.read().await;
        let mut args = vec![
            "rcd".to_string(),
            "--rc-addr".to_string(),
            "127.0.0.1:5572".to_string(),
            "--rc-no-auth".to_string(),
            "--color=never".to_string(),
        ];
        if let Some(cfg) = &rclone_cfg.config {
            args.push("--config".to_string());
            args.push(cfg.to_string_lossy().into_owned());
        }
        let bin = rclone_cfg.bin.clone();
        drop(rclone_cfg);
        let child = Command::new(&bin)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to start rclone rcd")?;
        self.rcd_child = Some(child);
        Ok(())
    }

    async fn stop_rcd(&mut self) {
        if let Some(mut child) = self.rcd_child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(eyre!("Config file does not exist: {}", path.display()));
        }
        let content = fs::read_to_string(path);
        let config: Config = match content {
            Ok(c) => toml::from_str(&c)?,
            Err(e) => return Err(eyre!("Error reading TOML file {}: {}", path.display(), e).into()),
        };

        let mut app = App::default();
        app.config_path = path.to_path_buf();
        app.active_view = View::Main(crate::views::main::MainView);
        app.sync_pairs = Arc::new(RwLock::new(
            config
                .sync_pairs
                .iter()
                .map(|p| {
                    Arc::new(RwLock::new(SyncPairUi {
                        key: Uuid::new_v4(),
                        selected: false,
                        status: SyncStatus::default(),
                        sync_state: SyncState::default(),
                        sync_pair: p.clone(),
                    }))
                })
                .collect::<Vec<_>>(),
        ));
        app.rclone = Arc::new(RwLock::new(config.rclone));
        app.general = Arc::new(RwLock::new(config.general));

        if !app.sync_pairs.try_read().unwrap().is_empty() {
            app.sync_pairs_tbl_state.select(Some(0));
        }

        app.file_progress = Arc::new(RwLock::new(HashMap::new()));
        Ok(app)
    }

    pub async fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        self.start_rcd().await?;

        let rclone_version_lock = self.rclone_version.clone();
        tokio::spawn(async move {
            // Give rcd a moment to boot up
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let client = reqwest::Client::new();
            let rc_url = "http://127.0.0.1:5572";

            // Retry a few times in case rcd is still starting
            for _ in 0..3 {
                if let Ok(resp) = client
                    .post(format!("{}/core/version", rc_url))
                    .json(&serde_json::json!({}))
                    .send()
                    .await
                {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        if let Some(v) = json.get("version").and_then(|v| v.as_str()) {
                            let mut lock = rclone_version_lock.write().await;
                            *lock = v.to_string();
                            return;
                        }
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            let mut lock = rclone_version_lock.write().await;
            *lock = "Unknown".to_string();
        });

        while !self.should_quit {
            terminal.draw(|frame| frame.render_widget(&self, frame.area()))?;
            match self.events.next().await? {
                Event::Tick => self.tick().await,
                Event::Crossterm(event) => match event {
                    crossterm::event::Event::Key(key_event)
                        if key_event.kind == crossterm::event::KeyEventKind::Press =>
                    {
                        self.handle_key_events(key_event)?
                    }
                    _ => {}
                },
                Event::App(app_event) => match app_event {
                    AppEvent::SelectAll => self.toggle_all(),
                    AppEvent::Next => self.next(),
                    AppEvent::Previous => self.previous(),
                    AppEvent::Select => self.toggle_select().await,
                    AppEvent::Synchronize => self.sync_selected(),
                    AppEvent::Quit => self.quit(),
                },
                Event::Progress(state) => self.handle_progress(state).await,
                Event::FileProgress(state) => {
                    let mut fp = self.file_progress.write().await;
                    let files = fp.entry(state.key).or_insert_with(HashMap::new);

                    // Merge both the in-progress and finished reports into one
                    // row per filename. FileEntry::apply keeps the forward and
                    // reverse directions separate internally, so a bisync file
                    // that's "Checked ->" and currently "Checking <-" doesn't
                    // clobber itself, and settles to a single combined status
                    // once both directions land.
                    for t in state.transferring.iter().chain(state.transferred.iter()) {
                        let entry = files.entry(t.name.clone()).or_insert_with(|| FileEntry {
                            name: t.name.clone(),
                            ..Default::default()
                        });
                        // `bidirectional` reflects the sync pair's operation
                        // type, not just whichever direction happened to
                        // report data - a file that only ever needed copying
                        // one way is still part of a two-way operation, and
                        // should keep showing that in the TUI.
                        entry.bidirectional = entry.bidirectional || state.bidirectional;
                        entry.apply(t);
                    }
                }
            }
        }
        self.stop_rcd().await;
        Ok(())
    }

    pub fn handle_key_events(
        &mut self,
        key_event: crossterm::event::KeyEvent,
    ) -> color_eyre::Result<()> {
        if let Some(mut popup) = self.popup.take() {
            let action = popup.handle_key_event(key_event, self);
            match action {
                ViewAction::ClosePopup | ViewAction::SwitchTo(_) => {}
                _ => self.popup = Some(popup),
            }
        } else {
            let mut active_view = std::mem::replace(
                &mut self.active_view,
                View::Main(crate::views::main::MainView),
            );
            let action = active_view.handle_key_event(key_event, self);
            match action {
                ViewAction::SwitchTo(new_view) => self.active_view = new_view,
                ViewAction::OpenPopup(new_popup) => {
                    self.active_view = active_view;
                    self.popup = Some(new_popup);
                }
                ViewAction::Quit => {
                    self.active_view = active_view;
                    self.events.send(AppEvent::Quit);
                }
                ViewAction::None | ViewAction::ClosePopup => self.active_view = active_view,
            }
        }
        Ok(())
    }

    pub async fn handle_progress<S>(&mut self, state: S)
    where
        S: Into<ProgressState>,
    {
        let state: ProgressState = state.into();
        let key = state.key();
        if let Some(s_lck) = self
            .sync_pairs
            .read()
            .await
            .iter()
            .find(|s| s.try_read().unwrap().key == *key)
        {
            let mut s = s_lck.write().await;
            match state {
                ProgressState::Transfer(t) => {
                    s.sync_state.percent = t.percent;
                    s.status = SyncStatus::Syncing;
                }
                ProgressState::Finished(t) => {
                    s.sync_state.percent = t.percent;
                    s.status = SyncStatus::Done;
                }
                ProgressState::Error(t) => {
                    s.status = SyncStatus::Error;
                    s.sync_state.messages.push(t.msg);
                }
            }
        }
    }

    pub async fn tick(&mut self) {}
    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn next(&mut self) {
        let sync_pairs = self.sync_pairs.try_read().unwrap();
        // A config can start out (or be left) with no pairs at all, and the
        // wrap-around arithmetic below underflows on an empty list.
        if sync_pairs.is_empty() {
            self.sync_pairs_tbl_state.select(None);
            return;
        }
        let i = match self.sync_pairs_tbl_state.selected() {
            Some(i) => {
                if i >= sync_pairs.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.sync_pairs_tbl_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let sync_pairs = self.sync_pairs.try_read().unwrap();
        if sync_pairs.is_empty() {
            self.sync_pairs_tbl_state.select(None);
            return;
        }
        let i = match self.sync_pairs_tbl_state.selected() {
            Some(i) => {
                if i == 0 {
                    sync_pairs.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.sync_pairs_tbl_state.select(Some(i));
    }

    pub async fn toggle_select(&mut self) {
        let sync_pairs = self.sync_pairs.try_read().unwrap();
        if let Some(i) = self.sync_pairs_tbl_state.selected() {
            let mut w = sync_pairs[i].write().await;
            w.selected = !w.selected;
        }
    }

    pub fn toggle_all(&mut self) {
        let mut a = self.sync_pairs.try_write().unwrap();
        if a.iter().all(|v| v.try_read().unwrap().selected) {
            a.iter_mut()
                .for_each(|v| v.try_write().unwrap().selected = false);
        } else {
            a.iter_mut()
                .for_each(|v| v.try_write().unwrap().selected = true);
        }
    }

    pub async fn sync(&self) -> color_eyre::Result<&Self> {
        let (targets, rclone_config) = {
            let sp_lock = self.sync_pairs.read().await;
            let rc_lock = self.rclone.read().await;
            (sp_lock.iter().cloned().collect::<Vec<_>>(), rc_lock.clone())
        };
        for chunk in targets.chunks(3) {
            let mut set: JoinSet<_> = JoinSet::new();
            for pair_arc in chunk {
                let rclone_config = rclone_config.clone();
                let pair_arc = pair_arc.clone();
                let sender = self.events.sender();
                set.spawn(async move {
                    let (key, sync_data) = {
                        let p_guard = pair_arc.read().await;
                        (p_guard.key.clone(), p_guard.sync_pair.clone())
                    };
                    SyncPairConfig::execute_sync(key, sync_data, rclone_config, Some(sender)).await
                });
            }
            while set.join_next().await.is_some() {}
        }
        Ok(self)
    }

    pub fn sync_selected(&self) {
        // Immediately update selected pairs to Queued
        if let Ok(sp_lock) = self.sync_pairs.try_read() {
            for pair_arc in sp_lock.iter() {
                if let Ok(mut pair) = pair_arc.try_write() {
                    if pair.selected {
                        pair.status = SyncStatus::Queued;
                    }
                }
            }
        }

        let sync_pairs = self.sync_pairs.clone();
        let rclone = self.rclone.clone();
        let sender = self.events.sender();

        tokio::spawn(async move {
            let (targets, rclone_config) = {
                let sp_lock = sync_pairs.read().await;
                let rc_lock = rclone.read().await;
                let targets: Vec<_> = sp_lock
                    .iter()
                    .filter(|v| v.try_read().map_or(false, |r| r.selected))
                    .cloned()
                    .collect();
                (targets, rc_lock.clone())
            };

            for chunk in targets.chunks(3) {
                let mut set = tokio::task::JoinSet::new();
                for pair_arc in chunk {
                    let sender = sender.clone();
                    let rclone_config = rclone_config.clone();
                    let pair_arc = pair_arc.clone();
                    set.spawn(async move {
                        let (key, sync_data) = {
                            let guard = pair_arc.read().await;
                            (guard.key.clone(), guard.sync_pair.clone())
                        };
                        SyncPairConfig::execute_sync(key, sync_data, rclone_config, Some(sender))
                            .await
                    });
                }
                while let Some(_) = set.join_next().await {}
            }
        });
    }

    pub fn save_config(&self) -> color_eyre::Result<()> {
        let mut general = (*self.general.try_read().unwrap()).clone();

        if let Some(ref log_path) = general.log_path {
            if log_path.is_file() {
                general.log_path = log_path.parent().map(|p| p.to_path_buf());
            }
        }

        let rclone = (*self.rclone.try_read().unwrap()).clone();
        let sync_pairs = self
            .sync_pairs
            .try_read()
            .unwrap()
            .iter()
            .map(|sp| {
                let mut pair = sp.try_read().unwrap().sync_pair.clone();
                // Options that don't apply to a pair aren't written for it:
                // a file-mode pair has no source directories to mirror, and
                // an option in the file that the Options popup never shows
                // would just be a lie about what the next run will do.
                if pair.is_file_mode() {
                    pair.options.create_empty_src_dirs = false;
                }
                pair
            })
            .collect();

        let config = Config {
            general,
            rclone,
            sync_pairs,
        };

        let toml_str = toml::to_string_pretty(&config)?;
        std::fs::write(&self.config_path, toml_str)?;
        Ok(())
    }
}

impl ActionHandler for App {
    fn close_message(&mut self) {
        self.popup = None;
    }

    fn delete_sync_pair(&mut self, idx: usize, wipe_bisync_state: bool) {
        let removed = {
            let mut sync_pairs = self.sync_pairs.try_write().unwrap();
            if idx >= sync_pairs.len() {
                return;
            }
            sync_pairs.remove(idx)
        };

        if wipe_bisync_state {
            let pair = removed.try_read().unwrap();
            if pair.sync_pair.sync_type == SyncType::BiSync {
                if let Some(workdir) = self.rclone.try_read().unwrap().workdir.clone() {
                    for f in pair.sync_pair.bisync_state_files(&workdir) {
                        if let Err(e) = std::fs::remove_file(&f) {
                            error!("Failed to remove bisync state file {}: {}", f.display(), e);
                        }
                    }
                }
            }
        }

        let len = self.sync_pairs.try_read().unwrap().len();
        self.sync_pairs_tbl_state
            .select(if len == 0 { None } else { Some(idx.min(len - 1)) });

        if let Err(e) = self.save_config() {
            error!("Failed to save config after deleting sync pair: {}", e);
        }
    }
}

impl SyncPairConfig {
    /// Whether this pair operates on a single file rather than a directory.
    ///
    /// Such a pair is run as an operation on the *parent* directories plus an
    /// include filter for the file name (see `run_command`), so options that
    /// only make sense for directory trees don't apply to it - see
    /// `views::options::fields_for`, which hides them, and `save_config`,
    /// which keeps them out of the config file.
    ///
    /// A path containing `:` is taken to be a remote, which can't be stat'ed
    /// locally and is assumed to be a directory.
    pub fn is_file_mode(&self) -> bool {
        let source = expand_tilde(PathBuf::from(&self.source));
        let destination = expand_tilde(PathBuf::from(&self.destination));
        source.is_file()
            || destination.is_file()
            || (!source.to_string_lossy().contains(':') && !source.is_dir())
    }

    /// Sanitizes a path the way rclone's bisync derives a workdir file
    /// prefix from Path1/Path2 (see `cmd/bisync/bilib.SessionName`
    /// upstream): a trailing path separator is trimmed first - otherwise it
    /// would leave a stray underscore right before the `..` join or the
    /// `.pathN` suffix - then colons, path separators, and spaces all become
    /// underscores.
    fn bisync_sanitize(path: &str) -> String {
        path.trim_end_matches(['/', '\\'])
            .chars()
            .map(|c| if c == ':' || c == '/' || c == '\\' || c == ' ' { '_' } else { c })
            .collect()
    }

    /// The workdir file prefix rclone bisync uses for this pair's Path1
    /// (source) / Path2 (destination), e.g. `<prefix>.path1.lst`,
    /// `<prefix>.lck`. Mirrors the src/dst strings `run_command` sends to
    /// `/sync/bisync`, so it matches what an actual run would have created.
    pub fn bisync_session_prefix(&self) -> String {
        let source = expand_tilde(PathBuf::from(&self.source))
            .to_string_lossy()
            .into_owned();
        let destination = expand_tilde(PathBuf::from(&self.destination))
            .to_string_lossy()
            .into_owned();
        format!(
            "{}..{}",
            Self::bisync_sanitize(&source),
            Self::bisync_sanitize(&destination)
        )
    }

    /// Any bisync state files (listings, lock file, ...) this pair has left
    /// in `workdir`. Empty if it was never resynced/run, or `workdir`
    /// doesn't exist. See https://rclone.org/bisync/#workdir-and-cleanup.
    pub fn bisync_state_files(&self, workdir: &Path) -> Vec<PathBuf> {
        let prefix = format!("{}.", self.bisync_session_prefix());
        let Ok(entries) = std::fs::read_dir(workdir) else {
            return Vec::new();
        };
        entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(&prefix))
            })
            .collect()
    }

    pub async fn execute_sync(
        key: Uuid,
        sync_pair: SyncPairConfig,
        rclone: RcloneConfig,
        sender: Option<mpsc::UnboundedSender<Event>>,
    ) -> Result<()> {
        Self::run_command(key, &sync_pair, &rclone, sender).await?;
        Ok(())
    }

    async fn run_command(
        key: Uuid,
        sync_pair: &SyncPairConfig,
        rclone: &RcloneConfig,
        sender: Option<mpsc::UnboundedSender<Event>>,
    ) -> Result<()> {
        /// Best-effort classification of which way a file moved.
        ///
        /// The rclone rc API (https://rclone.org/rc/, `core/stats` and
        /// `core/transferred`) doesn't label bisync operations with an explicit
        /// "direction" field. When an item carries real `srcFs`/`dstFs` values
        /// we compare them against the sync pair's configured source/destination
        /// (`path1`/`path2`) directly. For listing-only entries (no srcFs/dstFs,
        /// e.g. bisync's "listing file - Path1"/"listing file - Path2" phase)
        /// we fall back to the `what` string, which does mention the side.
        fn classify_direction(
            src: &str,
            dst: &str,
            what: &str,
            path1: &str,
            path2: &str,
        ) -> Direction {
            // rclone's rc API echoes fs strings back in whatever form the
            // backend prefers - notably the local backend on Windows resolves
            // paths to their extended-length form (a "\\?\" or "//?/" verbatim
            // prefix, confirmed from logs: srcFs "//?/C:/Users/2107/GoogleDrive"
            // vs the plain "C:/Users/2107/GoogleDrive" from the TOML config),
            // uses backslashes, and is case-insensitive - while path1/path2 are
            // taken as-is from the config. Comparing raw strings meant src ==
            // path2 (and dst == path1) essentially never matched, so every
            // genuine Reverse transfer silently fell through to the Forward
            // default below. Strip the verbatim prefix, normalize separators,
            // trailing separators, and case before comparing.
            fn norm(s: &str) -> String {
                let s = s.strip_prefix(r"\\?\").unwrap_or(s);
                let s = s.strip_prefix("//?/").unwrap_or(s);
                s.replace('\\', "/").trim_end_matches('/').to_lowercase()
            }
            if !src.is_empty() && !dst.is_empty() {
                let (n_src, n_dst, n_path1, n_path2) =
                    (norm(src), norm(dst), norm(path1), norm(path2));
                if n_src == n_path2 && n_dst == n_path1 {
                    return Direction::Reverse;
                }
                if n_src == n_path1 && n_dst == n_path2 {
                    return Direction::Forward;
                }
            }
            if what.contains("Path2") {
                return Direction::Reverse;
            }
            Direction::Forward
        }

        async fn execute_and_poll_job(
            client: &reqwest::Client,
            rc_url: &str,
            endpoint: &str,
            params: serde_json::Value,
            key: Uuid,
            // The rc `_group` this specific job was tagged with. Must match
            // exactly what's in `params["_group"]` - stats/transferred are
            // filtered by group, so a mismatch means this job's progress
            // (and thus its direction) silently never shows up.
            group: &str,
            path1: &str,
            path2: &str,
            // Whether this sync pair's operation is inherently two-way
            // (bisync, or the file-mode two-copy pseudo-bisync). Drives
            // whether the TUI always shows a direction indicator for this
            // file, even if only one direction ends up doing anything.
            bidirectional: bool,
            sender: &Option<mpsc::UnboundedSender<Event>>,
            send_finished: bool,
        ) -> Result<()> {
            let resp = client
                .post(endpoint)
                .json(&params)
                .send()
                .await
                .context("Failed to call rclone rc endpoint")?;
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let msg = format!("rclone rc call failed: {} - {}", status, body);
                error!("{}", msg);
                if let Some(tx) = sender {
                    let _ = tx.send(Event::Progress(ErrorState { key, msg }.into()));
                }
                return Ok(());
            }
            let rc_resp: serde_json::Value =
                resp.json().await.context("Failed to parse rc response")?;
            let job_id = rc_resp
                .get("jobid")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| eyre!("No jobid in rclone rc response"))?;

            let poll_interval = std::time::Duration::from_millis(100);
            let mut last_percent = 0u16;
            let mut consecutive_failures = 0;
            let max_failures = 10;
            loop {
                tokio::time::sleep(poll_interval).await;
                let status_resp = client
                    .post(format!("{}/job/status", rc_url))
                    .json(&serde_json::json!({ "jobid": job_id }))
                    .send()
                    .await;
                let mut finished = false;
                let mut has_error = false;
                let mut error_msg = String::new();
                if let Ok(resp) = status_resp {
                    if resp.status().is_success() {
                        if let Ok(status_json) = resp.json::<serde_json::Value>().await {
                            finished = status_json
                                .get("finished")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            if finished {
                                if let Some(error_str) =
                                    status_json.get("error").and_then(|v| v.as_str())
                                {
                                    if !error_str.is_empty() {
                                        has_error = true;
                                        error_msg = strip_ansi_escapes::strip_str(error_str);
                                    }
                                }
                                if let Some(output_text) = status_json
                                    .get("output")
                                    .and_then(|o| o.get("output").and_then(|v| v.as_str()))
                                {
                                    if !output_text.is_empty() {
                                        if !error_msg.is_empty() {
                                            error_msg.push_str("\n\n");
                                        }
                                        error_msg
                                            .push_str(&strip_ansi_escapes::strip_str(output_text));
                                        has_error = true;
                                    }
                                }
                                let success = status_json
                                    .get("success")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(true);
                                if !success && !has_error {
                                    has_error = true;
                                    if error_msg.is_empty() {
                                        error_msg.push_str(
                                            "Job failed without a specific error message.",
                                        );
                                    }
                                }
                            }
                            consecutive_failures = 0;
                        }
                    }
                } else {
                    consecutive_failures += 1;
                    if consecutive_failures >= max_failures {
                        if let Some(tx) = sender {
                            let _ = tx.send(Event::Progress(
                                ErrorState {
                                    key,
                                    msg: "Connection lost to rclone rc".to_string(),
                                }
                                .into(),
                            ));
                        }
                        break;
                    }
                    continue;
                }
                let stats_resp = client
                    .post(format!("{}/core/stats", rc_url))
                    .json(&serde_json::json!({ "group": group }))
                    .send()
                    .await;
                if let Ok(resp) = stats_resp {
                    if resp.status().is_success() {
                        if let Ok(stats_json) = resp.json::<serde_json::Value>().await {
                            let bytes = stats_json
                                .get("bytes")
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0);
                            let total_bytes = stats_json
                                .get("totalBytes")
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0);
                            let percent = if total_bytes > 0 {
                                ((bytes as f64 / total_bytes as f64) * 100.0).round() as u16
                            } else {
                                0
                            }
                            .clamp(0, 100);

                            let mut transferring = Vec::new();

                            if let Some(arr) =
                                stats_json.get("transferring").and_then(|v| v.as_array())
                            {
                                for item in arr {
                                    info!("transferring || {:?}", item);

                                    let name = item
                                        .get("name")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let src = item
                                        .get("srcFs")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let dst = item
                                        .get("dstFs")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let size =
                                        item.get("size").and_then(|v| v.as_i64()).unwrap_or(0);
                                    let bytes =
                                        item.get("bytes").and_then(|v| v.as_i64()).unwrap_or(0);
                                    let percentage = item
                                        .get("percentage")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0)
                                        as u8;
                                    let speed =
                                        item.get("speed").and_then(|v| v.as_f64()).unwrap_or(0.0);
                                    let eta =
                                        item.get("eta").and_then(|v| v.as_f64()).unwrap_or(-1.0);

                                    let direction =
                                        classify_direction(&src, &dst, "", path1, path2);

                                    transferring.push(FileTransferInfo {
                                        name,
                                        src,
                                        dst,
                                        size,
                                        bytes,
                                        percentage,
                                        speed,
                                        eta,
                                        status: "Transferring".to_string(),
                                        direction,
                                    });
                                }
                            }

                            if let Some(arr) = stats_json.get("checking").and_then(|v| v.as_array())
                            {
                                for item in arr {
                                    if let Some(name) = item.as_str() {
                                        // The bare "checking" list has no srcFs/dstFs, so we
                                        // can't tell which side it belongs to here; it's a
                                        // transient state anyway and gets replaced by a
                                        // direction-aware entry once core/transferred reports
                                        // "Checked" for this file.
                                        transferring.push(FileTransferInfo {
                                            name: name.to_string(),
                                            src: "".to_string(),
                                            dst: "".to_string(),
                                            size: 0,
                                            bytes: 0,
                                            percentage: 0,
                                            speed: 0.0,
                                            eta: 0.0,
                                            status: "Checking".to_string(),
                                            direction: Direction::Forward,
                                        });
                                    }
                                }
                            }

                            // Dedup within this single poll response. Keyed by
                            // (name, direction) rather than just name, since a
                            // bisync file legitimately appears once per direction
                            // and both reports need to survive.
                            let mut transferred_map: HashMap<
                                (String, Direction),
                                FileTransferInfo,
                            > = HashMap::new();
                            let transferred_resp = client
                                .post(format!("{}/core/transferred", rc_url))
                                .json(&serde_json::json!({ "group": group }))
                                .send()
                                .await;

                            if let Ok(resp) = transferred_resp {
                                if resp.status().is_success() {
                                    if let Ok(t_json) = resp.json::<serde_json::Value>().await {
                                        if let Some(arr) =
                                            t_json.get("transferred").and_then(|v| v.as_array())
                                        {
                                            for item in arr {
                                                info!("transferred || {:?}", item);

                                                let name = item
                                                    .get("name")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                let size = item
                                                    .get("size")
                                                    .and_then(|v| v.as_i64())
                                                    .unwrap_or(0);
                                                let bytes = item
                                                    .get("bytes")
                                                    .and_then(|v| v.as_i64())
                                                    .unwrap_or(0);
                                                let error = item
                                                    .get("error")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                let src = item
                                                    .get("srcFs")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                let dst = item
                                                    .get("dstFs")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                let checked = item
                                                    .get("checked")
                                                    .and_then(|v| v.as_bool())
                                                    .unwrap_or(false);
                                                let what = item
                                                    .get("what")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("transferring");

                                                let direction = classify_direction(
                                                    &src, &dst, what, path1, path2,
                                                );

                                                let status = if !error.is_empty() {
                                                    "Error".to_string()
                                                } else if checked && bytes == 0 {
                                                    "Checked".to_string()
                                                } else {
                                                    match what {
                                                        "transferring" | "copying"
                                                        | "uploading" | "downloading" => {
                                                            "Transferred"
                                                        }
                                                        "deleting" => "Deleted",
                                                        "checking" => "Checked",
                                                        "importing" => "Imported",
                                                        "hashing" => "Hashed",
                                                        "merging" => "Merged",
                                                        "listing" => "Listed",
                                                        "moving" => "Moved",
                                                        "renaming" => "Renamed",
                                                        _ => "Other",
                                                    }
                                                    .to_string()
                                                };

                                                let map_key = (name.clone(), direction);
                                                let is_better = if let Some(existing) =
                                                    transferred_map.get(&map_key)
                                                {
                                                    bytes > existing.bytes
                                                } else {
                                                    true
                                                };

                                                if is_better {
                                                    let new_info = FileTransferInfo {
                                                        name: name.clone(),
                                                        src: src.clone(),
                                                        dst: dst.clone(),
                                                        size,
                                                        bytes,
                                                        percentage: 100,
                                                        speed: 0.0,
                                                        eta: 0.0,
                                                        status,
                                                        direction,
                                                    };
                                                    transferred_map.insert(map_key, new_info);
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            let transferred: Vec<_> = transferred_map.into_values().collect();

                            if let Some(tx) = sender {
                                let _ = tx.send(Event::FileProgress(FileProgressState {
                                    key,
                                    bidirectional,
                                    transferring,
                                    transferred,
                                }));
                            }

                            if percent != last_percent {
                                last_percent = percent;
                                if let Some(tx) = sender {
                                    let _ = tx.send(Event::Progress(
                                        TransferState { key, percent }.into(),
                                    ));
                                }
                            }
                        }
                    }
                }
                if has_error {
                    if let Some(tx) = sender {
                        let _ = tx.send(Event::Progress(
                            ErrorState {
                                key,
                                msg: error_msg,
                            }
                            .into(),
                        ));
                    }
                    break;
                }
                if finished {
                    if send_finished {
                        if let Some(tx) = sender {
                            let _ = tx
                                .send(Event::Progress(FinishedState { key, percent: 100 }.into()));
                        }
                    }
                    break;
                }
            }
            Ok(())
        }

        let rc_url = "http://localhost:5572";
        let client = reqwest::Client::new();
        let mut source = expand_tilde(PathBuf::from(&sync_pair.source));
        let mut destination = expand_tilde(PathBuf::from(&sync_pair.destination));
        let mut includes = sync_pair.includes.clone().unwrap_or_default();
        let src_is_file = source.is_file();
        let dst_is_file = destination.is_file();
        // Same predicate the Options view and the config writer use, so what
        // gets sent can't drift from what's offered and stored.
        let is_file = sync_pair.is_file_mode();

        if is_file {
            let (src_dir, dst_dir, file_name) = if src_is_file {
                let file_name = source
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                let src_dir = source
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| ".".to_string());
                (
                    src_dir,
                    destination.to_string_lossy().into_owned(),
                    file_name,
                )
            } else if dst_is_file {
                let file_name = destination
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                let dst_dir = destination
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| ".".to_string());
                (source.to_string_lossy().into_owned(), dst_dir, file_name)
            } else {
                let file_name = source
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                let src_dir = source
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| ".".to_string());
                (
                    src_dir,
                    destination.to_string_lossy().into_owned(),
                    file_name,
                )
            };
            info!(
                "File operation detected. Translating to directory operation with include: {} -> {} (file: {})",
                src_dir, dst_dir, file_name
            );
            source = PathBuf::from(&src_dir);
            destination = PathBuf::from(&dst_dir);
            if !file_name.is_empty() {
                includes.push(format!("/{}", file_name));
            }
        }

        let src_str = source.to_string_lossy().into_owned();
        let dst_str = destination.to_string_lossy().into_owned();

        // Canonical path1/path2 for direction classification. Forward always
        // means source -> destination, Reverse always means destination ->
        // source, regardless of which job (bisync's single job, or the
        // file-mode pseudo-bisync's two copy jobs) reports it.
        let path1 = src_str.clone();
        let path2 = dst_str.clone();

        let is_sync_file = sync_pair.sync_type == SyncType::Sync && is_file;

        // Whether this sync pair's operation can move a file in either
        // direction: real bisync, or rcmate's file-mode "sync" which runs two
        // opposite `copy --update` jobs. Everything else (plain copy/move,
        // directory sync) only ever moves data one way.
        let is_two_way = sync_pair.sync_type == SyncType::BiSync || is_sync_file;

        // Per-entry options from the config. Only the ones that apply to this
        // pair are sent - see `views::options::fields_for`, which offers the
        // same set in the Options popup.
        let opts = &sync_pair.options;

        // A file-mode pair runs against the parent directories with an include
        // filter for one file name, so there are no source directories to
        // mirror. The Options popup hides this option for such a pair and
        // `save_config` keeps it out of the config file, so it should be false
        // here already; the guard makes the request independent of that.
        let create_empty_src_dirs = !is_file && opts.create_empty_src_dirs;

        let requests: Vec<(String, serde_json::Value)> = if is_sync_file {
            // Two opposite `copy --update` jobs standing in for bisync; both
            // need --update so the older side never overwrites the newer one.
            // `dry_run` is the only configurable option that applies.
            let req1 = crate::rclone_request::CopyBuilder::new(src_str.clone(), dst_str.clone())
                .exclude(sync_pair.excludes.clone().unwrap_or_default())
                .include(includes.clone())
                .update_older(true)
                .dry_run(opts.dry_run)
                .build();

            let req2 = crate::rclone_request::CopyBuilder::new(dst_str, src_str)
                .exclude(sync_pair.excludes.clone().unwrap_or_default())
                .include(includes)
                .update_older(true)
                .dry_run(opts.dry_run)
                .build();

            vec![
                (format!("{}/sync/copy", rc_url), serde_json::to_value(req1)?),
                (format!("{}/sync/copy", rc_url), serde_json::to_value(req2)?),
            ]
        } else {
            let (endpoint, request_val) = match sync_pair.sync_type {
                SyncType::BiSync => {
                    let mut req = crate::rclone_request::BiSyncBuilder::new(src_str, dst_str)
                        .exclude(sync_pair.excludes.clone().unwrap_or_default())
                        .include(includes)
                        // bisync has a dedicated dryRun parameter, unlike the
                        // other operations which go through `_config`.
                        .dry_run(opts.dry_run)
                        .create_empty_src_dirs(create_empty_src_dirs)
                        .check_access(opts.check_access)
                        .build();

                    if opts.resync {
                        req.resync = Some(true);
                        if opts.resync_mode != "none" {
                            req.resync_mode = Some(opts.resync_mode.clone());
                        }
                    }
                    if opts.force {
                        req.force = Some(true);
                    }
                    (
                        format!("{}/sync/bisync", rc_url),
                        serde_json::to_value(req)?,
                    )
                }
                SyncType::Sync => {
                    let req = crate::rclone_request::SyncBuilder::new(src_str, dst_str)
                        .exclude(sync_pair.excludes.clone().unwrap_or_default())
                        .include(includes)
                        .dry_run(opts.dry_run)
                        .create_empty_src_dirs(create_empty_src_dirs)
                        .build();
                    (format!("{}/sync/sync", rc_url), serde_json::to_value(req)?)
                }
                SyncType::Copy => {
                    let req = crate::rclone_request::CopyBuilder::new(src_str, dst_str)
                        .exclude(sync_pair.excludes.clone().unwrap_or_default())
                        .include(includes)
                        .dry_run(opts.dry_run)
                        .create_empty_src_dirs(create_empty_src_dirs)
                        .build();
                    (format!("{}/sync/copy", rc_url), serde_json::to_value(req)?)
                }
                SyncType::Move => {
                    let req = crate::rclone_request::MoveBuilder::new(src_str, dst_str)
                        .exclude(sync_pair.excludes.clone().unwrap_or_default())
                        .include(includes)
                        .dry_run(opts.dry_run)
                        .create_empty_src_dirs(create_empty_src_dirs)
                        .delete_empty_src_dirs(opts.delete_empty_src_dirs)
                        .build();
                    (format!("{}/sync/move", rc_url), serde_json::to_value(req)?)
                }
            };
            vec![(endpoint, request_val)]
        };

        for (i, (endpoint, mut json_val)) in requests.into_iter().enumerate() {
            let group_name = if i == 0 {
                key.to_string()
            } else {
                format!("{}-{}", key, i + 1)
            };

            if let Some(obj) = json_val.as_object_mut() {
                if let Some(filter) = &sync_pair.filter {
                    obj.insert(
                        "filter-from".to_string(),
                        serde_json::Value::String(filter.clone()),
                    );
                }
                if sync_pair.sync_type == SyncType::BiSync {
                    if let Some(w_path) = &rclone.workdir {
                        obj.insert(
                            "workdir".to_string(),
                            serde_json::Value::String(w_path.to_string_lossy().into_owned()),
                        );
                    }
                }
                obj.insert("_async".to_string(), serde_json::Value::Bool(true));
                obj.insert(
                    "_group".to_string(),
                    serde_json::Value::String(group_name.clone()),
                );
            }
            execute_and_poll_job(
                &client,
                rc_url,
                &endpoint,
                json_val,
                key,
                &group_name,
                &path1,
                &path2,
                is_two_way,
                &sender,
                true,
            )
            .await?;
        }
        Ok(())
    }
}
