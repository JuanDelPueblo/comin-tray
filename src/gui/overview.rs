use iced::{
    Alignment, Background, Border, Element, Fill, Font, Length, Theme,
    border::Radius,
    widget::{Column, Row, button, column, container, horizontal_space, row, scrollable, text},
};

use crate::{
    format::{commit_title, format_relative_time_now, short_commit, short_store_path},
    gui::{
        theme::{
            BREEZE_BG_CARD, BREEZE_BG_HOVER, BREEZE_BORDER_SUBTLE, BREEZE_DANGER, BREEZE_TEXT,
            BREEZE_TEXT_DIM, BREEZE_TEXT_MUTED, BREEZE_WARNING, primary_button_style,
            secondary_button_style, success_button_style,
        },
        widgets::{BannerKind, banner, card, card_with_height, retention_badges, status_chip},
    },
    model::{CominState, Phase},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OverviewMessage {
    /// Show the Deployments page.
    OpenDeployments,
    /// Show one deployment or generation on the Deployments page.
    OpenDeployment(String),
    Fetch,
    Suspend,
    Resume,
    SwitchLatest,
    RetryLatest,
    AcceptConfirmation,
    Refresh,
}

pub fn view<'a>(
    state: Option<&'a CominState>,
    error: Option<&'a str>,
    action_error: Option<&'a str>,
    busy_action: Option<&'a str>,
) -> Element<'a, OverviewMessage> {
    let mut content = Column::new().spacing(16).padding(20).width(Fill);

    // 1. Header (Hostname + Overall Badges + Refresh)
    content = content.push(header_section(state, busy_action.is_some()));

    // 2. Important State Banners
    if let Some(err) = error {
        content = content.push(banner(
            format!("Connection error: {err}"),
            BannerKind::Error,
            Some(("Retry", OverviewMessage::Refresh)),
        ));
    }

    if let Some(action_err) = action_error {
        content = content.push(banner(
            format!("Action failed: {action_err}"),
            BannerKind::Error,
            None,
        ));
    }

    if let Some(s) = state {
        if s.confirmation_needed() {
            content = content.push(banner(
                "Confirmation required: A generation is waiting for deployment approval.",
                BannerKind::Warning,
                Some(("Accept confirmation", OverviewMessage::AcceptConfirmation)),
            ));
        }

        if let Some(reason) = s.reboot_reason() {
            content = content.push(banner(
                format!("Reboot required: {reason}"),
                BannerKind::Warning,
                None,
            ));
        }

        if s.is_suspended.unwrap_or(false) {
            content = content.push(banner(
                "GitOps is currently suspended. Automatic polling and deployments are paused.",
                BannerKind::Warning,
                Some(("Resume GitOps", OverviewMessage::Resume)),
            ));
        }
    }

    // 3. Action Bar
    content = content.push(action_bar(state, busy_action));

    if let Some(s) = state {
        // 4. Cockpit-style Lifecycle Overview
        content = content.push(lifecycle_overview(s));

        // 5. Detailed Cards: Fetcher, Builder, Deployer in aligned grid
        content = content.push(
            row![
                card_with_height(
                    "Fetcher",
                    None,
                    fetcher_card_content(s),
                    Length::Fixed(240.0)
                ),
                card_with_height(
                    "Builder",
                    None,
                    builder_card_content(s),
                    Length::Fixed(240.0)
                ),
                card_with_height(
                    "Deployer",
                    None,
                    deployer_card_content(s),
                    Length::Fixed(240.0)
                ),
            ]
            .spacing(14)
            .width(Fill),
        );

        // 6. Recent Deployments Table
        content = content.push(recent_deployments_card(s));
    }

    scrollable(content).width(Fill).height(Fill).into()
}

fn header_section<'a>(
    state: Option<&'a CominState>,
    is_busy: bool,
) -> Element<'a, OverviewMessage> {
    let hostname = state
        .and_then(CominState::hostname)
        .unwrap_or("Local machine");

    let phase = state.map(CominState::phase).unwrap_or(Phase::Unavailable);

    let mut badges = Row::new().spacing(8).align_y(Alignment::Center);
    badges = badges.push(status_chip(phase.label()));

    if let Some(s) = state {
        // Avoid duplicate chip if phase is already RebootRequired
        if s.need_to_reboot.unwrap_or(false) && phase != Phase::RebootRequired {
            badges = badges.push(status_chip("Reboot required"));
        }
        if s.confirmation_needed() {
            badges = badges.push(status_chip("Confirmation required"));
        }
    }

    let refresh_button = button(text("Refresh").size(13))
        .on_press_maybe(if is_busy {
            None
        } else {
            Some(OverviewMessage::Refresh)
        })
        .style(secondary_button_style)
        .padding([6, 14]);

    row![
        column![
            text("Comin GitOps").size(13).color(BREEZE_TEXT_MUTED),
            row![text(hostname).size(22).color(BREEZE_TEXT), badges]
                .spacing(12)
                .align_y(Alignment::Center),
        ]
        .spacing(4),
        horizontal_space(),
        refresh_button,
    ]
    .align_y(Alignment::Center)
    .width(Fill)
    .into()
}

fn action_bar<'a>(
    state: Option<&'a CominState>,
    busy_action: Option<&'a str>,
) -> Element<'a, OverviewMessage> {
    let is_busy = busy_action.is_some();
    let is_suspended = state.is_some_and(|s| s.is_suspended.unwrap_or(false));
    let can_switch = state.is_some_and(CominState::can_switch_latest);
    let can_retry = state.is_some_and(CominState::can_retry_deployment);
    let can_confirm = state.is_some_and(CominState::confirmation_needed);

    let mut actions = Row::new().spacing(10).align_y(Alignment::Center);

    // Fetch now
    actions = actions.push(
        button(text("Fetch now").size(13))
            .on_press_maybe(if is_busy {
                None
            } else {
                Some(OverviewMessage::Fetch)
            })
            .style(primary_button_style)
            .padding([7, 14]),
    );

    // Suspend / Resume GitOps
    if is_suspended {
        actions = actions.push(
            button(text("Resume GitOps").size(13))
                .on_press_maybe(if is_busy {
                    None
                } else {
                    Some(OverviewMessage::Resume)
                })
                .style(secondary_button_style)
                .padding([7, 14]),
        );
    } else {
        actions = actions.push(
            button(text("Suspend GitOps").size(13))
                .on_press_maybe(if is_busy {
                    None
                } else {
                    Some(OverviewMessage::Suspend)
                })
                .style(secondary_button_style)
                .padding([7, 14]),
        );
    }

    // Switch live now (conditional)
    if can_switch {
        actions = actions.push(
            button(text("Switch live now").size(13))
                .on_press_maybe(if is_busy {
                    None
                } else {
                    Some(OverviewMessage::SwitchLatest)
                })
                .style(success_button_style)
                .padding([7, 14]),
        );
    }

    // Retry deployment (conditional)
    if can_retry {
        actions = actions.push(
            button(text("Retry deployment").size(13))
                .on_press_maybe(if is_busy {
                    None
                } else {
                    Some(OverviewMessage::RetryLatest)
                })
                .style(secondary_button_style)
                .padding([7, 14]),
        );
    }

    // Accept confirmation (conditional)
    if can_confirm {
        actions = actions.push(
            button(text("Accept confirmation").size(13))
                .on_press_maybe(if is_busy {
                    None
                } else {
                    Some(OverviewMessage::AcceptConfirmation)
                })
                .style(success_button_style)
                .padding([7, 14]),
        );
    }

    if let Some(busy) = busy_action {
        actions = actions.push(
            text(format!("Running: {busy}..."))
                .size(13)
                .color(BREEZE_WARNING),
        );
    }

    actions.into()
}

fn lifecycle_overview<'a>(state: &'a CominState) -> Element<'a, OverviewMessage> {
    let source_col = lifecycle_source(state);
    let fetch_col = lifecycle_fetch(state);
    let eval_col = lifecycle_eval(state);
    let build_col = lifecycle_build(state);
    let deploy_col = lifecycle_deploy(state);

    card(
        "Lifecycle Overview",
        None,
        row![
            container(source_col).width(Length::FillPortion(2)),
            container(fetch_col).width(Length::FillPortion(1)),
            container(eval_col).width(Length::FillPortion(1)),
            container(build_col).width(Length::FillPortion(1)),
            container(deploy_col).width(Length::FillPortion(1)),
        ]
        .spacing(16)
        .width(Fill)
        .into(),
    )
}

fn lifecycle_source<'a>(state: &'a CominState) -> Element<'a, OverviewMessage> {
    let repo = state.fetcher.repository_status.as_ref();
    let generation = state.builder.generation.as_ref();

    let remote = repo
        .and_then(|r| r.selected_remote_name.as_deref())
        .or_else(|| generation.and_then(|g| g.selected_remote_name.as_deref()))
        .unwrap_or("unknown");

    let branch = repo
        .and_then(|r| r.selected_branch_name.as_deref())
        .or_else(|| generation.and_then(|g| g.selected_branch_name.as_deref()))
        .unwrap_or("unknown");

    let commit_id = repo
        .and_then(|r| r.selected_commit_id.as_deref())
        .or_else(|| generation.and_then(|g| g.selected_commit_id.as_deref()))
        .unwrap_or("");

    let title = repo
        .and_then(|r| r.selected_commit_msg.as_deref())
        .or_else(|| generation.and_then(|g| g.selected_commit_msg.as_deref()))
        .map(commit_title)
        .unwrap_or_default();

    let mut col = column![
        text("Git source").size(13).color(BREEZE_TEXT_DIM),
        text(format!("{remote}/{branch}"))
            .size(14)
            .color(BREEZE_TEXT),
    ]
    .spacing(5);

    if !commit_id.is_empty() {
        col = col.push(
            text(short_commit(commit_id))
                .size(13)
                .font(Font::MONOSPACE)
                .color(BREEZE_TEXT_DIM),
        );
    } else {
        col = col.push(text("—").size(13).color(BREEZE_TEXT_MUTED));
    }

    if !title.is_empty() {
        col = col.push(text(title).size(12).color(BREEZE_TEXT_DIM));
    }

    col.width(Fill).into()
}

fn lifecycle_fetch<'a>(state: &'a CominState) -> Element<'a, OverviewMessage> {
    let is_fetching = state.fetcher.is_fetching.unwrap_or(false);
    let repo = state.fetcher.repository_status.as_ref();

    let (chip_text, time_str) = if is_fetching {
        ("Fetching", None)
    } else if let Some(r) = repo {
        let last_time = r
            .remotes
            .first()
            .and_then(|remote| remote.fetched_at.as_deref())
            .map(format_relative_time_now);
        ("Fetched", last_time)
    } else {
        ("Idle", None)
    };

    let error_text = repo
        .and_then(|r| r.error_msg.as_deref())
        .filter(|e| !e.is_empty());

    let mut col = column![
        text("Fetch").size(13).color(BREEZE_TEXT_DIM),
        status_chip(chip_text),
    ]
    .spacing(5);

    if let Some(t) = time_str {
        col = col.push(text(t).size(12).color(BREEZE_TEXT_MUTED));
    }
    if let Some(err) = error_text {
        col = col.push(text(err).size(12).color(BREEZE_DANGER));
    }

    col.width(Fill).into()
}

fn lifecycle_eval<'a>(state: &'a CominState) -> Element<'a, OverviewMessage> {
    let is_evaluating = state.builder.is_evaluating.unwrap_or(false);
    let generation = state.builder.generation.as_ref();

    let status = if is_evaluating {
        "evaluating"
    } else if let Some(g) = generation {
        g.eval_status.as_deref().unwrap_or("idle")
    } else {
        "idle"
    };

    let time_str = generation
        .and_then(|g| g.eval_ended_at.as_deref().or(g.eval_started_at.as_deref()))
        .map(format_relative_time_now);

    let error_text = generation
        .and_then(|g| g.eval_err.as_deref())
        .filter(|e| !e.is_empty());

    let mut col = column![
        text("Evaluation").size(13).color(BREEZE_TEXT_DIM),
        status_chip(status),
    ]
    .spacing(5);

    if let Some(t) = time_str {
        col = col.push(text(t).size(12).color(BREEZE_TEXT_MUTED));
    }
    if let Some(err) = error_text {
        col = col.push(text(err).size(12).color(BREEZE_DANGER));
    }

    col.width(Fill).into()
}

fn lifecycle_build<'a>(state: &'a CominState) -> Element<'a, OverviewMessage> {
    let is_building = state.builder.is_building.unwrap_or(false);
    let generation = state.builder.generation.as_ref();

    let status = if is_building {
        "building"
    } else if let Some(g) = generation {
        g.build_status.as_deref().unwrap_or("idle")
    } else {
        "idle"
    };

    let time_str = generation
        .and_then(|g| {
            g.build_ended_at
                .as_deref()
                .or(g.build_started_at.as_deref())
        })
        .map(format_relative_time_now);

    let reason = generation
        .and_then(|g| g.build_reason.as_deref())
        .filter(|r| !r.is_empty());

    let error_text = generation
        .and_then(|g| g.build_err.as_deref())
        .filter(|e| !e.is_empty());

    let mut col = column![
        text("Build").size(13).color(BREEZE_TEXT_DIM),
        status_chip(status),
    ]
    .spacing(5);

    if let Some(t) = time_str {
        col = col.push(text(t).size(12).color(BREEZE_TEXT_MUTED));
    }
    if let Some(r) = reason {
        col = col.push(text(r).size(12).color(BREEZE_TEXT_DIM));
    }
    if let Some(err) = error_text {
        col = col.push(text(err).size(12).color(BREEZE_DANGER));
    }

    col.width(Fill).into()
}

fn lifecycle_deploy<'a>(state: &'a CominState) -> Element<'a, OverviewMessage> {
    let is_deploying = state.deployer.is_deploying.unwrap_or(false);
    let deploy = state.latest_deployment();

    let status = if is_deploying {
        "deploying"
    } else if let Some(d) = deploy {
        d.status.as_deref().unwrap_or("idle")
    } else {
        "idle"
    };

    let op = deploy
        .and_then(|d| d.operation.as_deref())
        .or(state.deployer.operation.as_deref())
        .unwrap_or("—");

    let time_str = deploy
        .and_then(|d| d.ended_at.as_deref().or(d.started_at.as_deref()))
        .map(format_relative_time_now);

    let error_text = deploy
        .and_then(|d| d.error_msg.as_deref())
        .filter(|e| !e.is_empty());

    let mut col = column![
        text("Deployment").size(13).color(BREEZE_TEXT_DIM),
        row![status_chip(status), status_chip(op)].spacing(6),
    ]
    .spacing(5);

    if let Some(t) = time_str {
        col = col.push(text(t).size(12).color(BREEZE_TEXT_MUTED));
    }
    if let Some(err) = error_text {
        col = col.push(text(err).size(12).color(BREEZE_DANGER));
    }

    col.width(Fill).into()
}

fn fetcher_card_content<'a>(state: &'a CominState) -> Element<'a, OverviewMessage> {
    let mut col = Column::new().spacing(10).width(Fill);

    if let Some(repo) = &state.fetcher.repository_status {
        for remote in &repo.remotes {
            let fetched_ago = remote
                .fetched_at
                .as_deref()
                .map(format_relative_time_now)
                .unwrap_or_else(|| "unknown".into());

            let remote_header = row![
                text(&remote.name).size(14).color(BREEZE_TEXT),
                horizontal_space(),
                text(format!("fetched {fetched_ago}"))
                    .size(12)
                    .color(BREEZE_TEXT_MUTED),
            ]
            .align_y(Alignment::Center)
            .width(Fill);

            let url_line = text(&remote.url).size(12).color(BREEZE_TEXT_MUTED);

            let mut rem_col = column![remote_header, url_line].spacing(3).width(Fill);

            if let Some(err) = remote.fetch_error_msg.as_deref().filter(|s| !s.is_empty()) {
                rem_col = rem_col.push(text(err).size(12).color(BREEZE_DANGER));
            }

            if let Some(main) = &remote.main {
                let name = main.name.as_deref().unwrap_or("main");
                let commit = main
                    .commit_id
                    .as_deref()
                    .map(short_commit)
                    .unwrap_or_default();
                let title = main
                    .commit_msg
                    .as_deref()
                    .map(commit_title)
                    .unwrap_or_default();

                let mut line = row![
                    text("main:")
                        .size(12)
                        .color(BREEZE_TEXT_DIM)
                        .width(Length::Fixed(55.0)),
                    text(name).size(12).color(BREEZE_TEXT),
                ];
                if !commit.is_empty() {
                    line = line.push(
                        text(commit)
                            .size(12)
                            .font(Font::MONOSPACE)
                            .color(BREEZE_TEXT_DIM),
                    );
                }
                if !title.is_empty() {
                    line = line.push(text(title).size(12).color(BREEZE_TEXT_MUTED));
                }
                rem_col = rem_col.push(line.spacing(6));
            }

            if let Some(testing) = &remote.testing {
                let name = testing.name.as_deref().unwrap_or("testing");
                let err = testing.error_msg.as_deref().unwrap_or("");
                let commit = testing
                    .commit_id
                    .as_deref()
                    .map(short_commit)
                    .unwrap_or_default();

                if !err.is_empty() {
                    rem_col = rem_col.push(
                        row![
                            text("testing:")
                                .size(12)
                                .color(BREEZE_TEXT_DIM)
                                .width(Length::Fixed(55.0)),
                            text(format!("{name}: {err}"))
                                .size(12)
                                .color(BREEZE_TEXT_MUTED),
                        ]
                        .spacing(6),
                    );
                } else if !commit.is_empty() {
                    let title = testing
                        .commit_msg
                        .as_deref()
                        .map(commit_title)
                        .unwrap_or_default();
                    rem_col = rem_col.push(
                        row![
                            text("testing:")
                                .size(12)
                                .color(BREEZE_TEXT_DIM)
                                .width(Length::Fixed(55.0)),
                            text(name).size(12).color(BREEZE_TEXT),
                            text(commit)
                                .size(12)
                                .font(Font::MONOSPACE)
                                .color(BREEZE_TEXT_DIM),
                            text(title).size(12).color(BREEZE_TEXT_MUTED),
                        ]
                        .spacing(6),
                    );
                }
            }

            col = col.push(rem_col);
        }
    } else {
        col = col.push(
            text("No repository data available")
                .size(13)
                .color(BREEZE_TEXT_MUTED),
        );
    }

    col.into()
}

fn builder_card_content<'a>(state: &'a CominState) -> Element<'a, OverviewMessage> {
    let mut col = Column::new().spacing(8).width(Fill);

    if let Some(generation) = &state.builder.generation {
        let commit = generation
            .selected_commit_id
            .as_deref()
            .map(short_commit)
            .unwrap_or_default();
        let remote = generation.selected_remote_name.as_deref().unwrap_or("");
        let branch = generation.selected_branch_name.as_deref().unwrap_or("");
        let title = generation
            .selected_commit_msg
            .as_deref()
            .map(commit_title)
            .unwrap_or_default();

        col = col.push(
            row![
                text(format!("{remote}/{branch}"))
                    .size(13)
                    .color(BREEZE_TEXT),
                text(commit)
                    .size(13)
                    .font(Font::MONOSPACE)
                    .color(BREEZE_TEXT_DIM),
                text(title).size(12).color(BREEZE_TEXT_MUTED),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );

        let eval_status = generation.eval_status.as_deref().unwrap_or("idle");
        let eval_time = generation
            .eval_ended_at
            .as_deref()
            .or(generation.eval_started_at.as_deref())
            .map(format_relative_time_now)
            .unwrap_or_default();

        col = col.push(
            row![
                text("Evaluation:")
                    .size(13)
                    .color(BREEZE_TEXT_DIM)
                    .width(Length::Fixed(85.0)),
                status_chip(eval_status),
                text(eval_time).size(12).color(BREEZE_TEXT_MUTED),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );

        if let Some(err) = generation.eval_err.as_deref().filter(|s| !s.is_empty()) {
            col = col.push(text(err).size(12).color(BREEZE_DANGER));
        }

        let build_status = generation.build_status.as_deref().unwrap_or("idle");
        let build_time = generation
            .build_ended_at
            .as_deref()
            .or(generation.build_started_at.as_deref())
            .map(format_relative_time_now)
            .unwrap_or_default();

        col = col.push(
            row![
                text("Build:")
                    .size(13)
                    .color(BREEZE_TEXT_DIM)
                    .width(Length::Fixed(85.0)),
                status_chip(build_status),
                text(build_time).size(12).color(BREEZE_TEXT_MUTED),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );

        if let Some(reason) = generation.build_reason.as_deref().filter(|s| !s.is_empty()) {
            col = col.push(
                row![
                    text("Reason:")
                        .size(13)
                        .color(BREEZE_TEXT_DIM)
                        .width(Length::Fixed(85.0)),
                    text(reason).size(12).color(BREEZE_TEXT_DIM),
                ]
                .spacing(8),
            );
        }

        if let Some(err) = generation.build_err.as_deref().filter(|s| !s.is_empty()) {
            col = col.push(text(err).size(12).color(BREEZE_DANGER));
        }

        if let Some(out) = generation.out_path.as_deref().filter(|s| !s.is_empty()) {
            col = col.push(
                row![
                    text("Out:")
                        .size(13)
                        .color(BREEZE_TEXT_DIM)
                        .width(Length::Fixed(85.0)),
                    text(short_store_path(out))
                        .size(12)
                        .font(Font::MONOSPACE)
                        .color(BREEZE_TEXT_MUTED),
                ]
                .spacing(8),
            );
        }
    } else {
        col = col.push(
            text("No generation active")
                .size(13)
                .color(BREEZE_TEXT_MUTED),
        );
    }

    col.into()
}

fn deployer_card_content<'a>(state: &'a CominState) -> Element<'a, OverviewMessage> {
    let mut col = Column::new().spacing(8).width(Fill);

    if let Some(deploy) = state.latest_deployment() {
        let op = deploy.operation.as_deref().unwrap_or("unknown");
        let status = deploy.status.as_deref().unwrap_or("unknown");
        let ended = deploy
            .ended_at
            .as_deref()
            .map(format_relative_time_now)
            .unwrap_or_default();

        col = col.push(
            row![
                text("Status:")
                    .size(13)
                    .color(BREEZE_TEXT_DIM)
                    .width(Length::Fixed(85.0)),
                status_chip(status),
                status_chip(op),
                text(ended).size(12).color(BREEZE_TEXT_MUTED),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );

        if let Some(sub) = deploy.operation_submitted.as_deref().filter(|&s| s != op) {
            col = col.push(
                row![
                    text("Submitted:")
                        .size(13)
                        .color(BREEZE_TEXT_DIM)
                        .width(Length::Fixed(85.0)),
                    status_chip(sub),
                ]
                .spacing(8),
            );
        }

        if let Some(reason) = deploy.reason.as_deref().filter(|s| !s.is_empty()) {
            col = col.push(
                row![
                    text("Reason:")
                        .size(13)
                        .color(BREEZE_TEXT_DIM)
                        .width(Length::Fixed(85.0)),
                    text(reason).size(12).color(BREEZE_TEXT_DIM),
                ]
                .spacing(8),
            );
        }

        if let Some(profile) = deploy.profile_path.as_deref().filter(|s| !s.is_empty()) {
            col = col.push(
                row![
                    text("Profile:")
                        .size(13)
                        .color(BREEZE_TEXT_DIM)
                        .width(Length::Fixed(85.0)),
                    text(profile)
                        .size(12)
                        .font(Font::MONOSPACE)
                        .color(BREEZE_TEXT_MUTED),
                ]
                .spacing(8),
            );
        }

        if let Some(err) = deploy.error_msg.as_deref().filter(|s| !s.is_empty()) {
            col = col.push(text(err).size(12).color(BREEZE_DANGER));
        }
    } else {
        col = col.push(
            text("No deployment active")
                .size(13)
                .color(BREEZE_TEXT_MUTED),
        );
    }

    col.into()
}

/// The three newest items of the deployment history, with a link to the
/// Deployments page for the rest.
fn recent_deployments_card<'a>(state: &'a CominState) -> Element<'a, OverviewMessage> {
    let history = state.history();
    let mut col = Column::new().spacing(4).width(Fill);

    if history.is_empty() {
        col = col.push(
            text("No deployments recorded yet")
                .size(13)
                .color(BREEZE_TEXT_MUTED),
        );
    }

    for item in history.iter().take(3) {
        let when = if item.active {
            "in progress".to_string()
        } else if item.time_key().is_empty() {
            "—".to_string()
        } else {
            format_relative_time_now(item.time_key())
        };
        let mut chips = Row::new().spacing(6).align_y(Alignment::Center);
        chips = chips.push(status_chip(item.status()));
        if let Some(operation) = item.operation() {
            chips = chips.push(status_chip(operation));
        }

        let content = row![
            container(text(when).size(13).color(BREEZE_TEXT)).width(Length::Fixed(110.0)),
            container(chips).width(Length::Fixed(200.0)),
            text(
                item.commit_id()
                    .map(short_commit)
                    .unwrap_or("—")
                    .to_string()
            )
            .size(13)
            .font(Font::MONOSPACE)
            .color(BREEZE_TEXT_DIM)
            .width(Length::Fixed(80.0)),
            text(
                item.commit_msg()
                    .map(commit_title)
                    .unwrap_or_default()
                    .to_string()
            )
            .size(13)
            .color(BREEZE_TEXT_DIM)
            .width(Fill),
            container(match item.deployment {
                Some(deployment) => retention_badges(deployment, &state.store),
                None => Row::new().into(),
            }),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        col = col.push(
            button(content)
                .on_press(OverviewMessage::OpenDeployment(item.key.clone()))
                .width(Fill)
                .padding([8, 12])
                .style(|_theme: &Theme, status: button::Status| button::Style {
                    background: Some(Background::Color(match status {
                        button::Status::Hovered => BREEZE_BG_HOVER,
                        _ => BREEZE_BG_CARD,
                    })),
                    text_color: BREEZE_TEXT,
                    border: Border {
                        color: BREEZE_BORDER_SUBTLE,
                        width: 1.0,
                        radius: Radius::from(4.0),
                    },
                    ..Default::default()
                }),
        );
    }

    let view_all = button(text(format!("View all ({}) →", history.len())).size(12))
        .on_press(OverviewMessage::OpenDeployments)
        .style(secondary_button_style)
        .padding([4, 10]);

    card("Recent Deployments", Some(view_all.into()), col.into())
}
