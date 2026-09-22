use std::collections::VecDeque;

use iced::{
    Alignment, Background, Border, Color, Element, Fill, Font, Length, Task, Theme,
    border::Radius,
    widget::{
        Column, Space, button, checkbox, column, container, row, scrollable, text, text::Wrapping,
    },
};

use crate::{
    gui::theme::{
        BREEZE_BG_CARD, BREEZE_BORDER, BREEZE_DANGER, BREEZE_TEXT, BREEZE_TEXT_DIM,
        BREEZE_TEXT_MUTED, BREEZE_WARNING, secondary_button_style,
    },
    logs::{LogEntry, Priority},
};

pub const MAX_ENTRIES: usize = 2000;

/// The fixed height of one log row. The list maps the scroll offset to a row
/// index with this value, so every row must keep this exact height.
const ROW_HEIGHT: f32 = 20.0;

/// Extra rows drawn above and below the viewport. The overscan hides small
/// layout changes while the user scrolls.
const OVERSCAN: usize = 8;

#[derive(Debug)]
pub struct LogView {
    pub entries: VecDeque<LogEntry>,
    pub auto_scroll: bool,
    pub scroll_id: scrollable::Id,
    scroll_offset: f32,
    viewport_height: f32,
}

#[derive(Debug, Clone)]
pub enum LogMessage {
    LogReceived(LogEntry),
    ClearLogs,
    ToggleAutoScroll(bool),
    Scrolled(scrollable::Viewport),
}

impl Default for LogView {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            auto_scroll: true,
            scroll_id: scrollable::Id::unique(),
            scroll_offset: 0.0,
            viewport_height: 800.0,
        }
    }
}

impl LogView {
    pub fn update(&mut self, message: LogMessage) -> Task<LogMessage> {
        match message {
            LogMessage::LogReceived(entry) => {
                self.entries.push_back(entry);
                if self.entries.len() > MAX_ENTRIES {
                    self.entries.pop_front();
                }
                self.sync_to_end()
            }
            LogMessage::ClearLogs => {
                self.entries.clear();
                self.scroll_offset = 0.0;
                Task::none()
            }
            LogMessage::ToggleAutoScroll(enabled) => {
                self.auto_scroll = enabled;
                if enabled {
                    self.sync_to_end()
                } else {
                    Task::none()
                }
            }
            LogMessage::Scrolled(viewport) => {
                self.scroll_offset = viewport.absolute_offset().y;
                self.viewport_height = viewport.bounds().height;
                Task::none()
            }
        }
    }

    fn sync_to_end(&mut self) -> Task<LogMessage> {
        if !self.auto_scroll {
            return Task::none();
        }

        let content_height = self.entries.len() as f32 * ROW_HEIGHT;
        self.scroll_offset = (content_height - self.viewport_height).max(0.0);
        scrollable::snap_to(self.scroll_id.clone(), scrollable::RelativeOffset::END)
    }

    /// Returns the range of entry indices that the viewport shows, plus the
    /// overscan rows. The list renders only this range, so the cost of a
    /// rebuild does not grow with the number of buffered entries.
    fn visible_range(&self, total: usize) -> (usize, usize) {
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

    pub fn view(&self) -> Element<'_, LogMessage> {
        let total = self.entries.len();
        let (start, end) = self.visible_range(total);

        let mut entries = Column::new().spacing(0).width(Fill).padding([0, 12]);
        if start > 0 {
            entries = entries.push(Space::new(Fill, Length::Fixed(start as f32 * ROW_HEIGHT)));
        }
        for entry in self.entries.iter().skip(start).take(end - start) {
            entries = entries.push(log_row(entry));
        }
        if end < total {
            entries = entries.push(Space::new(
                Fill,
                Length::Fixed((total - end) as f32 * ROW_HEIGHT),
            ));
        }

        let toolbar = row![
            button(text("Clear logs").size(13))
                .on_press(LogMessage::ClearLogs)
                .style(secondary_button_style)
                .padding([6, 14]),
            checkbox("Auto-scroll", self.auto_scroll)
                .on_toggle(LogMessage::ToggleAutoScroll)
                .size(16),
            text(format!("{} entries", self.entries.len()))
                .size(13)
                .color(BREEZE_TEXT_MUTED),
        ]
        .spacing(16)
        .align_y(Alignment::Center)
        .padding([10, 16]);

        let log_container = container(
            scrollable(entries)
                .id(self.scroll_id.clone())
                .on_scroll(LogMessage::Scrolled)
                .width(Fill)
                .height(Fill),
        )
        .width(Fill)
        .height(Fill)
        .padding(4)
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(BREEZE_BG_CARD)),
            border: Border {
                color: BREEZE_BORDER,
                width: 1.0,
                radius: Radius::from(6.0),
            },
            ..Default::default()
        });

        column![toolbar, log_container]
            .spacing(8)
            .padding([0, 16])
            .width(Fill)
            .height(Fill)
            .into()
    }
}

fn log_row(entry: &LogEntry) -> Element<'_, LogMessage> {
    let message = entry.message.replace(['\n', '\r'], " ");

    container(
        row![
            text(entry.timestamp.as_str())
                .size(12)
                .font(Font::MONOSPACE)
                .width(Length::Fixed(190.0))
                .color(BREEZE_TEXT_MUTED),
            text(entry.priority.label())
                .size(12)
                .width(Length::Fixed(85.0))
                .color(priority_color(entry.priority)),
            text(message)
                .size(12)
                .font(Font::MONOSPACE)
                .color(BREEZE_TEXT)
                .width(Fill)
                .wrapping(Wrapping::None),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .height(Length::Fixed(ROW_HEIGHT))
    .width(Fill)
    .into()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn view_with(total: usize, scroll_offset: f32, viewport_height: f32) -> LogView {
        let mut view = LogView::default();
        for index in 0..total {
            view.entries.push_back(LogEntry {
                timestamp: String::new(),
                priority: Priority::Info,
                message: format!("entry {index}"),
            });
        }
        view.scroll_offset = scroll_offset;
        view.viewport_height = viewport_height;
        view
    }

    #[test]
    fn empty_log_renders_no_rows() {
        assert_eq!(LogView::default().visible_range(0), (0, 0));
    }

    #[test]
    fn visible_range_only_covers_the_viewport() {
        let view = view_with(2000, 0.0, 400.0);
        let (start, end) = view.visible_range(2000);

        assert_eq!(start, 0);
        assert_eq!(end, (400.0 / ROW_HEIGHT).ceil() as usize + 1 + OVERSCAN);
        assert!(end < 2000);
    }

    #[test]
    fn visible_range_tracks_the_scroll_offset() {
        let view = view_with(2000, 40.0 * ROW_HEIGHT, 400.0);
        let (start, end) = view.visible_range(2000);

        assert_eq!(start, 40 - OVERSCAN);
        assert!(start < 40 && end > 40);
        assert!(end < 2000);
    }

    #[test]
    fn visible_range_clamps_at_the_end() {
        let view = view_with(50, 1_000_000.0, 400.0);

        assert_eq!(view.visible_range(50), (50 - 1 - OVERSCAN, 50));
    }

    #[test]
    fn visible_range_covers_short_buffers() {
        let view = view_with(10, 0.0, 400.0);

        assert_eq!(view.visible_range(10), (0, 10));
    }
}
