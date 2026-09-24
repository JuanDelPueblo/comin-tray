use std::collections::VecDeque;

use iced::{
    Alignment, Element, Fill, Task,
    widget::{checkbox, column, horizontal_space, pick_list, row, text, text_input},
};

use crate::{
    build_log::{LineKind, classify, display_message},
    gui::{
        log_list::{LogList, LogListMessage, Row, save_log},
        theme::BREEZE_TEXT_MUTED,
        widgets::small_button,
    },
    logs::{LogEntry, Priority, to_copy_text},
};

/// Lines kept in memory. Only the visible rows are rendered, so the buffer
/// can be large; deployment logs are read from the journal on demand.
pub const MAX_ENTRIES: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelFilter {
    All,
    Info,
    Warning,
    Error,
}

impl LevelFilter {
    pub const ALL: [LevelFilter; 4] = [
        LevelFilter::All,
        LevelFilter::Info,
        LevelFilter::Warning,
        LevelFilter::Error,
    ];

    fn allows(self, priority: Priority) -> bool {
        match self {
            LevelFilter::All => true,
            LevelFilter::Info => priority <= Priority::Info,
            LevelFilter::Warning => priority <= Priority::Warning,
            LevelFilter::Error => priority <= Priority::Error,
        }
    }
}

impl std::fmt::Display for LevelFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            LevelFilter::All => "All levels",
            LevelFilter::Info => "Info and above",
            LevelFilter::Warning => "Warnings and errors",
            LevelFilter::Error => "Errors only",
        })
    }
}

struct Line {
    id: u64,
    entry: LogEntry,
    kind: LineKind,
}

pub struct LogView {
    lines: VecDeque<Line>,
    next_id: u64,
    pub list: LogList,
    pub search: String,
    pub level: LevelFilter,
    pub hide_noise: bool,
    /// Feedback for the last copy or save.
    pub notice: Option<String>,
    /// The number of lines that pass the filters, kept up to date so that
    /// following the log does not rescan the whole buffer per line.
    shown: usize,
}

#[derive(Debug, Clone)]
pub enum LogMessage {
    LogReceived(LogEntry),
    ClearLogs,
    ToggleAutoScroll(bool),
    List(LogListMessage),
    Search(String),
    Level(LevelFilter),
    HideNoise(bool),
    RangeMode(bool),
    Colors(bool),
    CopySelected,
    CopyAll,
    SelectAll,
    Save,
    Saved(Result<String, String>),
    /// Ask the application to show the deployment or generation `uuid`.
    OpenDeployment(String),
}

impl Default for LogView {
    fn default() -> Self {
        Self {
            lines: VecDeque::new(),
            next_id: 0,
            list: LogList::default(),
            search: String::new(),
            level: LevelFilter::All,
            hide_noise: true,
            notice: None,
            shown: 0,
        }
    }
}

impl LogView {
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn update(&mut self, message: LogMessage) -> Task<LogMessage> {
        match message {
            LogMessage::LogReceived(entry) => {
                let line = Line {
                    id: self.next_id,
                    kind: classify(&entry),
                    entry,
                };
                self.next_id += 1;
                if self.passes(&line) {
                    self.shown += 1;
                }
                self.lines.push_back(line);
                if self.lines.len() > MAX_ENTRIES
                    && let Some(dropped) = self.lines.pop_front()
                    && self.passes(&dropped)
                {
                    self.shown -= 1;
                }
                self.list.follow(self.shown).map(LogMessage::List)
            }
            LogMessage::ClearLogs => {
                self.lines.clear();
                self.shown = 0;
                self.list.clear();
                Task::none()
            }
            LogMessage::ToggleAutoScroll(enabled) => {
                self.list.auto_scroll = enabled;
                self.list.follow(self.shown).map(LogMessage::List)
            }
            LogMessage::List(message) => {
                self.list.update(message);
                Task::none()
            }
            LogMessage::Search(search) => {
                self.search = search;
                self.refilter()
            }
            LogMessage::Level(level) => {
                self.level = level;
                self.refilter()
            }
            LogMessage::HideNoise(hide) => {
                self.hide_noise = hide;
                self.refilter()
            }
            LogMessage::Colors(enabled) => {
                self.list.colors = enabled;
                Task::none()
            }
            LogMessage::RangeMode(enabled) => {
                self.list.range_mode = enabled;
                Task::none()
            }
            LogMessage::SelectAll => {
                let ids: Vec<u64> = self.filtered().map(|line| line.id).collect();
                self.list.select_all(&ids);
                Task::none()
            }
            LogMessage::CopySelected => {
                let text = self.selected_text();
                if text.is_empty() {
                    return Task::none();
                }
                self.notice = Some(format!("Copied {} lines", text.lines().count()));
                iced::clipboard::write(text)
            }
            LogMessage::CopyAll => {
                let text = to_copy_text(self.filtered().map(|line| &line.entry));
                self.notice = Some(format!("Copied {} lines", text.lines().count()));
                iced::clipboard::write(text)
            }
            LogMessage::Save => {
                let text = to_copy_text(self.filtered().map(|line| &line.entry));
                Task::perform(save_log("comin".into(), text), LogMessage::Saved)
            }
            LogMessage::Saved(result) => {
                self.notice = Some(match result {
                    Ok(path) => format!("Saved to {path}"),
                    Err(error) => error,
                });
                Task::none()
            }
            // Handled by the application.
            LogMessage::OpenDeployment(_) => Task::none(),
        }
    }

    /// The selected lines, formatted for the clipboard.
    pub fn selected_text(&self) -> String {
        let lines: Vec<&Line> = self.filtered().collect();
        let ids: Vec<u64> = lines.iter().map(|line| line.id).collect();
        let range = self.list.selected_positions(&ids);
        to_copy_text(lines[range].iter().map(|line| &line.entry))
    }

    fn refilter(&mut self) -> Task<LogMessage> {
        self.shown = self.filtered().count();
        self.list.follow(self.shown).map(LogMessage::List)
    }

    fn passes(&self, line: &Line) -> bool {
        (!self.hide_noise || !line.kind.is_noise())
            && self.level.allows(line.entry.priority)
            && (self.search.is_empty()
                || line
                    .entry
                    .message
                    .to_lowercase()
                    .contains(&self.search.to_lowercase()))
    }

    fn filtered(&self) -> impl Iterator<Item = &Line> {
        self.lines.iter().filter(move |line| self.passes(line))
    }

    /// The generation or deployment uuid named by the single selected line.
    fn selected_uuid(&self) -> Option<String> {
        let lines: Vec<&Line> = self.filtered().collect();
        let ids: Vec<u64> = lines.iter().map(|line| line.id).collect();
        let range = self.list.selected_positions(&ids);
        if range.len() != 1 {
            return None;
        }
        find_uuid(&lines[range.start].entry.message).map(str::to_string)
    }

    pub fn view(&self) -> Element<'_, LogMessage> {
        let rows: Vec<Row<'_>> = self
            .filtered()
            .map(|line| Row {
                id: line.id,
                entry: &line.entry,
                text: display_message(&line.entry),
                kind: &line.kind,
            })
            .collect();
        let shown = rows.len();
        let has_selection = self.list.has_selection();

        let filters = row![
            text_input("Search logs…", &self.search)
                .on_input(LogMessage::Search)
                .size(13)
                .padding([5, 10])
                .width(260),
            pick_list(LevelFilter::ALL, Some(self.level), LogMessage::Level)
                .text_size(13)
                .padding([5, 10]),
            checkbox("Hide noise", self.hide_noise)
                .on_toggle(LogMessage::HideNoise)
                .size(16)
                .text_size(13),
            checkbox("Colors", self.list.colors)
                .on_toggle(LogMessage::Colors)
                .size(16)
                .text_size(13),
            checkbox("Follow", self.list.auto_scroll)
                .on_toggle(LogMessage::ToggleAutoScroll)
                .size(16)
                .text_size(13),
            horizontal_space(),
            text(format!("{shown} of {} lines", self.lines.len()))
                .size(13)
                .color(BREEZE_TEXT_MUTED),
        ]
        .spacing(14)
        .align_y(Alignment::Center);

        let mut actions = row![
            small_button(
                "Copy selected",
                has_selection.then_some(LogMessage::CopySelected)
            ),
            small_button("Copy all shown", (shown > 0).then_some(LogMessage::CopyAll)),
            small_button("Save to file", (shown > 0).then_some(LogMessage::Save)),
            small_button("Clear", Some(LogMessage::ClearLogs)),
            checkbox("Select range", self.list.range_mode)
                .on_toggle(LogMessage::RangeMode)
                .size(16)
                .text_size(13),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if let Some(uuid) = self.selected_uuid() {
            actions = actions.push(small_button(
                "Open deployment",
                Some(LogMessage::OpenDeployment(uuid)),
            ));
        }
        actions = actions.push(
            text(
                "Click a line to select it; Shift+click or Select range extends it. Ctrl+C copies.",
            )
            .size(12)
            .color(BREEZE_TEXT_MUTED),
        );
        if let Some(notice) = &self.notice {
            actions = actions.push(horizontal_space());
            actions = actions.push(text(notice.as_str()).size(12).color(BREEZE_TEXT_MUTED));
        }

        column![
            filters,
            actions,
            self.list.view(rows, Fill).map(LogMessage::List)
        ]
        .spacing(10)
        .padding([4, 16])
        .width(Fill)
        .height(Fill)
        .into()
    }
}

/// The first UUID (8-4-4-4-12 hex digits) in `message`.
pub fn find_uuid(message: &str) -> Option<&str> {
    let bytes = message.as_bytes();
    (0..bytes.len().saturating_sub(35)).find_map(|start| {
        let candidate = message.get(start..start + 36)?;
        let is_uuid = candidate.char_indices().all(|(index, c)| match index {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        });
        let boundary_before = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        (is_uuid && boundary_before).then_some(candidate)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(message: &str, priority: Priority) -> LogEntry {
        LogEntry {
            message: message.into(),
            ..LogEntry::note(priority, "")
        }
    }

    #[test]
    fn filters_combine_noise_level_and_search() {
        let mut view = LogView::default();
        for (message, priority) in [
            ("remote: Enumerating objects: 1", Priority::Info),
            ("building '/nix/store/abc-foo.drv'...", Priority::Info),
            ("error: builder failed", Priority::Error),
        ] {
            let _ = view.update(LogMessage::LogReceived(entry(message, priority)));
        }

        assert_eq!(view.shown, 2);
        let _ = view.update(LogMessage::HideNoise(false));
        assert_eq!(view.shown, 3);
        let _ = view.update(LogMessage::Level(LevelFilter::Error));
        assert_eq!(view.shown, 1);
        let _ = view.update(LogMessage::Level(LevelFilter::All));
        let _ = view.update(LogMessage::Search("FOO".into()));
        assert_eq!(view.shown, 1);
        assert_eq!(view.filtered().count(), view.shown);
    }

    #[test]
    fn copying_uses_the_selected_rows() {
        let mut view = LogView::default();
        for message in ["one", "two", "three"] {
            let _ = view.update(LogMessage::LogReceived(entry(message, Priority::Info)));
        }
        let _ = view.update(LogMessage::List(LogListMessage::RowPressed(1)));
        view.list.modifiers = iced::keyboard::Modifiers::SHIFT;
        let _ = view.update(LogMessage::List(LogListMessage::RowPressed(2)));

        assert_eq!(
            view.selected_text(),
            "INFO  [comin-tray] two\nINFO  [comin-tray] three\n"
        );
    }

    #[test]
    fn uuids_are_found_in_comin_messages() {
        assert_eq!(
            find_uuid("confirmer: confirmed generation d558cf65-2df9-4869-8855-74165854ddf9"),
            Some("d558cf65-2df9-4869-8855-74165854ddf9")
        );
        assert_eq!(find_uuid("no uuid here"), None);
    }

    #[test]
    fn the_buffer_is_bounded() {
        let mut view = LogView::default();
        for index in 0..MAX_ENTRIES + 5 {
            let _ = view.update(LogMessage::LogReceived(entry(
                &format!("line {index}"),
                Priority::Info,
            )));
        }
        assert_eq!(view.len(), MAX_ENTRIES);
    }
}
