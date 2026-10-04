use crate::app::App;
use crate::views::ViewAction;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Padding, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, StatefulWidget, Widget,
    },
};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Width of the "12:20:41 ERROR  " prefix in front of every rclone log line.
/// Continuation lines are indented by the same amount so the messages line up.
const LOG_PREFIX_WIDTH: usize = 16;

/// Scrollable popup showing the error messages collected for a sync pair
/// (ALT+e). rclone log lines are split into time / level / message so the
/// actual message is readable instead of drowning in timestamps.
pub struct ErrorView {
    idx: usize,
    name: String,
    messages: Vec<String>,
    scroll: usize,
    /// Written by `render`, which is the only place that knows the wrapped
    /// line count; read by `handle_key_event` to clamp scrolling.
    max_scroll: AtomicUsize,
    page_size: AtomicUsize,
}

impl ErrorView {
    pub fn new(idx: usize, name: String, messages: Vec<String>) -> Self {
        Self {
            idx,
            name,
            messages,
            scroll: 0,
            max_scroll: AtomicUsize::new(0),
            page_size: AtomicUsize::new(1),
        }
    }

    pub fn help_text(&self) -> String {
        "↑/↓: scroll | PgUp/PgDn: page | Home/End: top/bottom | o: options | Esc/q: close"
            .to_string()
    }

    /// rclone tells you to run with `--resync` after a critical bisync
    /// failure; rcmate exposes that as a per-pair option.
    fn needs_resync(&self) -> bool {
        self.messages.iter().any(|m| m.contains("--resync"))
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent, app: &mut App) -> ViewAction {
        let max = self.max_scroll.load(Ordering::Relaxed);
        let page = self.page_size.load(Ordering::Relaxed).max(1);
        match key_event.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => return ViewAction::ClosePopup,
            KeyCode::Char('o') => {
                return match crate::views::options::OptionsView::new(self.idx, app) {
                    Some(view) => ViewAction::OpenPopup(crate::tui::View::Options(view)),
                    None => ViewAction::None,
                };
            }
            KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll = (self.scroll + 1).min(max),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(page),
            KeyCode::PageDown => self.scroll = (self.scroll + page).min(max),
            KeyCode::Home | KeyCode::Char('g') => self.scroll = 0,
            KeyCode::End | KeyCode::Char('G') => self.scroll = max,
            _ => {}
        }
        ViewAction::None
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, _app: &App) {
        let border_style = Style::default().fg(Color::Red);
        let muted = Style::default().fg(Color::DarkGray);

        // Borders (2) + padding (2) + gap and scrollbar column (2).
        let width = area.width.saturating_sub(4).min(110);
        let text_width = (width as usize).saturating_sub(6).max(10);
        let lines = self.build_lines(text_width);

        let hint = self.needs_resync().then(|| {
            wrap(
                "Hint: enable \"Resync\" in this pair's options (press o) and run it again.",
                (width as usize).saturating_sub(4).max(10),
            )
        });
        let hint_height = hint.as_ref().map_or(0, |h| h.len() + 1);

        // Borders (2) + blank line + footer line, plus the optional hint.
        let chrome = 4 + hint_height;
        let max_height = area.height.saturating_sub(6).max(chrome as u16 + 1);
        let height = ((lines.len() + chrome) as u16).min(max_height);
        let popup_area = Rect {
            x: area.x + area.width.saturating_sub(width) / 2,
            y: area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        };
        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .padding(Padding::horizontal(1))
            .title(Line::from(vec![
                Span::styled(
                    " ✗ Error ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("· {} ", self.name), Style::default().fg(Color::White)),
            ]))
            .style(Style::default().bg(Color::Black));
        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        let [body_area, hint_area, _, footer_area] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(hint_height as u16),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(inner);
        let [text_area, _, scrollbar_area] = Layout::horizontal([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(body_area);

        let total = lines.len();
        let visible = text_area.height as usize;
        let max_scroll = total.saturating_sub(visible);
        self.max_scroll.store(max_scroll, Ordering::Relaxed);
        self.page_size.store(visible, Ordering::Relaxed);
        let offset = self.scroll.min(max_scroll);

        Paragraph::new(lines.into_iter().skip(offset).take(visible).collect::<Vec<_>>())
            .render(text_area, buf);

        if max_scroll > 0 {
            let mut state = ScrollbarState::new(max_scroll + 1)
                .viewport_content_length(visible)
                .position(offset);
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_style(muted)
                .thumb_style(border_style)
                .render(scrollbar_area, buf, &mut state);
        }

        if let Some(hint) = hint {
            let hint_lines: Vec<Line> = hint
                .into_iter()
                .map(|l| Line::styled(l, Style::default().fg(Color::Yellow)))
                .collect();
            Paragraph::new(hint_lines).render(
                Rect {
                    y: hint_area.y + 1,
                    height: hint_area.height.saturating_sub(1),
                    ..hint_area
                },
                buf,
            );
        }

        if max_scroll > 0 {
            Paragraph::new(Span::styled(
                format!(
                    "lines {}-{} of {}",
                    offset + 1,
                    (offset + visible).min(total),
                    total
                ),
                muted,
            ))
            .render(footer_area, buf);
        }
        Paragraph::new(Line::from(vec![
            Span::styled("Esc ", muted),
            Span::styled("Close", border_style.add_modifier(Modifier::BOLD)),
        ]))
        .alignment(Alignment::Right)
        .render(footer_area, buf);
    }

    /// Turns the raw messages into styled, pre-wrapped lines. Pre-wrapping
    /// (instead of `Paragraph::wrap`) gives the exact line count needed for
    /// scrolling and lets continuation lines hang under the message column.
    fn build_lines(&self, width: usize) -> Vec<Line<'static>> {
        let heading = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);
        let muted = Style::default().fg(Color::DarkGray);
        let text = Style::default().fg(Color::White);
        let indent_width = if width >= LOG_PREFIX_WIDTH + 20 {
            LOG_PREFIX_WIDTH
        } else {
            0
        };
        let indent = " ".repeat(indent_width);
        let msg_width = width - indent_width;

        let mut out = Vec::new();
        for (i, message) in self.messages.iter().enumerate() {
            if i > 0 {
                out.push(Line::from(Span::styled("─".repeat(width), muted)));
            }
            let mut seen_log_line = false;
            for raw in message.lines() {
                let raw = raw.trim_end();
                if let Some((time, level, msg)) = parse_log_line(raw) {
                    seen_log_line = true;
                    // bisync prints table rows like `- Path1   Queue copy   - file`;
                    // a no-break space keeps each `-` with what follows it.
                    let msg = msg.replace("- ", "-\u{a0}");
                    for (j, part) in wrap(&msg, msg_width).into_iter().enumerate() {
                        let prefix = if j == 0 {
                            vec![
                                Span::styled(format!("{} ", time), muted),
                                Span::styled(format!("{:<7}", level), level_style(level)),
                            ]
                        } else {
                            vec![Span::raw(indent.clone())]
                        };
                        let mut spans = prefix;
                        if indent_width == 0 && j == 0 {
                            out.push(Line::from(spans));
                            spans = Vec::new();
                        }
                        spans.push(Span::styled(part, text));
                        out.push(Line::from(spans));
                    }
                } else if raw.is_empty() {
                    out.push(Line::from(""));
                } else if !seen_log_line {
                    for part in wrap(raw, width) {
                        out.push(Line::from(Span::styled(part, heading)));
                    }
                } else {
                    // Output rclone printed after a log line (tips, paths,
                    // commands) belongs to it, so it's indented under it.
                    for part in wrap(raw, msg_width) {
                        out.push(Line::from(vec![
                            Span::raw(indent.clone()),
                            Span::styled(part, text),
                        ]));
                    }
                }
            }
        }
        out
    }
}

/// Splits an rclone log line into (`12:20:41`, `ERROR`, `msg`). rclone writes
/// the level as `%-6s: `, so it's `ERROR : msg` but `NOTICE: msg`, and the
/// time may carry microseconds (`12:20:41.123456`) with `--log-format microseconds`.
fn parse_log_line(line: &str) -> Option<(&str, &str, &str)> {
    let b = line.as_bytes();
    let is_stamp = b.len() > 20
        && b[4] == b'/'
        && b[7] == b'/'
        && b[10] == b' '
        && b[13] == b':'
        && b[16] == b':'
        && b[..19]
            .iter()
            .enumerate()
            .all(|(i, c)| matches!(i, 4 | 7 | 10 | 13 | 16) || c.is_ascii_digit());
    if !is_stamp {
        return None;
    }
    let mut time_end = 19;
    if b[time_end] == b'.' {
        time_end += 1;
        while time_end < b.len() && b[time_end].is_ascii_digit() {
            time_end += 1;
        }
    }
    if b.get(time_end) != Some(&b' ') {
        return None;
    }
    let (level, msg) = line[time_end + 1..].split_once(':')?;
    let level = level.trim_end();
    if level.is_empty() || !level.bytes().all(|c| c.is_ascii_uppercase()) {
        return None;
    }
    Some((&line[11..19], level, msg.strip_prefix(' ').unwrap_or(msg)))
}

fn level_style(level: &str) -> Style {
    match level {
        "ERROR" | "CRITICAL" => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        "NOTICE" => Style::default().fg(Color::Yellow),
        "INFO" => Style::default().fg(Color::Cyan),
        _ => Style::default().fg(Color::DarkGray),
    }
}

/// Wraps on spaces only, so Windows paths aren't split at `\` or `:` -
/// they're only broken when longer than a whole line.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let options = textwrap::Options::new(width.max(1))
        .word_separator(textwrap::WordSeparator::AsciiSpace)
        .break_words(true);
    textwrap::wrap(text, options)
        .into_iter()
        .map(|l| l.into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rclone_log_line() {
        assert_eq!(
            parse_log_line("2026/10/04 12:20:41 ERROR : Bisync aborted. Must run --resync."),
            Some(("12:20:41", "ERROR", "Bisync aborted. Must run --resync."))
        );
        assert_eq!(
            parse_log_line("2026/10/04 12:34:27 NOTICE: - Path1    Queue copy to Path2"),
            Some(("12:34:27", "NOTICE", "- Path1    Queue copy to Path2"))
        );
        assert_eq!(
            parse_log_line("2026/10/04 12:34:27.123456 INFO  : file.txt: Copied"),
            Some(("12:34:27", "INFO", "file.txt: Copied"))
        );
    }

    #[test]
    fn ignores_non_log_lines() {
        assert_eq!(parse_log_line("bisync aborted"), None);
        assert_eq!(parse_log_line("Path1: C:\\Users\\x.lst"), None);
        assert_eq!(parse_log_line("2026/10/04 12:20:41 no level here"), None);
    }

    #[test]
    fn wrap_keeps_paths_intact() {
        let lines = wrap("Path1: C:\\Users\\2107\\file.lst", 40);
        assert_eq!(lines, vec!["Path1: C:\\Users\\2107\\file.lst"]);
    }
}
