//! The Deployments page: every deployment and generation on the left, and
//! the details, stage timeline, build summary and raw log of the selected
//! one on the right.

use iced::{
    Alignment, Background, Border, Element, Fill, Font, Length, Task, Theme,
    border::Radius,
    widget::{
        Column, Row, button, checkbox, column, container, horizontal_space, row, scrollable, text,
        text::Wrapping,
    },
};
use time::OffsetDateTime;

use crate::{
    build_log::{BuildLog, DerivationState, Stage, StageStatus, display_message},
    format::{
        commit_title, format_clock, format_duration, format_local_datetime,
        format_relative_time_now, parse_rfc3339, short_commit, short_store_path,
    },
    gui::{
        log_list::{LogList, LogListMessage, save_log},
        theme::{
            BREEZE_ACCENT, BREEZE_BG_CARD, BREEZE_BG_HOVER, BREEZE_BORDER_SUBTLE, BREEZE_DANGER,
            BREEZE_SUCCESS, BREEZE_TEXT, BREEZE_TEXT_DIM, BREEZE_TEXT_MUTED, BREEZE_WARNING,
        },
        widgets::{
            BannerKind, banner, card, retention_badges, segmented, small_button, status_chip,
            tab_button,
        },
    },
    logs::{self, LogEntry, to_copy_text},
    model::{CominState, HistoryItem},
};

const LIST_WIDTH: f32 = 380.0;
const RAW_LOG_HEIGHT: f32 = 420.0;

#[derive(Debug)]
enum LogState {
    Idle,
    Loading {
        key: String,
        live: bool,
    },
    Loaded {
        key: String,
        live: bool,
        since: OffsetDateTime,
        log: Box<BuildLog>,
    },
    Failed {
        key: String,
        live: bool,
        error: String,
    },
}

impl LogState {
    fn key_and_live(&self) -> Option<(&str, bool)> {
        match self {
            LogState::Idle => None,
            LogState::Loading { key, live }
            | LogState::Loaded { key, live, .. }
            | LogState::Failed { key, live, .. } => Some((key.as_str(), *live)),
        }
    }
}

#[derive(Debug)]
pub struct DeploymentsView {
    pub selected: Option<String>,
    log: LogState,
    pub list: LogList,
    pub stage: Option<Stage>,
    pub hide_noise: bool,
    pub show_derivations: bool,
    pub notice: Option<String>,
}

impl Default for DeploymentsView {
    fn default() -> Self {
        let mut list = LogList::default();
        list.auto_scroll = false;
        Self {
            selected: None,
            log: LogState::Idle,
            list,
            stage: None,
            hide_noise: true,
            show_derivations: false,
            notice: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum DeploymentsMessage {
    Select(String),
    Loaded {
        key: String,
        live: bool,
        since: OffsetDateTime,
        result: Result<Vec<LogEntry>, String>,
    },
    Reload,
    Stage(Option<Stage>),
    HideNoise(bool),
    RangeMode(bool),
    Colors(bool),
    ToggleDerivations,
    List(LogListMessage),
    CopyValue(String),
    CopySelected,
    CopyAll,
    SelectAll,
    Save,
    Saved(Result<String, String>),
}

impl DeploymentsView {
    pub fn update(
        &mut self,
        message: DeploymentsMessage,
        state: Option<&CominState>,
    ) -> Task<DeploymentsMessage> {
        match message {
            DeploymentsMessage::Select(key) => {
                if self.selected.as_deref() == Some(key.as_str()) {
                    return Task::none();
                }
                self.selected = Some(key);
                self.list.clear();
                self.list.auto_scroll = false;
                self.notice = None;
                self.log = LogState::Idle;
                state.map_or_else(Task::none, |state| self.sync(state))
            }
            DeploymentsMessage::Reload => {
                self.log = LogState::Idle;
                state.map_or_else(Task::none, |state| self.sync(state))
            }
            DeploymentsMessage::Loaded {
                key,
                live,
                since,
                result,
            } => {
                // Ignore answers for an item that is no longer selected.
                if self.log.key_and_live() != Some((key.as_str(), live)) {
                    return Task::none();
                }
                self.log = match result {
                    Ok(entries) => LogState::Loaded {
                        key,
                        live,
                        since,
                        log: Box::new(BuildLog::from_entries(entries, live)),
                    },
                    Err(error) => LogState::Failed { key, live, error },
                };
                if live {
                    self.list.auto_scroll = true;
                    let total = self.visible_count();
                    return self.list.follow(total).map(DeploymentsMessage::List);
                }
                Task::none()
            }
            DeploymentsMessage::Stage(stage) => {
                self.stage = stage;
                self.list.clear_selection();
                Task::none()
            }
            DeploymentsMessage::HideNoise(hide) => {
                self.hide_noise = hide;
                self.list.clear_selection();
                Task::none()
            }
            DeploymentsMessage::Colors(enabled) => {
                self.list.colors = enabled;
                Task::none()
            }
            DeploymentsMessage::RangeMode(enabled) => {
                self.list.range_mode = enabled;
                Task::none()
            }
            DeploymentsMessage::ToggleDerivations => {
                self.show_derivations = !self.show_derivations;
                Task::none()
            }
            DeploymentsMessage::List(message) => {
                self.list.update(message);
                Task::none()
            }
            DeploymentsMessage::CopyValue(value) => {
                self.notice = Some("Copied to the clipboard".into());
                iced::clipboard::write(value)
            }
            DeploymentsMessage::SelectAll => {
                let ids = self.visible_ids();
                self.list.select_all(&ids);
                Task::none()
            }
            DeploymentsMessage::CopySelected => {
                let text = self.selected_text();
                if text.is_empty() {
                    return Task::none();
                }
                self.notice = Some(format!("Copied {} lines", text.lines().count()));
                iced::clipboard::write(text)
            }
            DeploymentsMessage::CopyAll => {
                let text = self.visible_text();
                self.notice = Some(format!("Copied {} lines", text.lines().count()));
                iced::clipboard::write(text)
            }
            DeploymentsMessage::Save => {
                let stem = self
                    .selected
                    .as_deref()
                    .map(|key| format!("comin-deployment-{}", &key[..key.len().min(8)]))
                    .unwrap_or_else(|| "comin-deployment".into());
                Task::perform(
                    save_log(stem, self.visible_text()),
                    DeploymentsMessage::Saved,
                )
            }
            DeploymentsMessage::Saved(result) => {
                self.notice = Some(match result {
                    Ok(path) => format!("Saved to {path}"),
                    Err(error) => error,
                });
                Task::none()
            }
        }
    }

    /// Keeps the selection valid and the log loaded after a status refresh:
    /// selects the newest item when nothing is selected, and reloads the log
    /// when the selected item starts or stops being in progress.
    pub fn sync(&mut self, state: &CominState) -> Task<DeploymentsMessage> {
        let history = state.history();
        let selected = self
            .selected
            .as_deref()
            .and_then(|key| history.iter().find(|item| item.key == key))
            .or_else(|| history.first());
        let Some(item) = selected else {
            self.selected = None;
            self.log = LogState::Idle;
            return Task::none();
        };
        self.selected = Some(item.key.clone());

        if self.log.key_and_live() == Some((item.key.as_str(), item.active)) {
            return Task::none();
        }
        let Some((since, until)) = item.log_window() else {
            self.log = LogState::Failed {
                key: item.key.clone(),
                live: item.active,
                error: "Comin did not record when this generation started.".into(),
            };
            return Task::none();
        };

        let key = item.key.clone();
        let live = item.active;
        self.log = LogState::Loading {
            key: key.clone(),
            live,
        };
        Task::perform(
            async move {
                logs::read_range(since, until).await.map(|entries| {
                    entries
                        .into_iter()
                        .filter(|entry| {
                            entry.time.is_some_and(|time| {
                                time >= since && until.is_none_or(|until| time <= until)
                            })
                        })
                        .collect()
                })
            },
            move |result| DeploymentsMessage::Loaded {
                key: key.clone(),
                live,
                since,
                result,
            },
        )
    }

    /// Selects the item whose deployment or generation is `uuid`. Returns
    /// `false` when no item matches.
    pub fn select_uuid(
        &mut self,
        uuid: &str,
        state: &CominState,
    ) -> Option<Task<DeploymentsMessage>> {
        let key = state
            .history()
            .into_iter()
            .find(|item| item.matches(uuid))?
            .key;
        Some(self.update(DeploymentsMessage::Select(key), Some(state)))
    }

    /// Feeds a line from the live journal stream into the log of the
    /// selected item while Comin is still working on it.
    pub fn live_entry(&mut self, entry: &LogEntry) -> Task<DeploymentsMessage> {
        let LogState::Loaded {
            live: true,
            since,
            log,
            ..
        } = &mut self.log
        else {
            return Task::none();
        };
        if entry.time.is_none_or(|time| time < *since) {
            return Task::none();
        }
        let mut entries = std::mem::take(&mut log.entries);
        entries.push(entry.clone());
        **log = BuildLog::from_entries(entries, true);
        let total = self.visible_count();
        self.list.follow(total).map(DeploymentsMessage::List)
    }

    fn build_log(&self) -> Option<&BuildLog> {
        match &self.log {
            LogState::Loaded { log, .. } => Some(log),
            _ => None,
        }
    }

    fn visible_lines(&self) -> Vec<usize> {
        self.build_log()
            .map(|log| log.visible_lines(self.stage, self.hide_noise))
            .unwrap_or_default()
    }

    fn visible_count(&self) -> usize {
        self.visible_lines().len()
    }

    fn visible_ids(&self) -> Vec<u64> {
        self.visible_lines()
            .into_iter()
            .map(|index| index as u64)
            .collect()
    }

    fn visible_text(&self) -> String {
        let Some(log) = self.build_log() else {
            return String::new();
        };
        to_copy_text(
            self.visible_lines()
                .into_iter()
                .map(|index| &log.entries[index]),
        )
    }

    /// The selected raw log lines, formatted for the clipboard.
    pub fn selected_text(&self) -> String {
        let Some(log) = self.build_log() else {
            return String::new();
        };
        let lines = self.visible_lines();
        let ids: Vec<u64> = lines.iter().map(|&index| index as u64).collect();
        let range = self.list.selected_positions(&ids);
        to_copy_text(lines[range].iter().map(|&index| &log.entries[index]))
    }

    pub fn view<'a>(&'a self, state: Option<&'a CominState>) -> Element<'a, DeploymentsMessage> {
        let Some(state) = state else {
            return container(
                text("Waiting for Comin status…")
                    .size(14)
                    .color(BREEZE_TEXT_MUTED),
            )
            .padding(24)
            .into();
        };

        let history = state.history();
        let selected = self
            .selected
            .as_deref()
            .and_then(|key| history.iter().find(|item| item.key == key));

        let list = history_list(&history, self.selected.as_deref());
        let detail: Element<'a, DeploymentsMessage> = match selected {
            Some(item) => self.detail(item, state),
            None => container(
                text("No deployments recorded yet.")
                    .size(14)
                    .color(BREEZE_TEXT_MUTED),
            )
            .padding(24)
            .into(),
        };

        row![
            container(list)
                .width(Length::Fixed(LIST_WIDTH))
                .height(Fill),
            container(detail).width(Fill).height(Fill),
        ]
        .spacing(12)
        .padding([4, 16])
        .height(Fill)
        .into()
    }

    fn detail<'a>(
        &'a self,
        item: &HistoryItem<'a>,
        state: &'a CominState,
    ) -> Element<'a, DeploymentsMessage> {
        let mut content = Column::new().spacing(14).padding([0, 12]).width(Fill);
        content = content.push(detail_header(item, state));

        for error in item_errors(item) {
            content = content.push(banner(error, BannerKind::Error, None));
        }

        let log = self.build_log();
        if let Some(log) = log {
            for &index in log.errors.iter().take(3) {
                content = content.push(
                    text(log.entries[index].message.as_str())
                        .size(12)
                        .font(Font::MONOSPACE)
                        .color(BREEZE_DANGER),
                );
            }
        }

        content = content.push(card("Stages", None, stages(item, log)));
        content = content.push(card("Details", None, details(item)));
        if let Some(log) =
            log.filter(|log| !log.derivations.is_empty() || !log.downloads.is_empty())
        {
            content = content.push(self.build_summary(log));
        }
        content = content.push(card("Log", None, self.raw_log()));

        scrollable(content).width(Fill).height(Fill).into()
    }

    fn build_summary<'a>(&'a self, log: &'a BuildLog) -> Element<'a, DeploymentsMessage> {
        let toggle = small_button(
            if self.show_derivations {
                "Hide"
            } else {
                "Show"
            },
            Some(DeploymentsMessage::ToggleDerivations),
        );
        let summary = text(format!(
            "{} derivations built · {} store paths fetched",
            log.derivations.len(),
            log.downloads.len()
        ))
        .size(13)
        .color(BREEZE_TEXT_DIM);

        let mut body = Column::new()
            .spacing(6)
            .push(row![summary, horizontal_space(), toggle].align_y(Alignment::Center));

        if self.show_derivations {
            for derivation in &log.derivations {
                let (icon, color) = match derivation.state {
                    DerivationState::Built => ("✓", BREEZE_SUCCESS),
                    DerivationState::Building => ("◐", BREEZE_ACCENT),
                    DerivationState::Failed => ("✗", BREEZE_DANGER),
                };
                body = body.push(
                    row![
                        text(icon).size(12).color(color).width(Length::Fixed(16.0)),
                        text(derivation.started.map(format_clock).unwrap_or_default())
                            .size(12)
                            .font(Font::MONOSPACE)
                            .color(BREEZE_TEXT_MUTED)
                            .width(Length::Fixed(64.0)),
                        text("build").size(12).color(BREEZE_TEXT_MUTED).width(48),
                        text(derivation.name.as_str())
                            .size(12)
                            .font(Font::MONOSPACE)
                            .color(BREEZE_TEXT),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
            for download in &log.downloads {
                let cache = download
                    .cache
                    .trim_start_matches("https://")
                    .trim_start_matches("http://");
                body = body.push(
                    row![
                        text("↓")
                            .size(12)
                            .color(BREEZE_ACCENT)
                            .width(Length::Fixed(16.0)),
                        text(download.started.map(format_clock).unwrap_or_default())
                            .size(12)
                            .font(Font::MONOSPACE)
                            .color(BREEZE_TEXT_MUTED)
                            .width(Length::Fixed(64.0)),
                        text("fetch").size(12).color(BREEZE_TEXT_MUTED).width(48),
                        text(download.name.as_str())
                            .size(12)
                            .font(Font::MONOSPACE)
                            .color(BREEZE_TEXT),
                        text(format!("from {cache}"))
                            .size(12)
                            .color(BREEZE_TEXT_MUTED),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
        }

        card("Build summary", None, body.into())
    }

    fn raw_log(&self) -> Element<'_, DeploymentsMessage> {
        let stage_filter = segmented(
            [
                ("All", None),
                ("Evaluate", Some(Stage::Evaluate)),
                ("Build", Some(Stage::Build)),
                ("Deploy", Some(Stage::Deploy)),
            ]
            .into_iter()
            .map(|(label, stage)| {
                tab_button(label, self.stage == stage, DeploymentsMessage::Stage(stage))
            })
            .collect(),
        );

        let lines = self.visible_lines();
        let has_lines = !lines.is_empty();
        let filters = row![
            stage_filter,
            checkbox("Hide noise", self.hide_noise)
                .on_toggle(DeploymentsMessage::HideNoise)
                .size(16)
                .text_size(13),
            checkbox("Colors", self.list.colors)
                .on_toggle(DeploymentsMessage::Colors)
                .size(16)
                .text_size(13),
            checkbox("Select range", self.list.range_mode)
                .on_toggle(DeploymentsMessage::RangeMode)
                .size(16)
                .text_size(13),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let actions = row![
            small_button(
                "Copy selected",
                self.list
                    .has_selection()
                    .then_some(DeploymentsMessage::CopySelected)
            ),
            small_button("Copy all", has_lines.then_some(DeploymentsMessage::CopyAll)),
            small_button(
                "Save to file",
                has_lines.then_some(DeploymentsMessage::Save)
            ),
            small_button("Reload", Some(DeploymentsMessage::Reload)),
            text(
                "Click a line to select it; Shift+click or Select range extends it. Ctrl+C copies."
            )
            .size(12)
            .color(BREEZE_TEXT_MUTED),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let toolbar = column![filters, actions].spacing(8);

        let mut body = Column::new().spacing(10).push(toolbar);
        if let Some(notice) = &self.notice {
            body = body.push(text(notice.as_str()).size(12).color(BREEZE_TEXT_MUTED));
        }

        body = body.push(match &self.log {
            LogState::Idle | LogState::Loading { .. } => muted("Loading the journal…"),
            LogState::Failed { error, .. } => muted(error.as_str()),
            LogState::Loaded { log, .. } if log.is_empty() => muted(
                "The journal has no records for this deployment. They may have been rotated out.",
            ),
            LogState::Loaded { log, .. } => {
                let rows = lines
                    .iter()
                    .map(|&index| {
                        let entry = &log.entries[index];
                        crate::gui::log_list::Row {
                            id: index as u64,
                            entry,
                            text: display_message(entry),
                            kind: &log.kinds[index],
                        }
                    })
                    .collect();
                self.list
                    .view(rows, Length::Fixed(RAW_LOG_HEIGHT))
                    .map(DeploymentsMessage::List)
            }
        });

        body.into()
    }
}

fn muted<'a>(message: &'a str) -> Element<'a, DeploymentsMessage> {
    text(message).size(13).color(BREEZE_TEXT_MUTED).into()
}

fn history_list<'a>(
    history: &[HistoryItem<'a>],
    selected: Option<&str>,
) -> Element<'a, DeploymentsMessage> {
    if history.is_empty() {
        return muted("No deployments recorded yet.");
    }

    let mut list = Column::new()
        .spacing(4)
        .width(Fill)
        .padding(iced::Padding::ZERO.right(10));
    for item in history {
        let is_selected = selected == Some(item.key.as_str());
        let title = item
            .commit_msg()
            .map(commit_title)
            .unwrap_or("(no commit message)");
        let when = if item.active {
            "in progress".to_string()
        } else {
            let key = item.time_key();
            if key.is_empty() {
                "—".into()
            } else {
                format_relative_time_now(key)
            }
        };

        let mut chips = Row::new().spacing(6).align_y(Alignment::Center);
        chips = chips.push(status_chip(item.status()));
        if let Some(operation) = item.operation() {
            chips = chips.push(status_chip(operation));
        }
        chips = chips.push(horizontal_space());
        chips = chips.push(
            text(
                item.commit_id()
                    .map(short_commit)
                    .unwrap_or("—")
                    .to_string(),
            )
            .size(12)
            .font(Font::MONOSPACE)
            .color(BREEZE_TEXT_DIM),
        );

        let content = column![
            chips,
            text(title.to_string())
                .size(13)
                .color(BREEZE_TEXT)
                .wrapping(Wrapping::None),
            text(when).size(12).color(BREEZE_TEXT_MUTED),
        ]
        .spacing(6);

        list = list.push(
            button(content)
                .on_press(DeploymentsMessage::Select(item.key.clone()))
                .width(Fill)
                .padding([10, 12])
                .style(move |_theme: &Theme, status: button::Status| {
                    let hovered = matches!(status, button::Status::Hovered);
                    button::Style {
                        background: Some(Background::Color(if is_selected || hovered {
                            BREEZE_BG_HOVER
                        } else {
                            BREEZE_BG_CARD
                        })),
                        text_color: BREEZE_TEXT,
                        border: Border {
                            color: if is_selected {
                                BREEZE_ACCENT
                            } else {
                                BREEZE_BORDER_SUBTLE
                            },
                            width: 1.0,
                            radius: Radius::from(6.0),
                        },
                        ..Default::default()
                    }
                }),
        );
    }

    scrollable(list).height(Fill).into()
}

fn detail_header<'a>(
    item: &HistoryItem<'a>,
    state: &'a CominState,
) -> Element<'a, DeploymentsMessage> {
    let title = item
        .commit_msg()
        .map(commit_title)
        .unwrap_or("(no commit message)");
    let mut chips = Row::new().spacing(8).align_y(Alignment::Center);
    chips = chips.push(status_chip(item.status()));
    if let Some(operation) = item.operation() {
        chips = chips.push(status_chip(operation));
    }
    if let Some(deployment) = item.deployment {
        chips = chips.push(retention_badges(deployment, &state.store));
    }
    if let Some(when) = parse_rfc3339(item.time_key()) {
        chips = chips.push(
            text(format_local_datetime(when))
                .size(12)
                .color(BREEZE_TEXT_MUTED),
        );
    }

    column![text(title.to_string()).size(18).color(BREEZE_TEXT), chips]
        .spacing(8)
        .into()
}

fn item_errors(item: &HistoryItem<'_>) -> Vec<String> {
    let mut errors = Vec::new();
    if let Some(generation) = item.generation {
        if let Some(error) = generation.eval_err.as_deref().filter(|e| !e.is_empty()) {
            errors.push(format!("Evaluation failed: {error}"));
        }
        if let Some(error) = generation.build_err.as_deref().filter(|e| !e.is_empty()) {
            errors.push(format!("Build failed: {error}"));
        }
    }
    if let Some(error) = item
        .deployment
        .and_then(|d| d.error_msg.as_deref())
        .filter(|e| !e.is_empty())
    {
        errors.push(format!("Deployment failed: {error}"));
    }
    errors
}

/// One row per stage: status icon, name, duration, result and start time.
fn stages<'a>(
    item: &HistoryItem<'a>,
    log: Option<&'a BuildLog>,
) -> Element<'a, DeploymentsMessage> {
    let generation = item.generation;
    let deployment = item.deployment;

    let mut column = Column::new().spacing(8);
    for stage in Stage::ALL {
        let (started, ended) = match stage {
            Stage::Evaluate => (
                generation.and_then(|g| g.eval_started_at.as_deref()),
                generation.and_then(|g| g.eval_ended_at.as_deref()),
            ),
            Stage::Build => (
                generation.and_then(|g| g.build_started_at.as_deref()),
                generation.and_then(|g| g.build_ended_at.as_deref()),
            ),
            Stage::Deploy => (
                deployment.and_then(|d| d.started_at.as_deref()),
                deployment.and_then(|d| d.ended_at.as_deref()),
            ),
        };
        let started = started.and_then(parse_rfc3339);
        let ended = ended.and_then(parse_rfc3339);
        let summary = log.map(|log| log.stage(stage));

        let status = stage_status(stage, item, summary.map(|summary| summary.status));
        let duration = match (started, ended) {
            (Some(started), Some(ended)) => Some(ended - started),
            (Some(started), None) if status == StageStatus::Running => {
                Some(OffsetDateTime::now_utc() - started)
            }
            _ => summary.and_then(|summary| summary.duration()),
        };
        let started = started.or_else(|| summary.and_then(|summary| summary.start));

        let mut detail = log.map(|log| log.stage_detail(stage)).unwrap_or_default();
        if detail.is_empty() {
            detail = match stage {
                Stage::Build => generation
                    .and_then(|g| g.build_reason.clone())
                    .unwrap_or_default(),
                Stage::Deploy => deployment
                    .and_then(|d| d.reason.clone())
                    .unwrap_or_default(),
                Stage::Evaluate => String::new(),
            };
        }

        let color = match status {
            StageStatus::Done => BREEZE_SUCCESS,
            StageStatus::Running => BREEZE_ACCENT,
            StageStatus::Failed => BREEZE_DANGER,
            StageStatus::Skipped => BREEZE_WARNING,
            StageStatus::NotReached => BREEZE_TEXT_MUTED,
        };

        column = column.push(
            row![
                text(status.icon())
                    .size(15)
                    .color(color)
                    .width(Length::Fixed(20.0)),
                text(stage.label())
                    .size(13)
                    .color(BREEZE_TEXT)
                    .width(Length::Fixed(80.0)),
                text(duration.map(format_duration).unwrap_or_default())
                    .size(13)
                    .font(Font::MONOSPACE)
                    .color(BREEZE_TEXT_DIM)
                    .width(Length::Fixed(70.0)),
                text(detail).size(13).color(BREEZE_TEXT_DIM).width(Fill),
                text(started.map(format_clock).unwrap_or_default())
                    .size(12)
                    .font(Font::MONOSPACE)
                    .color(BREEZE_TEXT_MUTED),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        );
    }
    column.into()
}

/// A stage's status from Comin's own state, refined by the log when it has
/// something more specific (running, failed, skipped).
fn stage_status(
    stage: Stage,
    item: &HistoryItem<'_>,
    from_log: Option<StageStatus>,
) -> StageStatus {
    let generation = item.generation;
    let from_state = match stage {
        Stage::Evaluate => generation.map(|g| match g.eval_status.as_deref() {
            Some("failed") => StageStatus::Failed,
            Some("evaluating") => StageStatus::Running,
            _ if g.eval_err.as_deref().is_some_and(|e| !e.is_empty()) => StageStatus::Failed,
            _ if g.eval_ended_at.is_some() => StageStatus::Done,
            _ if g.eval_started_at.is_some() && item.active => StageStatus::Running,
            _ => StageStatus::NotReached,
        }),
        Stage::Build => generation.map(|g| match g.build_status.as_deref() {
            Some("failed") => StageStatus::Failed,
            Some("building") => StageStatus::Running,
            _ if g.build_err.as_deref().is_some_and(|e| !e.is_empty()) => StageStatus::Failed,
            _ if g.build_ended_at.is_some() => StageStatus::Done,
            _ if g.build_started_at.is_some() && item.active => StageStatus::Running,
            _ => StageStatus::NotReached,
        }),
        Stage::Deploy => item.deployment.map(|d| {
            if d.error_msg.as_deref().is_some_and(|e| !e.is_empty())
                || d.status.as_deref() == Some("failed")
            {
                StageStatus::Failed
            } else if item.active {
                StageStatus::Running
            } else if d.ended_at.is_some() || d.status.as_deref() == Some("done") {
                StageStatus::Done
            } else {
                StageStatus::NotReached
            }
        }),
    };

    match (from_state, from_log) {
        (Some(StageStatus::Failed), _) | (_, Some(StageStatus::Failed)) => StageStatus::Failed,
        (_, Some(StageStatus::Skipped)) => StageStatus::Skipped,
        (Some(status), _) if status != StageStatus::NotReached => status,
        (_, Some(status)) => status,
        (Some(status), None) => status,
        (None, None) => StageStatus::NotReached,
    }
}

/// Key/value rows with a copy button for every value.
fn details<'a>(item: &HistoryItem<'a>) -> Element<'a, DeploymentsMessage> {
    let generation = item.generation;
    let deployment = item.deployment;
    let mut rows: Vec<(&'static str, String)> = Vec::new();

    let mut push = |label: &'static str, value: Option<&str>| {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            rows.push((label, value.to_string()));
        }
    };

    push("Commit", item.commit_id());
    if let Some(g) = generation {
        let source = match (
            g.selected_remote_name.as_deref(),
            g.selected_branch_name.as_deref(),
        ) {
            (Some(remote), Some(branch)) => Some(format!("{remote}/{branch}")),
            _ => None,
        };
        push("Branch", source.as_deref());
    }
    push("Message", item.commit_msg().map(str::trim));
    if let Some(d) = deployment {
        push("Deployment", Some(d.uuid.as_str()));
        if d.operation_submitted.is_some() && d.operation_submitted != d.operation {
            push("Submitted as", d.operation_submitted.as_deref());
        }
        push("Reason", d.reason.as_deref());
        push("Profile", d.profile_path.as_deref());
    }
    if let Some(g) = generation {
        push("Generation", g.uuid.as_deref());
        push("Derivation", g.drv_path.as_deref());
        push("Output", g.out_path.as_deref());
    }

    let mut column = Column::new().spacing(6);
    for (label, value) in rows {
        let shown = match label {
            "Derivation" | "Output" => short_store_path(&value).to_string(),
            _ => value.clone(),
        };
        let multiline = shown.contains('\n');
        column = column.push(
            row![
                text(label)
                    .size(12)
                    .color(BREEZE_TEXT_MUTED)
                    .width(Length::Fixed(92.0)),
                text(shown)
                    .size(12)
                    .font(if multiline {
                        Font::DEFAULT
                    } else {
                        Font::MONOSPACE
                    })
                    .color(BREEZE_TEXT)
                    .width(Fill),
                small_button("Copy", Some(DeploymentsMessage::CopyValue(value))),
            ]
            .spacing(10)
            .align_y(if multiline {
                Alignment::Start
            } else {
                Alignment::Center
            }),
        );
    }
    column.into()
}
