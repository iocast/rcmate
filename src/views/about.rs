use crate::app::App;
use crate::views::ViewAction;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::Widget,
};
use tui_popup::Popup;

#[derive(Debug, Clone)]
pub struct AboutInfo {
    pub rclone_version: String,
    pub rclone_bin: String,
    pub app_config_path: String,
}

pub struct AboutView {
    pub info: AboutInfo,
}

impl AboutView {
    pub fn new(info: AboutInfo) -> Self {
        Self { info }
    }

    pub fn help_text(&self) -> String {
        "ESC: close".to_string()
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent, _app: &mut App) -> ViewAction {
        if key_event.code == KeyCode::Esc {
            ViewAction::ClosePopup
        } else {
            ViewAction::None
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, _app: &App) {
        let primary = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        let muted = Style::default().fg(Color::DarkGray);
        let accent = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        let label_style = Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD);

        let project_version = env!("CARGO_PKG_VERSION");

        let lines = vec![
            Line::from(Span::styled(
                "  _ __   ___ _ __ ___   __ _| |_ ___ ",
                primary,
            )),
            Line::from(Span::styled(
                " | '__| / __| '_ ` _ \\ / _` | __/ _ \\",
                primary,
            )),
            Line::from(Span::styled(
                " | |   | (__| | | | | | (_| | ||  __/",
                primary,
            )),
            Line::from(Span::styled(
                " |_|    \\___|_| |_| |_|\\__,_|\\__\\___|",
                primary,
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("rcmate version:   ", label_style),
                Span::styled(project_version, accent),
            ]),
            Line::from(vec![
                Span::styled("rclone version:   ", label_style),
                Span::styled(&self.info.rclone_version, accent),
            ]),
            Line::from(vec![
                Span::styled("rclone binary:    ", label_style),
                Span::styled(&self.info.rclone_bin, muted),
            ]),
            Line::from(vec![
                Span::styled("config path:      ", label_style),
                Span::styled(&self.info.app_config_path, muted),
            ]),
            Line::from(""),
            Line::from(Span::styled("Your Friendly Rclone Companion", muted)),
        ];

        let popup = Popup::new(Text::from(lines))
            .title(" About ")
            .style(Style::default());

        Widget::render(popup, area, buf);
    }
}
