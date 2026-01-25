use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Quit,
    Submit,
    ToggleDetailsMode,
    ToggleFollow,
    ToggleOutputView,
    ToggleTheme,
    CopySelected,
    SelectPrev,
    SelectNext,
    CompactionAuto,
    CompactionAutoSchedule,
    CompactionCutPoints,
}

#[derive(Debug, Clone)]
pub struct Keymap {
    bindings: HashMap<String, Command>,
}

impl Keymap {
    pub fn default() -> Self {
        let mut bindings = HashMap::new();

        // Core lifecycle
        bindings.insert("C-c".to_string(), Command::Quit);
        bindings.insert("Enter".to_string(), Command::Submit);

        // View
        bindings.insert("Tab".to_string(), Command::ToggleDetailsMode);
        bindings.insert("Up".to_string(), Command::SelectPrev);
        bindings.insert("Down".to_string(), Command::SelectNext);
        bindings.insert("C-f".to_string(), Command::ToggleFollow);
        bindings.insert("C-r".to_string(), Command::ToggleOutputView);
        bindings.insert("C-t".to_string(), Command::ToggleTheme);
        bindings.insert("C-y".to_string(), Command::CopySelected);
        bindings.insert("C-k".to_string(), Command::CompactionAuto);
        bindings.insert("C-j".to_string(), Command::CompactionAutoSchedule);
        bindings.insert("C-g".to_string(), Command::CompactionCutPoints);

        Self { bindings }
    }

    pub fn load() -> (Self, Option<String>) {
        let mut map = Self::default();
        let Some(path) = keybindings_path() else {
            return (map, None);
        };

        let Ok(raw) = fs::read_to_string(&path) else {
            return (map, None);
        };

        let parsed: HashMap<String, String> = match serde_json::from_str(&raw) {
            Ok(value) => value,
            Err(err) => {
                return (
                    map,
                    Some(format!(
                        "keybindings: invalid json at {}: {err}",
                        path.display()
                    )),
                );
            }
        };

        let mut warnings = Vec::new();
        for (key, value) in parsed {
            let Some(notation) = normalize_notation(&key) else {
                warnings.push(format!("keybindings: invalid key '{key}'"));
                continue;
            };
            let Some(cmd) = parse_command(&value) else {
                warnings.push(format!("keybindings: invalid command '{value}'"));
                continue;
            };
            map.bindings.insert(notation, cmd);
        }

        let warning = if warnings.is_empty() {
            None
        } else {
            Some(warnings.join("; "))
        };
        (map, warning)
    }

    pub fn command_for(&self, key: KeyEvent) -> Option<Command> {
        let notation = event_to_notation(key)?;
        self.bindings.get(&notation).copied()
    }
}

pub fn event_to_notation(key: KeyEvent) -> Option<String> {
    let mut modifiers = key.modifiers;
    let key_name = match key.code {
        KeyCode::Char(mut ch) => {
            if ch.is_ascii_uppercase() {
                ch = ch.to_ascii_lowercase();
                modifiers |= KeyModifiers::SHIFT;
            }
            ch.to_string()
        }
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),
        KeyCode::F(n) => format!("F{n}"),
        _ => return None,
    };

    let mut out = String::new();
    if modifiers.contains(KeyModifiers::CONTROL) {
        out.push_str("C-");
    }
    if modifiers.contains(KeyModifiers::ALT) {
        out.push_str("M-");
    }
    if modifiers.contains(KeyModifiers::SHIFT) {
        out.push_str("S-");
    }
    out.push_str(&key_name);
    Some(out)
}

fn parse_command(raw: &str) -> Option<Command> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "quit" => Some(Command::Quit),
        "submit" => Some(Command::Submit),
        "toggledetailsmode" | "toggle_details" | "toggle_details_mode" => {
            Some(Command::ToggleDetailsMode)
        }
        "togglefollow" | "toggle_follow" => Some(Command::ToggleFollow),
        "toggleoutputview" | "toggle_output" | "toggle_output_view" | "toggleraw"
        | "toggle_raw" => Some(Command::ToggleOutputView),
        "toggletheme" | "toggle_theme" => Some(Command::ToggleTheme),
        "copyselected" | "copy_selected" | "copy" => Some(Command::CopySelected),
        "selectprev" | "select_prev" | "up" => Some(Command::SelectPrev),
        "selectnext" | "select_next" | "down" => Some(Command::SelectNext),
        "compactionauto" | "compaction_auto" | "compaction-auto" => Some(Command::CompactionAuto),
        "compactionautoschedule"
        | "compaction_auto_schedule"
        | "compaction-auto-schedule"
        | "compactionautoscheduler"
        | "compaction_auto_scheduler" => Some(Command::CompactionAutoSchedule),
        "compactioncutpoints" | "compaction_cut_points" | "compaction-cut-points" => {
            Some(Command::CompactionCutPoints)
        }
        _ => None,
    }
}

fn normalize_notation(input: &str) -> Option<String> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    let parts: Vec<&str> = input.split('-').filter(|p| !p.is_empty()).collect();
    let (mods, key) = match parts.as_slice() {
        [] => return None,
        [key] => (&[][..], *key),
        _ => (&parts[..parts.len() - 1], parts[parts.len() - 1]),
    };

    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    for m in mods {
        match m.to_ascii_lowercase().as_str() {
            "c" | "ctrl" | "control" => ctrl = true,
            "m" | "alt" | "meta" => alt = true,
            "s" | "shift" => shift = true,
            _ => return None,
        }
    }

    let key = match key {
        "Enter" | "enter" => "Enter".to_string(),
        "Tab" | "tab" => "Tab".to_string(),
        "Up" | "up" => "Up".to_string(),
        "Down" | "down" => "Down".to_string(),
        "Left" | "left" => "Left".to_string(),
        "Right" | "right" => "Right".to_string(),
        "Esc" | "esc" => "Esc".to_string(),
        "Backspace" | "backspace" => "Backspace".to_string(),
        "Delete" | "delete" => "Delete".to_string(),
        "Home" | "home" => "Home".to_string(),
        "End" | "end" => "End".to_string(),
        "PageUp" | "pageup" => "PageUp".to_string(),
        "PageDown" | "pagedown" => "PageDown".to_string(),
        _ if key.len() == 1 => key.to_ascii_lowercase(),
        _ if key.starts_with('F') || key.starts_with('f') => {
            let n: u8 = key[1..].parse().ok()?;
            format!("F{n}")
        }
        _ => return None,
    };

    let mut out = String::new();
    if ctrl {
        out.push_str("C-");
    }
    if alt {
        out.push_str("M-");
    }
    if shift {
        out.push_str("S-");
    }
    out.push_str(&key);
    Some(out)
}

fn keybindings_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("RIP_KEYBINDINGS_PATH") {
        return Some(PathBuf::from(path));
    }
    Some(config_dir()?.join("keybindings.json"))
}

fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("RIP_CONFIG_HOME") {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".rip"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn event_to_notation_encodes_ctrl_chars() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(event_to_notation(key).as_deref(), Some("C-c"));
    }

    #[test]
    fn event_to_notation_encodes_special_keys() {
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
        assert_eq!(event_to_notation(key).as_deref(), Some("Enter"));
        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::empty());
        assert_eq!(event_to_notation(key).as_deref(), Some("Tab"));
        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::empty());
        assert_eq!(event_to_notation(key).as_deref(), Some("Up"));
    }

    #[test]
    fn normalize_notation_accepts_common_forms() {
        assert_eq!(normalize_notation("C-c").as_deref(), Some("C-c"));
        assert_eq!(normalize_notation("c").as_deref(), Some("c"));
        assert_eq!(normalize_notation("Tab").as_deref(), Some("Tab"));
        assert_eq!(normalize_notation("M-Tab").as_deref(), Some("M-Tab"));
        assert_eq!(normalize_notation("S-Enter").as_deref(), Some("S-Enter"));
    }

    #[test]
    fn normalize_notation_rejects_unknown_modifiers_and_keys() {
        assert!(normalize_notation("Z-x").is_none());
        assert!(normalize_notation("C-Unknown").is_none());
        assert!(normalize_notation("").is_none());
    }

    #[test]
    fn parse_command_accepts_aliases() {
        assert_eq!(parse_command("quit"), Some(Command::Quit));
        assert_eq!(parse_command("toggle_raw"), Some(Command::ToggleOutputView));
        assert_eq!(parse_command("copy"), Some(Command::CopySelected));
        assert_eq!(parse_command("down"), Some(Command::SelectNext));
        assert!(parse_command("nope").is_none());
    }

    #[test]
    fn keymap_load_can_override_defaults_via_env_path() {
        let _guard = test_env::lock_env();
        let prev = std::env::var_os("RIP_KEYBINDINGS_PATH");

        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("rip-keybindings-test-{n}.json"));
        std::fs::write(&path, r#"{"C-x":"Quit"}"#).expect("write");
        std::env::set_var("RIP_KEYBINDINGS_PATH", &path);

        let (map, warning) = Keymap::load();
        assert!(warning.is_none());
        let cmd = map.command_for(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
        assert_eq!(cmd, Some(Command::Quit));

        std::env::remove_var("RIP_KEYBINDINGS_PATH");
        if let Some(prev) = prev {
            std::env::set_var("RIP_KEYBINDINGS_PATH", prev);
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn keymap_load_reports_invalid_json_as_warning() {
        let _guard = test_env::lock_env();
        let prev = std::env::var_os("RIP_KEYBINDINGS_PATH");

        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("rip-keybindings-test-invalid-{n}.json"));
        std::fs::write(&path, "not json").expect("write");
        std::env::set_var("RIP_KEYBINDINGS_PATH", &path);

        let (_map, warning) = Keymap::load();
        assert!(warning.is_some());

        std::env::remove_var("RIP_KEYBINDINGS_PATH");
        if let Some(prev) = prev {
            std::env::set_var("RIP_KEYBINDINGS_PATH", prev);
        }
        let _ = std::fs::remove_file(path);
    }
}
