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
