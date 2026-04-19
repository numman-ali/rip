use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};

use crate::ThemeId;

/// Terminal color-depth classes used when degrading semantic tokens.
///
/// Detected once at startup via `supports-color` and cached. Current
/// chrome (Phase A) runs at whatever depth the terminal reports and
/// does not yet act on the result; tokens are prepared so that the
/// new chrome (Phases B–C) can drive `theme.tint(...)` / degradation
/// without plumbing detection through every call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColorDepth {
    TrueColor,
    Ansi256,
    Ansi16,
    Mono,
}

impl ColorDepth {
    fn detect() -> Self {
        if std::env::var_os("NO_COLOR").is_some() {
            return ColorDepth::Mono;
        }
        match supports_color::on_cached(supports_color::Stream::Stdout) {
            None => ColorDepth::Mono,
            Some(level) if level.has_16m => ColorDepth::TrueColor,
            Some(level) if level.has_256 => ColorDepth::Ansi256,
            Some(_) => ColorDepth::Ansi16,
        }
    }

    pub(crate) fn current() -> Self {
        static DEPTH: OnceLock<ColorDepth> = OnceLock::new();
        *DEPTH.get_or_init(ColorDepth::detect)
    }
}

/// Semantic color tokens for the TUI (Part 2.1 of the revamp plan).
///
/// Values here preserve the Phase A visual contract — they map to the
/// existing legacy palette, so snapshots stay identical. The Graphite /
/// Ink end-state hex values land with the Phase C chrome rewrite,
/// where snapshots regenerate together with the new layout.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub(crate) struct Theme {
    pub fg_primary: Color,
    pub fg_body: Color,
    pub fg_muted: Color,
    pub fg_quiet: Color,
    pub accent_user: Color,
    pub accent_agent: Color,
    pub accent_subagent_a: Color,
    pub accent_subagent_b: Color,
    pub accent_subagent_c: Color,
    pub accent_subagent_d: Color,
    pub accent_tool: Color,
    pub accent_task: Color,
    pub accent_reviewer: Color,
    pub accent_extension: Color,
    pub accent_warn: Color,
    pub accent_danger: Color,
    pub accent_success: Color,
    pub rule: Color,
    pub focus_tint: Color,
    pub prompt_ribbon: Color,
}

impl Theme {
    pub(crate) fn graphite() -> Self {
        Self {
            fg_primary: Color::White,
            fg_body: Color::Gray,
            fg_muted: Color::Gray,
            fg_quiet: Color::DarkGray,
            accent_user: Color::LightBlue,
            accent_agent: Color::Cyan,
            accent_subagent_a: Color::LightMagenta,
            accent_subagent_b: Color::LightGreen,
            accent_subagent_c: Color::LightYellow,
            accent_subagent_d: Color::LightRed,
            accent_tool: Color::Cyan,
            accent_task: Color::Cyan,
            accent_reviewer: Color::LightYellow,
            accent_extension: Color::LightCyan,
            accent_warn: Color::Yellow,
            accent_danger: Color::Red,
            accent_success: Color::Green,
            rule: Color::DarkGray,
            focus_tint: Color::Rgb(23, 34, 48),
            prompt_ribbon: Color::Rgb(23, 34, 48),
        }
    }

    pub(crate) fn ink() -> Self {
        Self {
            fg_primary: Color::Black,
            fg_body: Color::Black,
            fg_muted: Color::Gray,
            fg_quiet: Color::DarkGray,
            accent_user: Color::Blue,
            accent_agent: Color::Blue,
            accent_subagent_a: Color::Magenta,
            accent_subagent_b: Color::Green,
            accent_subagent_c: Color::Yellow,
            accent_subagent_d: Color::Red,
            accent_tool: Color::Blue,
            accent_task: Color::Blue,
            accent_reviewer: Color::Yellow,
            accent_extension: Color::Cyan,
            accent_warn: Color::Yellow,
            accent_danger: Color::Red,
            accent_success: Color::Green,
            rule: Color::Gray,
            focus_tint: Color::Rgb(228, 236, 242),
            prompt_ribbon: Color::Rgb(228, 236, 242),
        }
    }

    pub(crate) fn for_id(id: ThemeId) -> Self {
        match id {
            ThemeId::DefaultDark => Self::graphite(),
            ThemeId::DefaultLight => Self::ink(),
        }
    }

    /// Degrade a token color for the active terminal depth. At Mono,
    /// colors collapse to `Reset` so foreground/background follow the
    /// terminal default; Ansi16/256 pass through to the ratatui
    /// backend for its own quantization.
    pub(crate) fn tint(&self, color: Color, depth: ColorDepth) -> Color {
        match depth {
            ColorDepth::Mono => Color::Reset,
            ColorDepth::TrueColor | ColorDepth::Ansi256 | ColorDepth::Ansi16 => color,
        }
    }
}

/// Rendered styles used by legacy Phase A chrome. Computed from `Theme`;
/// later phases will drive widgets off `Theme` directly and retire this
/// projection.
#[derive(Debug, Clone, Copy)]
pub(super) struct ThemeStyles {
    pub(super) chrome: Style,
    pub(super) header: Style,
    pub(super) highlight: Style,
    pub(super) accent: Style,
    pub(super) prompt: Style,
    pub(super) prompt_label: Style,
    pub(super) subagent_accents: [Style; 4],
    pub(super) reviewer: Style,
    pub(super) extension: Style,
    /// Secondary text: timestamps, model chip, separator dots.
    pub(super) muted: Style,
    /// Tertiary text: rules, bottom card corners, palette mode chips.
    pub(super) quiet: Style,
    /// Semantic warning accent (stalled runs, warn notices).
    pub(super) warn: Style,
    /// Semantic danger accent (errors, failed provider events).
    pub(super) danger: Style,
    /// Semantic success accent (succeeded tools, completed tasks).
    /// Currently surfaced via the `HeroState` mapping and will be
    /// consumed by the C.2 activity strip / C.10 error recovery rows.
    #[allow(dead_code)]
    pub(super) success: Style,
}

impl ThemeStyles {
    pub(super) fn for_theme(id: ThemeId) -> Self {
        let raw = Theme::for_id(id);
        let depth = ColorDepth::current();
        // Route every token through `tint` so Mono / NO_COLOR users get
        // uncolored chrome today; at TrueColor this is a passthrough and
        // snapshot output is unchanged.
        let theme = Theme {
            fg_primary: raw.tint(raw.fg_primary, depth),
            fg_body: raw.tint(raw.fg_body, depth),
            fg_muted: raw.tint(raw.fg_muted, depth),
            fg_quiet: raw.tint(raw.fg_quiet, depth),
            accent_user: raw.tint(raw.accent_user, depth),
            accent_agent: raw.tint(raw.accent_agent, depth),
            accent_subagent_a: raw.tint(raw.accent_subagent_a, depth),
            accent_subagent_b: raw.tint(raw.accent_subagent_b, depth),
            accent_subagent_c: raw.tint(raw.accent_subagent_c, depth),
            accent_subagent_d: raw.tint(raw.accent_subagent_d, depth),
            accent_tool: raw.tint(raw.accent_tool, depth),
            accent_task: raw.tint(raw.accent_task, depth),
            accent_reviewer: raw.tint(raw.accent_reviewer, depth),
            accent_extension: raw.tint(raw.accent_extension, depth),
            accent_warn: raw.tint(raw.accent_warn, depth),
            accent_danger: raw.tint(raw.accent_danger, depth),
            accent_success: raw.tint(raw.accent_success, depth),
            rule: raw.tint(raw.rule, depth),
            focus_tint: raw.tint(raw.focus_tint, depth),
            prompt_ribbon: raw.tint(raw.prompt_ribbon, depth),
        };
        match id {
            ThemeId::DefaultDark => Self {
                chrome: Style::default().fg(theme.fg_body),
                header: Style::default()
                    .fg(theme.accent_agent)
                    .add_modifier(Modifier::BOLD),
                highlight: Style::default()
                    .fg(Color::Black)
                    .bg(theme.accent_agent)
                    .add_modifier(Modifier::BOLD),
                accent: Style::default().fg(theme.accent_user),
                prompt: Style::default()
                    .fg(theme.fg_primary)
                    .bg(theme.prompt_ribbon),
                prompt_label: Style::default()
                    .fg(theme.accent_agent)
                    .bg(theme.prompt_ribbon)
                    .add_modifier(Modifier::BOLD),
                subagent_accents: [
                    Style::default()
                        .fg(theme.accent_subagent_a)
                        .add_modifier(Modifier::BOLD),
                    Style::default()
                        .fg(theme.accent_subagent_b)
                        .add_modifier(Modifier::BOLD),
                    Style::default()
                        .fg(theme.accent_subagent_c)
                        .add_modifier(Modifier::BOLD),
                    Style::default()
                        .fg(theme.accent_subagent_d)
                        .add_modifier(Modifier::BOLD),
                ],
                reviewer: Style::default()
                    .fg(theme.accent_reviewer)
                    .add_modifier(Modifier::BOLD),
                extension: Style::default()
                    .fg(theme.accent_extension)
                    .add_modifier(Modifier::BOLD),
                muted: Style::default().fg(theme.fg_muted),
                quiet: Style::default().fg(theme.fg_quiet),
                warn: Style::default().fg(theme.accent_warn),
                danger: Style::default()
                    .fg(theme.accent_danger)
                    .add_modifier(Modifier::BOLD),
                success: Style::default().fg(theme.accent_success),
            },
            ThemeId::DefaultLight => Self {
                chrome: Style::default().fg(theme.fg_primary),
                header: Style::default()
                    .fg(theme.accent_agent)
                    .add_modifier(Modifier::BOLD),
                highlight: Style::default()
                    .fg(Color::Black)
                    .bg(Color::LightBlue)
                    .add_modifier(Modifier::BOLD),
                accent: Style::default().fg(theme.accent_agent),
                prompt: Style::default()
                    .fg(theme.fg_primary)
                    .bg(theme.prompt_ribbon),
                prompt_label: Style::default()
                    .fg(theme.accent_agent)
                    .bg(theme.prompt_ribbon)
                    .add_modifier(Modifier::BOLD),
                subagent_accents: [
                    Style::default()
                        .fg(theme.accent_subagent_a)
                        .add_modifier(Modifier::BOLD),
                    Style::default()
                        .fg(theme.accent_subagent_b)
                        .add_modifier(Modifier::BOLD),
                    Style::default()
                        .fg(theme.accent_subagent_c)
                        .add_modifier(Modifier::BOLD),
                    Style::default()
                        .fg(theme.accent_subagent_d)
                        .add_modifier(Modifier::BOLD),
                ],
                reviewer: Style::default()
                    .fg(theme.accent_reviewer)
                    .add_modifier(Modifier::BOLD),
                extension: Style::default()
                    .fg(theme.accent_extension)
                    .add_modifier(Modifier::BOLD),
                muted: Style::default().fg(theme.fg_muted),
                quiet: Style::default().fg(theme.fg_quiet),
                warn: Style::default().fg(theme.accent_warn),
                danger: Style::default()
                    .fg(theme.accent_danger)
                    .add_modifier(Modifier::BOLD),
                success: Style::default().fg(theme.accent_success),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphite_and_ink_produce_distinct_palettes() {
        let dark = Theme::for_id(ThemeId::DefaultDark);
        let light = Theme::for_id(ThemeId::DefaultLight);
        assert_ne!(dark.fg_primary, light.fg_primary);
        assert_ne!(dark.prompt_ribbon, light.prompt_ribbon);
    }

    #[test]
    fn tint_collapses_to_reset_in_mono() {
        let theme = Theme::graphite();
        let token = theme.accent_agent;
        assert_eq!(theme.tint(token, ColorDepth::Mono), Color::Reset);
        for depth in [
            ColorDepth::TrueColor,
            ColorDepth::Ansi256,
            ColorDepth::Ansi16,
        ] {
            assert_eq!(theme.tint(token, depth), token);
        }
    }

    #[test]
    fn color_depth_current_is_stable() {
        let a = ColorDepth::current();
        let b = ColorDepth::current();
        assert_eq!(a, b);
    }
}
