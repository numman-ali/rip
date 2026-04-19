//! Debug overlay (Phase C.1).
//!
//! The old status bar carried a long, dense line of debug tokens
//! (`view:canvas  session:s1  seq:6  hdr:-  fb:-  evt:-  TTFT:…`) that
//! was invaluable when something was wrong and just noise the rest of
//! the time. C.1 moves those tokens behind this overlay — reachable
//! in Phase C.5 via `Command → Show debug info`, and directly via
//! `set_overlay(Overlay::Debug)` today — so the hero is clear and the
//! debug data is still one keystroke away.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::TuiState;

use super::super::theme::ThemeStyles;

pub(super) fn render_debug_overlay(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
) {
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Line::from(Span::styled(" Debug ", theme.header)))
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = build_debug_lines(state, theme);
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .style(theme.chrome),
        inner,
    );
}

fn build_debug_lines(state: &TuiState, theme: &ThemeStyles) -> Vec<Line<'static>> {
    let mut lines = vec![
        section_heading("session", theme),
        kv("session_id", state.session_id.as_deref(), theme),
        kv("continuity_id", state.continuity_id.as_deref(), theme),
        kv_owned(
            "last_seq",
            state.frames.last_seq().map(|seq| seq.to_string()),
            theme,
        ),
        Line::default(),
    ];

    lines.push(section_heading("timings", theme));
    lines.push(kv_ms("ttft", state.ttft_ms(), theme));
    lines.push(kv_ms("e2e", state.e2e_ms(), theme));
    lines.push(kv_ms("handshake", state.openresponses_headers_ms(), theme));
    lines.push(kv_ms(
        "first_byte",
        state.openresponses_first_byte_ms(),
        theme,
    ));
    lines.push(kv_ms(
        "first_event",
        state.openresponses_first_provider_event_ms(),
        theme,
    ));
    lines.push(Line::default());

    lines.push(section_heading("provider", theme));
    let endpoint = state
        .openresponses_endpoint
        .as_deref()
        .or(state.preferred_openresponses_endpoint.as_deref());
    let model = state
        .openresponses_model
        .as_deref()
        .or(state.preferred_openresponses_model.as_deref());
    lines.push(kv("endpoint", endpoint, theme));
    lines.push(kv("model", model, theme));
    lines.push(Line::default());

    lines.push(section_heading("counts", theme));
    lines.push(kv_owned(
        "tools",
        Some(format!(
            "{}/{}",
            state.running_tool_ids().count(),
            state.tools.len()
        )),
        theme,
    ));
    lines.push(kv_owned(
        "tasks",
        Some(format!(
            "{}/{}",
            state.running_task_ids().count(),
            state.tasks.len()
        )),
        theme,
    ));
    lines.push(kv_owned(
        "jobs",
        Some(format!(
            "{}/{}",
            state.running_job_ids().count(),
            state.jobs.len()
        )),
        theme,
    ));
    lines.push(kv_owned(
        "artifacts",
        Some(state.artifacts.len().to_string()),
        theme,
    ));
    lines.push(Line::default());

    lines.push(section_heading("flags", theme));
    lines.push(kv_owned(
        "stalled",
        Some(state.is_stalled(5_000).to_string()),
        theme,
    ));
    lines.push(kv_owned(
        "error",
        Some(state.has_error().to_string()),
        theme,
    ));
    lines.push(kv_owned(
        "theme",
        Some(state.theme.as_str().to_string()),
        theme,
    ));
    lines.push(kv_owned(
        "view",
        Some(state.output_view.as_str().to_string()),
        theme,
    ));
    if let Some(msg) = state.status_message.as_deref() {
        lines.push(kv("status", Some(msg), theme));
    }

    lines
}

fn section_heading(label: &str, theme: &ThemeStyles) -> Line<'static> {
    Line::from(Span::styled(label.to_string(), theme.header))
}

fn kv(label: &str, value: Option<&str>, theme: &ThemeStyles) -> Line<'static> {
    let value = value.filter(|v| !v.trim().is_empty()).unwrap_or("—");
    Line::from(vec![
        Span::styled(format!("  {label:<12} "), theme.muted),
        Span::styled(value.to_string(), theme.chrome),
    ])
}

fn kv_owned(label: &str, value: Option<String>, theme: &ThemeStyles) -> Line<'static> {
    let value = value
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "—".to_string());
    Line::from(vec![
        Span::styled(format!("  {label:<12} "), theme.muted),
        Span::styled(value, theme.chrome),
    ])
}

fn kv_ms(label: &str, ms: Option<u64>, theme: &ThemeStyles) -> Line<'static> {
    let value = match ms {
        Some(v) => format!("{v}ms"),
        None => "—".to_string(),
    };
    Line::from(vec![
        Span::styled(format!("  {label:<12} "), theme.muted),
        Span::styled(value, theme.chrome),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ThemeId;
    use crate::TuiState;

    #[test]
    fn debug_lines_cover_all_sections() {
        let mut state = TuiState::new(10);
        state.session_id = Some("s1".to_string());
        state.continuity_id = Some("c-slide-prep".to_string());
        let theme = ThemeStyles::for_theme(ThemeId::DefaultDark);
        let lines = build_debug_lines(&state, &theme);
        let text: String = lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        for heading in ["session", "timings", "provider", "counts", "flags"] {
            assert!(text.contains(heading), "missing {heading}: {text}");
        }
        assert!(text.contains("s1"));
        assert!(text.contains("c-slide-prep"));
    }
}
