use crate::app::App;
use crate::views::ViewAction;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Clear, Row, Table, Widget},
};
use uuid::Uuid;

pub struct ProgressView {
    pub key: Uuid,
    pub scroll_offset: usize,
    pub active_filter: Option<String>,
    pub available_filters: Vec<String>,
    pub search_query: String,
    pub search_mode: bool,
}

impl ProgressView {
    pub fn help_text(&self) -> String {
        if self.search_mode {
            format!(
                "Search: {}_ | Enter: confirm | Esc: cancel",
                self.search_query
            )
        } else {
            let filter_str = match &self.active_filter {
                Some(f) => format!("Filter: {}", f),
                None => "Filter: All".to_string(),
            };
            let search_str = if !self.search_query.is_empty() {
                format!(" | Search: {}", self.search_query)
            } else {
                "".to_string()
            };
            format!(
                "{}{} | ↑/↓: scroll | f: cycle filter | /: search | Esc/q: close",
                filter_str, search_str
            )
        }
    }

    pub fn new(key: Uuid) -> Self {
        Self {
            key,
            scroll_offset: 0,
            active_filter: None,
            available_filters: vec![
                "Transferring".to_string(),
                "Checking".to_string(),
                "Transferred".to_string(),
                "Checked".to_string(),
                "Deleted".to_string(),
                "Moved".to_string(),
                "Error".to_string(),
                "Other".to_string(),
            ],
            search_query: String::new(),
            search_mode: false,
        }
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent, _app: &mut App) -> ViewAction {
        if self.search_mode {
            match key_event.code {
                KeyCode::Esc => {
                    self.search_mode = false;
                    self.search_query = String::new();
                    ViewAction::None
                }
                KeyCode::Enter => {
                    self.search_mode = false;
                    self.scroll_offset = 0;
                    ViewAction::None
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.scroll_offset = 0;
                    ViewAction::None
                }
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.scroll_offset = 0;
                    ViewAction::None
                }
                _ => ViewAction::None,
            }
        } else {
            match key_event.code {
                KeyCode::Esc | KeyCode::Char('q') => ViewAction::ClosePopup,
                KeyCode::Char('/') => {
                    self.search_mode = true;
                    ViewAction::None
                }
                KeyCode::Up => {
                    self.scroll_offset = self.scroll_offset.saturating_sub(1);
                    ViewAction::None
                }
                KeyCode::Down => {
                    self.scroll_offset = self.scroll_offset.saturating_add(1);
                    ViewAction::None
                }
                KeyCode::Char('f') => {
                    if let Some(current) = &self.active_filter {
                        let idx = self
                            .available_filters
                            .iter()
                            .position(|f| f == current)
                            .unwrap_or(0);
                        if idx + 1 >= self.available_filters.len() {
                            self.active_filter = None;
                        } else {
                            self.active_filter = Some(self.available_filters[idx + 1].clone());
                        }
                    } else {
                        self.active_filter = Some(self.available_filters[0].clone());
                    }
                    self.scroll_offset = 0;
                    ViewAction::None
                }
                _ => ViewAction::None,
            }
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, app: &App) {
        let popup_area = centered_rect(90, 80, area);
        Clear.render(popup_area, buf);

        let fp = app.file_progress.try_read().unwrap();
        let (active, completed, _) = fp
            .get(&self.key)
            .cloned()
            .unwrap_or_else(|| (Vec::new(), Vec::new(), std::collections::HashSet::new()));

        let header_row = Row::new(vec![
            Cell::from("File"),
            Cell::from("Direction / Status"),
            Cell::from("Progress"),
            Cell::from("Speed"),
            Cell::from("ETA"),
        ])
        .style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        );

        let mut all_files = Vec::new();
        all_files.extend(active);
        all_files.extend(completed);

        let filtered_files: Vec<_> = all_files
            .into_iter()
            .filter(|f| {
                let matches_status = match &self.active_filter {
                    Some(filter) => &f.status == filter,
                    None => true,
                };

                let matches_search = if self.search_query.is_empty() {
                    true
                } else {
                    // If user didn't type wildcards, implicitly wrap in *query* for substring search
                    let query =
                        if self.search_query.contains('*') || self.search_query.contains('?') {
                            self.search_query.clone()
                        } else {
                            format!("*{}*", self.search_query)
                        };
                    wildcard_match(&query, &f.name)
                };

                matches_status && matches_search
            })
            .collect();

        let mut data_rows = Vec::new();
        for t in filtered_files {
            let direction = if !t.src.is_empty() && !t.dst.is_empty() {
                format!("{} -> {}", t.src, t.dst)
            } else {
                t.status.clone()
            };

            let progress =
                if t.status == "Checking" || t.status == "Checked" || t.status == "Deleted" {
                    "-".to_string()
                } else {
                    format!("{}%", t.percentage)
                };

            let speed = if t.speed > 0.0 {
                format!("{}/s", human_readable_bytes(t.speed as i64))
            } else {
                "-".to_string()
            };

            let eta = format_eta(t.eta);

            let status_style = match t.status.as_str() {
                "Transferring" | "Transferred" | "Moved" => Style::default().fg(Color::Green),
                "Checking" | "Checked" => Style::default().fg(Color::Blue),
                "Deleted" => Style::default().fg(Color::Red),
                "Error" => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                _ => Style::default().fg(Color::Yellow),
            };

            data_rows.push(
                Row::new(vec![
                    Cell::from(t.name.clone()),
                    Cell::from(direction),
                    Cell::from(progress),
                    Cell::from(speed),
                    Cell::from(eta),
                ])
                .style(status_style),
            );
        }

        let inner_height = popup_area.height.saturating_sub(2) as usize;
        let visible_data_rows_count = inner_height.saturating_sub(1);

        let total_data_rows = data_rows.len();
        let max_offset = total_data_rows.saturating_sub(visible_data_rows_count);
        let offset = self.scroll_offset.min(max_offset);

        let visible_data_rows: Vec<_> = data_rows
            .into_iter()
            .skip(offset)
            .take(visible_data_rows_count)
            .collect();

        let mut all_rows = vec![header_row];
        all_rows.extend(visible_data_rows);

        let table = Table::new(
            all_rows,
            &[
                Constraint::Min(30),
                Constraint::Min(25),
                Constraint::Length(10),
                Constraint::Length(15),
                Constraint::Length(10),
            ],
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("File Progress")
                .style(Style::default().bg(Color::Black)),
        );

        Widget::render(table, popup_area, buf);
    }
}

/// Standard glob-like wildcard matcher supporting '*' and '?'
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let text = text.to_lowercase();

    let mut p_idx = 0;
    let mut t_idx = 0;
    let mut star_p_idx = None;
    let mut star_t_idx = None;

    let p_bytes = pattern.as_bytes();
    let t_bytes = text.as_bytes();

    while t_idx < t_bytes.len() {
        if p_idx < p_bytes.len() && (p_bytes[p_idx] == t_bytes[t_idx] || p_bytes[p_idx] == b'?') {
            p_idx += 1;
            t_idx += 1;
        } else if p_idx < p_bytes.len() && p_bytes[p_idx] == b'*' {
            star_p_idx = Some(p_idx);
            star_t_idx = Some(t_idx);
            p_idx += 1;
        } else if let (Some(sp), Some(st)) = (star_p_idx, star_t_idx) {
            p_idx = sp + 1;
            t_idx = st + 1;
            star_t_idx = Some(t_idx);
        } else {
            return false;
        }
    }

    while p_idx < p_bytes.len() && p_bytes[p_idx] == b'*' {
        p_idx += 1;
    }

    p_idx == p_bytes.len()
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn human_readable_bytes(bytes: i64) -> String {
    if bytes < 1024 {
        return format!("{} B", bytes);
    }
    if bytes < 1024 * 1024 {
        return format!("{:.1} KB", bytes as f64 / 1024.0);
    }
    if bytes < 1024 * 1024 * 1024 {
        return format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0));
    }
    format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

fn format_eta(eta: f64) -> String {
    if eta <= 0.0 || eta.is_infinite() || eta.is_nan() {
        return "-".to_string();
    }
    let secs = eta as u64;
    if secs < 60 {
        return format!("{}s", secs);
    }
    if secs < 3600 {
        return format!("{}m {}s", secs / 60, secs % 60);
    }
    format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
}
