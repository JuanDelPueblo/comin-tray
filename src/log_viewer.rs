use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use iced::{
    Color, Element, Fill, Length, Size, Subscription, Task, Theme,
    widget::{Column, button, column, row, scrollable, text},
    window,
};

use crate::logs::{self, LogEntry, Priority};

const MAX_ENTRIES: usize = 2000;
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const LOG_STREAM_ID: &str = "comin-log-stream";

pub fn run(open_requested: Arc<AtomicBool>) -> iced::Result {
    iced::daemon("Comin service log", LogViewer::update, LogViewer::view)
        .subscription(LogViewer::subscription)
        .theme(|_, _| Theme::Dark)
        .run_with(move || (LogViewer::new(open_requested), Task::none()))
}

struct LogViewer {
    open_requested: Arc<AtomicBool>,
    window_id: Option<window::Id>,
    entries: VecDeque<LogEntry>,
    scroll_id: scrollable::Id,
}

#[derive(Debug, Clone)]
enum Message {
    PollOpenRequest,
    WindowOpened(window::Id),
    WindowClosed(window::Id),
    LogReceived(LogEntry),
    ClearView,
}

impl LogViewer {
    fn new(open_requested: Arc<AtomicBool>) -> Self {
        Self {
            open_requested,
            window_id: None,
            entries: VecDeque::new(),
            scroll_id: scrollable::Id::unique(),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PollOpenRequest => {
                if self.open_requested.swap(false, Ordering::SeqCst) {
                    self.open_or_focus_window()
                } else {
                    Task::none()
                }
            }
            Message::WindowOpened(id) => {
                self.window_id = Some(id);
                Task::none()
            }
            Message::WindowClosed(id) => {
                if self.window_id == Some(id) {
                    self.window_id = None;
                }
                Task::none()
            }
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
        let mut subscriptions = vec![
            iced::time::every(POLL_INTERVAL).map(|_| Message::PollOpenRequest),
            window::close_events().map(Message::WindowClosed),
        ];

        if self.window_id.is_some() {
            let log_stream = iced::stream::channel(100, |mut output| async move {
                logs::send_history(&mut output).await;
                logs::send_follow(&mut output).await;
            });

            subscriptions.push(
                Subscription::run_with_id(LOG_STREAM_ID, log_stream).map(Message::LogReceived),
            );
        }

        Subscription::batch(subscriptions)
    }

    fn view(&self, _window_id: window::Id) -> Element<'_, Message> {
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

    fn open_or_focus_window(&mut self) -> Task<Message> {
        if let Some(id) = self.window_id {
            window::gain_focus(id)
        } else {
            let (id, open) = window::open(window::Settings {
                size: Size::new(900.0, 600.0),
                platform_specific: window::settings::PlatformSpecific {
                    // Must match data/comin-tray.desktop's basename, or KWin
                    // can't associate the window with the app and leaves a
                    // dangling taskbar entry behind after it closes.
                    application_id: "comin-tray".into(),
                    ..Default::default()
                },
                ..window::Settings::default()
            });
            self.window_id = Some(id);
            open.map(Message::WindowOpened)
        }
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
