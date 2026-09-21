use std::collections::VecDeque;

use iced::{
    Color, Element, Fill, Font, Length, Task,
    widget::{Column, button, checkbox, column, row, scrollable, text},
};

use crate::logs::{LogEntry, Priority};

pub const MAX_ENTRIES: usize = 2000;

#[derive(Debug)]
pub struct LogView {
    pub entries: VecDeque<LogEntry>,
    pub auto_scroll: bool,
    pub scroll_id: scrollable::Id,
}

#[derive(Debug, Clone)]
pub enum LogMessage {
    LogReceived(LogEntry),
    ClearLogs,
    ToggleAutoScroll(bool),
}

impl Default for LogView {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            auto_scroll: true,
            scroll_id: scrollable::Id::unique(),
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
                if self.auto_scroll {
                    scrollable::snap_to(self.scroll_id.clone(), scrollable::RelativeOffset::END)
                } else {
                    Task::none()
                }
            }
            LogMessage::ClearLogs => {
                self.entries.clear();
                Task::none()
            }
            LogMessage::ToggleAutoScroll(enabled) => {
                self.auto_scroll = enabled;
                if enabled {
                    scrollable::snap_to(self.scroll_id.clone(), scrollable::RelativeOffset::END)
                } else {
                    Task::none()
                }
            }
        }
    }

    pub fn view(&self) -> Element<'_, LogMessage> {
        let entries = self
            .entries
            .iter()
            .fold(Column::new().spacing(4).padding(10), |col, entry| {
                col.push(log_row(entry))
            });

        let toolbar = row![
            button(text("Clear logs").size(12))
                .on_press(LogMessage::ClearLogs)
                .style(button::secondary)
                .padding([4, 10]),
            checkbox("Auto-scroll", self.auto_scroll)
                .on_toggle(LogMessage::ToggleAutoScroll)
                .size(14),
            text(format!("{} entries", self.entries.len()))
                .size(12)
                .color(Color::from_rgb(0.6, 0.6, 0.65)),
        ]
        .spacing(14)
        .align_y(iced::Alignment::Center)
        .padding([8, 12]);

        column![
            toolbar,
            scrollable(entries)
                .id(self.scroll_id.clone())
                .width(Fill)
                .height(Fill),
        ]
        .into()
    }
}

fn log_row(entry: &LogEntry) -> Element<'_, LogMessage> {
    row![
        text(entry.timestamp.clone())
            .size(12)
            .font(Font::MONOSPACE)
            .width(Length::Fixed(180.0))
            .color(Color::from_rgb(0.65, 0.65, 0.70)),
        text(entry.priority.label())
            .size(12)
            .width(Length::Fixed(80.0))
            .color(priority_color(entry.priority)),
        text(entry.message.clone())
            .size(12)
            .font(Font::MONOSPACE)
            .width(Fill),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center)
    .into()
}

fn priority_color(priority: Priority) -> Color {
    match priority {
        Priority::Emergency | Priority::Alert | Priority::Critical | Priority::Error => {
            Color::from_rgb(0.90, 0.30, 0.30)
        }
        Priority::Warning => Color::from_rgb(0.90, 0.65, 0.20),
        Priority::Notice | Priority::Info => Color::from_rgb(0.55, 0.55, 0.60),
        Priority::Debug => Color::from_rgb(0.40, 0.40, 0.45),
    }
}
