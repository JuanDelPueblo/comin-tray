use std::time::{Duration, Instant};

use iced::{
    Alignment, Background, Element, Event, Fill, Font, Size, Subscription, Task, Theme, event,
    futures::{SinkExt, StreamExt},
    keyboard::{self, Modifiers},
    widget::{column, container, horizontal_space, row, text},
    window,
};

use crate::{
    comin::CominClient,
    dbus,
    gui::{
        deployments::{DeploymentsMessage, DeploymentsView},
        log_view::{LogMessage, LogView},
        overview::{self, OverviewMessage},
        theme::{BREEZE_BG_WINDOW, BREEZE_TEXT_MUTED, BREEZE_WARNING},
        widgets::{segmented, tab_button},
    },
    logs,
    model::CominState,
    tray::GuiPage,
};

const LOG_STREAM_ID: &str = "comin-gui-log-stream";
const DBUS_SERVICE_ID: &str = "comin-gui-dbus-service";

/// After this long without a successful status read, the window says the
/// data it shows may be out of date.
const STALE_AFTER: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Deployments,
    Logs,
}

impl From<GuiPage> for Tab {
    fn from(page: GuiPage) -> Self {
        match page {
            GuiPage::Overview => Tab::Overview,
            GuiPage::Deployments => Tab::Deployments,
            GuiPage::Logs => Tab::Logs,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    Copy,
    SelectAll,
}

pub struct CominGui {
    pub active_tab: Tab,
    pub client: CominClient,
    pub state: Option<CominState>,
    pub error: Option<String>,
    pub action_error: Option<String>,
    pub busy_action: Option<String>,
    pub log_view: LogView,
    pub deployments: DeploymentsView,
    /// A `comin status` call is running; polls are skipped until it returns
    /// so that a slow Comin does not pile up processes.
    status_in_flight: bool,
    last_status: Option<Instant>,
}

#[derive(Debug, Clone)]
pub enum Message {
    TabSelected(Tab),
    Overview(OverviewMessage),
    Log(LogMessage),
    Deployments(DeploymentsMessage),
    StatusLoaded(Box<Result<CominState, String>>),
    ActionResult(Result<(), String>),
    /// Another `comin-tray gui` launch asked this window to show a page.
    ShowPage(String),
    ModifiersChanged(Modifiers),
    Shortcut(Shortcut),
    Tick,
}

impl CominGui {
    pub fn new(initial_page: GuiPage) -> (Self, Task<Message>) {
        let mut app = Self {
            active_tab: initial_page.into(),
            client: CominClient::default(),
            state: None,
            error: None,
            action_error: None,
            busy_action: None,
            log_view: LogView::default(),
            deployments: DeploymentsView::default(),
            status_in_flight: false,
            last_status: None,
        };
        let initial_task = app.refresh_status();
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

    fn refresh_status(&mut self) -> Task<Message> {
        if self.status_in_flight {
            return Task::none();
        }
        self.status_in_flight = true;
        let client = self.client.clone();
        Task::perform(
            async move { Box::new(client.status().await.map_err(|e| format!("{e:#}"))) },
            Message::StatusLoaded,
        )
    }

    fn run_action(
        &mut self,
        label: &str,
        action: impl Future<Output = anyhow::Result<()>> + Send + 'static,
    ) -> Task<Message> {
        self.busy_action = Some(label.into());
        self.action_error = None;
        Task::perform(
            async move { action.await.map_err(|e| format!("{e:#}")) },
            Message::ActionResult,
        )
    }

    fn select_tab(&mut self, tab: Tab) -> Task<Message> {
        self.active_tab = tab;
        match (tab, &self.state) {
            (Tab::Deployments, Some(state)) => {
                self.deployments.sync(state).map(Message::Deployments)
            }
            _ => Task::none(),
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TabSelected(tab) => self.select_tab(tab),
            Message::Tick => Task::none(),
            Message::StatusLoaded(boxed_result) => {
                self.status_in_flight = false;
                match *boxed_result {
                    Ok(state) => {
                        self.state = Some(state);
                        self.error = None;
                        self.last_status = Some(Instant::now());
                    }
                    Err(err) => {
                        self.error = Some(err);
                    }
                }
                match (&self.state, self.active_tab) {
                    (Some(state), Tab::Deployments) => {
                        self.deployments.sync(state).map(Message::Deployments)
                    }
                    _ => Task::none(),
                }
            }
            Message::ActionResult(result) => {
                self.busy_action = None;
                match result {
                    Ok(()) => {
                        self.action_error = None;
                        self.refresh_status()
                    }
                    Err(err) => {
                        self.action_error = Some(err);
                        Task::none()
                    }
                }
            }
            Message::Overview(msg) => {
                let client = self.client.clone();
                match msg {
                    OverviewMessage::Refresh => self.refresh_status(),
                    OverviewMessage::OpenDeployments => self.select_tab(Tab::Deployments),
                    OverviewMessage::OpenDeployment(key) => {
                        let select = self
                            .deployments
                            .update(DeploymentsMessage::Select(key), self.state.as_ref())
                            .map(Message::Deployments);
                        Task::batch([select, self.select_tab(Tab::Deployments)])
                    }
                    OverviewMessage::Fetch => {
                        self.run_action("Fetching", async move { client.fetch().await })
                    }
                    OverviewMessage::Suspend => {
                        self.run_action("Suspending GitOps", async move { client.suspend().await })
                    }
                    OverviewMessage::Resume => {
                        self.run_action("Resuming GitOps", async move { client.resume().await })
                    }
                    OverviewMessage::SwitchLatest => self
                        .run_action("Switching to latest live", async move {
                            client.switch_latest().await
                        }),
                    OverviewMessage::RetryLatest => {
                        self.run_action("Retrying deployment", async move {
                            client.retry_latest().await
                        })
                    }
                    OverviewMessage::AcceptConfirmation => self
                        .run_action("Accepting confirmation", async move {
                            client.accept_confirmation().await
                        }),
                }
            }
            Message::Log(LogMessage::OpenDeployment(uuid)) => {
                let Some(state) = &self.state else {
                    return Task::none();
                };
                match self.deployments.select_uuid(&uuid, state) {
                    Some(select) => Task::batch([
                        select.map(Message::Deployments),
                        self.select_tab(Tab::Deployments),
                    ]),
                    None => {
                        self.log_view.notice =
                            Some("That generation is no longer in Comin's history".into());
                        Task::none()
                    }
                }
            }
            Message::Log(log_msg) => {
                let live = match &log_msg {
                    LogMessage::LogReceived(entry) => {
                        self.deployments.live_entry(entry).map(Message::Deployments)
                    }
                    _ => Task::none(),
                };
                Task::batch([live, self.log_view.update(log_msg).map(Message::Log)])
            }
            Message::Deployments(msg) => self
                .deployments
                .update(msg, self.state.as_ref())
                .map(Message::Deployments),
            Message::ShowPage(page) => {
                let tab = GuiPage::parse(&page).map_or(self.active_tab, Tab::from);
                let focus = window::get_latest().and_then(window::gain_focus);
                Task::batch([self.select_tab(tab), focus])
            }
            Message::ModifiersChanged(modifiers) => {
                self.log_view.list.modifiers = modifiers;
                self.deployments.list.modifiers = modifiers;
                Task::none()
            }
            Message::Shortcut(shortcut) => match (self.active_tab, shortcut) {
                (Tab::Logs, Shortcut::Copy) => self
                    .log_view
                    .update(LogMessage::CopySelected)
                    .map(Message::Log),
                (Tab::Logs, Shortcut::SelectAll) => self
                    .log_view
                    .update(LogMessage::SelectAll)
                    .map(Message::Log),
                (Tab::Deployments, Shortcut::Copy) => self
                    .deployments
                    .update(DeploymentsMessage::CopySelected, self.state.as_ref())
                    .map(Message::Deployments),
                (Tab::Deployments, Shortcut::SelectAll) => self
                    .deployments
                    .update(DeploymentsMessage::SelectAll, self.state.as_ref())
                    .map(Message::Deployments),
                (Tab::Overview, _) => Task::none(),
            },
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let status_poll = iced::time::every(Duration::from_secs(3))
            .map(|_| Message::Overview(OverviewMessage::Refresh));

        let tick = iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick);

        let log_stream = iced::stream::channel(100, |mut output| async move {
            logs::stream(&mut output).await;
        });
        let log_sub = Subscription::run_with_id(LOG_STREAM_ID, log_stream)
            .map(|entry| Message::Log(LogMessage::LogReceived(entry)));

        let dbus_service = iced::stream::channel(8, |mut output| async move {
            let (pages, mut requests) = iced::futures::channel::mpsc::channel(8);
            match dbus::serve_gui(dbus::GuiService { pages }).await {
                Ok(_connection) => {
                    while let Some(page) = requests.next().await {
                        if output.send(page).await.is_err() {
                            break;
                        }
                    }
                }
                Err(error) => eprintln!("comin-tray: {error:#}"),
            }
            std::future::pending::<()>().await;
        });
        let dbus_sub =
            Subscription::run_with_id(DBUS_SERVICE_ID, dbus_service).map(Message::ShowPage);

        let keys = event::listen_with(|event, status, _window| match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                Some(Message::ModifiersChanged(modifiers))
            }
            // Some X11 setups never send `ModifiersChanged`, so Shift is also
            // tracked from its own key events for Shift+click selection.
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Shift),
                modifiers,
                ..
            }) => Some(Message::ModifiersChanged(modifiers | Modifiers::SHIFT)),
            Event::Keyboard(keyboard::Event::KeyReleased {
                key: keyboard::Key::Named(keyboard::key::Named::Shift),
                modifiers,
                ..
            }) => Some(Message::ModifiersChanged(modifiers - Modifiers::SHIFT)),
            // Leave shortcuts to a focused text input.
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                if status == event::Status::Ignored && modifiers.command() =>
            {
                match key.as_ref() {
                    keyboard::Key::Character("c") => Some(Message::Shortcut(Shortcut::Copy)),
                    keyboard::Key::Character("a") => Some(Message::Shortcut(Shortcut::SelectAll)),
                    _ => None,
                }
            }
            _ => None,
        });

        Subscription::batch([status_poll, tick, log_sub, dbus_sub, keys])
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
            Tab::Deployments => self
                .deployments
                .view(self.state.as_ref())
                .map(Message::Deployments),
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
        let tabs = segmented(
            [
                ("Overview", Tab::Overview),
                ("Deployments", Tab::Deployments),
                ("Logs", Tab::Logs),
            ]
            .into_iter()
            .map(|(label, tab)| {
                tab_button(label, self.active_tab == tab, Message::TabSelected(tab))
            })
            .collect(),
        );

        let freshness = match self.last_status {
            Some(last) if last.elapsed() > STALE_AFTER => text(format!(
                "Comin is not answering; data from {}s ago",
                last.elapsed().as_secs()
            ))
            .size(12)
            .color(BREEZE_WARNING),
            None if self.error.is_some() => text("Comin is not answering")
                .size(12)
                .color(BREEZE_WARNING),
            Some(last) => text(format!("Updated {}s ago", last.elapsed().as_secs()))
                .size(12)
                .color(BREEZE_TEXT_MUTED),
            None => text("Loading…").size(12).color(BREEZE_TEXT_MUTED),
        };

        row![tabs, horizontal_space(), freshness]
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
    // One window per session: when a window is already open, ask it to show
    // the requested page instead of opening a second one.
    let already_running = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()
        .and_then(|runtime| {
            runtime
                .block_on(dbus::show_running_gui(initial_page.as_str()))
                .ok()
        })
        .unwrap_or(false);
    if already_running {
        eprintln!("Comin is already open; showing its window.");
        return Ok(());
    }

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
