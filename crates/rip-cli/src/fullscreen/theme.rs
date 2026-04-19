//! TUI theme loading.
//!
//! Reads `RIP_TUI_THEME` env var or `$RIP_CONFIG_HOME/theme.json`
//! (defaulting to `~/.rip/theme.json`) to pick a `ThemeId`. The file
//! is optional — absence is "use the built-in default" rather than an
//! error. This lives outside the render loop so `scripts/check-fast`
//! never touches the user's home in tests.

use std::path::PathBuf;

pub(super) fn load_theme() -> anyhow::Result<Option<rip_tui::ThemeId>> {
    if let Some(raw) = std::env::var_os("RIP_TUI_THEME") {
        return parse_theme(&raw.to_string_lossy());
    }

    let path = theme_path().ok_or_else(|| anyhow::anyhow!("missing $HOME for theme.json"))?;
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };

    let value: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|err| anyhow::anyhow!("theme.json invalid json at {}: {err}", path.display()))?;

    match value {
        serde_json::Value::String(s) => parse_theme(&s),
        serde_json::Value::Object(map) => map
            .get("theme")
            .and_then(|v| v.as_str())
            .map(parse_theme)
            .transpose()
            .map(|theme| theme.flatten()),
        _ => Ok(None),
    }
}

pub(super) fn parse_theme(raw: &str) -> anyhow::Result<Option<rip_tui::ThemeId>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }

    match raw.to_ascii_lowercase().as_str() {
        "default-dark" | "dark" => Ok(Some(rip_tui::ThemeId::DefaultDark)),
        "default-light" | "light" => Ok(Some(rip_tui::ThemeId::DefaultLight)),
        _ => Err(anyhow::anyhow!("unknown theme '{raw}'")),
    }
}

pub(super) fn theme_path() -> Option<PathBuf> {
    Some(config_dir()?.join("theme.json"))
}

pub(super) fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("RIP_CONFIG_HOME") {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".rip"))
}
