use crate::app::App;
use crate::config::SyncState;
use crate::event::AppEvent;
use crate::event::Severity;
use crate::tui::{Action, Message, View};
use crate::views::ViewAction;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Cell, Row, StatefulWidget, Table},
};

pub struct MainView;

impl MainView {
    pub fn help_text(&self) -> String {
        "q: quit | ↑/↓: Up/Down | SPACE: select | a: all | ALT+s: run | o: bisync opts | ALT+e: error | e: edit | ALT+SHIFT+I: info".to_string()
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent, app: &mut App) -> ViewAction {
        match key_event.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                app.events.send(AppEvent::Quit);
                ViewAction::None
            }
            KeyCode::Char('c' | 'C') if key_event.modifiers == KeyModifiers::CONTROL => {
                app.events.send(AppEvent::Quit);
                ViewAction::None
            }
            KeyCode::Char('I') if key_event.modifiers.contains(KeyModifiers::ALT) => {
                // Read the pre-fetched data synchronously
                let rclone_version = app.rclone_version.try_read().unwrap().clone();
                let rclone_bin = app.rclone.try_read().unwrap().bin.clone();
                let config_path = app.config_path.to_string_lossy().into_owned();

                let info = crate::views::about::AboutInfo {
                    rclone_version,
                    rclone_bin,
                    app_config_path: config_path,
                };

                // Return the ViewAction directly, matching your architecture
                ViewAction::OpenPopup(View::About(crate::views::about::AboutView::new(info)))
            }
            KeyCode::Char('o') => {
                let sync_pairs = app.sync_pairs.try_read().unwrap();
                let has_bisync = sync_pairs
                    .iter()
                    .filter(|v| v.try_read().unwrap().selected)
                    .any(|v| {
                        v.try_read().unwrap().sync_pair.sync_type == crate::config::SyncType::BiSync
                    });
                if has_bisync {
                    ViewAction::OpenPopup(View::BisyncOptions(
                        crate::views::bisync_options::BisyncOptionsView::new(app),
                    ))
                } else {
                    let close = Action {
                        name: "Close".to_string(),
                        description: "ESC: close".to_string(),
                        key_code: KeyCode::Esc,
                        callback: std::sync::Arc::new(|handler| handler.close_message()),
                    };
                    ViewAction::OpenPopup(View::Message(Message::new(
                        Severity::Info,
                        "Bisync Options".to_string(),
                        "Please select at least one bisync pair to configure options.".to_string(),
                        vec![close],
                    )))
                }
            }
            KeyCode::Char('s') if key_event.modifiers == KeyModifiers::ALT => {
                let sync_pairs = app.sync_pairs.try_read().unwrap();
                let any_selected = sync_pairs.iter().any(|v| v.try_read().unwrap().selected);
                if any_selected {
                    app.events.send(AppEvent::Synchronize);
                    ViewAction::None
                } else {
                    let close = Action {
                        name: "Close".to_string(),
                        description: "ESC: close".to_string(),
                        key_code: KeyCode::Esc,
                        callback: std::sync::Arc::new(|handler| handler.close_message()),
                    };
                    ViewAction::OpenPopup(View::Message(Message::new(
                        Severity::Warn,
                        "sync pair selection".to_string(),
                        "Please select at least one sync pair.".to_string(),
                        vec![close],
                    )))
                }
            }
            KeyCode::Char('e') => {
                if key_event.modifiers == KeyModifiers::ALT {
                    if let Some(idx) = app.sync_pairs_tbl_state.selected() {
                        let sync_pairs = app.sync_pairs.try_read().unwrap();
                        if let Some(pair_arc) = sync_pairs.get(idx) {
                            let pair = pair_arc.try_read().unwrap();
                            if pair.status == crate::config::SyncStatus::Error {
                                let close = Action {
                                    name: "Close".to_string(),
                                    description: "ESC: close".to_string(),
                                    key_code: KeyCode::Esc,
                                    callback: std::sync::Arc::new(|handler| {
                                        handler.close_message()
                                    }),
                                };
                                ViewAction::OpenPopup(View::Message(Message::new(
                                    Severity::Error,
                                    "sync pair error".to_string(),
                                    pair.sync_state.messages.join("\n"),
                                    vec![close],
                                )))
                            } else {
                                ViewAction::None
                            }
                        } else {
                            ViewAction::None
                        }
                    } else {
                        ViewAction::None
                    }
                } else {
                    if let Some(idx) = app.sync_pairs_tbl_state.selected() {
                        ViewAction::SwitchTo(View::Edit(crate::views::edit::EditView::new(
                            idx, app,
                        )))
                    } else {
                        ViewAction::None
                    }
                }
            }
            KeyCode::Char('a') => {
                app.events.send(AppEvent::SelectAll);
                ViewAction::None
            }
            KeyCode::Up => {
                app.events.send(AppEvent::Previous);
                ViewAction::None
            }
            KeyCode::Down => {
                app.events.send(AppEvent::Next);
                ViewAction::None
            }
            KeyCode::Char(' ') => {
                app.events.send(AppEvent::Select);
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, app: &App) {
        let header = Row::new(vec![
            Cell::from(Span::styled(
                "  ",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Cell::from(Span::styled(
                "Name  ",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Cell::from(Span::styled(
                "Progress  ",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Cell::from(Span::styled(
                "Type  ",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Cell::from(Span::styled(
                "Status  ",
                Style::default().add_modifier(Modifier::BOLD),
            )),
        ]);
        let b = app.sync_pairs.clone();
        let sync_pairs = b.try_read().unwrap();
        let mut rows: Vec<Row> = Vec::with_capacity(sync_pairs.len());
        let mut max_name = 0usize;
        for v in sync_pairs.iter() {
            let i = v.try_read().unwrap();
            let name_len = i.sync_pair.name.len();
            if name_len > max_name {
                max_name = name_len;
            }
            rows.push({
                let mut row = Row::new(vec![
                    Cell::from(if i.selected { "[X]  " } else { "[ ]  " }),
                    Cell::from(i.sync_pair.name.clone()),
                    Cell::from(i.sync_state.to_string()).style(i.sync_state.style),
                    Cell::from(i.sync_pair.sync_type.to_string()),
                    Cell::from(i.status.to_string()),
                ]);
                if i.selected {
                    row = row.style(Style::default().fg(Color::Green));
                }
                row
            });
        }
        let mut table = Table::new(
            rows,
            &[
                Constraint::Length(3),
                Constraint::Min(max_name as u16),
                Constraint::Length(SyncState::BAR_LENGTH as u16),
                Constraint::Length(10),
                Constraint::Length(12),
            ],
        )
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Sync Pairs  "))
        .row_highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::REVERSED),
        );

        if app.popup.is_some() {
            table = table.style(Style::default().fg(Color::DarkGray).bg(Color::Black));
        }
        StatefulWidget::render(table, area, buf, &mut app.sync_pairs_tbl_state.clone());
    }
}
