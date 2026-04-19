//! Options palette mode (Phase C.5).
//!
//! Fast toggles for UI-local preferences: theme, auto-follow,
//! reasoning visibility, vim input mode, mouse capture, activity rail
//! pinning. These are pure UI-local — they never touch the event log,
//! they live in `~/.rip/state/tui.json` (per Part 5.3 of the revamp
//! plan), and they are never continuity truth.
//!
//! Applying an entry yields a string value matching one of the
//! `CommandAction` ids under the `OPTIONS` category, so the driver
//! can route Options-mode selections through the same dispatch path
//! as direct Command-mode invocations of the same toggle.

use crate::PaletteEntry;

use super::super::PaletteSource;
use super::command::CommandAction;

#[derive(Debug, Clone, Default)]
pub struct OptionsMode {
    pub current_theme: Option<&'static str>,
    pub auto_follow: bool,
    pub reasoning_visible: bool,
    pub vim_input_mode: bool,
    pub mouse_capture: bool,
    pub activity_rail_pinned: bool,
}

impl OptionsMode {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PaletteSource for OptionsMode {
    fn id(&self) -> &'static str {
        "options"
    }

    fn label(&self) -> &str {
        "Options"
    }

    fn placeholder(&self) -> &str {
        "toggle UI preferences"
    }

    fn entries(&self) -> Vec<PaletteEntry> {
        let toggles = [
            (
                CommandAction::ToggleTheme,
                self.current_theme.unwrap_or("graphite").to_string(),
            ),
            (
                CommandAction::ToggleAutoFollow,
                on_off(self.auto_follow).to_string(),
            ),
            (
                CommandAction::ToggleReasoningVisibility,
                on_off(self.reasoning_visible).to_string(),
            ),
            (
                CommandAction::ToggleVimInputMode,
                on_off(self.vim_input_mode).to_string(),
            ),
            (
                CommandAction::ToggleMouseCapture,
                on_off(self.mouse_capture).to_string(),
            ),
            (
                CommandAction::PinActivityRail,
                on_off(self.activity_rail_pinned).to_string(),
            ),
        ];
        toggles
            .into_iter()
            .map(|(action, state)| PaletteEntry {
                value: action.id().to_string(),
                title: action.title().to_string(),
                subtitle: Some(format!("current: {state}")),
                chips: if action.is_available() {
                    Vec::new()
                } else {
                    vec!["unavailable".to_string()]
                },
            })
            .collect()
    }

    fn empty_state(&self) -> &str {
        "No options match"
    }
}

fn on_off(flag: bool) -> &'static str {
    if flag {
        "on"
    } else {
        "off"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_mode_exposes_all_toggles_with_current_values() {
        let mode = OptionsMode {
            current_theme: Some("ink"),
            auto_follow: true,
            reasoning_visible: false,
            vim_input_mode: false,
            mouse_capture: true,
            activity_rail_pinned: false,
        };
        let entries = mode.entries();
        assert_eq!(entries.len(), 6);
        assert_eq!(entries[0].value, "options.theme");
        let theme_sub = entries[0]
            .subtitle
            .as_deref()
            .expect("theme has current-state subtitle");
        assert!(theme_sub.contains("ink"));
        let auto_sub = entries[1]
            .subtitle
            .as_deref()
            .expect("auto-follow has subtitle");
        assert!(auto_sub.contains("on"));
    }
}
