use ratatui::style::{Color, Modifier, Style};

use crate::ThemeId;

#[derive(Debug, Clone, Copy)]
pub(super) struct ThemeStyles {
    pub(super) chrome: Style,
    pub(super) header: Style,
    pub(super) highlight: Style,
    pub(super) accent: Style,
    pub(super) prompt: Style,
    pub(super) prompt_label: Style,
}

impl ThemeStyles {
    pub(super) fn for_theme(theme: ThemeId) -> Self {
        match theme {
            ThemeId::DefaultDark => Self {
                chrome: Style::default().fg(Color::Gray),
                header: Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
                highlight: Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
                accent: Style::default().fg(Color::LightBlue),
                prompt: Style::default().fg(Color::White).bg(Color::Rgb(23, 34, 48)),
                prompt_label: Style::default()
                    .fg(Color::Cyan)
                    .bg(Color::Rgb(23, 34, 48))
                    .add_modifier(Modifier::BOLD),
            },
            ThemeId::DefaultLight => Self {
                chrome: Style::default().fg(Color::Black),
                header: Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
                highlight: Style::default()
                    .fg(Color::Black)
                    .bg(Color::LightBlue)
                    .add_modifier(Modifier::BOLD),
                accent: Style::default().fg(Color::Blue),
                prompt: Style::default()
                    .fg(Color::Black)
                    .bg(Color::Rgb(228, 236, 242)),
                prompt_label: Style::default()
                    .fg(Color::Blue)
                    .bg(Color::Rgb(228, 236, 242))
                    .add_modifier(Modifier::BOLD),
            },
        }
    }
}
