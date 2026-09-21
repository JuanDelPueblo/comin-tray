use std::{path::Path, time::Duration};

use iced::{
    Alignment, Background, Border, Color, Element, Fill, Font, Size, Subscription, Task, Theme,
    border::Radius,
    widget::{button, column, container, horizontal_space, row, text},
    window,
};

use crate::{
    comin::CominClient,
    gui::{
        log_view::{LogMessage, LogView},
        overview::{self, OverviewMessage},
        theme::{
            BREEZE_ACCENT, BREEZE_BG_HEADER, BREEZE_BG_HOVER, BREEZE_BG_WINDOW,
            BREEZE_BORDER_SUBTLE, BREEZE_TEXT_DIM,
        },
    },
    logs,
    model::CominState,
    tray::GuiPage,
};

const LOG_STREAM_ID: &str = "comin-gui-log-stream";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Logs,
}

pub struct CominGui {
    pub active_tab: Tab,
    pub client: CominClient,
    pub state: Option<CominState>,
    pub error: Option<String>,
    pub action_error: Option<String>,
    pub busy_action: Option<String>,
    pub log_view: LogView,
}

#[derive(Debug, Clone)]
pub enum Message {
    TabSelected(Tab),
    Overview(OverviewMessage),
    Log(LogMessage),
    StatusLoaded(Box<Result<CominState, String>>),
    ActionResult(Result<(), String>),
    Tick,
}

impl CominGui {
    pub fn new(initial_page: GuiPage) -> (Self, Task<Message>) {
        let active_tab = match initial_page {
            GuiPage::Overview => Tab::Overview,
            GuiPage::Logs => Tab::Logs,
        };

        let client = CominClient::default();
        let app = Self {
            active_tab,
            client: client.clone(),
            state: None,
            error: None,
            action_error: None,
            busy_action: None,
            log_view: LogView::default(),
        };

        let initial_task = Task::perform(
            async move { Box::new(client.status().await.map_err(|e| e.to_string())) },
            Message::StatusLoaded,
        );

        (app, initial_task)
    }

    pub fn title(&self) -> String {
        let hostname = self
            .state
            .as_ref()
            .and_then(CominState::hostname)
            .unwrap_or("Local");
        format!("Comin — {hostname}")
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TabSelected(tab) => {
                self.active_tab = tab;
                Task::none()
            }
            Message::Tick => Task::none(),
            Message::StatusLoaded(boxed_result) => {
                match *boxed_result {
                    Ok(state) => {
                        self.state = Some(state);
                        self.error = None;
                    }
                    Err(err) => {
                        self.error = Some(err);
                    }
                }
                Task::none()
            }
            Message::ActionResult(result) => {
                self.busy_action = None;
                match result {
                    Ok(()) => {
                        self.action_error = None;
                        let client = self.client.clone();
                        Task::perform(
                            async move { Box::new(client.status().await.map_err(|e| e.to_string())) },
                            Message::StatusLoaded,
                        )
                    }
                    Err(err) => {
                        self.action_error = Some(err);
                        Task::none()
                    }
                }
            }
            Message::Overview(msg) => match msg {
                OverviewMessage::Refresh => {
                    let client = self.client.clone();
                    Task::perform(
                        async move { Box::new(client.status().await.map_err(|e| e.to_string())) },
                        Message::StatusLoaded,
                    )
                }
                OverviewMessage::Fetch => {
                    self.busy_action = Some("Fetching".into());
                    self.action_error = None;
                    let client = self.client.clone();
                    Task::perform(
                        async move { client.fetch().await.map_err(|e| e.to_string()) },
                        Message::ActionResult,
                    )
                }
                OverviewMessage::Suspend => {
                    self.busy_action = Some("Suspending GitOps".into());
                    self.action_error = None;
                    let client = self.client.clone();
                    Task::perform(
                        async move { client.suspend().await.map_err(|e| e.to_string()) },
                        Message::ActionResult,
                    )
                }
                OverviewMessage::Resume => {
                    self.busy_action = Some("Resuming GitOps".into());
                    self.action_error = None;
                    let client = self.client.clone();
                    Task::perform(
                        async move { client.resume().await.map_err(|e| e.to_string()) },
                        Message::ActionResult,
                    )
                }
                OverviewMessage::SwitchLatest => {
                    self.busy_action = Some("Switching to latest live".into());
                    self.action_error = None;
                    let client = self.client.clone();
                    Task::perform(
                        async move { client.switch_latest().await.map_err(|e| e.to_string()) },
                        Message::ActionResult,
                    )
                }
                OverviewMessage::RetryLatest => {
                    self.busy_action = Some("Retrying deployment".into());
                    self.action_error = None;
                    let client = self.client.clone();
                    Task::perform(
                        async move { client.retry_latest().await.map_err(|e| e.to_string()) },
                        Message::ActionResult,
                    )
                }
                OverviewMessage::AcceptConfirmation => {
                    self.busy_action = Some("Accepting confirmation".into());
                    self.action_error = None;
                    let client = self.client.clone();
                    Task::perform(
                        async move {
                            client
                                .accept_confirmation()
                                .await
                                .map_err(|e| e.to_string())
                        },
                        Message::ActionResult,
                    )
                }
            },
            Message::Log(log_msg) => self.log_view.update(log_msg).map(Message::Log),
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let status_poll = iced::time::every(Duration::from_secs(3))
            .map(|_| Message::Overview(OverviewMessage::Refresh));

        let tick = iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick);

        let log_stream = iced::stream::channel(100, |mut output| async move {
            logs::send_history(&mut output).await;
            logs::send_follow(&mut output).await;
        });
        let log_sub = Subscription::run_with_id(LOG_STREAM_ID, log_stream)
            .map(|entry| Message::Log(LogMessage::LogReceived(entry)));

        Subscription::batch([status_poll, tick, log_sub])
    }

    pub fn view(&self) -> Element<'_, Message> {
        let nav = self.view_navbar();

        let page_content: Element<'_, Message> = match self.active_tab {
            Tab::Overview => overview::view(
                self.state.as_ref(),
                self.error.as_deref(),
                self.action_error.as_deref(),
                self.busy_action.as_deref(),
            )
            .map(Message::Overview),
            Tab::Logs => self.log_view.view().map(Message::Log),
        };

        container(column![nav, page_content].width(Fill).height(Fill))
            .width(Fill)
            .height(Fill)
            .style(|_theme: &Theme| container::Style {
                background: Some(Background::Color(BREEZE_BG_WINDOW)),
                ..Default::default()
            })
            .into()
    }

    fn view_navbar(&self) -> Element<'_, Message> {
        let is_overview = self.active_tab == Tab::Overview;
        let is_logs = self.active_tab == Tab::Logs;

        let overview_tab = button(text("Overview").size(13))
            .on_press(Message::TabSelected(Tab::Overview))
            .style(move |_theme: &Theme, status: button::Status| {
                if is_overview {
                    button::Style {
                        background: Some(Background::Color(BREEZE_ACCENT)),
                        text_color: Color::WHITE,
                        border: Border {
                            radius: Radius::from(4.0),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                } else {
                    let bg = match status {
                        button::Status::Hovered => Some(Background::Color(BREEZE_BG_HOVER)),
                        _ => None,
                    };
                    button::Style {
                        background: bg,
                        text_color: BREEZE_TEXT_DIM,
                        border: Border {
                            radius: Radius::from(4.0),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                }
            })
            .padding([7, 20]);

        let logs_tab = button(text("Logs").size(13))
            .on_press(Message::TabSelected(Tab::Logs))
            .style(move |_theme: &Theme, status: button::Status| {
                if is_logs {
                    button::Style {
                        background: Some(Background::Color(BREEZE_ACCENT)),
                        text_color: Color::WHITE,
                        border: Border {
                            radius: Radius::from(4.0),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                } else {
                    let bg = match status {
                        button::Status::Hovered => Some(Background::Color(BREEZE_BG_HOVER)),
                        _ => None,
                    };
                    button::Style {
                        background: bg,
                        text_color: BREEZE_TEXT_DIM,
                        border: Border {
                            radius: Radius::from(4.0),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                }
            })
            .padding([7, 20]);

        let tab_segment = container(row![overview_tab, logs_tab].spacing(2))
            .padding(3)
            .style(|_theme: &Theme| container::Style {
                background: Some(Background::Color(BREEZE_BG_HEADER)),
                border: Border {
                    color: BREEZE_BORDER_SUBTLE,
                    width: 1.0,
                    radius: Radius::from(6.0),
                },
                ..Default::default()
            });

        row![tab_segment, horizontal_space(),]
            .spacing(8)
            .padding([12, 20])
            .align_y(Alignment::Center)
            .into()
    }
}

fn load_font_bytes(pattern: &str) -> Option<Vec<u8>> {
    let output = std::process::Command::new("fc-match")
        .arg("-f")
        .arg("%{file}")
        .arg(pattern)
        .output()
        .ok()?;

    if output.status.success() {
        let path_str = String::from_utf8(output.stdout).ok()?;
        let path = path_str.trim();
        if !path.is_empty() {
            return std::fs::read(path).ok();
        }
    }
    None
}

pub fn run(initial_page: GuiPage) -> iced::Result {
    // Single instance check via runtime pidfile
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let lock_file = format!("{runtime_dir}/comin-tray-gui.pid");

    let existing_pid = std::fs::read_to_string(&lock_file)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok());

    if let Some(pid) = existing_pid {
        let proc_path = format!("/proc/{pid}");
        if Path::new(&proc_path).exists() {
            eprintln!("Comin GUI is already running (PID {pid}).");
            return Ok(());
        }
    }

    let _ = std::fs::write(&lock_file, std::process::id().to_string());

    struct PidCleanup(String);
    impl Drop for PidCleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _cleanup = PidCleanup(lock_file);

    let mut app = iced::application(CominGui::title, CominGui::update, CominGui::view)
        .subscription(CominGui::subscription)
        .theme(|_| Theme::Dark)
        .window(window::Settings {
            size: Size::new(1180.0, 820.0),
            min_size: Some(Size::new(900.0, 600.0)),
            resizable: true,
            platform_specific: window::settings::PlatformSpecific {
                application_id: "comin-tray".into(),
                ..Default::default()
            },
            ..window::Settings::default()
        });

    if let Some(sans_bytes) = load_font_bytes("Noto Sans") {
        app = app.font(sans_bytes).default_font(Font {
            family: iced::font::Family::Name("Noto Sans"),
            ..Font::DEFAULT
        });
    }

    if let Some(mono_bytes) = load_font_bytes("monospace") {
        app = app.font(mono_bytes);
    }

    app.run_with(move || CominGui::new(initial_page))
}
