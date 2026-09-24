//! A virtualized, selectable list of journal lines, shared by the Logs page
//! and the raw log of a deployment.
//!
//! iced text cannot be selected with the mouse, so the list selects whole
//! rows instead: click selects a row, Shift+click extends the selection, and
//! the owner copies the selected rows with Ctrl+C or a toolbar button.

use std::borrow::Cow;

use iced::{
    Alignment, Background, Border, Color, Element, Fill, Font, Length, Task, Theme,
    border::Radius,
    font,
    keyboard::Modifiers,
    widget::{
        Column, Space, container, mouse_area, rich_text, row, scrollable, span, text,
        text::Wrapping,
    },
};

use crate::{
    build_log::LineKind,
    gui::theme::{
        BREEZE_ACCENT, BREEZE_BG_CARD, BREEZE_BORDER, BREEZE_DANGER, BREEZE_PURPLE, BREEZE_SUCCESS,
        BREEZE_TEAL, BREEZE_TEXT, BREEZE_TEXT_DIM, BREEZE_TEXT_MUTED, BREEZE_WARNING,
        BREEZE_YELLOW,
    },
    highlight::{Token, compact_store_paths, highlight},
    logs::{LogEntry, Priority},
};

/// The fixed height of one log row. The list maps the scroll offset to a row
/// index with this value, so every row must keep this exact height.
pub const ROW_HEIGHT: f32 = 20.0;

/// Extra rows drawn above and below the viewport. The overscan hides small
/// layout changes while the user scrolls.
pub const OVERSCAN: usize = 8;

/// Width of the time, level and source columns plus spacing and padding.
const FIXED_COLUMNS_WIDTH: f32 =
    64.0 + 58.0 + 70.0 + ICON_WIDTH + 4.0 * 10.0 + 2.0 * 8.0 + 2.0 * 4.0 + 16.0;

/// Width of the column with the nom-style status icon.
const ICON_WIDTH: f32 = 14.0;

/// The advance of one 12px monospace glyph, slightly rounded up.
const MONO_CHAR_WIDTH: f32 = 7.4;

/// One row handed to [`LogList::view`]. `id` must identify the same line
/// across rebuilds (a sequence number or an index into a fixed log).
pub struct Row<'a> {
    pub id: u64,
    pub entry: &'a LogEntry,
    pub text: Cow<'a, str>,
    /// What the line is, for its icon and colors.
    pub kind: &'a LineKind,
}

#[derive(Debug, Clone)]
pub enum LogListMessage {
    Scrolled(scrollable::Viewport),
    RowPressed(u64),
}

#[derive(Debug)]
pub struct LogList {
    pub scroll_id: scrollable::Id,
    pub auto_scroll: bool,
    /// Keyboard modifiers, kept current by the application, so a row press
    /// can tell a plain click from a Shift+click.
    pub modifiers: Modifiers,
    /// When on, a click extends the selection like Shift+click does. This
    /// is for when Shift is awkward or its key events do not reach the app.
    pub range_mode: bool,
    /// Color the parts of each line like `nh os switch` does.
    pub colors: bool,
    scroll_offset: f32,
    viewport_height: f32,
    viewport_width: f32,
    /// The first and the last selected row ids, in click order.
    selection: Option<(u64, u64)>,
}

impl Default for LogList {
    fn default() -> Self {
        Self {
            scroll_id: scrollable::Id::unique(),
            auto_scroll: true,
            modifiers: Modifiers::default(),
            range_mode: false,
            colors: true,
            scroll_offset: 0.0,
            viewport_height: 800.0,
            viewport_width: 900.0,
            selection: None,
        }
    }
}

impl LogList {
    pub fn update(&mut self, message: LogListMessage) {
        match message {
            LogListMessage::Scrolled(viewport) => {
                let offset = viewport.absolute_offset().y;
                // Scrolling up pauses following the log; scrolling back to
                // the end resumes it. New rows only ever move the offset
                // down, so they never pause it.
                let at_end = viewport.relative_offset().y >= 0.999
                    || viewport.content_bounds().height <= viewport.bounds().height;
                if at_end {
                    self.auto_scroll = true;
                } else if offset < self.scroll_offset - 1.0 {
                    self.auto_scroll = false;
                }
                self.scroll_offset = offset;
                self.viewport_height = viewport.bounds().height;
                self.viewport_width = viewport.bounds().width;
            }
            LogListMessage::RowPressed(id) => {
                self.selection = match self.selection {
                    Some((anchor, _)) if self.modifiers.shift() || self.range_mode => {
                        Some((anchor, id))
                    }
                    Some((anchor, head)) if anchor == id && head == id => None,
                    _ => Some((id, id)),
                };
            }
        }
    }

    pub fn clear(&mut self) {
        self.selection = None;
        self.scroll_offset = 0.0;
        self.auto_scroll = true;
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    pub fn has_selection(&self) -> bool {
        self.selection.is_some()
    }

    /// Selects every row in `ids`.
    pub fn select_all(&mut self, ids: &[u64]) {
        self.selection = match (ids.first(), ids.last()) {
            (Some(&first), Some(&last)) => Some((first, last)),
            _ => None,
        };
    }

    /// The positions in `ids` that are selected. A selection whose ends are
    /// no longer listed (filtered out or dropped from the buffer) is empty.
    pub fn selected_positions(&self, ids: &[u64]) -> std::ops::Range<usize> {
        let Some((anchor, head)) = self.selection else {
            return 0..0;
        };
        let find = |id: u64| ids.iter().position(|&candidate| candidate == id);
        match (find(anchor), find(head)) {
            (Some(a), Some(b)) => a.min(b)..a.max(b) + 1,
            _ => 0..0,
        }
    }

    /// Scrolls to the newest row when following the log.
    pub fn follow(&mut self, total: usize) -> Task<LogListMessage> {
        if !self.auto_scroll {
            return Task::none();
        }
        let content_height = total as f32 * ROW_HEIGHT;
        self.scroll_offset = (content_height - self.viewport_height).max(0.0);
        scrollable::snap_to(self.scroll_id.clone(), scrollable::RelativeOffset::END)
    }

    /// Returns the range of row positions that the viewport shows, plus the
    /// overscan rows. The list renders only this range, so the cost of a
    /// rebuild does not grow with the number of rows.
    pub fn visible_range(&self, total: usize) -> (usize, usize) {
        if total == 0 {
            return (0, 0);
        }

        let viewport_height = self.viewport_height.max(ROW_HEIGHT);
        let first = ((self.scroll_offset.max(0.0) / ROW_HEIGHT).floor() as usize).min(total - 1);
        let visible = (viewport_height / ROW_HEIGHT).ceil() as usize + 1;

        let start = first.saturating_sub(OVERSCAN);
        let end = (first + visible + OVERSCAN).min(total);
        (start, end)
    }

    /// How many message characters fit on a row. Long lines are cut to
    /// this length: iced would otherwise wrap them inside the fixed-height
    /// row and hide the rest. Copying always uses the full line.
    fn message_chars(&self) -> usize {
        (((self.viewport_width - FIXED_COLUMNS_WIDTH) / MONO_CHAR_WIDTH).floor() as usize).max(20)
    }

    pub fn view<'a>(&self, rows: Vec<Row<'a>>, height: Length) -> Element<'a, LogListMessage> {
        let total = rows.len();
        let (start, end) = self.visible_range(total);
        let ids: Vec<u64> = rows.iter().map(|row| row.id).collect();
        let selected = self.selected_positions(&ids);

        let mut list = Column::new().spacing(0).width(Fill).padding([0, 8]);
        if start > 0 {
            list = list.push(Space::new(Fill, Length::Fixed(start as f32 * ROW_HEIGHT)));
        }
        let max_chars = self.message_chars();
        for (position, row) in rows.into_iter().enumerate().skip(start).take(end - start) {
            list = list.push(log_row(
                row,
                selected.contains(&position),
                max_chars,
                self.colors,
            ));
        }
        if end < total {
            list = list.push(Space::new(
                Fill,
                Length::Fixed((total - end) as f32 * ROW_HEIGHT),
            ));
        }

        container(
            scrollable(list)
                .id(self.scroll_id.clone())
                .on_scroll(LogListMessage::Scrolled)
                .width(Fill)
                .height(Fill),
        )
        .width(Fill)
        .height(height)
        .padding(4)
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(BREEZE_BG_CARD)),
            border: Border {
                color: BREEZE_BORDER,
                width: 1.0,
                radius: Radius::from(6.0),
            },
            ..Default::default()
        })
        .into()
    }
}

fn log_row(
    row: Row<'_>,
    selected: bool,
    max_chars: usize,
    colors: bool,
) -> Element<'_, LogListMessage> {
    let entry = row.entry;
    let message = fit_line(row.text, max_chars);
    let source = if entry.identifier.is_empty() || entry.identifier == "comin" {
        String::new()
    } else {
        entry.identifier.clone()
    };
    let (icon, icon_color) = if colors {
        kind_icon(row.kind, &message)
    } else {
        ("", BREEZE_TEXT_MUTED)
    };

    let body: Element<'_, LogListMessage> = if colors {
        let spans: Vec<_> = highlight(&message, row.kind, entry.from_comin)
            .into_iter()
            .map(|(range, token)| {
                let (color, bold) = token_style(token);
                let font = if bold {
                    Font {
                        weight: font::Weight::Bold,
                        ..Font::MONOSPACE
                    }
                } else {
                    Font::MONOSPACE
                };
                span(message[range].to_string()).color(color).font(font)
            })
            .collect();
        rich_text(spans)
            .size(12)
            .font(Font::MONOSPACE)
            .wrapping(Wrapping::None)
            .into()
    } else {
        let color = if row.kind.is_noise() {
            BREEZE_TEXT_MUTED
        } else if entry.priority <= Priority::Error {
            BREEZE_DANGER
        } else if entry.priority == Priority::Warning {
            BREEZE_WARNING
        } else {
            BREEZE_TEXT
        };
        text(message)
            .size(12)
            .font(Font::MONOSPACE)
            .color(color)
            .wrapping(Wrapping::None)
            .into()
    };

    let content = row![
        text(entry.clock())
            .size(12)
            .font(Font::MONOSPACE)
            .width(Length::Fixed(64.0))
            .color(BREEZE_TEXT_MUTED),
        text(entry.priority.label())
            .size(12)
            .width(Length::Fixed(58.0))
            .color(priority_color(entry.priority)),
        text(source)
            .size(12)
            .width(Length::Fixed(70.0))
            .color(BREEZE_TEXT_MUTED)
            .wrapping(Wrapping::None),
        text(icon)
            .size(12)
            .width(Length::Fixed(ICON_WIDTH))
            .color(icon_color),
        body,
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    mouse_area(
        container(content)
            .width(Fill)
            .height(Length::Fixed(ROW_HEIGHT))
            .padding([0, 4])
            .clip(true)
            .style(move |_theme: &Theme| container::Style {
                background: selected.then_some(Background::Color(Color {
                    a: 0.28,
                    ..BREEZE_ACCENT
                })),
                ..Default::default()
            }),
    )
    .on_press(LogListMessage::RowPressed(row.id))
    .into()
}

/// The color of a highlighted part, and whether it is bold.
fn token_style(token: Token) -> (Color, bool) {
    match token {
        Token::Plain => (BREEZE_TEXT_DIM, false),
        Token::Header => (BREEZE_TEXT, true),
        Token::Build => (BREEZE_YELLOW, true),
        Token::Fetch => (BREEZE_ACCENT, true),
        Token::Component => (BREEZE_TEAL, true),
        Token::StoreHash => (BREEZE_TEXT_MUTED, false),
        Token::StoreName => (BREEZE_TEXT, true),
        Token::DrvPrefix => (BREEZE_PURPLE, false),
        Token::Url => (BREEZE_ACCENT, false),
        Token::Hash | Token::Number => (BREEZE_YELLOW, false),
        Token::Success => (BREEZE_SUCCESS, true),
        Token::Error => (BREEZE_DANGER, false),
        Token::Warning => (BREEZE_WARNING, true),
        Token::Muted => (BREEZE_TEXT_MUTED, false),
    }
}

/// The nom-style status icon of a line.
fn kind_icon(kind: &LineKind, message: &str) -> (&'static str, Color) {
    match kind {
        LineKind::Building(_) => ("▸", BREEZE_YELLOW),
        LineKind::Fetching { .. } => ("↓", BREEZE_ACCENT),
        LineKind::Plan => ("≡", BREEZE_TEXT_DIM),
        LineKind::BuildOutput(_) => ("│", BREEZE_PURPLE),
        LineKind::Command => ("$", BREEZE_TEAL),
        LineKind::Activation => ("⚙", BREEZE_TEAL),
        LineKind::Error => ("✗", BREEZE_DANGER),
        LineKind::Warning => ("!", BREEZE_WARNING),
        LineKind::Step
            if message.contains("successfully")
                || message.contains("succeeded")
                || message.ends_with("deployment ended") =>
        {
            ("✓", BREEZE_SUCCESS)
        }
        LineKind::Step => ("•", BREEZE_TEXT_MUTED),
        LineKind::Noise | LineKind::Other => ("", BREEZE_TEXT_MUTED),
    }
}

/// Puts `text` on one line and cuts it to `max_chars`, ending with `…`.
/// Store hashes are shortened first when that is enough to show more of the
/// line, since the name after the hash is the useful part.
fn fit_line(text: Cow<'_, str>, max_chars: usize) -> Cow<'_, str> {
    let mut text = if text.contains(['\n', '\r']) {
        Cow::Owned(text.replace(['\n', '\r'], " ⏎ "))
    } else {
        text
    };
    if text.chars().nth(max_chars).is_some() && text.contains("/nix/store/") {
        text = Cow::Owned(compact_store_paths(&text));
    }
    match text.char_indices().nth(max_chars.saturating_sub(1)) {
        Some((cut, _)) if text[cut..].chars().nth(1).is_some() => {
            Cow::Owned(format!("{}…", &text[..cut]))
        }
        _ => text,
    }
}

fn priority_color(priority: Priority) -> Color {
    match priority {
        Priority::Emergency | Priority::Alert | Priority::Critical | Priority::Error => {
            BREEZE_DANGER
        }
        Priority::Warning => BREEZE_WARNING,
        Priority::Notice | Priority::Info => BREEZE_TEXT_DIM,
        Priority::Debug => BREEZE_TEXT_MUTED,
    }
}

/// Where "Save to file" writes logs: the XDG download directory when it can
/// be found, the home directory otherwise.
pub fn save_directory() -> std::path::PathBuf {
    let from_xdg = std::process::Command::new("xdg-user-dir")
        .arg("DOWNLOAD")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|path| !path.is_empty())
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_dir());
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    from_xdg
        .or_else(|| {
            home.as_ref()
                .map(|home| home.join("Downloads"))
                .filter(|path| path.is_dir())
        })
        .or(home)
        .unwrap_or_else(std::env::temp_dir)
}

/// Writes `contents` to `<save_directory>/<stem>-<timestamp>.log` and returns
/// the path.
pub async fn save_log(stem: String, contents: String) -> Result<String, String> {
    let stamp = time::OffsetDateTime::now_utc()
        .to_offset(crate::format::local_offset())
        .format(time::macros::format_description!(
            "[year][month][day]-[hour][minute][second]"
        ))
        .unwrap_or_default();
    let path = save_directory().join(format!("{stem}-{stamp}.log"));
    tokio::fs::write(&path, contents)
        .await
        .map_err(|error| format!("Could not save {}: {error}", path.display()))?;
    Ok(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list_with(scroll_offset: f32, viewport_height: f32) -> LogList {
        LogList {
            scroll_offset,
            viewport_height,
            ..LogList::default()
        }
    }

    #[test]
    fn empty_log_renders_no_rows() {
        assert_eq!(LogList::default().visible_range(0), (0, 0));
    }

    #[test]
    fn visible_range_only_covers_the_viewport() {
        let (start, end) = list_with(0.0, 400.0).visible_range(2000);

        assert_eq!(start, 0);
        assert_eq!(end, (400.0 / ROW_HEIGHT).ceil() as usize + 1 + OVERSCAN);
        assert!(end < 2000);
    }

    #[test]
    fn visible_range_tracks_the_scroll_offset() {
        let (start, end) = list_with(40.0 * ROW_HEIGHT, 400.0).visible_range(2000);

        assert_eq!(start, 40 - OVERSCAN);
        assert!(start < 40 && end > 40);
        assert!(end < 2000);
    }

    #[test]
    fn visible_range_clamps_at_the_end() {
        let list = list_with(1_000_000.0, 400.0);
        assert_eq!(list.visible_range(50), (50 - 1 - OVERSCAN, 50));
    }

    #[test]
    fn visible_range_covers_short_buffers() {
        assert_eq!(list_with(0.0, 400.0).visible_range(10), (0, 10));
    }

    #[test]
    fn click_and_shift_click_select_a_range() {
        let ids = [10, 11, 12, 13, 14];
        let mut list = LogList::default();

        list.update(LogListMessage::RowPressed(13));
        assert_eq!(list.selected_positions(&ids), 3..4);

        list.modifiers = Modifiers::SHIFT;
        list.update(LogListMessage::RowPressed(11));
        assert_eq!(list.selected_positions(&ids), 1..4);

        list.modifiers = Modifiers::default();
        list.update(LogListMessage::RowPressed(12));
        assert_eq!(list.selected_positions(&ids), 2..3);

        // Clicking the only selected row again clears the selection.
        list.update(LogListMessage::RowPressed(12));
        assert!(!list.has_selection());
    }

    #[test]
    fn long_lines_are_cut_to_fit() {
        assert_eq!(fit_line(Cow::Borrowed("short"), 10), "short");
        assert_eq!(fit_line(Cow::Borrowed("0123456789"), 10), "0123456789");
        assert_eq!(fit_line(Cow::Borrowed("0123456789abc"), 10), "012345678…");
        assert_eq!(fit_line(Cow::Borrowed("a\nb"), 10), "a ⏎ b");
        assert_eq!(fit_line(Cow::Borrowed("ééééééé"), 3), "éé…");
        assert_eq!(
            fit_line(
                Cow::Borrowed("  /nix/store/1hcxlkvd7hmp5r0kvfz6mn12zpm211p2-home-manager.drv"),
                40
            ),
            "  /nix/store/1hcxlkv…-home-manager.drv"
        );
    }

    #[test]
    fn range_mode_extends_without_shift() {
        let ids = [1, 2, 3, 4];
        let mut list = LogList::default();
        list.update(LogListMessage::RowPressed(1));
        list.range_mode = true;
        list.update(LogListMessage::RowPressed(4));
        assert_eq!(list.selected_positions(&ids), 0..4);
    }

    #[test]
    fn selection_of_dropped_rows_is_empty() {
        let mut list = LogList::default();
        list.update(LogListMessage::RowPressed(1));
        assert_eq!(list.selected_positions(&[2, 3]), 0..0);

        list.select_all(&[2, 3, 4]);
        assert_eq!(list.selected_positions(&[2, 3, 4]), 0..3);
    }
}
