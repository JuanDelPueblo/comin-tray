use iced::{
    Alignment, Color, Element, Fill, Font, Length,
    widget::{Column, Row, button, column, container, horizontal_space, row, scrollable, text},
};

use crate::{
    format::{commit_title, format_relative_time_now, short_commit, short_store_path},
    gui::widgets::{
        COLOR_AMBER, COLOR_BLUE, COLOR_GRAY, COLOR_PURPLE, COLOR_RED, COLOR_TEAL, badge, banner,
        card, status_chip,
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
    let mut content = Column::new().spacing(16).padding(16).width(Fill);

    // 1. Header (Hostname + Overall Badges + Refresh)
    content = content.push(header_section(state, busy_action.is_some()));

    // 2. Important State Banners
    if let Some(err) = error {
        content = content.push(banner(
            format!("Connection error: {err}"),
            Color::from_rgba(COLOR_RED.r, COLOR_RED.g, COLOR_RED.b, 0.25),
            COLOR_RED,
            Some(("Retry", OverviewMessage::Refresh)),
        ));
    }

    if let Some(action_err) = action_error {
        content = content.push(banner(
            format!("Action failed: {action_err}"),
            Color::from_rgba(COLOR_RED.r, COLOR_RED.g, COLOR_RED.b, 0.25),
            COLOR_RED,
            None,
        ));
    }

    if let Some(s) = state {
        if s.confirmation_needed() {
            content = content.push(banner(
                "Confirmation required: A generation is waiting for deployment approval.",
                Color::from_rgba(COLOR_AMBER.r, COLOR_AMBER.g, COLOR_AMBER.b, 0.25),
                COLOR_AMBER,
                Some(("Accept confirmation", OverviewMessage::AcceptConfirmation)),
            ));
        }

        if let Some(reason) = s.reboot_reason() {
            content = content.push(banner(
                format!("Reboot required: {reason}"),
                Color::from_rgba(COLOR_AMBER.r, COLOR_AMBER.g, COLOR_AMBER.b, 0.2),
                COLOR_AMBER,
                None,
            ));
        }

        if s.is_suspended.unwrap_or(false) {
            content = content.push(banner(
                "GitOps is currently suspended. Automatic polling and deployments are paused.",
                Color::from_rgba(COLOR_AMBER.r, COLOR_AMBER.g, COLOR_AMBER.b, 0.2),
                COLOR_AMBER,
                Some(("Resume GitOps", OverviewMessage::Resume)),
            ));
        }
    }

    // 3. Action Bar
    content = content.push(action_bar(state, busy_action));

    if let Some(s) = state {
        // 4. Cockpit-style Lifecycle Overview
        content = content.push(lifecycle_overview(s));

        // 5. Detailed Cards: Fetcher, Builder, Deployer
        content = content.push(
            row![
                card("Fetcher", None, fetcher_card_content(s)),
                card("Builder", None, builder_card_content(s)),
                card("Deployer", None, deployer_card_content(s)),
            ]
            .spacing(12)
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
        if s.need_to_reboot.unwrap_or(false) {
            badges = badges.push(status_chip("Reboot required"));
        }
        if s.confirmation_needed() {
            badges = badges.push(status_chip("Confirmation required"));
        }
    }

    let refresh_button = button(text("Refresh").size(12))
        .on_press_maybe(if is_busy {
            None
        } else {
            Some(OverviewMessage::Refresh)
        })
        .style(button::secondary)
        .padding([5, 12]);

    row![
        column![
            text("Comin")
                .size(13)
                .color(Color::from_rgb(0.65, 0.65, 0.70)),
            row![text(hostname).size(20), badges]
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

    let mut actions = Row::new().spacing(8).align_y(Alignment::Center);

    // Fetch now
    actions = actions.push(
        button(text("Fetch now").size(12))
            .on_press_maybe(if is_busy {
                None
            } else {
                Some(OverviewMessage::Fetch)
            })
            .style(button::primary)
            .padding([6, 12]),
    );

    // Suspend / Resume GitOps
    if is_suspended {
        actions = actions.push(
            button(text("Resume GitOps").size(12))
                .on_press_maybe(if is_busy {
                    None
                } else {
                    Some(OverviewMessage::Resume)
                })
                .style(button::secondary)
                .padding([6, 12]),
        );
    } else {
        actions = actions.push(
            button(text("Suspend GitOps").size(12))
                .on_press_maybe(if is_busy {
                    None
                } else {
                    Some(OverviewMessage::Suspend)
                })
                .style(button::secondary)
                .padding([6, 12]),
        );
    }

    // Switch live now (conditional)
    if can_switch {
        actions = actions.push(
            button(text("Switch live now").size(12))
                .on_press_maybe(if is_busy {
                    None
                } else {
                    Some(OverviewMessage::SwitchLatest)
                })
                .style(button::success)
                .padding([6, 12]),
        );
    }

    // Retry deployment (conditional)
    if can_retry {
        actions = actions.push(
            button(text("Retry deployment").size(12))
                .on_press_maybe(if is_busy {
                    None
                } else {
                    Some(OverviewMessage::RetryLatest)
                })
                .style(button::secondary)
                .padding([6, 12]),
        );
    }

    // Accept confirmation (conditional)
    if can_confirm {
        actions = actions.push(
            button(text("Accept confirmation").size(12))
                .on_press_maybe(if is_busy {
                    None
                } else {
                    Some(OverviewMessage::AcceptConfirmation)
                })
                .style(button::success)
                .padding([6, 12]),
        );
    }

    if let Some(busy) = busy_action {
        actions = actions.push(
            text(format!("Running: {busy}..."))
                .size(12)
                .color(COLOR_AMBER),
        );
    }

    actions.into()
}

fn lifecycle_overview<'a>(state: &'a CominState) -> Element<'a, OverviewMessage> {
    // 5 Stages: Git source, Fetch, Evaluation, Build, Deployment
    let source_col = lifecycle_source(state);
    let fetch_col = lifecycle_fetch(state);
    let eval_col = lifecycle_eval(state);
    let build_col = lifecycle_build(state);
    let deploy_col = lifecycle_deploy(state);

    card(
        "Lifecycle Overview",
        None,
        row![source_col, fetch_col, eval_col, build_col, deploy_col]
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

    column![
        text("Git source").size(12).color(COLOR_GRAY),
        text(format!("{remote}/{branch}")).size(13),
        if !commit_id.is_empty() {
            text(short_commit(commit_id))
                .size(12)
                .font(Font::MONOSPACE)
                .color(Color::from_rgb(0.7, 0.7, 0.75))
        } else {
            text("—").size(12)
        },
        if !title.is_empty() {
            text(title).size(11).color(Color::from_rgb(0.6, 0.6, 0.65))
        } else {
            text("").size(0)
        },
    ]
    .spacing(4)
    .width(Fill)
    .into()
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
        text("Fetch").size(12).color(COLOR_GRAY),
        status_chip(chip_text),
    ]
    .spacing(4);

    if let Some(t) = time_str {
        col = col.push(text(t).size(11).color(Color::from_rgb(0.6, 0.6, 0.65)));
    }
    if let Some(err) = error_text {
        col = col.push(text(err).size(11).color(COLOR_RED));
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
        text("Evaluation").size(12).color(COLOR_GRAY),
        status_chip(status),
    ]
    .spacing(4);

    if let Some(t) = time_str {
        col = col.push(text(t).size(11).color(Color::from_rgb(0.6, 0.6, 0.65)));
    }
    if let Some(err) = error_text {
        col = col.push(text(err).size(11).color(COLOR_RED));
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
        text("Build").size(12).color(COLOR_GRAY),
        status_chip(status),
    ]
    .spacing(4);

    if let Some(t) = time_str {
        col = col.push(text(t).size(11).color(Color::from_rgb(0.6, 0.6, 0.65)));
    }
    if let Some(r) = reason {
        col = col.push(text(r).size(11).color(Color::from_rgb(0.55, 0.55, 0.60)));
    }
    if let Some(err) = error_text {
        col = col.push(text(err).size(11).color(COLOR_RED));
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
        text("Deployment").size(12).color(COLOR_GRAY),
        row![status_chip(status), status_chip(op)].spacing(6),
    ]
    .spacing(4);

    if let Some(t) = time_str {
        col = col.push(text(t).size(11).color(Color::from_rgb(0.6, 0.6, 0.65)));
    }
    if let Some(err) = error_text {
        col = col.push(text(err).size(11).color(COLOR_RED));
    }

    col.width(Fill).into()
}

fn fetcher_card_content<'a>(state: &'a CominState) -> Element<'a, OverviewMessage> {
    let mut col = Column::new().spacing(8);

    if let Some(repo) = &state.fetcher.repository_status {
        for remote in &repo.remotes {
            let fetched_ago = remote
                .fetched_at
                .as_deref()
                .map(format_relative_time_now)
                .unwrap_or_else(|| "unknown".into());

            let mut rem_col = column![
                row![
                    text(format!("{} ({})", remote.name, remote.url))
                        .size(12)
                        .color(Color::from_rgb(0.85, 0.85, 0.90)),
                    horizontal_space(),
                    text(format!("fetched {fetched_ago}"))
                        .size(11)
                        .color(COLOR_GRAY),
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(4);

            if let Some(err) = remote.fetch_error_msg.as_deref().filter(|s| !s.is_empty()) {
                rem_col = rem_col.push(text(err).size(11).color(COLOR_RED));
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
                let mut line = row![text(format!("main: {name}")).size(11).color(COLOR_GRAY),];
                if !commit.is_empty() {
                    line = line.push(text(commit).size(11).font(Font::MONOSPACE));
                }
                if !title.is_empty() {
                    line = line.push(
                        text(title)
                            .size(11)
                            .color(Color::from_rgb(0.65, 0.65, 0.70)),
                    );
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
                            text(format!("testing: {name}")).size(11).color(COLOR_GRAY),
                            text(err).size(11).color(COLOR_GRAY),
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
                            text(format!("testing: {name}")).size(11).color(COLOR_GRAY),
                            text(commit).size(11).font(Font::MONOSPACE),
                            text(title)
                                .size(11)
                                .color(Color::from_rgb(0.65, 0.65, 0.70)),
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
                .size(12)
                .color(COLOR_GRAY),
        );
    }

    col.into()
}

fn builder_card_content<'a>(state: &'a CominState) -> Element<'a, OverviewMessage> {
    let mut col = Column::new().spacing(8);

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
                    .size(12)
                    .color(COLOR_GRAY),
                text(commit).size(12).font(Font::MONOSPACE),
                text(title)
                    .size(11)
                    .color(Color::from_rgb(0.65, 0.65, 0.70)),
            ]
            .spacing(6),
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
                    .size(12)
                    .color(COLOR_GRAY)
                    .width(Length::Fixed(80.0)),
                status_chip(eval_status),
                text(eval_time).size(11).color(COLOR_GRAY),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );

        if let Some(err) = generation.eval_err.as_deref().filter(|s| !s.is_empty()) {
            col = col.push(text(err).size(11).color(COLOR_RED));
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
                    .size(12)
                    .color(COLOR_GRAY)
                    .width(Length::Fixed(80.0)),
                status_chip(build_status),
                text(build_time).size(11).color(COLOR_GRAY),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );

        if let Some(reason) = generation.build_reason.as_deref().filter(|s| !s.is_empty()) {
            col = col.push(
                row![
                    text("Reason:")
                        .size(11)
                        .color(COLOR_GRAY)
                        .width(Length::Fixed(80.0)),
                    text(reason).size(11).color(Color::from_rgb(0.7, 0.7, 0.75)),
                ]
                .spacing(8),
            );
        }

        if let Some(err) = generation.build_err.as_deref().filter(|s| !s.is_empty()) {
            col = col.push(text(err).size(11).color(COLOR_RED));
        }

        if let Some(out) = generation.out_path.as_deref().filter(|s| !s.is_empty()) {
            col = col.push(
                row![
                    text("Out:")
                        .size(11)
                        .color(COLOR_GRAY)
                        .width(Length::Fixed(40.0)),
                    text(short_store_path(out))
                        .size(11)
                        .font(Font::MONOSPACE)
                        .color(Color::from_rgb(0.6, 0.6, 0.65)),
                ]
                .spacing(6),
            );
        }
    } else {
        col = col.push(text("No generation active").size(12).color(COLOR_GRAY));
    }

    col.into()
}

fn deployer_card_content<'a>(state: &'a CominState) -> Element<'a, OverviewMessage> {
    let mut col = Column::new().spacing(8);

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
                    .size(12)
                    .color(COLOR_GRAY)
                    .width(Length::Fixed(70.0)),
                status_chip(status),
                status_chip(op),
                text(ended).size(11).color(COLOR_GRAY),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );

        if let Some(sub) = deploy.operation_submitted.as_deref().filter(|&s| s != op) {
            col = col.push(
                row![
                    text("Submitted:")
                        .size(11)
                        .color(COLOR_GRAY)
                        .width(Length::Fixed(70.0)),
                    status_chip(sub),
                ]
                .spacing(8),
            );
        }

        if let Some(reason) = deploy.reason.as_deref().filter(|s| !s.is_empty()) {
            col = col.push(
                row![
                    text("Reason:")
                        .size(11)
                        .color(COLOR_GRAY)
                        .width(Length::Fixed(70.0)),
                    text(reason).size(11).color(Color::from_rgb(0.7, 0.7, 0.75)),
                ]
                .spacing(8),
            );
        }

        if let Some(profile) = deploy.profile_path.as_deref().filter(|s| !s.is_empty()) {
            col = col.push(
                row![
                    text("Profile:")
                        .size(11)
                        .color(COLOR_GRAY)
                        .width(Length::Fixed(70.0)),
                    text(profile)
                        .size(11)
                        .font(Font::MONOSPACE)
                        .color(Color::from_rgb(0.6, 0.6, 0.65)),
                ]
                .spacing(8),
            );
        }

        if let Some(err) = deploy.error_msg.as_deref().filter(|s| !s.is_empty()) {
            col = col.push(text(err).size(11).color(COLOR_RED));
        }
    } else {
        col = col.push(text("No deployment active").size(12).color(COLOR_GRAY));
    }

    col.into()
}

fn recent_deployments_card<'a>(state: &'a CominState) -> Element<'a, OverviewMessage> {
    let mut indices: Vec<usize> = (0..state.store.deployments.len()).collect();
    // Sort newest first
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
    let mut col = Column::new().spacing(6).width(Fill);

    // Table Header
    col = col.push(
        row![
            text("Ended")
                .size(11)
                .color(COLOR_GRAY)
                .width(Length::Fixed(120.0)),
            text("Operation")
                .size(11)
                .color(COLOR_GRAY)
                .width(Length::Fixed(90.0)),
            text("Status")
                .size(11)
                .color(COLOR_GRAY)
                .width(Length::Fixed(80.0)),
            text("Commit")
                .size(11)
                .color(COLOR_GRAY)
                .width(Length::Fixed(90.0)),
            text("Commit title").size(11).color(COLOR_GRAY).width(Fill),
            text("Retention")
                .size(11)
                .color(COLOR_GRAY)
                .width(Length::Fixed(200.0)),
        ]
        .spacing(8)
        .padding([4, 6]),
    );

    if indices.is_empty() {
        col = col.push(
            text("No recent deployments recorded")
                .size(12)
                .color(COLOR_GRAY),
        );
    } else {
        for idx in indices.into_iter().take(limit) {
            let dep = &state.store.deployments[idx];
            col = col.push(deployment_row(dep, &state.store));
        }
    }

    card("Recent Deployments", None, col.into())
}

fn deployment_row<'a>(dep: &'a Deployment, store: &'a Store) -> Element<'a, OverviewMessage> {
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

    let mut roles = Row::new().spacing(4);
    if dep.is_switched(store) {
        roles = roles.push(badge(
            "switched",
            Color::from_rgba(COLOR_BLUE.r, COLOR_BLUE.g, COLOR_BLUE.b, 0.2),
            COLOR_BLUE,
        ));
    }
    if dep.is_booted(store) {
        roles = roles.push(badge(
            "booted",
            Color::from_rgba(COLOR_TEAL.r, COLOR_TEAL.g, COLOR_TEAL.b, 0.2),
            COLOR_TEAL,
        ));
    }
    if dep.is_boot_entry(store) {
        roles = roles.push(badge(
            "boot entry",
            Color::from_rgba(COLOR_PURPLE.r, COLOR_PURPLE.g, COLOR_PURPLE.b, 0.2),
            COLOR_PURPLE,
        ));
    }
    if dep.is_successful(store) {
        roles = roles.push(badge(
            "successful",
            Color::from_rgba(COLOR_AMBER.r, COLOR_AMBER.g, COLOR_AMBER.b, 0.2),
            COLOR_AMBER,
        ));
    }

    row![
        text(ended).size(12).width(Length::Fixed(120.0)),
        container(status_chip(op)).width(Length::Fixed(90.0)),
        container(status_chip(status)).width(Length::Fixed(80.0)),
        text(commit)
            .size(12)
            .font(Font::MONOSPACE)
            .width(Length::Fixed(90.0)),
        text(title)
            .size(11)
            .color(Color::from_rgb(0.7, 0.7, 0.75))
            .width(Fill),
        roles.width(Length::Fixed(200.0)),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .padding([4, 6])
    .into()
}
