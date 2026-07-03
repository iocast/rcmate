use crate::app::App;
use crate::views::ViewAction;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::text::Text;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};
use tui_popup::Popup;

pub struct BisyncOptionsView {
    selected_pairs: Vec<(String, String, String)>,
    active_field: usize,
}

impl BisyncOptionsView {
    pub fn help_text(&self) -> String {
        "Tab/↑/↓: navigate | Space: toggle | Left/Right: change | Esc/Enter: close ".to_string()
    }

    pub fn new(app: &App) -> Self {
        let sync_pairs = app.sync_pairs.try_read().unwrap();
        let selected = sync_pairs
            .iter()
            .filter(|v| {
                let p = v.try_read().unwrap();
                p.selected && p.sync_pair.sync_type == crate::config::SyncType::BiSync
            })
            .map(|v| {
                let p = v.try_read().unwrap();
                (
                    p.sync_pair.name.clone(),
                    p.sync_pair.source.clone(),
                    p.sync_pair.destination.clone(),
                )
            })
            .collect();
        Self {
            selected_pairs: selected,
            active_field: 0,
        }
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent, app: &mut App) -> ViewAction {
        let fields = 3;
        let modes = [
            "none", "path1", "path2", "newer", "older", "larger", "smaller",
        ];
        match key_event.code {
            KeyCode::Esc | KeyCode::Enter => ViewAction::ClosePopup,
            KeyCode::Tab | KeyCode::Down => {
                self.active_field = (self.active_field + 1) % fields;
                ViewAction::None
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.active_field = (self.active_field + fields - 1) % fields;
                ViewAction::None
            }
            KeyCode::Char(' ') => {
                let mut sync_pairs = app.sync_pairs.try_write().unwrap();
                for v in sync_pairs.iter_mut() {
                    // FIX: Use try_write() instead of async write()
                    let mut p = v.try_write().unwrap();
                    if p.selected && p.sync_pair.sync_type == crate::config::SyncType::BiSync {
                        match self.active_field {
                            0 => p.sync_pair.bisync_opts.resync = !p.sync_pair.bisync_opts.resync,
                            1 => p.sync_pair.bisync_opts.force = !p.sync_pair.bisync_opts.force,
                            2 => {
                                let current = p.sync_pair.bisync_opts.resync_mode.as_str();
                                let idx = modes.iter().position(|&m| m == current).unwrap_or(0);
                                p.sync_pair.bisync_opts.resync_mode =
                                    modes[(idx + 1) % modes.len()].to_string();
                            }
                            _ => {}
                        }
                    }
                }
                ViewAction::None
            }
            KeyCode::Left | KeyCode::Right => {
                if self.active_field == 2 {
                    let mut sync_pairs = app.sync_pairs.try_write().unwrap();
                    for v in sync_pairs.iter_mut() {
                        // FIX: Use try_write() instead of async write()
                        let mut p = v.try_write().unwrap();
                        if p.selected && p.sync_pair.sync_type == crate::config::SyncType::BiSync {
                            let current = p.sync_pair.bisync_opts.resync_mode.as_str();
                            let idx = modes.iter().position(|&m| m == current).unwrap_or(0);
                            let next_idx = if key_event.code == KeyCode::Right {
                                (idx + 1) % modes.len()
                            } else {
                                (idx + modes.len() - 1) % modes.len()
                            };
                            p.sync_pair.bisync_opts.resync_mode = modes[next_idx].to_string();
                        }
                    }
                }
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, app: &App) {
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(Span::styled(
            "Selected Bisync Pairs: ",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        let sync_pairs_read = app.sync_pairs.try_read().unwrap();
        for (name, src, dst) in &self.selected_pairs {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{}: ", name),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("{} <-> {}", src, dst)),
            ]));
        }
        if self.selected_pairs.is_empty() {
            lines.push(Line::from("No bisync pairs selected."));
        }
        lines.push(Line::from(" "));

        let opts = if let Some(v) = sync_pairs_read.iter().find(|v| {
            let p = v.try_read().unwrap();
            p.selected && p.sync_pair.sync_type == crate::config::SyncType::BiSync
        }) {
            v.try_read().unwrap().sync_pair.bisync_opts.clone()
        } else {
            crate::config::BisyncOptions::default()
        };

        let resync_style = if self.active_field == 0 {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let force_style = if self.active_field == 1 {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let mode_style = if self.active_field == 2 {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        lines.push(Line::from(vec![
            Span::styled(if opts.resync { "[X] " } else { "[ ] " }, resync_style),
            Span::raw(" Resync"),
        ]));
        lines.push(Line::from(vec![
            Span::styled(if opts.force { "[X] " } else { "[ ] " }, force_style),
            Span::raw(" Force"),
        ]));
        lines.push(Line::from(vec![
            Span::styled(format!("< {} >", opts.resync_mode), mode_style),
            Span::raw(" Resync Mode"),
        ]));

        let popup = Popup::new(Text::from(lines))
            .title("Bisync Options")
            .style(Style::new().fg(Color::White).bg(Color::Blue));
        Widget::render(popup, area, buf);
    }
}
