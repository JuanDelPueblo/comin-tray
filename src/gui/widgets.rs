use iced::{
    Background, Border, Color, Element, Fill, Theme,
    border::Radius,
    widget::{Row, button, column, container, text},
};

pub const COLOR_GREEN: Color = Color::from_rgb(0.18, 0.68, 0.38);
pub const COLOR_RED: Color = Color::from_rgb(0.85, 0.25, 0.25);
pub const COLOR_BLUE: Color = Color::from_rgb(0.25, 0.55, 0.85);
pub const COLOR_AMBER: Color = Color::from_rgb(0.88, 0.58, 0.15);
pub const COLOR_GRAY: Color = Color::from_rgb(0.45, 0.45, 0.50);
pub const COLOR_TEAL: Color = Color::from_rgb(0.15, 0.65, 0.65);
pub const COLOR_PURPLE: Color = Color::from_rgb(0.60, 0.40, 0.80);

pub fn badge<'a, Message: 'a>(
    label: impl Into<String>,
    bg: Color,
    fg: Color,
) -> Element<'a, Message> {
    container(text(label.into()).size(12).color(fg))
        .padding([3, 8])
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(bg)),
            border: Border {
                radius: Radius::from(10.0),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

pub fn status_chip<'a, Message: 'a>(status: &str) -> Element<'a, Message> {
    let s = status.to_ascii_lowercase();
    let (bg, fg) = match s.as_str() {
        "done" | "evaluated" | "built" | "successful" | "active" | "fetched" => (
            Color::from_rgba(COLOR_GREEN.r, COLOR_GREEN.g, COLOR_GREEN.b, 0.2),
            COLOR_GREEN,
        ),
        "failed" | "error" => (
            Color::from_rgba(COLOR_RED.r, COLOR_RED.g, COLOR_RED.b, 0.2),
            COLOR_RED,
        ),
        "evaluating" | "building" | "deploying" | "fetching" | "running" => (
            Color::from_rgba(COLOR_BLUE.r, COLOR_BLUE.g, COLOR_BLUE.b, 0.2),
            COLOR_BLUE,
        ),
        "suspended" | "reboot required" | "confirmation required" => (
            Color::from_rgba(COLOR_AMBER.r, COLOR_AMBER.g, COLOR_AMBER.b, 0.2),
            COLOR_AMBER,
        ),
        "boot" => (
            Color::from_rgba(COLOR_TEAL.r, COLOR_TEAL.g, COLOR_TEAL.b, 0.2),
            COLOR_TEAL,
        ),
        "switch" => (
            Color::from_rgba(COLOR_PURPLE.r, COLOR_PURPLE.g, COLOR_PURPLE.b, 0.2),
            COLOR_PURPLE,
        ),
        _ => (
            Color::from_rgba(COLOR_GRAY.r, COLOR_GRAY.g, COLOR_GRAY.b, 0.2),
            COLOR_GRAY,
        ),
    };

    badge(status, bg, fg)
}

pub fn card<'a, Message: 'a>(
    title: impl Into<String>,
    badge_element: Option<Element<'a, Message>>,
    content: Element<'a, Message>,
) -> Element<'a, Message> {
    let mut header = Row::new().spacing(8).align_y(iced::Alignment::Center);
    header = header.push(
        text(title.into())
            .size(14)
            .color(Color::from_rgb(0.85, 0.85, 0.88)),
    );

    if let Some(b) = badge_element {
        header = header.push(b);
    }

    container(column![header, content].spacing(12))
        .padding(14)
        .width(Fill)
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(Color::from_rgba(0.16, 0.17, 0.20, 0.7))),
            border: Border {
                color: Color::from_rgba(0.3, 0.32, 0.38, 0.4),
                width: 1.0,
                radius: Radius::from(8.0),
            },
            ..Default::default()
        })
        .into()
}

pub fn banner<'a, Message: Clone + 'a>(
    message: impl Into<String>,
    bg: Color,
    border_color: Color,
    action: Option<(&'a str, Message)>,
) -> Element<'a, Message> {
    let mut row = Row::new()
        .spacing(12)
        .align_y(iced::Alignment::Center)
        .width(Fill);

    row = row.push(
        text(message.into())
            .size(13)
            .color(Color::WHITE)
            .width(Fill),
    );

    if let Some((label, msg)) = action {
        row = row.push(
            button(text(label).size(12))
                .on_press(msg)
                .style(button::secondary)
                .padding([4, 10]),
        );
    }

    container(row)
        .padding([10, 14])
        .width(Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(bg)),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: Radius::from(6.0),
            },
            ..Default::default()
        })
        .into()
}
