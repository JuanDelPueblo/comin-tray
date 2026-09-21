use std::collections::VecDeque;

use iced::{
    Alignment, Background, Border, Color, Element, Fill, Font, Length, Task, Theme,
    border::Radius,
    widget::{Column, button, checkbox, column, container, row, scrollable, text},
};

use crate::{
    gui::theme::{
        BREEZE_BG_CARD, BREEZE_BORDER, BREEZE_DANGER, BREEZE_TEXT, BREEZE_TEXT_DIM,
        BREEZE_TEXT_MUTED, BREEZE_WARNING, secondary_button_style,
    },
    logs::{LogEntry, Priority},
};

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
            .fold(Column::new().spacing(4).padding(12), |col, entry| {
                col.push(log_row(entry))
            });

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
    row![
        text(entry.timestamp.clone())
            .size(12)
            .font(Font::MONOSPACE)
            .width(Length::Fixed(190.0))
            .color(BREEZE_TEXT_MUTED),
        text(entry.priority.label())
            .size(12)
            .width(Length::Fixed(85.0))
            .color(priority_color(entry.priority)),
        text(entry.message.clone())
            .size(12)
            .font(Font::MONOSPACE)
            .color(BREEZE_TEXT)
            .width(Fill),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
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
