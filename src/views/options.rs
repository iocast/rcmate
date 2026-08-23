use crate::app::App;
use crate::config::{SyncOptions, SyncType};
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
use tracing::error;
use tui_popup::Popup;

/// Selectable values for bisync's `resyncMode`, in display order. `none`
/// means the parameter isn't sent at all.
const RESYNC_MODES: [&str; 7] = [
    "none", "path1", "path2", "newer", "older", "larger", "smaller",
];

/// One editable row in the Options popup.
enum Field {
    /// A boolean rclone flag, toggled with SPACE.
    Toggle {
        label: &'static str,
        /// Accessor for the flag inside [`SyncOptions`]. A function pointer
        /// keeps the field list declarative instead of a positional match.
        get: fn(&mut SyncOptions) -> &mut bool,
    },
    /// Bisync's `resyncMode`, cycled through [`RESYNC_MODES`].
    ResyncMode,
}

impl Field {
    fn label(&self) -> &'static str {
        match self {
            Field::Toggle { label, .. } => label,
            Field::ResyncMode => "Resync mode",
        }
    }
}

/// The options that apply to a pair, per the rc API docs for `sync/bisync`,
/// `sync/sync`, `sync/copy` and `sync/move`. Options that don't apply are
/// hidden rather than shown and then ignored, so what the popup offers is
/// exactly what the run will use.
///
/// `file_mode` pairs (see `SyncPairConfig::is_file_mode`) run against the
/// parent directories with an include filter for a single file name, and for
/// `sync` as two opposite `copy --update` jobs, so nothing about creating
/// source directories applies to them.
fn fields_for(sync_type: &SyncType, file_mode: bool) -> Vec<Field> {
    let dry_run = Field::Toggle {
        label: "Dry run",
        get: |o| &mut o.dry_run,
    };
    let create_empty = (!file_mode).then_some(Field::Toggle {
        label: "Create empty source dirs",
        get: |o| &mut o.create_empty_src_dirs,
    });
    match sync_type {
        SyncType::BiSync => vec![
            Some(dry_run),
            create_empty,
            Some(Field::Toggle {
                label: "Resync",
                get: |o| &mut o.resync,
            }),
            Some(Field::ResyncMode),
            Some(Field::Toggle {
                label: "Force",
                get: |o| &mut o.force,
            }),
            Some(Field::Toggle {
                label: "Check access",
                get: |o| &mut o.check_access,
            }),
        ],
        SyncType::Move => vec![
            Some(dry_run),
            create_empty,
            Some(Field::Toggle {
                label: "Delete empty source dirs",
                get: |o| &mut o.delete_empty_src_dirs,
            }),
        ],
        SyncType::Sync | SyncType::Copy => vec![Some(dry_run), create_empty],
    }
    .into_iter()
    .flatten()
    .collect()
}

/// Edits the options of a single sync pair - the one under the cursor.
/// Options are per entry and depend on its type, so there is no meaningful
/// way to edit several pairs of possibly different types at once.
pub struct OptionsView {
    /// Index into `app.sync_pairs`.
    index: usize,
    name: String,
    sync_type: SyncType,
    fields: Vec<Field>,
    active_field: usize,
}

impl OptionsView {
    pub fn help_text(&self) -> String {
        "↑/↓: navigate | Space: toggle | Left/Right: change | Esc/Enter: save & close ".to_string()
    }

    /// `None` when `index` doesn't point at a sync pair, e.g. an empty config.
    pub fn new(index: usize, app: &App) -> Option<Self> {
        let sync_pairs = app.sync_pairs.try_read().unwrap();
        let pair = sync_pairs.get(index)?.try_read().unwrap();
        let sync_type = pair.sync_pair.sync_type.clone();
        // Resolved once, when the popup opens, rather than per frame - it
        // stats the paths.
        let file_mode = pair.sync_pair.is_file_mode();
        Some(Self {
            index,
            name: pair.sync_pair.name.clone(),
            fields: fields_for(&sync_type, file_mode),
            sync_type,
            active_field: 0,
        })
    }

    /// Runs `f` against the edited pair's options, or does nothing if the
    /// pair has since disappeared.
    fn with_options<R>(&self, app: &App, f: impl FnOnce(&mut SyncOptions) -> R) -> Option<R> {
        let sync_pairs = app.sync_pairs.try_read().unwrap();
        let mut pair = sync_pairs.get(self.index)?.try_write().unwrap();
        Some(f(&mut pair.sync_pair.options))
    }

    fn options(&self, app: &App) -> SyncOptions {
        self.with_options(app, |o| o.clone()).unwrap_or_default()
    }

    /// Steps `resync_mode` forwards or backwards through [`RESYNC_MODES`].
    fn cycle_resync_mode(&self, app: &App, forward: bool) {
        self.with_options(app, |o| {
            let idx = RESYNC_MODES
                .iter()
                .position(|m| *m == o.resync_mode)
                .unwrap_or(0);
            let next = if forward {
                (idx + 1) % RESYNC_MODES.len()
            } else {
                (idx + RESYNC_MODES.len() - 1) % RESYNC_MODES.len()
            };
            o.resync_mode = RESYNC_MODES[next].to_string();
        });
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent, app: &mut App) -> ViewAction {
        let fields = self.fields.len().max(1);
        match key_event.code {
            // Options live in the config file, so closing the popup persists
            // them the same way the sync pair form does on save.
            KeyCode::Esc | KeyCode::Enter => {
                if let Err(e) = app.save_config() {
                    error!("Failed to save options: {}", e);
                }
                ViewAction::ClosePopup
            }
            KeyCode::Tab | KeyCode::Down => {
                self.active_field = (self.active_field + 1) % fields;
                ViewAction::None
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.active_field = (self.active_field + fields - 1) % fields;
                ViewAction::None
            }
            KeyCode::Char(' ') => {
                match self.fields.get(self.active_field) {
                    Some(Field::Toggle { get, .. }) => {
                        let get = *get;
                        self.with_options(app, |o| {
                            let flag = get(o);
                            *flag = !*flag;
                        });
                    }
                    Some(Field::ResyncMode) => self.cycle_resync_mode(app, true),
                    None => {}
                }
                ViewAction::None
            }
            KeyCode::Left | KeyCode::Right => {
                let forward = key_event.code == KeyCode::Right;
                match self.fields.get(self.active_field) {
                    Some(Field::Toggle { get, .. }) => {
                        let get = *get;
                        self.with_options(app, |o| *get(o) = forward);
                    }
                    Some(Field::ResyncMode) => self.cycle_resync_mode(app, forward),
                    None => {}
                }
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, app: &App) {
        let mut opts = self.options(app);

        let mut lines: Vec<Line> = vec![
            Line::from(vec![
                Span::styled(
                    format!("{}: ", self.name),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(self.sync_type.to_string()),
            ]),
            Line::from(" "),
        ];

        for (i, field) in self.fields.iter().enumerate() {
            let style = if i == self.active_field {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let value = match field {
                Field::Toggle { get, .. } => {
                    if *get(&mut opts) {
                        "[X]".to_string()
                    } else {
                        "[ ]".to_string()
                    }
                }
                Field::ResyncMode => format!("< {} >", opts.resync_mode),
            };
            lines.push(Line::from(vec![
                Span::styled(value, style),
                Span::raw(format!("  {}", field.label())),
            ]));
        }

        let popup = Popup::new(Text::from(lines))
            .title("Options")
            .style(Style::new().fg(Color::White).bg(Color::Blue));
        Widget::render(popup, area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(sync_type: &SyncType, file_mode: bool) -> Vec<&'static str> {
        fields_for(sync_type, file_mode)
            .iter()
            .map(|f| f.label())
            .collect()
    }

    /// A single-file pair runs against the parent directories with an include
    /// filter, so there is nothing for "create empty source dirs" to act on
    /// and the popup must not offer it.
    #[test]
    fn file_mode_hides_create_empty_src_dirs() {
        for sync_type in [
            SyncType::Sync,
            SyncType::BiSync,
            SyncType::Copy,
            SyncType::Move,
        ] {
            assert!(
                labels(&sync_type, false).contains(&"Create empty source dirs"),
                "{sync_type} should offer it for a directory pair"
            );
            assert!(
                !labels(&sync_type, true).contains(&"Create empty source dirs"),
                "{sync_type} should hide it for a file pair"
            );
        }
    }

    /// Hiding one option must not disturb the rest of the type's list.
    #[test]
    fn file_mode_keeps_the_other_options() {
        assert_eq!(labels(&SyncType::Sync, true), vec!["Dry run"]);
        assert_eq!(
            labels(&SyncType::Move, true),
            vec!["Dry run", "Delete empty source dirs"]
        );
        assert_eq!(
            labels(&SyncType::BiSync, true),
            vec!["Dry run", "Resync", "Resync mode", "Force", "Check access"]
        );
    }
}
