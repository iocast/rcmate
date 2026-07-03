use crossterm::event::KeyCode;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Paragraph, Widget},
};
use std::fmt;
use std::sync::Arc;
use textwrap;
use tui_popup::Popup;

use crate::{
    app::App,
    event::Severity,
    views::{
        ViewAction, about::AboutView, bisync_options::BisyncOptionsView, edit::EditView,
        main::MainView,
    },
};

// Unified View enum replacing Screen and Overlay
pub enum View {
    Main(MainView),
    Edit(EditView),
    BisyncOptions(BisyncOptionsView),
    Message(Message),
    About(AboutView),
}

impl View {
    pub fn help_text(&self) -> String {
        match self {
            View::Main(v) => v.help_text(),
            View::Edit(v) => v.help_text(),
            View::BisyncOptions(v) => v.help_text(),
            View::About(v) => v.help_text(),
            View::Message(msg) => msg.help_text(),
        }
    }

    pub fn handle_key_event(
        &mut self,
        key_event: crossterm::event::KeyEvent,
        app: &mut App,
    ) -> ViewAction {
        match self {
            View::Main(v) => v.handle_key_event(key_event, app),
            View::Edit(v) => v.handle_key_event(key_event, app),
            View::BisyncOptions(v) => v.handle_key_event(key_event, app),
            View::About(v) => v.handle_key_event(key_event, app),
            View::Message(msg) => {
                let action_to_run = msg
                    .actions
                    .iter()
                    .find(|a| a.key_code == key_event.code)
                    .map(|a| Arc::clone(&a.callback));

                if let Some(cb) = action_to_run {
                    (cb)(app);
                    return ViewAction::ClosePopup;
                } else if key_event.code == KeyCode::Esc {
                    return ViewAction::ClosePopup;
                }
                ViewAction::None
            }
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, app: &App) {
        match self {
            View::Main(v) => v.render(area, buf, app),
            View::Edit(v) => v.render(area, buf, app),
            View::BisyncOptions(v) => v.render(area, buf, app),
            View::About(v) => v.render(area, buf, app),
            View::Message(msg) => Widget::render(msg.clone(), area, buf),
        }
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let chunks = Layout::default()
            .constraints([Constraint::Min(3), Constraint::Length(2)])
            .split(area);

        // 1. Render the active base view
        self.active_view.render(chunks[0], buf, self);

        // 2. Render popup overlay if active
        if let Some(popup) = &self.popup {
            popup.render(area, buf, self);
        }

        // 3. Render navigation keys at the bottom (delegated to the active view/popup)
        let help_text = if let Some(popup) = &self.popup {
            popup.help_text()
        } else {
            self.active_view.help_text()
        };

        Paragraph::new(Text::from(help_text))
            .block(Block::default())
            .alignment(Alignment::Center)
            .render(chunks[1], buf);
    }
}

#[derive(Clone)]
pub struct Action {
    pub name: String,
    pub description: String,
    pub key_code: KeyCode,
    pub callback: Arc<dyn Fn(&mut dyn ActionHandler) + Send + Sync + 'static>,
}

impl fmt::Debug for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Action")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("key_code", &self.key_code)
            .field("callback", &"<closure>")
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub severity: Severity,
    pub title: String,
    pub msg: String,
    pub actions: Vec<Action>,
}

pub trait ActionHandler {
    fn close_message(&mut self);
}

impl Message {
    pub fn new(severity: Severity, title: String, msg: String, actions: Vec<Action>) -> Self {
        Self {
            severity,
            title,
            msg,
            actions,
        }
    }

    pub fn help_text(&self) -> String {
        self.actions
            .iter()
            .map(|action| action.description.as_str())
            .collect::<Vec<_>>()
            .join(" |  ")
    }
}

impl Widget for Message {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let max_text_width = (area.width as usize).saturating_sub(4).min(80);
        let wrapped_msg = self
            .msg
            .lines()
            .flat_map(|line| textwrap::wrap(line, max_text_width))
            .collect::<Vec<_>>()
            .join("\n");

        let mut action_spans = Vec::new();
        for (i, action) in self.actions.iter().enumerate() {
            if i > 0 {
                action_spans.push(Span::raw(" |  "));
            }
            action_spans.push(Span::styled(
                &action.name,
                Style::default().add_modifier(Modifier::BOLD),
            ));
        }

        let mut lines: Vec<Line> = wrapped_msg
            .lines()
            .map(|l| Line::from(l.to_string()))
            .collect();
        lines.push(Line::from(" "));
        lines.push(Line::from(action_spans).alignment(Alignment::Right));

        let popup = Popup::new(Text::from(lines))
            .title(self.title)
            .style(Style::new().fg(match self.severity {
                Severity::Success => Color::Green,
                Severity::Info => Color::Cyan,
                Severity::Warn => Color::Yellow,
                Severity::Error => Color::Red,
            }));

        Widget::render(popup, area, buf);
    }
}

impl crate::config::SyncState {
    pub const BAR_LENGTH: usize = 40;
    pub fn new(percent: u16, style: Style) -> Self {
        Self {
            percent,
            style,
            ..Default::default()
        }
    }
}

impl std::fmt::Display for crate::config::SyncState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bar_length = Self::BAR_LENGTH - 5;
        let filled_chars = (self.percent as f32 / 100.0 * bar_length as f32).round() as usize;
        let empty_chars = bar_length.saturating_sub(filled_chars);
        write!(
            f,
            "{}{}{:>4}%",
            "█".repeat(filled_chars),
            "░".repeat(empty_chars),
            self.percent
        )
    }
}
