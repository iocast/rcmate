use crate::app::App;
use crate::config::{SyncOptions, SyncPairConfig, SyncPairUi, SyncState, SyncStatus, SyncType};
use crate::event::Severity;
use crate::tui::{Action, Message, View};
use crate::views::ViewAction;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, StatefulWidget, Widget},
};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Selectable values for the Type field's dropdown, in display order.
const SYNC_TYPES: [SyncType; 4] = [
    SyncType::Sync,
    SyncType::BiSync,
    SyncType::Copy,
    SyncType::Move,
];

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

/// Whether the form edits an existing sync pair in place or appends a new one.
enum Mode {
    Edit { sync_pair_index: usize },
    Add,
}

pub struct SyncPairFormView {
    mode: Mode,
    name: Input,
    /// Index into `SYNC_TYPES` for the committed Type value.
    sync_type: usize,
    source: Input,
    destination: Input,
    excludes: Input,
    includes: Input,
    filter: Input,
    active_field: usize,
    /// Whether the Type field's dropdown is currently expanded.
    type_dropdown_open: bool,
    /// Index into `SYNC_TYPES` highlighted while the dropdown is open; only
    /// committed to `sync_type` when the user confirms with Enter.
    type_highlight: usize,
}

impl SyncPairFormView {
    /// Form pre-filled with the sync pair at `index`; saving overwrites it.
    pub fn edit(index: usize, app: &App) -> Self {
        let sync_pairs = app.sync_pairs.try_read().unwrap();
        let pair = sync_pairs[index].try_read().unwrap();
        let sp = &pair.sync_pair;
        let sync_type = SYNC_TYPES
            .iter()
            .position(|t| *t == sp.sync_type)
            .unwrap_or(0);
        Self {
            mode: Mode::Edit {
                sync_pair_index: index,
            },
            name: Input::new(sp.name.clone()),
            sync_type,
            source: Input::new(sp.source.clone()),
            destination: Input::new(sp.destination.clone()),
            excludes: Input::new(sp.excludes.clone().unwrap_or_default().join(", ")),
            includes: Input::new(sp.includes.clone().unwrap_or_default().join(", ")),
            filter: Input::new(sp.filter.clone().unwrap_or_default()),
            active_field: 0,
            type_dropdown_open: false,
            type_highlight: sync_type,
        }
    }

    /// Empty form; saving appends a new sync pair to the config.
    pub fn add() -> Self {
        Self {
            mode: Mode::Add,
            name: Input::new(String::new()),
            sync_type: 0,
            source: Input::new(String::new()),
            destination: Input::new(String::new()),
            excludes: Input::new(String::new()),
            includes: Input::new(String::new()),
            filter: Input::new(String::new()),
            active_field: 0,
            type_dropdown_open: false,
            type_highlight: 0,
        }
    }

    fn title(&self) -> &'static str {
        match self.mode {
            Mode::Edit { .. } => "Edit Sync Pair  ",
            Mode::Add => "Add Sync Pair  ",
        }
    }

    pub fn help_text(&self) -> String {
        if self.type_dropdown_open {
            "↑/↓: choose | SPACE: confirm | Esc: cancel ".to_string()
        } else {
            "Esc: cancel | Enter: save | SPACE: choose type | Tab/Shift+Tab: navigate ".to_string()
        }
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent, app: &mut App) -> ViewAction {
        let fields = 7;

        // The dropdown takes over all input while expanded; other fields and
        // the Tab/Esc/Enter form-level bindings are unreachable until it's
        // closed, mirroring how a native combobox traps focus.
        if self.type_dropdown_open {
            match key_event.code {
                KeyCode::Up => {
                    self.type_highlight = (self.type_highlight + SYNC_TYPES.len() - 1)
                        % SYNC_TYPES.len();
                }
                KeyCode::Down => {
                    self.type_highlight = (self.type_highlight + 1) % SYNC_TYPES.len();
                }
                KeyCode::Char(' ') => {
                    self.sync_type = self.type_highlight;
                    self.type_dropdown_open = false;
                }
                KeyCode::Esc => {
                    // Discard the highlight change and keep the committed value.
                    self.type_dropdown_open = false;
                }
                _ => {}
            }
            return ViewAction::None;
        }

        match key_event.code {
            KeyCode::Esc => ViewAction::SwitchTo(View::Main(crate::views::main::MainView)),
            // Enter always saves the form, on every field including Type -
            // SPACE (below) is what opens the dropdown, so there's no
            // ambiguity about what Enter does depending on which field has
            // focus.
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
                    // Stay in the form so the invalid input can be corrected.
                    return ViewAction::None;
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
            KeyCode::Char(' ') if self.active_field == 1 => {
                self.type_highlight = self.sync_type;
                self.type_dropdown_open = true;
                ViewAction::None
            }
            KeyCode::Char(c) => {
                if self.active_field != 1 {
                    self.get_active_input().insert_char(c);
                }
                ViewAction::None
            }
            KeyCode::Backspace => {
                if self.active_field != 1 {
                    self.get_active_input().delete_char();
                }
                ViewAction::None
            }
            KeyCode::Left => {
                if self.active_field != 1 {
                    self.get_active_input().move_left();
                }
                ViewAction::None
            }
            KeyCode::Right => {
                if self.active_field != 1 {
                    self.get_active_input().move_right();
                }
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    fn get_active_input(&mut self) -> &mut Input {
        match self.active_field {
            0 => &mut self.name,
            2 => &mut self.source,
            3 => &mut self.destination,
            4 => &mut self.excludes,
            5 => &mut self.includes,
            6 => &mut self.filter,
            _ => unreachable!("field 1 (Type) has no text input; callers must guard for it"),
        }
    }

    /// Splits a comma separated field into a list, or `None` when empty.
    fn split_list(value: &str) -> Option<Vec<String>> {
        if value.trim().is_empty() {
            None
        } else {
            Some(
                value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
            )
        }
    }

    /// Builds a `SyncPairConfig` from the current field values, rejecting
    /// input that would produce a pair rclone can't run.
    fn to_config(&self) -> color_eyre::Result<SyncPairConfig> {
        let sync_type = SYNC_TYPES[self.sync_type].clone();
        if self.name.value.trim().is_empty() {
            return Err(color_eyre::eyre::eyre!("Name must not be empty"));
        }
        if self.source.value.trim().is_empty() {
            return Err(color_eyre::eyre::eyre!("Source must not be empty"));
        }
        if self.destination.value.trim().is_empty() {
            return Err(color_eyre::eyre::eyre!("Destination must not be empty"));
        }
        Ok(SyncPairConfig {
            name: self.name.value.trim().to_string(),
            sync_type,
            source: self.source.value.trim().to_string(),
            destination: self.destination.value.trim().to_string(),
            excludes: Self::split_list(&self.excludes.value),
            includes: Self::split_list(&self.includes.value),
            filter: if self.filter.value.trim().is_empty() {
                None
            } else {
                Some(self.filter.value.trim().to_string())
            },
            options: SyncOptions::default(),
        })
    }

    fn save(&self, app: &mut App) -> color_eyre::Result<()> {
        let config = self.to_config()?;
        match self.mode {
            Mode::Edit { sync_pair_index } => {
                let sync_pairs = app.sync_pairs.try_write().unwrap();
                let mut pair = sync_pairs[sync_pair_index].try_write().unwrap();
                // Keep the existing options - they're edited from the Options
                // popup on the main view, not from this form.
                let options = pair.sync_pair.options.clone();
                pair.sync_pair = SyncPairConfig { options, ..config };
            }
            Mode::Add => {
                let index = {
                    let mut sync_pairs = app.sync_pairs.try_write().unwrap();
                    sync_pairs.push(Arc::new(RwLock::new(SyncPairUi {
                        key: Uuid::new_v4(),
                        selected: false,
                        status: SyncStatus::default(),
                        sync_state: SyncState::default(),
                        sync_pair: config,
                    })));
                    sync_pairs.len() - 1
                };
                // Move the cursor onto the pair that was just created.
                app.sync_pairs_tbl_state.select(Some(index));
            }
        }
        app.save_config()?;
        Ok(())
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, _app: &App) {
        let block = Block::default().borders(Borders::ALL).title(self.title());
        let inner = block.inner(area);
        block.render(area, buf);

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
            .split(inner);
        self.render_input("Name", &self.name, 0, chunks[0], buf);
        self.render_type_field(chunks[1], buf);
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

        if self.type_dropdown_open {
            self.render_type_dropdown(chunks[1], buf);
        }
    }

    fn render_type_field(&self, area: Rect, buf: &mut Buffer) {
        let active = self.active_field == 1;
        let value = SYNC_TYPES[self.sync_type].to_string();
        let style = if active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        let line = Line::from(vec![
            Span::raw(value),
            Span::styled(
                if self.type_dropdown_open { " ▲" } else { " ▼" },
                Style::default().add_modifier(Modifier::DIM),
            ),
        ]);
        Paragraph::new(line)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Type (SPACE to choose)")
                    .style(style),
            )
            .render(area, buf);
    }

    /// Renders the expanded options list directly below the Type field,
    /// overlaying whatever field comes next.
    fn render_type_dropdown(&self, field_area: Rect, buf: &mut Buffer) {
        let items: Vec<ListItem> = SYNC_TYPES
            .iter()
            .map(|t| ListItem::new(t.to_string()))
            .collect();
        let height = items.len() as u16 + 2; // + borders
        let area = Rect {
            x: field_area.x,
            y: field_area.y + field_area.height,
            width: field_area.width,
            height,
        };
        Clear.render(area, buf);
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Select Type"),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("> ");
        let mut state = ListState::default();
        state.select(Some(self.type_highlight));
        StatefulWidget::render(list, area, buf, &mut state);
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
