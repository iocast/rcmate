use crate::config::{
    Config, GeneralConfig, RcloneConfig, SyncPairConfig, SyncPairUi, SyncState, SyncStatus,
    SyncType,
};
use crate::event::{
    AppEvent, ErrorState, Event, EventHandler, FileTransferInfo, FinishedState, HasKey,
    ProgressState, TransferState,
};
use crate::rclone_request::Builder;
use crate::tui::{ActionHandler, View};
use crate::views::ViewAction;
use color_eyre::{Result, eyre::Context, eyre::eyre};
use ratatui::DefaultTerminal;
use reqwest;
use serde_json;
use std::collections::{HashMap, HashSet};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};
use tokio::process::Command;
use tokio::sync::{RwLock, mpsc};
use tokio::task::JoinSet;
use tracing::{debug, error, info};
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
    pub file_progress: Arc<
        RwLock<
            HashMap<
                Uuid,
                (
                    Vec<crate::event::FileTransferInfo>,
                    Vec<crate::event::FileTransferInfo>,
                    HashSet<String>,
                ),
            >,
        >,
    >,
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
                    let entry = fp
                        .entry(state.key)
                        .or_insert_with(|| (Vec::new(), Vec::new(), HashSet::new()));

                    // 1. Overwrite active transfers
                    entry.0 = state.transferring;

                    // 2. Accumulate completed transfers using op_key
                    for t in state.transferred {
                        if entry.2.contains(&t.op_key) {
                            // This exact operation already exists, update if bytes increased
                            if let Some(existing) =
                                entry.1.iter_mut().find(|x| x.op_key == t.op_key)
                            {
                                if t.bytes > existing.bytes {
                                    *existing = t;
                                }
                            }
                        } else {
                            // New operation, add it as a separate row
                            entry.2.insert(t.op_key.clone());
                            entry.1.push(t);
                        }
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
                ViewAction::ClosePopup | ViewAction::SwitchTo(_) => {} // popup remains None
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

        // If the current path points to an existing file, extract its parent directory.
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
            .map(|sp| sp.try_read().unwrap().sync_pair.clone())
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
}

impl SyncPairConfig {
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
        async fn execute_and_poll_job(
            client: &reqwest::Client,
            rc_url: &str,
            endpoint: &str,
            params: serde_json::Value,
            key: Uuid,
            sender: &Option<mpsc::UnboundedSender<Event>>,
            send_finished: bool,
        ) -> Result<()> {
            debug!(
                "Executing job: endpoint={}, params={}",
                endpoint,
                serde_json::to_string_pretty(&params).unwrap_or_default()
            );
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
            debug!(
                "Job start response: {}",
                serde_json::to_string_pretty(&rc_resp).unwrap_or_default()
            );
            let job_id = rc_resp
                .get("jobid")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| eyre!("No jobid in rclone rc response"))?;
            debug!("Started rclone job {} via rc", job_id);
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
                            debug!("{}", status_json);
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
                    .json(&serde_json::json!({ "group": key.to_string() }))
                    .send()
                    .await;
                if let Ok(resp) = stats_resp {
                    if resp.status().is_success() {
                        if let Ok(stats_json) = resp.json::<serde_json::Value>().await {
                            debug!("Job {} core/stats response: {}", job_id, stats_json);
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

                            // --- NEW CODE: Extract per-file progress ---
                            let mut transferring = Vec::new();

                            // 1. Parse active transferring
                            if let Some(arr) =
                                stats_json.get("transferring").and_then(|v| v.as_array())
                            {
                                for item in arr {
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
                                    let op_key = if !src.is_empty() && !dst.is_empty() {
                                        format!("{}|{}->{}", name, src, dst)
                                    } else {
                                        format!("{}|{}", name, "transferring")
                                    };
                                    transferring.push(crate::event::FileTransferInfo {
                                        name,
                                        src,
                                        dst,
                                        size,
                                        bytes,
                                        percentage,
                                        speed,
                                        eta,
                                        status: "Transferring".to_string(),
                                        op_key,
                                    });
                                }
                            }

                            // 2. Parse active checking (core/stats returns this as an array of strings)
                            if let Some(arr) = stats_json.get("checking").and_then(|v| v.as_array())
                            {
                                for item in arr {
                                    if let Some(name) = item.as_str() {
                                        transferring.push(crate::event::FileTransferInfo {
                                            name: name.to_string(),
                                            src: "".to_string(),
                                            dst: "".to_string(),
                                            size: 0,
                                            bytes: 0,
                                            percentage: 0,
                                            speed: 0.0,
                                            eta: 0.0,
                                            status: "Checking".to_string(),
                                            op_key: format!("{}|{}", name, "checking"),
                                        });
                                    }
                                }
                            }

                            let mut transferred_map: HashMap<
                                String,
                                crate::event::FileTransferInfo,
                            > = HashMap::new();
                            let transferred_resp = client
                                .post(format!("{}/core/transferred", rc_url))
                                .json(&serde_json::json!({ "group": key.to_string() }))
                                .send()
                                .await;

                            if let Ok(resp) = transferred_resp {
                                if resp.status().is_success() {
                                    if let Ok(t_json) = resp.json::<serde_json::Value>().await {
                                        if let Some(arr) =
                                            t_json.get("transferred").and_then(|v| v.as_array())
                                        {
                                            for item in arr {
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
                                                let op_key = if !src.is_empty() && !dst.is_empty() {
                                                    format!("{}|{}->{}", name, src, dst)
                                                } else {
                                                    format!("{}|{}", name, what)
                                                };

                                                let status = if !error.is_empty() {
                                                    "Error".to_string()
                                                } else if checked {
                                                    "Checked".to_string()
                                                } else {
                                                    match what {
                                                        "transferring" => "Transferred",
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
                                                    op_key: op_key.clone(),
                                                };

                                                // Create a unique key for the operation: filename + direction
                                                // If src/dst are present (actual transfer), use them. Otherwise, use the 'what' field (e.g., "listing file - Path1")
                                                let op_key = if !src.is_empty() && !dst.is_empty() {
                                                    format!("{}|{}->{}", name, src, dst)
                                                } else {
                                                    format!("{}|{}", name, what)
                                                };

                                                // Deduplicate: Keep the "best" state for this specific operation
                                                let is_better = if let Some(existing) =
                                                    transferred_map.get(&op_key)
                                                {
                                                    (!checked && existing.status == "Checked")
                                                        || (bytes > existing.bytes)
                                                } else {
                                                    true
                                                };

                                                if is_better {
                                                    transferred_map.insert(op_key, new_info);
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            let transferred: Vec<_> = transferred_map.into_values().collect();

                            if let Some(tx) = sender {
                                let _ =
                                    tx.send(Event::FileProgress(crate::event::FileProgressState {
                                        key,
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
        let is_file = src_is_file
            || dst_is_file
            || (!source.to_string_lossy().contains(':') && !source.is_dir());

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

        // Check if we need to override Sync to Copy for single files
        let is_sync_file = sync_pair.sync_type == SyncType::Sync && is_file;

        let requests: Vec<(String, serde_json::Value)> = if is_sync_file {
            // Job 1: Copy source -> destination with --update
            let req1 = crate::rclone_request::CopyBuilder::new(src_str.clone(), dst_str.clone())
                .exclude(sync_pair.excludes.clone().unwrap_or_default())
                .include(includes.clone())
                .build();
            let mut json1 = serde_json::to_value(req1)?;
            if let Some(obj) = json1.as_object_mut() {
                obj.insert("update".to_string(), serde_json::Value::Bool(true));
            }

            // Job 2: Copy destination -> source with --update
            let req2 = crate::rclone_request::CopyBuilder::new(dst_str, src_str)
                .exclude(sync_pair.excludes.clone().unwrap_or_default())
                .include(includes)
                .build();
            let mut json2 = serde_json::to_value(req2)?;
            if let Some(obj) = json2.as_object_mut() {
                obj.insert("update".to_string(), serde_json::Value::Bool(true));
            }

            vec![
                (format!("{}/sync/copy", rc_url), json1),
                (format!("{}/sync/copy", rc_url), json2),
            ]
        } else {
            let (endpoint, request_val) = match sync_pair.sync_type {
                SyncType::BiSync => {
                    let mut req = crate::rclone_request::BiSyncBuilder::new(src_str, dst_str)
                        .exclude(sync_pair.excludes.clone().unwrap_or_default())
                        .include(includes)
                        .build();

                    let bisync_opts = &sync_pair.bisync_opts;
                    if bisync_opts.resync {
                        req.resync = Some(true);
                        if bisync_opts.resync_mode != "none" {
                            req.resync_mode = Some(bisync_opts.resync_mode.clone());
                        }
                    }
                    if bisync_opts.force {
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
                        .build();
                    (format!("{}/sync/sync", rc_url), serde_json::to_value(req)?)
                }
                SyncType::Copy => {
                    let req = crate::rclone_request::CopyBuilder::new(src_str, dst_str)
                        .exclude(sync_pair.excludes.clone().unwrap_or_default())
                        .include(includes)
                        .build();
                    (format!("{}/sync/copy", rc_url), serde_json::to_value(req)?)
                }
                SyncType::Move => {
                    let req = crate::rclone_request::MoveBuilder::new(src_str, dst_str)
                        .exclude(sync_pair.excludes.clone().unwrap_or_default())
                        .include(includes)
                        .build();
                    (format!("{}/sync/move", rc_url), serde_json::to_value(req)?)
                }
            };
            vec![(endpoint, request_val)]
        };

        // Execute the job(s) sequentially
        for (i, (endpoint, mut json_val)) in requests.into_iter().enumerate() {
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

                // Use a distinct group for the second job to prevent rclone stats accumulation
                let group_name = if i == 0 {
                    key.to_string()
                } else {
                    format!("{}-{}", key, i + 1)
                };
                obj.insert("_group".to_string(), serde_json::Value::String(group_name));
            }

            debug!(
                "rclone rc {} call: src={}, dst={}",
                sync_pair.sync_type,
                source.display(),
                destination.display()
            );
            execute_and_poll_job(&client, rc_url, &endpoint, json_val, key, &sender, true).await?;
        }
        Ok(())
    }
}
