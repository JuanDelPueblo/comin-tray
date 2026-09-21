use std::collections::VecDeque;

use iced::{
    Color, Element, Fill, Length, Size, Subscription, Task, Theme,
    widget::{Column, button, column, row, scrollable, text},
    window,
};

use crate::logs::{self, LogEntry, Priority};

const MAX_ENTRIES: usize = 2000;
const LOG_STREAM_ID: &str = "comin-log-stream";

pub fn run() -> iced::Result {
    iced::application("Comin service log", LogViewer::update, LogViewer::view)
        .subscription(LogViewer::subscription)
        .theme(|_| Theme::Dark)
        .window(window::Settings {
            size: Size::new(900.0, 600.0),
            platform_specific: window::settings::PlatformSpecific {
                // Must match data/comin-tray.desktop's basename, or KWin
                // can't associate the window with the app and leaves a
                // dangling taskbar entry behind after it closes.
                application_id: "comin-tray".into(),
                ..Default::default()
            },
            ..window::Settings::default()
        })
        .run()
}

#[derive(Debug)]
struct LogViewer {
    entries: VecDeque<LogEntry>,
    scroll_id: scrollable::Id,
}

impl Default for LogViewer {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            scroll_id: scrollable::Id::unique(),
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    LogReceived(LogEntry),
    ClearView,
}

impl LogViewer {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::LogReceived(entry) => {
                self.entries.push_back(entry);
                if self.entries.len() > MAX_ENTRIES {
                    self.entries.pop_front();
                }
                scrollable::snap_to(self.scroll_id.clone(), scrollable::RelativeOffset::END)
            }
            Message::ClearView => {
                self.entries.clear();
                Task::none()
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let log_stream = iced::stream::channel(100, |mut output| async move {
            logs::send_history(&mut output).await;
            logs::send_follow(&mut output).await;
        });

        Subscription::run_with_id(LOG_STREAM_ID, log_stream).map(Message::LogReceived)
    }

    fn view(&self) -> Element<'_, Message> {
        let entries = self
            .entries
            .iter()
            .fold(Column::new().spacing(2).padding(12), |column, entry| {
                column.push(log_row(entry))
            });

        let toolbar = row![button("Clear view").on_press(Message::ClearView)]
            .spacing(8)
            .padding(8);

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

fn log_row(entry: &LogEntry) -> Element<'_, Message> {
    row![
        text(entry.timestamp.clone()).width(Length::Fixed(180.0)),
        text(entry.priority.label())
            .width(Length::Fixed(90.0))
            .color(priority_color(entry.priority)),
        text(entry.message.clone()).width(Fill),
    ]
    .spacing(12)
    .into()
}

fn priority_color(priority: Priority) -> Color {
    match priority {
        Priority::Emergency | Priority::Alert | Priority::Critical | Priority::Error => {
            Color::from_rgb(0.85, 0.25, 0.25)
        }
        Priority::Warning => Color::from_rgb(0.85, 0.6, 0.15),
        Priority::Notice | Priority::Info => Color::from_rgb(0.5, 0.5, 0.55),
        Priority::Debug => Color::from_rgb(0.4, 0.4, 0.45),
    }
}
