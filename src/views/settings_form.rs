use crate::app::App;
use crate::views::ViewAction;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, StatefulWidget, Widget},
};
use std::path::PathBuf;

/// Selectable values for the Log level field's dropdown, in display order.
const LOG_LEVELS: [&str; 5] = ["trace", "debug", "info", "warn", "error"];

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

/// Edits the `[general]` and `[rclone]` sections of config.toml. Unlike sync
/// pairs there's only ever one of each, so this form always edits the live
/// values in `app.general` / `app.rclone` rather than needing an Add/Edit
/// mode.
pub struct SettingsFormView {
    log_path: Input,
    /// Index into `LOG_LEVELS` for the committed log level value.
    log_level: usize,
    bin: Input,
    config: Input,
    workdir: Input,
    active_field: usize,
    /// Whether the Log level field's dropdown is currently expanded.
    level_dropdown_open: bool,
    /// Index into `LOG_LEVELS` highlighted while the dropdown is open; only
    /// committed to `log_level` when the user confirms with SPACE.
    level_highlight: usize,
}

impl SettingsFormView {
    pub fn new(app: &App) -> Self {
        let general = app.general.try_read().unwrap();
        let rclone = app.rclone.try_read().unwrap();
        let log_level = LOG_LEVELS
            .iter()
            .position(|l| *l == general.log_level)
            .unwrap_or(2);
        Self {
            log_path: Input::new(
                general
                    .log_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
            log_level,
            bin: Input::new(rclone.bin.clone()),
            config: Input::new(
                rclone
                    .config
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
            workdir: Input::new(
                rclone
                    .workdir
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
            active_field: 0,
            level_dropdown_open: false,
            level_highlight: log_level,
        }
    }

    pub fn help_text(&self) -> String {
        if self.level_dropdown_open {
            "↑/↓: choose | SPACE: confirm | Esc: cancel ".to_string()
        } else {
            "Esc: cancel | Enter: save | SPACE: choose level | Tab/Shift+Tab: navigate ".to_string()
        }
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent, app: &mut App) -> ViewAction {
        let fields = 5;

        // The dropdown takes over all input while expanded, mirroring the
        // Type dropdown on the sync pair form.
        if self.level_dropdown_open {
            match key_event.code {
                KeyCode::Up => {
                    self.level_highlight =
                        (self.level_highlight + LOG_LEVELS.len() - 1) % LOG_LEVELS.len();
                }
                KeyCode::Down => {
                    self.level_highlight = (self.level_highlight + 1) % LOG_LEVELS.len();
                }
                KeyCode::Char(' ') => {
                    self.log_level = self.level_highlight;
                    self.level_dropdown_open = false;
                }
                KeyCode::Esc => {
                    self.level_dropdown_open = false;
                }
                _ => {}
            }
            return ViewAction::None;
        }

        match key_event.code {
            KeyCode::Esc => ViewAction::SwitchTo(crate::tui::View::Main(
                crate::views::main::MainView,
            )),
            KeyCode::Enter => {
                if let Err(e) = self.save(app) {
                    let close = crate::tui::Action {
                        name: "Close".to_string(),
                        description: "ESC: close".to_string(),
                        key_code: KeyCode::Esc,
                        callback: std::sync::Arc::new(|handler| handler.close_message()),
                    };
                    app.popup = Some(crate::tui::View::Message(crate::tui::Message::new(
                        crate::event::Severity::Error,
                        "Save Error".to_string(),
                        e.to_string(),
                        vec![close],
                    )));
                    return ViewAction::None;
                }
                ViewAction::SwitchTo(crate::tui::View::Main(crate::views::main::MainView))
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
                self.level_highlight = self.log_level;
                self.level_dropdown_open = true;
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
            0 => &mut self.log_path,
            2 => &mut self.bin,
            3 => &mut self.config,
            4 => &mut self.workdir,
            _ => unreachable!("field 1 (Log level) has no text input; callers must guard for it"),
        }
    }

    /// `None` when the field is blank.
    fn opt_path(value: &str) -> Option<PathBuf> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(PathBuf::from(trimmed))
        }
    }

    fn save(&self, app: &mut App) -> color_eyre::Result<()> {
        if self.bin.value.trim().is_empty() {
            return Err(color_eyre::eyre::eyre!("Rclone binary must not be empty"));
        }

        {
            let mut general = app.general.try_write().unwrap();
            general.log_path = Self::opt_path(&self.log_path.value);
            general.log_level = LOG_LEVELS[self.log_level].to_string();
        }
        {
            let mut rclone = app.rclone.try_write().unwrap();
            rclone.bin = self.bin.value.trim().to_string();
            rclone.config = Self::opt_path(&self.config.value);
            rclone.workdir = Self::opt_path(&self.workdir.value);
        }

        app.save_config()?;
        Ok(())
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, _app: &App) {
        let block = Block::default().borders(Borders::ALL).title("Settings  ");
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
                Constraint::Min(0),
            ])
            .split(inner);
        self.render_input("Log path (empty = none)", &self.log_path, 0, chunks[0], buf);
        self.render_level_field(chunks[1], buf);
        self.render_input("Rclone binary", &self.bin, 2, chunks[2], buf);
        self.render_input(
            "Rclone config file (empty = default)",
            &self.config,
            3,
            chunks[3],
            buf,
        );
        self.render_input(
            "Rclone workdir (empty = default)",
            &self.workdir,
            4,
            chunks[4],
            buf,
        );

        if self.level_dropdown_open {
            self.render_level_dropdown(chunks[1], buf);
        }
    }

    fn render_level_field(&self, area: Rect, buf: &mut Buffer) {
        let active = self.active_field == 1;
        let value = LOG_LEVELS[self.log_level].to_string();
        let style = if active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        let line = Line::from(vec![
            Span::raw(value),
            Span::styled(
                if self.level_dropdown_open { " ▲" } else { " ▼" },
                Style::default().add_modifier(Modifier::DIM),
            ),
        ]);
        Paragraph::new(line)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Log level (SPACE to choose)")
                    .style(style),
            )
            .render(area, buf);
    }

    fn render_level_dropdown(&self, field_area: Rect, buf: &mut Buffer) {
        let items: Vec<ListItem> = LOG_LEVELS.iter().map(|l| ListItem::new(*l)).collect();
        let height = items.len() as u16 + 2; // + borders
        let area = Rect {
            x: field_area.x,
            y: field_area.y + field_area.height,
            width: field_area.width,
            height,
        };
        Clear.render(area, buf);
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Select Level"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("> ");
        let mut state = ListState::default();
        state.select(Some(self.level_highlight));
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
