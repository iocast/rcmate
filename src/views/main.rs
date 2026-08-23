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
        [
            "q: quit | ↑/↓: navigate | SPACE: select | a: select all",
            "CTRL+s: run | p: progress | o: options | S: settings",
            "CTRL+a: add | e: edit | DEL: delete | ALT+e: error | ALT+SHIFT+I: info",
        ]
        .join("\n")
    }

    /// Builds the confirmation popup for DEL. A bisync pair always gets a
    /// third choice - delete the config entry only, or also the listing/lock
    /// files bisync keeps in the rclone workdir - so deletion never silently
    /// orphans or silently wipes that state. The popup shows the exact file
    /// prefix (everything before `.pathX.xxx`) so it's clear what would be
    /// removed - see `SyncPairConfig::bisync_state_files`.
    fn confirm_delete(&mut self, app: &App) -> ViewAction {
        let Some(idx) = app.sync_pairs_tbl_state.selected() else {
            return ViewAction::None;
        };
        let sync_pairs = app.sync_pairs.try_read().unwrap();
        let Some(pair_arc) = sync_pairs.get(idx).cloned() else {
            return ViewAction::None;
        };
        drop(sync_pairs);
        let pair = pair_arc.try_read().unwrap();
        let name = pair.sync_pair.name.clone();
        let is_bisync = pair.sync_pair.sync_type == crate::config::SyncType::BiSync;
        let prefix = is_bisync.then(|| pair.sync_pair.bisync_session_prefix());
        let state_file_count = match (&prefix, app.rclone.try_read().unwrap().workdir.as_ref()) {
            (Some(_), Some(workdir)) => pair.sync_pair.bisync_state_files(workdir).len(),
            _ => 0,
        };
        drop(pair);

        let cancel = Action {
            name: "Cancel".to_string(),
            description: "Esc: cancel".to_string(),
            key_code: KeyCode::Esc,
            callback: std::sync::Arc::new(|handler| handler.close_message()),
        };

        match prefix {
            None => {
                let confirm = Action {
                    name: "Delete".to_string(),
                    description: "Enter: delete".to_string(),
                    key_code: KeyCode::Enter,
                    callback: std::sync::Arc::new(move |handler| {
                        handler.delete_sync_pair(idx, false)
                    }),
                };
                ViewAction::OpenPopup(View::Message(Message::new(
                    Severity::Warn,
                    "Delete sync pair".to_string(),
                    format!("Delete sync pair \"{}\"? This cannot be undone.", name),
                    vec![confirm, cancel],
                )))
            }
            Some(prefix) => {
                let both = Action {
                    name: "Delete both".to_string(),
                    description: "Enter: entry + bisync state".to_string(),
                    key_code: KeyCode::Enter,
                    callback: std::sync::Arc::new(move |handler| handler.delete_sync_pair(idx, true)),
                };
                let entry_only = Action {
                    name: "Entry only".to_string(),
                    description: "d: entry only".to_string(),
                    key_code: KeyCode::Char('d'),
                    callback: std::sync::Arc::new(move |handler| {
                        handler.delete_sync_pair(idx, false)
                    }),
                };
                let found_note = if state_file_count > 0 {
                    format!(
                        "{} matching state file(s) found in the rclone workdir.",
                        state_file_count
                    )
                } else {
                    "No matching state files found in the rclone workdir right now.".to_string()
                };
                ViewAction::OpenPopup(View::Message(Message::new(
                    Severity::Warn,
                    "Delete sync pair".to_string(),
                    format!(
                        "Delete sync pair \"{}\"?\n\nThis is a bisync pair. Its workdir file prefix is:\n{}\n\n{}\n\nAlso delete those bisync state files (listings/lock), or keep them and only delete the config entry?",
                        name, prefix, found_note
                    ),
                    vec![both, entry_only, cancel],
                )))
            }
        }
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
            // Options belong to a single entry and depend on its type, so
            // they're edited for the pair under the cursor - not for the
            // (possibly mixed-type) selection.
            KeyCode::Char('o') => {
                let view = app
                    .sync_pairs_tbl_state
                    .selected()
                    .and_then(|idx| crate::views::options::OptionsView::new(idx, app));
                match view {
                    Some(view) => ViewAction::OpenPopup(View::Options(view)),
                    None => {
                        let close = Action {
                            name: "Close".to_string(),
                            description: "ESC: close".to_string(),
                            key_code: KeyCode::Esc,
                            callback: std::sync::Arc::new(|handler| handler.close_message()),
                        };
                        ViewAction::OpenPopup(View::Message(Message::new(
                            Severity::Info,
                            "Options".to_string(),
                            "There is no sync pair to configure options for.".to_string(),
                            vec![close],
                        )))
                    }
                }
            }
            KeyCode::Char('S') if key_event.modifiers != KeyModifiers::CONTROL => {
                ViewAction::SwitchTo(View::SettingsForm(
                    crate::views::settings_form::SettingsFormView::new(app),
                ))
            }
            KeyCode::Char('s' | 'S') if key_event.modifiers == KeyModifiers::CONTROL => {
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
                        ViewAction::SwitchTo(View::SyncPairForm(
                            crate::views::sync_pair_form::SyncPairFormView::edit(idx, app),
                        ))
                    } else {
                        ViewAction::None
                    }
                }
            }
            // Must come before the bare 'a' arm below, which would otherwise
            // also match CTRL+a and toggle the selection instead.
            KeyCode::Char('a' | 'A') if key_event.modifiers == KeyModifiers::CONTROL => {
                ViewAction::SwitchTo(View::SyncPairForm(
                    crate::views::sync_pair_form::SyncPairFormView::add(),
                ))
            }
            KeyCode::Char('a') => {
                app.events.send(AppEvent::SelectAll);
                ViewAction::None
            }
            KeyCode::Delete => self.confirm_delete(app),
            KeyCode::Char('p') => {
                if let Some(idx) = app.sync_pairs_tbl_state.selected() {
                    let sync_pairs = app.sync_pairs.try_read().unwrap();
                    if let Some(pair_arc) = sync_pairs.get(idx) {
                        let pair = pair_arc.try_read().unwrap();
                        let key = pair.key;
                        ViewAction::OpenPopup(View::Progress(
                            crate::views::progress::ProgressView::new(key),
                        ))
                    } else {
                        ViewAction::None
                    }
                } else {
                    ViewAction::None
                }
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
