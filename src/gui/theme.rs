use iced::{
    Background, Border, Color, Theme,
    border::Radius,
    widget::{button, container},
};

// KDE Breeze Dark Palette
pub const BREEZE_BG_WINDOW: Color = Color::from_rgb(0.137, 0.149, 0.161); // #232629
pub const BREEZE_BG_VIEW: Color = Color::from_rgb(0.192, 0.212, 0.231); // #31363b
pub const BREEZE_BG_CARD: Color = Color::from_rgb(0.157, 0.173, 0.188); // #282c30
pub const BREEZE_BG_HEADER: Color = Color::from_rgb(0.145, 0.157, 0.169); // #25282b
pub const BREEZE_BG_ROW_ALT: Color = Color::from_rgb(0.176, 0.192, 0.208); // #2d3135
pub const BREEZE_BG_HOVER: Color = Color::from_rgb(0.227, 0.251, 0.275); // #3a4046

// Borders
pub const BREEZE_BORDER: Color = Color::from_rgb(0.278, 0.306, 0.333); // #474e55
pub const BREEZE_BORDER_SUBTLE: Color = Color::from_rgb(0.212, 0.231, 0.251); // #363b40

// Typography
pub const BREEZE_TEXT: Color = Color::from_rgb(0.937, 0.941, 0.945); // #eff0f1
pub const BREEZE_TEXT_DIM: Color = Color::from_rgb(0.741, 0.765, 0.780); // #bdc3c7
pub const BREEZE_TEXT_MUTED: Color = Color::from_rgb(0.498, 0.549, 0.553); // #7f8c8d

// Semantic Accents
pub const BREEZE_ACCENT: Color = Color::from_rgb(0.239, 0.682, 0.914); // #3daee9 (Plasma highlight)
pub const BREEZE_ACCENT_HOVER: Color = Color::from_rgb(0.337, 0.733, 0.941); // #56bbf0
pub const BREEZE_SUCCESS: Color = Color::from_rgb(0.153, 0.682, 0.376); // #27ae60 (Positive)
pub const BREEZE_SUCCESS_HOVER: Color = Color::from_rgb(0.180, 0.800, 0.443); // #2ecc71
pub const BREEZE_WARNING: Color = Color::from_rgb(0.965, 0.455, 0.000); // #f67400 (Neutral / Warning)
pub const BREEZE_DANGER: Color = Color::from_rgb(0.855, 0.267, 0.325); // #da4453 (Negative / Error)
pub const BREEZE_TEAL: Color = Color::from_rgb(0.102, 0.737, 0.612); // #1abc9c (Teal / Boot)
pub const BREEZE_PURPLE: Color = Color::from_rgb(0.608, 0.349, 0.714); // #9b59b6 (Purple / Switch)

// Button styles in Breeze aesthetic
pub fn primary_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(BREEZE_ACCENT_HOVER)),
            text_color: Color::WHITE,
            border: Border {
                color: BREEZE_ACCENT_HOVER,
                width: 1.0,
                radius: Radius::from(5.0),
            },
            ..Default::default()
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(BREEZE_ACCENT)),
            text_color: Color::WHITE,
            border: Border {
                color: BREEZE_ACCENT,
                width: 1.0,
                radius: Radius::from(5.0),
            },
            ..Default::default()
        },
        button::Status::Disabled => button::Style {
            background: Some(Background::Color(Color::from_rgba(
                BREEZE_ACCENT.r,
                BREEZE_ACCENT.g,
                BREEZE_ACCENT.b,
                0.3,
            ))),
            text_color: BREEZE_TEXT_MUTED,
            border: Border {
                color: Color::TRANSPARENT,
                width: 1.0,
                radius: Radius::from(5.0),
            },
            ..Default::default()
        },
        _ => button::Style {
            background: Some(Background::Color(BREEZE_ACCENT)),
            text_color: Color::WHITE,
            border: Border {
                color: BREEZE_ACCENT,
                width: 1.0,
                radius: Radius::from(5.0),
            },
            ..Default::default()
        },
    }
}

pub fn secondary_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(BREEZE_BG_HOVER)),
            text_color: BREEZE_TEXT,
            border: Border {
                color: BREEZE_ACCENT,
                width: 1.0,
                radius: Radius::from(5.0),
            },
            ..Default::default()
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(BREEZE_BG_VIEW)),
            text_color: BREEZE_TEXT,
            border: Border {
                color: BREEZE_ACCENT,
                width: 1.0,
                radius: Radius::from(5.0),
            },
            ..Default::default()
        },
        button::Status::Disabled => button::Style {
            background: Some(Background::Color(Color::from_rgba(
                BREEZE_BG_CARD.r,
                BREEZE_BG_CARD.g,
                BREEZE_BG_CARD.b,
                0.4,
            ))),
            text_color: BREEZE_TEXT_MUTED,
            border: Border {
                color: BREEZE_BORDER_SUBTLE,
                width: 1.0,
                radius: Radius::from(5.0),
            },
            ..Default::default()
        },
        _ => button::Style {
            background: Some(Background::Color(BREEZE_BG_VIEW)),
            text_color: BREEZE_TEXT,
            border: Border {
                color: BREEZE_BORDER,
                width: 1.0,
                radius: Radius::from(5.0),
            },
            ..Default::default()
        },
    }
}

pub fn success_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(BREEZE_SUCCESS_HOVER)),
            text_color: Color::WHITE,
            border: Border {
                color: BREEZE_SUCCESS_HOVER,
                width: 1.0,
                radius: Radius::from(5.0),
            },
            ..Default::default()
        },
        button::Status::Disabled => button::Style {
            background: Some(Background::Color(Color::from_rgba(
                BREEZE_SUCCESS.r,
                BREEZE_SUCCESS.g,
                BREEZE_SUCCESS.b,
                0.3,
            ))),
            text_color: BREEZE_TEXT_MUTED,
            border: Border {
                color: Color::TRANSPARENT,
                width: 1.0,
                radius: Radius::from(5.0),
            },
            ..Default::default()
        },
        _ => button::Style {
            background: Some(Background::Color(BREEZE_SUCCESS)),
            text_color: Color::WHITE,
            border: Border {
                color: BREEZE_SUCCESS,
                width: 1.0,
                radius: Radius::from(5.0),
            },
            ..Default::default()
        },
    }
}

pub fn card_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BREEZE_BG_CARD)),
        border: Border {
            color: BREEZE_BORDER,
            width: 1.0,
            radius: Radius::from(6.0),
        },
        ..Default::default()
    }
}
