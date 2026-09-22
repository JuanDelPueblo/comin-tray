use iced::{
    Alignment, Background, Border, Color, Element, Fill, Font, Length, Theme,
    border::Radius,
    widget::{Column, Row, button, column, container, horizontal_space, row, scrollable, text},
};

use crate::{
    format::{commit_title, format_relative_time_now, short_commit, short_store_path},
    gui::{
        theme::{
            BREEZE_ACCENT, BREEZE_BG_CARD, BREEZE_BG_HEADER, BREEZE_BG_ROW_ALT,
            BREEZE_BORDER_SUBTLE, BREEZE_DANGER, BREEZE_PURPLE, BREEZE_TEAL, BREEZE_TEXT,
            BREEZE_TEXT_DIM, BREEZE_TEXT_MUTED, BREEZE_WARNING, primary_button_style,
            secondary_button_style, success_button_style,
        },
        widgets::{BannerKind, badge, banner, card, card_with_height, status_chip},
    },
    model::{CominState, Deployment, Phase, Store},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverviewMessage {
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

fn recent_deployments_card<'a>(state: &'a CominState) -> Element<'a, OverviewMessage> {
    let mut indices: Vec<usize> = (0..state.store.deployments.len()).collect();
    indices.sort_by(|&a, &b| {
        let dep_a = &state.store.deployments[a];
        let dep_b = &state.store.deployments[b];
        let ta = dep_a
            .ended_at
            .as_deref()
            .or(dep_a.created_at.as_deref())
            .unwrap_or("");
        let tb = dep_b
            .ended_at
            .as_deref()
            .or(dep_b.created_at.as_deref())
            .unwrap_or("");
        tb.cmp(ta)
    });

    let limit = 8;
    let mut col = Column::new().spacing(0).width(Fill);

    // Breeze Table Header
    let header_row = container(
        row![
            container(text("Ended").size(12).color(BREEZE_TEXT_DIM))
                .width(Length::Fixed(125.0))
                .align_x(Alignment::Start),
            container(text("Operation").size(12).color(BREEZE_TEXT_DIM))
                .width(Length::Fixed(95.0))
                .align_x(Alignment::Start),
            container(text("Status").size(12).color(BREEZE_TEXT_DIM))
                .width(Length::Fixed(90.0))
                .align_x(Alignment::Start),
            container(text("Commit").size(12).color(BREEZE_TEXT_DIM))
                .width(Length::Fixed(95.0))
                .align_x(Alignment::Start),
            container(text("Commit title").size(12).color(BREEZE_TEXT_DIM))
                .width(Fill)
                .align_x(Alignment::Start),
            container(text("Retention").size(12).color(BREEZE_TEXT_DIM))
                .width(Length::Fixed(240.0))
                .align_x(Alignment::Start),
        ]
        .spacing(10)
        .padding([8, 12])
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .style(|_theme: &Theme| container::Style {
        background: Some(Background::Color(BREEZE_BG_HEADER)),
        border: Border {
            color: BREEZE_BORDER_SUBTLE,
            width: 1.0,
            radius: Radius::default(),
        },
        ..Default::default()
    });

    col = col.push(header_row);

    if indices.is_empty() {
        col = col.push(
            container(
                text("No recent deployments recorded")
                    .size(13)
                    .color(BREEZE_TEXT_MUTED),
            )
            .padding([16, 12])
            .width(Fill),
        );
    } else {
        for (i, idx) in indices.into_iter().take(limit).enumerate() {
            let dep = &state.store.deployments[idx];
            let is_alt = i % 2 == 1;
            col = col.push(deployment_row(dep, &state.store, is_alt));
        }
    }

    card("Recent Deployments", None, col.into())
}

fn deployment_row<'a>(
    dep: &'a Deployment,
    store: &'a Store,
    is_alt: bool,
) -> Element<'a, OverviewMessage> {
    let ended = dep
        .ended_at
        .as_deref()
        .or(dep.created_at.as_deref())
        .map(format_relative_time_now)
        .unwrap_or_else(|| "—".into());

    let op = dep.operation.as_deref().unwrap_or("—");
    let status = dep.status.as_deref().unwrap_or("—");
    let commit = dep.commit_id().map(short_commit).unwrap_or("—");
    let title = dep.commit_msg().map(commit_title).unwrap_or_default();

    let mut roles = Row::new().spacing(6).align_y(Alignment::Center);
    if dep.is_switched(store) {
        roles = roles.push(badge(
            "switched",
            Color::from_rgba(BREEZE_ACCENT.r, BREEZE_ACCENT.g, BREEZE_ACCENT.b, 0.16),
            Color::from_rgba(BREEZE_ACCENT.r, BREEZE_ACCENT.g, BREEZE_ACCENT.b, 0.45),
            BREEZE_ACCENT,
        ));
    }
    if dep.is_booted(store) {
        roles = roles.push(badge(
            "booted",
            Color::from_rgba(BREEZE_TEAL.r, BREEZE_TEAL.g, BREEZE_TEAL.b, 0.16),
            Color::from_rgba(BREEZE_TEAL.r, BREEZE_TEAL.g, BREEZE_TEAL.b, 0.45),
            BREEZE_TEAL,
        ));
    }
    if dep.is_boot_entry(store) {
        roles = roles.push(badge(
            "boot entry",
            Color::from_rgba(BREEZE_PURPLE.r, BREEZE_PURPLE.g, BREEZE_PURPLE.b, 0.16),
            Color::from_rgba(BREEZE_PURPLE.r, BREEZE_PURPLE.g, BREEZE_PURPLE.b, 0.45),
            BREEZE_PURPLE,
        ));
    }
    if dep.is_successful(store) {
        roles = roles.push(badge(
            "successful",
            Color::from_rgba(BREEZE_WARNING.r, BREEZE_WARNING.g, BREEZE_WARNING.b, 0.16),
            Color::from_rgba(BREEZE_WARNING.r, BREEZE_WARNING.g, BREEZE_WARNING.b, 0.45),
            BREEZE_WARNING,
        ));
    }

    let bg_color = if is_alt {
        BREEZE_BG_ROW_ALT
    } else {
        BREEZE_BG_CARD
    };

    container(
        row![
            container(text(ended).size(13).color(BREEZE_TEXT))
                .width(Length::Fixed(125.0))
                .align_x(Alignment::Start),
            container(status_chip(op))
                .width(Length::Fixed(95.0))
                .align_x(Alignment::Start),
            container(status_chip(status))
                .width(Length::Fixed(90.0))
                .align_x(Alignment::Start),
            container(
                text(commit)
                    .size(13)
                    .font(Font::MONOSPACE)
                    .color(BREEZE_TEXT_DIM),
            )
            .width(Length::Fixed(95.0))
            .align_x(Alignment::Start),
            container(text(title).size(13).color(BREEZE_TEXT_DIM))
                .width(Fill)
                .align_x(Alignment::Start),
            container(roles)
                .width(Length::Fixed(240.0))
                .align_x(Alignment::Start),
        ]
        .spacing(10)
        .padding([8, 12])
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(bg_color)),
        border: Border {
            color: BREEZE_BORDER_SUBTLE,
            width: 1.0,
            radius: Radius::default(),
        },
        ..Default::default()
    })
    .into()
}
