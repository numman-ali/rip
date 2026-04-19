//! Borderless input editor + state-aware keylight (Phase C.3).
//!
//! The input zone is two rows: a `▎`-gutter editor row on top, and a
//! keylight strip underneath. C.3 makes the keylight reconfigure per
//! situation — idle / typing / thinking / streaming / error / overlay
//! open — so the user sees the bindings that actually help them *now*
//! without a fixed ribbon of irrelevant keys.
//!
//! **Why keylights, not a help bar.** A help bar ("Enter send …") has
//! to be wide and static. A keylight knows the context, so on xs
//! terminals it can drop less relevant keys right-to-left while still
//! surfacing the two that matter.
//!
//! The key → command mapping here is the *display* — the driver
//! (`fullscreen/keymap.rs`) is still where events get resolved. We
//! just show what *currently* does something.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::TuiState;

use super::theme::ThemeStyles;
use super::util::truncate;

pub(super) fn render_input(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
    input: &str,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    // Editor grows with the number of `\n`s in the buffer, bounded by
    // the 6-row cap from Part 7 of the plan. The area we're given
    // already reserves one row for the keylight, so we split
    // `area.height - 1` between the editor and the keylight; everything
    // above the cap keeps scrolling internally via Paragraph wrap.
    let editor_rows = editor_rows_for(input, area.height).max(1);
    let keylight_rows = area.height.saturating_sub(editor_rows).min(1);
    let editor_rows = area.height - keylight_rows;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(editor_rows),
            Constraint::Length(keylight_rows),
        ])
        .split(area);

    render_editor_row(frame, state, theme, chunks[0], input);
    if keylight_rows > 0 {
        render_keylight_row(frame, state, theme, chunks[1], input);
    }
}

/// How many rows the editor needs: 1 + number of `\n`s in the input,
/// clamped to the available area minus the keylight (1 row) and to
/// the revamp's 6-row cap. Empty input always gets 1 row so the
/// prompt glyph is visible.
fn editor_rows_for(input: &str, available: u16) -> u16 {
    let newlines = input.chars().filter(|c| *c == '\n').count() as u16 + 1;
    let cap = 6u16; // Part 7: editor grows up to 6 rows
    let keylight_reserve = 1u16;
    newlines
        .min(cap)
        .min(available.saturating_sub(keylight_reserve))
}

fn render_editor_row(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
    input: &str,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    // Placeholder picks a context-aware string when the buffer is
    // empty: fresh canvas → "Ask anything", mid-thread → "Continue the
    // thread", post-error → "Retry, or r for recovery". This matches
    // the plan's Part 7 input contract. The placeholder is dimmed so
    // it visibly differs from real input.
    let show_placeholder = input.is_empty();
    let body_text = if show_placeholder {
        placeholder_for(state).to_string()
    } else {
        input.to_string()
    };
    let body_style = if show_placeholder {
        theme.quiet
    } else {
        theme.chrome
    };

    let body_width = area.width.saturating_sub(3) as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (row_idx, segment) in body_text.split('\n').enumerate() {
        let gutter = if row_idx == 0 {
            vec![
                Span::styled("▎".to_string(), theme.header),
                Span::raw(" "),
                Span::styled("› ".to_string(), theme.header),
            ]
        } else {
            vec![
                Span::styled("▎".to_string(), theme.header),
                Span::raw("   "),
            ]
        };
        let trimmed = truncate(segment, body_width);
        let mut spans = gutter;
        spans.push(Span::styled(trimmed, body_style));
        lines.push(Line::from(spans));
        if lines.len() as u16 >= area.height {
            break;
        }
    }
    frame.render_widget(Paragraph::new(lines).style(Style::default()), area);
}

fn placeholder_for(state: &TuiState) -> &'static str {
    if state.has_error() {
        return "Retry, or press r for recovery";
    }
    if state.is_stalled(5_000) {
        return "Run is quiet. ⎋ cancel, r retry";
    }
    if state.awaiting_response {
        return "Thinking… ⎋ to cancel";
    }
    if state.canvas.messages.is_empty() {
        return "Ask anything";
    }
    "Continue the thread"
}

fn render_keylight_row(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
    input: &str,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let width = area.width as usize;
    let indent = Span::raw("   ");
    let keylight = keylight_for(state, input);
    // Fit the segments right-to-left: drop the last segment when the
    // line overflows, but keep at least 2 so the user always sees the
    // most load-bearing keys.
    let selected = fit_keylight(&keylight, width.saturating_sub(3));

    let mut line: Vec<Span<'static>> = Vec::with_capacity(selected.len() * 3 + 1);
    line.push(indent);
    for (key, label) in &selected {
        line.push(Span::styled(key.to_string(), theme.chrome));
        line.push(Span::styled(format!(" {label}"), theme.muted));
        line.push(Span::raw("   "));
    }
    let body = pad_line(Line::from(line), width);
    frame.render_widget(Paragraph::new(body), area);
}

fn fit_keylight(
    all: &[(&'static str, &'static str)],
    width: usize,
) -> Vec<(&'static str, &'static str)> {
    if all.is_empty() {
        return Vec::new();
    }
    let min_keep = all.len().min(2);
    for keep in (min_keep..=all.len()).rev() {
        let slice = &all[..keep];
        if keylight_char_width(slice) <= width {
            return slice.to_vec();
        }
    }
    // Even the minimum doesn't fit — ship the first two and let the
    // row truncate. Better than nothing.
    all[..min_keep].to_vec()
}

fn keylight_char_width(items: &[(&'static str, &'static str)]) -> usize {
    items
        .iter()
        .map(|(key, label)| key.chars().count() + 1 + label.chars().count() + 3)
        .sum()
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

/// State-aware keylight segments. The `input` string is the current
/// editor buffer — we treat non-empty input as "typing" so send / newline
/// are the headline keys, regardless of other state.
///
/// Key glyphs (⏎ / ⇧⏎ / ⌘K / …) are the *advertised* bindings; the
/// driver still accepts the ASCII fallbacks (Enter / Alt-Enter / Ctrl-K).
/// That matches the plan's "hotkeys are aliases into palette actions"
/// philosophy — the keylight shows the prettier glyph, but the driver
/// is tolerant about what the terminal actually sent.
pub(super) fn keylight_for(state: &TuiState, input: &str) -> Vec<(&'static str, &'static str)> {
    use crate::Overlay;

    let overlay = state.overlay();
    match overlay {
        Overlay::None => {}
        Overlay::Palette(_) => {
            return vec![
                ("↑↓", "select"),
                ("⏎", "apply"),
                ("⇥", "mode"),
                ("⎋", "close"),
            ];
        }
        _ => {
            return vec![("⎋", "close"), ("↑↓", "scroll"), ("x", "raw")];
        }
    }

    if state.has_error() {
        return vec![
            ("r", "retry"),
            ("c", "rotate cursor"),
            ("x", "raw"),
            ("⎋", "dismiss"),
        ];
    }

    if state.is_stalled(5_000) {
        return vec![("⎋", "cancel"), ("r", "retry"), ("x", "raw")];
    }

    if state.awaiting_response {
        // Streaming (first output received) vs thinking (still waiting).
        if state.first_output_ms.is_some() {
            return vec![("⎋", "stop"), ("⌘[", "prev msg"), ("⌘]", "next msg")];
        }
        return vec![("⎋", "stop"), ("⌘[", "prev msg")];
    }

    if !input.trim().is_empty() {
        return vec![("⏎", "send"), ("⇧⏎", "newline"), ("⌘K", "palette")];
    }

    vec![
        ("?", "help"),
        ("⌘K", "command"),
        ("⌘M", "model"),
        ("⌘G", "go to"),
    ]
}

/// Render the keylight as a single truncated string. Legacy test
/// helper — only present for unit tests that assert truncation works
/// on narrow terminals. Uses the idle / no-input keylight so the
/// behavior is predictable.
#[cfg(test)]
pub(super) fn build_help_line(max_width: usize) -> String {
    let state = TuiState::new(10);
    let segments = keylight_for(&state, "");
    let mut out = String::new();
    let mut first = true;
    for (key, label) in segments {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Overlay;

    fn keys(state: &TuiState, input: &str) -> Vec<&'static str> {
        keylight_for(state, input)
            .into_iter()
            .map(|(k, _)| k)
            .collect()
    }

    #[test]
    fn idle_state_shows_navigation_defaults() {
        let state = TuiState::new(10);
        assert_eq!(keys(&state, ""), vec!["?", "⌘K", "⌘M", "⌘G"]);
    }

    #[test]
    fn typing_swaps_headline_keys_to_send_and_newline() {
        let state = TuiState::new(10);
        let k = keys(&state, "hello");
        assert_eq!(&k[..3], &["⏎", "⇧⏎", "⌘K"]);
    }

    #[test]
    fn thinking_exposes_stop_only_in_keylight() {
        let mut state = TuiState::new(10);
        state.awaiting_response = true;
        let k = keys(&state, "");
        assert!(k.contains(&"⎋"));
        assert!(!k.contains(&"?"));
    }

    #[test]
    fn streaming_adds_message_navigation_shortcuts() {
        let mut state = TuiState::new(10);
        state.awaiting_response = true;
        state.start_ms = Some(0);
        state.first_output_ms = Some(5);
        let k = keys(&state, "");
        assert!(k.contains(&"⎋"));
        assert!(k.contains(&"⌘["));
        assert!(k.contains(&"⌘]"));
    }

    #[test]
    fn error_state_shows_recovery_actions() {
        let mut state = TuiState::new(10);
        state.last_error_seq = Some(3);
        let k = keys(&state, "");
        assert_eq!(k, vec!["r", "c", "x", "⎋"]);
    }

    #[test]
    fn palette_overlay_shows_palette_controls() {
        let mut state = TuiState::new(10);
        state.open_palette(
            crate::PaletteMode::Command,
            Vec::new(),
            "no results",
            false,
            "",
        );
        let k = keys(&state, "");
        assert_eq!(k, vec!["↑↓", "⏎", "⇥", "⎋"]);
    }

    #[test]
    fn generic_overlay_shows_close_scroll_raw() {
        let mut state = TuiState::new(10);
        state.set_overlay(Overlay::Debug);
        let k = keys(&state, "");
        assert_eq!(k, vec!["⎋", "↑↓", "x"]);
    }

    #[test]
    fn fit_keylight_never_drops_below_two_entries() {
        let items = vec![
            ("⏎", "send"),
            ("⌘K", "palette"),
            ("⌘M", "model"),
            ("⌘G", "go to"),
        ];
        let fit = fit_keylight(&items, 0);
        assert_eq!(fit.len(), 2);
        let full = fit_keylight(&items, 80);
        assert_eq!(full.len(), 4);
    }
}
