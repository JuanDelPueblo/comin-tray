use iced::{
    Alignment, Background, Border, Color, Element, Fill, Length, Theme,
    border::Radius,
    widget::{Row, button, column, container, text},
};

use crate::{
    gui::theme::{
        BREEZE_ACCENT, BREEZE_BG_HEADER, BREEZE_BG_HOVER, BREEZE_BORDER_SUBTLE, BREEZE_PURPLE,
        BREEZE_TEAL, BREEZE_TEXT, BREEZE_TEXT_DIM, BREEZE_WARNING, card_style,
        secondary_button_style,
    },
    model::{Deployment, Store},
};

pub fn badge<'a, Message: 'a>(
    label: impl Into<String>,
    bg: Color,
    border_color: Color,
    fg: Color,
) -> Element<'a, Message> {
    container(
        text(label.into())
            .size(12)
            .color(fg)
            .align_x(Alignment::Center),
    )
    .padding([3, 8])
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: Radius::from(4.0),
        },
        ..Default::default()
    })
    .into()
}

pub fn status_chip<'a, Message: 'a>(status: &str) -> Element<'a, Message> {
    let s = status.to_ascii_lowercase();
    let (bg, border, fg) = match s.as_str() {
        "done" | "evaluated" | "built" | "successful" | "active" | "fetched" => (
            Color::from_rgba(0.18, 0.76, 0.49, 0.15),
            Color::from_rgba(0.18, 0.76, 0.49, 0.40),
            Color::from_rgb(0.24, 0.85, 0.55),
        ),
        "failed" | "error" => (
            Color::from_rgba(0.93, 0.27, 0.27, 0.15),
            Color::from_rgba(0.93, 0.27, 0.27, 0.40),
            Color::from_rgb(0.98, 0.40, 0.40),
        ),
        "evaluating" | "building" | "deploying" | "fetching" | "running" => (
            Color::from_rgba(0.24, 0.62, 0.96, 0.15),
            Color::from_rgba(0.24, 0.62, 0.96, 0.40),
            Color::from_rgb(0.40, 0.75, 1.0),
        ),
        "suspended" | "reboot required" | "confirmation required" => (
            Color::from_rgba(0.96, 0.65, 0.14, 0.15),
            Color::from_rgba(0.96, 0.65, 0.14, 0.40),
            Color::from_rgb(1.0, 0.75, 0.25),
        ),
        "boot" => (
            Color::from_rgba(0.12, 0.70, 0.70, 0.15),
            Color::from_rgba(0.12, 0.70, 0.70, 0.40),
            Color::from_rgb(0.20, 0.82, 0.82),
        ),
        "switch" => (
            Color::from_rgba(0.60, 0.40, 0.85, 0.15),
            Color::from_rgba(0.60, 0.40, 0.85, 0.40),
            Color::from_rgb(0.72, 0.52, 0.95),
        ),
        _ => (
            Color::from_rgba(0.45, 0.48, 0.55, 0.15),
            Color::from_rgba(0.45, 0.48, 0.55, 0.40),
            Color::from_rgb(0.70, 0.73, 0.78),
        ),
    };

    badge(status, bg, border, fg)
}

pub fn card<'a, Message: 'a>(
    title: impl Into<String>,
    badge_element: Option<Element<'a, Message>>,
    content: Element<'a, Message>,
) -> Element<'a, Message> {
    card_with_height(title, badge_element, content, Length::Shrink)
}

pub fn card_with_height<'a, Message: 'a>(
    title: impl Into<String>,
    badge_element: Option<Element<'a, Message>>,
    content: Element<'a, Message>,
    height: Length,
) -> Element<'a, Message> {
    let mut header = Row::new().spacing(10).align_y(Alignment::Center);
    header = header.push(text(title.into()).size(14).color(BREEZE_TEXT));

    if let Some(b) = badge_element {
        header = header.push(b);
    }

    container(column![header, content].spacing(14))
        .padding(16)
        .width(Fill)
        .height(height)
        .style(card_style)
        .into()
}

#[allow(dead_code)]
pub fn section_divider<'a, Message: 'a>() -> Element<'a, Message> {
    container(iced::widget::Space::new(Fill, Length::Fixed(1.0)))
        .height(Length::Fixed(1.0))
        .width(Fill)
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(BREEZE_BORDER_SUBTLE)),
            ..Default::default()
        })
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum BannerKind {
    Info,
    Warning,
    Error,
}

pub fn banner<'a, Message: Clone + 'a>(
    message: impl Into<String>,
    kind: BannerKind,
    action: Option<(&'a str, Message)>,
) -> Element<'a, Message> {
    let (bg_color, border_color, icon) = match kind {
        BannerKind::Info => (
            Color::from_rgba(0.24, 0.62, 0.96, 0.12),
            Color::from_rgba(0.24, 0.62, 0.96, 0.40),
            "ℹ",
        ),
        BannerKind::Warning => (
            Color::from_rgba(0.96, 0.65, 0.14, 0.12),
            Color::from_rgba(0.96, 0.65, 0.14, 0.40),
            "⚠",
        ),
        BannerKind::Error => (
            Color::from_rgba(0.93, 0.27, 0.27, 0.12),
            Color::from_rgba(0.93, 0.27, 0.27, 0.40),
            "✕",
        ),
    };

    let mut content_row = Row::new()
        .spacing(10)
        .align_y(Alignment::Center)
        .width(Fill);

    content_row = content_row.push(text(icon).size(14).color(border_color));

    content_row = content_row.push(text(message.into()).size(13).color(BREEZE_TEXT).width(Fill));

    if let Some((label, msg)) = action {
        content_row = content_row.push(
            button(text(label).size(12))
                .on_press(msg)
                .style(secondary_button_style)
                .padding([5, 12]),
        );
    }

    container(content_row)
        .padding([10, 14])
        .width(Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(bg_color)),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: Radius::from(5.0),
            },
            ..Default::default()
        })
        .into()
}

/// A segment of the navigation bar or of a segmented filter.
pub fn tab_button<'a, Message: Clone + 'a>(
    label: impl Into<String>,
    active: bool,
    on_press: Message,
) -> Element<'a, Message> {
    button(text(label.into()).size(13))
        .on_press(on_press)
        .style(move |_theme: &Theme, status: button::Status| {
            let (background, text_color) = if active {
                (Some(Background::Color(BREEZE_ACCENT)), Color::WHITE)
            } else {
                let background = match status {
                    button::Status::Hovered => Some(Background::Color(BREEZE_BG_HOVER)),
                    _ => None,
                };
                (background, BREEZE_TEXT_DIM)
            };
            button::Style {
                background,
                text_color,
                border: Border {
                    radius: Radius::from(4.0),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .padding([6, 16])
        .into()
}

/// Wraps segments from [`tab_button`] in a Breeze-style segmented frame.
pub fn segmented<'a, Message: 'a>(segments: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    container(Row::with_children(segments).spacing(2))
        .padding(3)
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(BREEZE_BG_HEADER)),
            border: Border {
                color: BREEZE_BORDER_SUBTLE,
                width: 1.0,
                radius: Radius::from(6.0),
            },
            ..Default::default()
        })
        .into()
}

/// A compact secondary button, used for "Copy" and toolbar actions.
pub fn small_button<'a, Message: Clone + 'a>(
    label: impl Into<String>,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    button(text(label.into()).size(12))
        .on_press_maybe(on_press)
        .style(secondary_button_style)
        .padding([4, 10])
        .into()
}

/// The `switched`, `booted`, `boot entry` and `successful` badges of a
/// deployment.
pub fn retention_badges<'a, Message: 'a>(
    deployment: &Deployment,
    store: &Store,
) -> Element<'a, Message> {
    let tinted = |label: &'static str, color: Color| {
        badge(
            label,
            Color { a: 0.16, ..color },
            Color { a: 0.45, ..color },
            color,
        )
    };

    let mut roles = Row::new().spacing(6).align_y(Alignment::Center);
    if deployment.is_switched(store) {
        roles = roles.push(tinted("switched", BREEZE_ACCENT));
    }
    if deployment.is_booted(store) {
        roles = roles.push(tinted("booted", BREEZE_TEAL));
    }
    if deployment.is_boot_entry(store) {
        roles = roles.push(tinted("boot entry", BREEZE_PURPLE));
    }
    if deployment.is_successful(store) {
        roles = roles.push(tinted("successful", BREEZE_WARNING));
    }
    roles.into()
}
