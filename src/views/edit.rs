use crate::app::App;
use crate::config::SyncType;
use crate::event::Severity;
use crate::tui::{Action, Message, View};
use crate::views::ViewAction;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

struct Input {
    value: String,
    cursor: usize,
}
impl Input {
    fn new(value: String) -> Self {
        let cursor = value.len();
        Self { value, cursor }
    }
    fn insert_char(&mut self, c: char) {
        self.value.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }
    fn delete_char(&mut self) {
        if self.cursor > 0 {
            let mut prev = self.cursor - 1;
            while prev > 0 && !self.value.is_char_boundary(prev) {
                prev -= 1;
            }
            self.value.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }
    fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            while self.cursor > 0 && !self.value.is_char_boundary(self.cursor) {
                self.cursor -= 1;
            }
        }
    }
    fn move_right(&mut self) {
        if self.cursor < self.value.len() {
            self.cursor += 1;
            while self.cursor < self.value.len() && !self.value.is_char_boundary(self.cursor) {
                self.cursor += 1;
            }
        }
    }
}

pub struct EditView {
    sync_pair_index: usize,
    name: Input,
    sync_type: Input,
    source: Input,
    destination: Input,
    excludes: Input,
    includes: Input,
    filter: Input,
    active_field: usize,
}

impl EditView {
    pub fn new(index: usize, app: &App) -> Self {
        let sync_pairs = app.sync_pairs.try_read().unwrap();
        let pair = sync_pairs[index].try_read().unwrap();
        let sp = &pair.sync_pair;
        Self {
            sync_pair_index: index,
            name: Input::new(sp.name.clone()),
            sync_type: Input::new(sp.sync_type.to_string()),
            source: Input::new(sp.source.clone()),
            destination: Input::new(sp.destination.clone()),
            excludes: Input::new(sp.excludes.clone().unwrap_or_default().join(", ")),
            includes: Input::new(sp.includes.clone().unwrap_or_default().join(", ")),
            filter: Input::new(sp.filter.clone().unwrap_or_default()),
            active_field: 0,
        }
    }

    pub fn help_text(&self) -> String {
        "Esc: cancel | Enter: save | Tab/Shift+Tab: navigate ".to_string()
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent, app: &mut App) -> ViewAction {
        let fields = 7;
        match key_event.code {
            KeyCode::Esc => ViewAction::SwitchTo(View::Main(crate::views::main::MainView)),
            KeyCode::Enter => {
                if let Err(e) = self.save(app) {
                    let close = Action {
                        name: "Close".to_string(),
                        description: "ESC: close".to_string(),
                        key_code: KeyCode::Esc,
                        callback: std::sync::Arc::new(|handler| handler.close_message()),
                    };
                    app.popup = Some(View::Message(Message::new(
                        Severity::Error,
                        "Save Error".to_string(),
                        e.to_string(),
                        vec![close],
                    )));
                }
                ViewAction::SwitchTo(View::Main(crate::views::main::MainView))
            }
            KeyCode::Tab => {
                self.active_field = (self.active_field + 1) % fields;
                ViewAction::None
            }
            KeyCode::BackTab => {
                self.active_field = (self.active_field + fields - 1) % fields;
                ViewAction::None
            }
            KeyCode::Char(c) => {
                self.get_active_input().insert_char(c);
                ViewAction::None
            }
            KeyCode::Backspace => {
                self.get_active_input().delete_char();
                ViewAction::None
            }
            KeyCode::Left => {
                self.get_active_input().move_left();
                ViewAction::None
            }
            KeyCode::Right => {
                self.get_active_input().move_right();
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    fn get_active_input(&mut self) -> &mut Input {
        match self.active_field {
            0 => &mut self.name,
            1 => &mut self.sync_type,
            2 => &mut self.source,
            3 => &mut self.destination,
            4 => &mut self.excludes,
            5 => &mut self.includes,
            6 => &mut self.filter,
            _ => unreachable!(),
        }
    }

    fn save(&self, app: &mut App) -> color_eyre::Result<()> {
        let sync_type = match self.sync_type.value.to_lowercase().as_str() {
            "sync" => SyncType::Sync,
            "bisync" => SyncType::BiSync,
            "copy" => SyncType::Copy,
            "move" => SyncType::Move,
            _ => {
                return Err(color_eyre::eyre::eyre!(
                    "Invalid sync type. Use: sync, bisync, copy, move"
                ));
            }
        };
        let includes = if self.includes.value.trim().is_empty() {
            None
        } else {
            Some(
                self.includes
                    .value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
            )
        };
        let excludes = if self.excludes.value.trim().is_empty() {
            None
        } else {
            Some(
                self.excludes
                    .value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
            )
        };
        {
            let sync_pairs = app.sync_pairs.try_write().unwrap();
            let mut pair = sync_pairs[self.sync_pair_index].try_write().unwrap();
            pair.sync_pair.name = self.name.value.clone();
            pair.sync_pair.sync_type = sync_type;
            pair.sync_pair.source = self.source.value.clone();
            pair.sync_pair.destination = self.destination.value.clone();
            pair.sync_pair.includes = includes;
            pair.sync_pair.excludes = excludes;
            pair.sync_pair.filter = if self.filter.value.trim().is_empty() {
                None
            } else {
                Some(self.filter.value.clone())
            };
        }
        app.save_config()?;
        Ok(())
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, _app: &App) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(area);
        self.render_input("Name", &self.name, 0, chunks[0], buf);
        self.render_input(
            "Type (sync/bisync/copy/move)",
            &self.sync_type,
            1,
            chunks[1],
            buf,
        );
        self.render_input("Source", &self.source, 2, chunks[2], buf);
        self.render_input("Destination", &self.destination, 3, chunks[3], buf);
        self.render_input(
            "Excludes (comma separated)",
            &self.excludes,
            4,
            chunks[4],
            buf,
        );
        self.render_input(
            "Includes (comma separated)",
            &self.includes,
            5,
            chunks[5],
            buf,
        );
        self.render_input("Filter", &self.filter, 6, chunks[6], buf);
    }

    fn render_input(&self, title: &str, input: &Input, index: usize, area: Rect, buf: &mut Buffer) {
        let active = self.active_field == index;
        let (before, _) = input.value.split_at(input.cursor);
        let mut spans = vec![Span::raw(before)];
        if input.cursor < input.value.len() {
            let mut chars = input.value[input.cursor..].chars();
            if let Some(c) = chars.next() {
                spans.push(Span::styled(
                    c.to_string(),
                    Style::default().add_modifier(Modifier::REVERSED),
                ));
                spans.push(Span::raw(chars.as_str()));
            }
        } else {
            spans.push(Span::styled(
                " ",
                Style::default().add_modifier(Modifier::REVERSED),
            ));
        }
        let style = if active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        Paragraph::new(Line::from(spans))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .style(style),
            )
            .render(area, buf);
    }
}
