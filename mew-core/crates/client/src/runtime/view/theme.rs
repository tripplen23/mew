use ratatui::style::Color;

use super::super::model::ThemeId;

#[derive(Debug, Clone, Copy)]
pub(super) struct Theme {
    pub ink_bg: Color,
    pub panel_bg: Color,
    pub hot_pink: Color,
    pub lavender: Color,
    pub mew_gold: Color,
    pub psy_cyan: Color,
    pub muted: Color,
    pub text: Color,
    pub chip_fg: Color,
}

pub(super) const COMPOSER_LEFT_PAD: u16 = 1;
pub(super) const COMPOSER_HORIZONTAL_PAD: u16 = 4;

pub(super) const DEFAULT_THEME: Theme = Theme {
    ink_bg: Color::Rgb(18, 15, 24),
    panel_bg: Color::Rgb(34, 24, 42),
    hot_pink: Color::Rgb(255, 111, 190),
    lavender: Color::Rgb(244, 188, 255),
    mew_gold: Color::Rgb(255, 222, 83),
    psy_cyan: Color::Rgb(111, 239, 255),
    muted: Color::Rgb(158, 137, 168),
    text: Color::Rgb(235, 229, 241),
    chip_fg: Color::Rgb(33, 18, 42),
};

pub(super) fn theme_for(id: ThemeId) -> Theme {
    match id {
        ThemeId::MewY2k => DEFAULT_THEME,
    }
}
