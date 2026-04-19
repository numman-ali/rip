//! Borderless input editor (Phase C.1).
//!
//! Two rows: a `▎`-gutter editor row on top, and a keylight strip
//! underneath. The keylight is static in C.1 — C.3 will pipe it
//! through state so it reconfigures per "state" (idle / typing /
//! streaming / overlay / …). For now we keep one concise line of
//! default shortcuts that matches what the driver actually binds.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::theme::ThemeStyles;
use super::util::truncate;

pub(super) fn render_input(frame: &mut Frame<'_>, theme: &ThemeStyles, area: Rect, input: &str) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    render_editor_row(frame, theme, chunks[0], input);
    if chunks.len() > 1 {
        render_keylight_row(frame, theme, chunks[1]);
    }
}

fn render_editor_row(frame: &mut Frame<'_>, theme: &ThemeStyles, area: Rect, input: &str) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    // `▎` in col 0 (focused accent), space in col 1, then the editor.
    let gutter = Span::styled("▎".to_string(), theme.header);
    let prompt = Span::styled("› ".to_string(), theme.header);
    let body_width = area.width.saturating_sub(3) as usize;
    let text = truncate(input, body_width);
    let body = Span::styled(text, theme.chrome);
    let line = Line::from(vec![gutter, Span::raw(" "), prompt, body]);
    frame.render_widget(Paragraph::new(line).style(Style::default()), area);
}

fn render_keylight_row(frame: &mut Frame<'_>, theme: &ThemeStyles, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    // Col 0 reserved (keeps keylight aligned with the editor body).
    let indent = Span::raw("   ");
    let keylight = keylight_segments();
    let mut line = Vec::with_capacity(keylight.len() * 3 + 1);
    line.push(indent);
    for (key, label) in keylight {
        line.push(Span::styled(key.to_string(), theme.chrome));
        line.push(Span::styled(format!(" {label}"), theme.muted));
        line.push(Span::raw("   "));
    }
    let body = pad_line(Line::from(line), area.width as usize);
    frame.render_widget(Paragraph::new(body), area);
}

fn pad_line(line: Line<'static>, width: usize) -> Line<'static> {
    let current = line.to_string().chars().count();
    if current >= width {
        return truncate_line(line, width);
    }
    let pad_len = width - current;
    let mut spans = line.spans;
    spans.push(Span::raw(" ".repeat(pad_len)));
    Line::from(spans)
}

fn truncate_line(line: Line<'static>, width: usize) -> Line<'static> {
    if width == 0 {
        return Line::from("");
    }
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    for span in line.spans {
        let span_len = span.content.chars().count();
        if used + span_len <= width {
            used += span_len;
            out.push(span);
            continue;
        }
        let remaining = width.saturating_sub(used);
        if remaining > 1 {
            let keep = remaining.saturating_sub(1);
            let mut trimmed: String = span.content.chars().take(keep).collect();
            trimmed.push('…');
            out.push(Span::styled(trimmed, span.style));
        } else if remaining == 1 {
            out.push(Span::styled("…".to_string(), span.style));
        }
        break;
    }
    Line::from(out)
}

pub(super) fn keylight_segments() -> &'static [(&'static str, &'static str)] {
    &[
        ("⏎", "send"),
        ("⌘K", "palette"),
        ("⌘M", "model"),
        ("⌘G", "go to"),
        ("?", "help"),
    ]
}

/// Render the keylight as a single truncated string. C.3 replaces the
/// keylight with state-aware content, but the helper is still used by
/// unit tests that assert narrow-terminal truncation works.
#[cfg(test)]
pub(super) fn build_help_line(max_width: usize) -> String {
    let mut out = String::new();
    let mut first = true;
    for (key, label) in keylight_segments() {
        if !first {
            out.push_str("   ");
        }
        first = false;
        out.push_str(key);
        out.push(' ');
        out.push_str(label);
    }
    truncate(&out, max_width)
}
