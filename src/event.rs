use color_eyre::eyre::OptionExt;
use crossterm::event::Event as CrosstermEvent;
use futures::StreamExt; // Removed FutureExt as .fuse() is no longer needed
use std::time::Duration;
use tokio::sync::mpsc;
use uuid::Uuid;

/// The frequency at which tick events are emitted.
const TICK_FPS: f64 = 30.0;

#[derive(Clone, Debug)]
pub enum Severity {
    Success,
    Info,
    Warn,
    Error,
}

/// Representation of all possible events.
#[derive(Clone, Debug)]
pub enum Event {
    /// An event that is emitted on a regular schedule.
    Tick,
    /// Crossterm events.
    Crossterm(CrosstermEvent),
    /// Application events.
    App(AppEvent),
    /// Progress events.
    Progress(ProgressState),
    FileProgress(FileProgressState),
}

/// Application events.
#[derive(Clone, Debug)]
pub enum AppEvent {
    /// Select all sync pairs
    SelectAll,
    /// Next sync pair
    Next,
    /// Previous sync pair
    Previous,
    /// Select sync pair
    Select,
    /// Sync selected
    Synchronize,
    /// Quit the application.
    Quit,
}

pub trait HasKey {
    fn key(&self) -> &Uuid;
}

#[derive(Clone, Debug)]
pub struct TransferState {
    pub(crate) key: Uuid,
    pub(crate) percent: u16,
}

#[derive(Clone, Debug)]
pub struct ErrorState {
    pub(crate) key: Uuid,
    pub(crate) msg: String,
}

#[derive(Clone, Debug)]
pub struct FinishedState {
    pub(crate) key: Uuid,
    pub(crate) percent: u16,
}

// The target Enum that wraps all types
#[derive(Clone, Debug)]
pub enum ProgressState {
    Transfer(TransferState),
    Finished(FinishedState),
    Error(ErrorState),
}

impl From<TransferState> for ProgressState {
    fn from(state: TransferState) -> Self {
        ProgressState::Transfer(state)
    }
}

impl From<ErrorState> for ProgressState {
    fn from(state: ErrorState) -> Self {
        ProgressState::Error(state)
    }
}

impl From<FinishedState> for ProgressState {
    fn from(state: FinishedState) -> Self {
        ProgressState::Finished(state)
    }
}

impl HasKey for ProgressState {
    fn key(&self) -> &Uuid {
        match self {
            ProgressState::Finished(s) => &s.key,
            ProgressState::Transfer(s) => &s.key,
            ProgressState::Error(s) => &s.key,
        }
    }
}

/// Which side of a sync pair an operation moves data along.
///
/// `sync`/`copy`/`move` only ever move data one way, so every event for those
/// sync types is `Forward`. `bisync` (and rcmate's file-mode "sync" pseudo-bisync,
/// which runs two opposite `copy` jobs) can report the *same file* twice: once
/// for source -> destination and once for destination -> source. `Direction` is
/// how we tell those two reports apart so they can be merged back into a single
/// row instead of showing up as two unrelated entries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Direction {
    /// source -> destination (Path1 -> Path2 in rclone bisync terms).
    #[default]
    Forward,
    /// destination -> source (Path2 -> Path1 in rclone bisync terms).
    Reverse,
}

impl Direction {
    pub fn arrow(&self) -> &'static str {
        match self {
            Direction::Forward => "→",
            Direction::Reverse => "←",
        }
    }
}

/// A single raw progress/result report for one file, as reported by the rclone
/// rc API (`core/stats` for in-progress transfers, `core/transferred` for
/// finished ones). `name` (the filename) is the identifier - rcmate no longer
/// invents a synthetic key, since the same file naturally maps to one row.
#[derive(Clone, Debug, Default)]
pub struct FileTransferInfo {
    pub name: String,
    pub src: String,
    pub dst: String,
    pub size: i64,
    pub bytes: i64,
    pub percentage: u8,
    pub speed: f64,
    pub eta: f64,
    /// e.g. "Checking", "Checked", "Transferring", "Transferred", "Deleted",
    /// "Moved", "Error", "Other".
    pub status: String,
    pub direction: Direction,
}

#[derive(Clone, Debug)]
pub struct FileProgressState {
    pub key: Uuid,
    /// Whether this sync pair's operation can move data in either direction
    /// (bisync, or rcmate's file-mode "sync" which runs two opposite copy
    /// jobs). Set from the sync pair's config, not inferred from which
    /// directions happened to report data - a file that only ever needed
    /// copying one way is still part of a two-way operation.
    pub bidirectional: bool,
    pub transferring: Vec<FileTransferInfo>,
    pub transferred: Vec<FileTransferInfo>,
}

fn is_terminal_status(status: &str) -> bool {
    !matches!(status.trim(), "" | "Checking" | "Transferring")
}

/// The progress of a single direction of a file operation.
#[derive(Clone, Debug, Default)]
pub struct DirectionProgress {
    pub status: String,
    pub percentage: u8,
    pub bytes: i64,
    pub size: i64,
    pub speed: f64,
    pub eta: f64,
    /// Whether this direction has ever reported for this file. Lets us tell
    /// "hasn't happened yet" apart from "happened, status is empty".
    pub touched: bool,
}

impl DirectionProgress {
    fn apply(&mut self, info: &FileTransferInfo) {
        let is_active = info.status == "Checking" || info.status == "Transferring";
        // Don't let a stale/duplicate poll regress a direction that already
        // reached a terminal status (e.g. re-reporting "Checking" after
        // "Checked" already landed for this direction).
        let should_update = !self.touched
            || is_active
            || info.status == "Error"
            || info.bytes > self.bytes
            || (is_terminal_status(&info.status) && !is_terminal_status(&self.status));
        if should_update {
            self.status = info.status.clone();
            self.percentage = info.percentage;
            self.bytes = info.bytes;
            self.size = info.size;
            self.speed = info.speed;
            self.eta = info.eta;
        }
        self.touched = true;
    }

    fn icon(&self) -> &'static str {
        if !self.touched {
            "○"
        } else {
            match self.status.as_str() {
                "Checking" | "Transferring" => "…",
                "Error" => "✗",
                _ => "✓",
            }
        }
    }
}

/// The merged, per-file view shown in the TUI: one row per filename, with the
/// forward and reverse directions tracked separately so a bisync file can show
/// "Checking ←" while indicating the → side already finished.
#[derive(Clone, Debug, Default)]
pub struct FileEntry {
    pub name: String,
    /// True once this file has reported activity in the reverse direction,
    /// i.e. it's part of a bisync (or file-mode two-way sync).
    pub bidirectional: bool,
    pub forward: DirectionProgress,
    pub reverse: DirectionProgress,
}

impl FileEntry {
    pub fn apply(&mut self, info: &FileTransferInfo) {
        if info.direction == Direction::Reverse {
            self.reverse.apply(info);
        } else {
            self.forward.apply(info);
        }

        // A bidirectional operation (real bisync, or file-mode two-way sync)
        // only ever produces ONE report per file when the file already
        // matches on both sides ("Checked", 0 bytes moved) - there's no
        // second, independent "the other side checked out fine too" event
        // coming. Without this, an in-sync file would sit with its other
        // direction stuck at "never reported" (○) forever, which reads as
        // "still pending" even once the whole sync has finished. Mirror the
        // no-op result onto the other direction, but only if it hasn't
        // already reported something more specific itself.
        if self.bidirectional && info.status == "Checked" && info.bytes == 0 {
            let other = if info.direction == Direction::Reverse {
                &mut self.forward
            } else {
                &mut self.reverse
            };
            if !other.touched {
                let mut mirrored = info.clone();
                mirrored.direction = match info.direction {
                    Direction::Reverse => Direction::Forward,
                    Direction::Forward => Direction::Reverse,
                };
                other.apply(&mirrored);
            }
        }
    }

    /// A single status label to show in the main file list.
    /// While one direction is active it's shown with an arrow, e.g. "Checking ←".
    /// Once settled, a two-way file still shows which direction actually did
    /// the work, e.g. "Transferred →" if only the forward copy touched it, or
    /// plain "Transferred" once both sides moved the file. A bisync file that
    /// only needed checking on both sides collapses to plain "Checked".
    pub fn combined_status(&self) -> String {
        if self.forward.status == "Error" || self.reverse.status == "Error" {
            return "Error".to_string();
        }
        if self.forward.status == "Checking" || self.forward.status == "Transferring" {
            return format!("{} {}", self.forward.status, Direction::Forward.arrow());
        }
        if self.reverse.status == "Checking" || self.reverse.status == "Transferring" {
            return format!("{} {}", self.reverse.status, Direction::Reverse.arrow());
        }
        if !self.bidirectional {
            return if self.forward.touched {
                self.forward.status.clone()
            } else {
                "-".to_string()
            };
        }
        match (self.forward.touched, self.reverse.touched) {
            (false, false) => "-".to_string(),
            (true, false) => format!("{} {}", self.forward.status, Direction::Forward.arrow()),
            (false, true) => format!("{} {}", self.reverse.status, Direction::Reverse.arrow()),
            (true, true) => {
                fn rank(status: &str) -> u8 {
                    match status {
                        "Transferred" | "Moved" | "Deleted" => 2,
                        "Checked" => 1,
                        _ => 0,
                    }
                }
                let (winning_status, winning_dir) =
                    if rank(&self.forward.status) >= rank(&self.reverse.status) {
                        (&self.forward.status, Direction::Forward)
                    } else {
                        (&self.reverse.status, Direction::Reverse)
                    };
                // "Checked" on both sides is direction-agnostic (nothing moved,
                // so which side "won" doesn't matter) - leave it plain. Anything
                // that actually moved data (Transferred/Moved/Deleted) keeps the
                // arrow so it's clear which direction did the work, even when
                // the other side also reported for this file.
                if winning_status == "Checked" {
                    winning_status.clone()
                } else {
                    format!("{} {}", winning_status, winning_dir.arrow())
                }
            }
        }
    }

    /// Per-direction icons for a two-way file, e.g. "✓→ ○←" (only the forward
    /// copy did anything; ○ means the reverse direction never needed to act,
    /// not that it's missing). Empty for genuinely one-way syncs.
    pub fn direction_icons(&self) -> String {
        if !self.bidirectional {
            return String::new();
        }
        format!(
            "{}{} {}{}",
            self.forward.icon(),
            Direction::Forward.arrow(),
            self.reverse.icon(),
            Direction::Reverse.arrow()
        )
    }

    /// True once every direction that has ever reported for this file has
    /// reached a terminal (non-active) status.
    pub fn is_settled(&self) -> bool {
        let forward_done = !self.forward.touched || is_terminal_status(&self.forward.status);
        let reverse_done = !self.bidirectional
            || !self.reverse.touched
            || is_terminal_status(&self.reverse.status);
        forward_done && reverse_done
    }
}

/// Terminal event handler.
#[derive(Debug)]
pub struct EventHandler {
    /// Event sender channel.
    sender: mpsc::UnboundedSender<Event>,
    /// Event receiver channel.
    receiver: mpsc::UnboundedReceiver<Event>,
}

impl EventHandler {
    /// Constructs a new instance of [`EventHandler`] and spawns a new thread to handle events.
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        let actor = EventTask::new(sender.clone());
        tokio::spawn(async { actor.run().await });
        Self { sender, receiver }
    }

    /// Receives an event from the sender.
    pub async fn next(&mut self) -> color_eyre::Result<Event> {
        self.receiver
            .recv()
            .await
            .ok_or_eyre("Failed to receive event")
    }

    /// Queue an app event to be sent to the event receiver.
    pub fn send(&mut self, app_event: AppEvent) {
        let _ = self.sender.send(Event::App(app_event));
    }

    /// Create a new sender to send app events to the event receiver.
    pub fn sender(&self) -> mpsc::UnboundedSender<Event> {
        self.sender.clone()
    }
}

/// A thread that handles reading crossterm events and emitting tick events on a regular schedule.
struct EventTask {
    /// Event sender channel.
    sender: mpsc::UnboundedSender<Event>,
}

impl EventTask {
    /// Constructs a new instance of [`EventTask`].
    fn new(sender: mpsc::UnboundedSender<Event>) -> Self {
        Self { sender }
    }

    /// Runs the event thread.
    async fn run(self) -> color_eyre::Result<()> {
        let tick_rate = Duration::from_secs_f64(1.0 / TICK_FPS);

        // FIX: EventStream::new() returns a Result, so we must unwrap it with `?`
        let mut reader = crossterm::event::EventStream::new();
        let mut tick = tokio::time::interval(tick_rate);

        loop {
            let tick_delay = tick.tick();
            let crossterm_event = reader.next();

            tokio::select! {
                _ = self.sender.closed() => {
                    break;
                }
                _ = tick_delay => {
                    self.send(Event::Tick);
                }
                // FIX: Handle the stream result safely to prevent potential spin-loops on error
                event = crossterm_event => {
                    if let Some(Ok(evt)) = event {
                        self.send(Event::Crossterm(evt));
                    }
                }
            };
        }
        Ok(())
    }

    /// Sends an event to the receiver.
    fn send(&self, event: Event) {
        let _ = self.sender.send(event);
    }
}
