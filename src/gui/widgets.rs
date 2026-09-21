use iced::{
    Alignment, Background, Border, Color, Element, Fill, Length, Theme,
    border::Radius,
    widget::{Row, button, column, container, row, text},
};

use crate::gui::theme::{
    BREEZE_ACCENT, BREEZE_BG_CARD, BREEZE_BORDER, BREEZE_DANGER, BREEZE_PURPLE, BREEZE_SUCCESS,
    BREEZE_TEAL, BREEZE_TEXT, BREEZE_TEXT_DIM, BREEZE_TEXT_MUTED, BREEZE_WARNING, card_style,
    secondary_button_style,
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
    let (color, fg) = match s.as_str() {
        "done" | "evaluated" | "built" | "successful" | "active" | "fetched" => {
            (BREEZE_SUCCESS, BREEZE_SUCCESS)
        }
        "failed" | "error" => (BREEZE_DANGER, BREEZE_DANGER),
        "evaluating" | "building" | "deploying" | "fetching" | "running" => {
            (BREEZE_ACCENT, BREEZE_ACCENT)
        }
        "suspended" | "reboot required" | "confirmation required" => {
            (BREEZE_WARNING, BREEZE_WARNING)
        }
        "boot" => (BREEZE_TEAL, BREEZE_TEAL),
        "switch" => (BREEZE_PURPLE, BREEZE_PURPLE),
        _ => (BREEZE_TEXT_MUTED, BREEZE_TEXT_DIM),
    };

    let bg = Color::from_rgba(color.r, color.g, color.b, 0.16);
    let border = Color::from_rgba(color.r, color.g, color.b, 0.45);

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
    height: impl Into<Length>,
) -> Element<'a, Message> {
    let mut header = Row::new().spacing(10).align_y(Alignment::Center);
    header = header.push(text(title.into()).size(15).color(BREEZE_TEXT));

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

pub enum BannerKind {
    #[allow(dead_code)]
    Info,
    Warning,
    Error,
}

pub fn banner<'a, Message: Clone + 'a>(
    message: impl Into<String>,
    kind: BannerKind,
    action: Option<(&'a str, Message)>,
) -> Element<'a, Message> {
    let accent_color = match kind {
        BannerKind::Info => BREEZE_ACCENT,
        BannerKind::Warning => BREEZE_WARNING,
        BannerKind::Error => BREEZE_DANGER,
    };

    let indicator_bar = container(text("").size(0))
        .width(Length::Fixed(4.0))
        .height(Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(accent_color)),
            border: Border {
                radius: Radius::from(2.0),
                ..Default::default()
            },
            ..Default::default()
        });

    let mut content_row = Row::new()
        .spacing(12)
        .align_y(Alignment::Center)
        .width(Fill);

    content_row = content_row.push(text(message.into()).size(13).color(BREEZE_TEXT).width(Fill));

    if let Some((label, msg)) = action {
        content_row = content_row.push(
            button(text(label).size(12))
                .on_press(msg)
                .style(secondary_button_style)
                .padding([5, 12]),
        );
    }

    let inner = row![indicator_bar, content_row]
        .spacing(12)
        .align_y(Alignment::Center)
        .width(Fill);

    container(inner)
        .padding([10, 14])
        .width(Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(BREEZE_BG_CARD)),
            border: Border {
                color: BREEZE_BORDER,
                width: 1.0,
                radius: Radius::from(6.0),
            },
            ..Default::default()
        })
        .into()
}
