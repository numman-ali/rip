//! Borderless input editor + state-aware keylight (Phase C.3 / C.4).
//!
//! The input zone is two rows: a `▎`-gutter editor row on top, and a
//! keylight strip underneath. C.3 makes the keylight reconfigure per
//! situation — idle / typing / thinking / streaming / error / overlay
//! open — so the user sees the bindings that actually help them *now*
//! without a fixed ribbon of irrelevant keys.
//!
//! C.4 swaps the hand-rolled `String` buffer for `ratatui-textarea`:
//! the textarea draws its own cursor + content in the body slot, and
//! we still own the gutter glyph (`▎ › `), the context-aware
//! placeholder, and the keylight below.
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
use ratatui_textarea::TextArea;

use crate::{Overlay, TuiState};

use super::theme::ThemeStyles;
use super::util::truncate;

pub(super) fn render_input(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
    input: &TextArea<'static>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    // Editor grows with the number of lines in the textarea, bounded by
    // the 6-row cap from Part 7 of the plan. The area we're given
    // already reserves one row for the keylight, so we split
    // `area.height - 1` between the editor and the keylight; everything
    // above the cap keeps scrolling internally via the textarea's
    // viewport.
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
        render_keylight_row(
            frame,
            state,
            theme,
            chunks[1],
            !buffer_is_effectively_empty(input),
        );
    }
}

/// How many rows the editor needs: `max(1, lines)`, clamped to the
/// available area minus the keylight (1 row) and to the revamp's
/// 6-row cap. Empty input always gets 1 row so the prompt glyph is
/// visible.
pub(super) fn editor_rows_for(input: &TextArea<'static>, available: u16) -> u16 {
    let lines = input.lines().len().max(1) as u16;
    let cap = 6u16; // Part 7: editor grows up to 6 rows
    let keylight_reserve = 1u16;
    lines
        .min(cap)
        .min(available.saturating_sub(keylight_reserve))
}

/// Whitespace-only counts as empty for the keylight / placeholder
/// decisions. The textarea's own `is_empty()` reports true only when
/// there's literally no character at all — typing a single space
/// already flips it to non-empty, which would toggle the placeholder
/// off mid-type. Matching on `trim().is_empty()` keeps the keylight
/// feeling calm while the user pauses.
fn buffer_is_effectively_empty(input: &TextArea<'static>) -> bool {
    input.lines().iter().all(|line| line.trim().is_empty())
}

fn render_editor_row(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
    input: &TextArea<'static>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    // Gutter is fixed at 4 cols: `▎` (1) + space (1) + `› ` (2). The
    // body area is everything to the right — that is where the
    // textarea renders itself (including its cursor) when the buffer
    // is non-empty, and where we draw the context-aware placeholder
    // ourselves when the buffer is empty (the textarea's own
    // placeholder is static, and ours is state-aware).
    let gutter_width = 4u16.min(area.width);
    let gutter = Rect {
        x: area.x,
        y: area.y,
        width: gutter_width,
        height: area.height,
    };
    let body = Rect {
        x: area.x.saturating_add(gutter_width),
        y: area.y,
        width: area.width.saturating_sub(gutter_width),
        height: area.height,
    };

    // Gutter row 0 hosts either the prompt arrow `›` or the breath
    // glyph `·` (C.9 motion). The breath kicks in only when the
    // operator is *fully* idle — no streaming, no in-flight turn, no
    // overlay, empty buffer. That way the dot is a genuine signal
    // that RIP is waiting on the operator, not ornament.
    let (lead_glyph, lead_style) = gutter_lead(state, theme, input);
    let mut gutter_lines: Vec<Line<'static>> = Vec::with_capacity(area.height as usize);
    for row in 0..area.height {
        let spans = if row == 0 {
            vec![
                Span::styled("▎".to_string(), theme.header),
                Span::raw(" "),
                Span::styled(format!("{lead_glyph} "), lead_style),
            ]
        } else {
            vec![
                Span::styled("▎".to_string(), theme.header),
                Span::raw("   "),
            ]
        };
        gutter_lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(gutter_lines).style(Style::default()), gutter);

    if body.width == 0 || body.height == 0 {
        return;
    }

    if buffer_is_effectively_empty(input) {
        let placeholder = placeholder_for(state);
        let line = Line::from(Span::styled(
            truncate(placeholder, body.width as usize),
            theme.quiet,
        ));
        frame.render_widget(Paragraph::new(vec![line]), body);
    } else {
        // The textarea draws its buffer + cursor + any scroll state in
        // the body rect. We keep its default cursor style so the
        // block-reverse cursor reads correctly under both Graphite
        // (dark) and Ink (light) themes.
        frame.render_widget(input, body);
    }
}

/// Pick the row-0 lead glyph: `·` (breath) when fully idle, else `›`
/// (prompt). The breath cycles through two quiet colors so the dot
/// reads as alive without stealing attention.
fn gutter_lead(
    state: &TuiState,
    theme: &ThemeStyles,
    input: &TextArea<'static>,
) -> (&'static str, Style) {
    if is_fully_idle(state, input) {
        let phase = breath_phase(state.now_ms.unwrap_or(0));
        let style = match phase {
            BreathPhase::Quiet => theme.quiet,
            BreathPhase::Muted => theme.muted,
        };
        ("·", style)
    } else {
        ("›", theme.header)
    }
}

fn is_fully_idle(state: &TuiState, input: &TextArea<'static>) -> bool {
    matches!(state.overlay(), Overlay::None)
        && !state.awaiting_response
        && state.pending_prompt.is_none()
        && buffer_is_effectively_empty(input)
}

/// 2400ms 2-phase breath cycle. Pinned at phase 0 when `now_ms == 0`
/// so tests render the quiet phase.
fn breath_phase(now_ms: u64) -> BreathPhase {
    const CYCLE_MS: u64 = 2400;
    let phase = now_ms % CYCLE_MS;
    if (800..1600).contains(&phase) {
        BreathPhase::Muted
    } else {
        BreathPhase::Quiet
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BreathPhase {
    Quiet,
    Muted,
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
    typing: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let width = area.width as usize;
    let indent = Span::raw("   ");
    let keylight = keylight_for(state, typing);
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

/// State-aware keylight segments. `typing` is true when the editor
/// buffer has non-whitespace content — that's when send / newline
/// become the headline keys, regardless of other state.
///
/// Key glyphs (⏎ / ⇧⏎ / ⌃K / …) are the *advertised* bindings; the
/// driver still accepts the ASCII fallbacks (Enter / Alt-Enter / Ctrl-K).
/// The keylight should show the real default keymap, not a "Mac-like"
/// alias that terminals never actually send.
pub(super) fn keylight_for(state: &TuiState, typing: bool) -> Vec<(&'static str, &'static str)> {
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
            return vec![("⎋", "stop"), ("[", "prev msg"), ("]", "next msg")];
        }
        return vec![("⎋", "stop"), ("[", "prev msg")];
    }

    if typing {
        return vec![("⏎", "send"), ("⇧⏎", "newline"), ("⌃K", "palette")];
    }

    vec![
        ("?", "help"),
        ("⌃K", "command"),
        ("⌥M", "models"),
        ("click", "top row"),
    ]
}

/// Render the keylight as a single truncated string. Legacy test
/// helper — only present for unit tests that assert truncation works
/// on narrow terminals. Uses the idle / no-input keylight so the
/// behavior is predictable.
#[cfg(test)]
pub(super) fn build_help_line(max_width: usize) -> String {
    let state = TuiState::new(10);
    let segments = keylight_for(&state, false);
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

    fn keys(state: &TuiState, typing: bool) -> Vec<&'static str> {
        keylight_for(state, typing)
            .into_iter()
            .map(|(k, _)| k)
            .collect()
    }

    #[test]
    fn idle_state_shows_navigation_defaults() {
        let state = TuiState::new(10);
        assert_eq!(keys(&state, false), vec!["?", "⌃K", "⌥M", "click"]);
    }

    #[test]
    fn typing_swaps_headline_keys_to_send_and_newline() {
        let state = TuiState::new(10);
        let k = keys(&state, true);
        assert_eq!(&k[..3], &["⏎", "⇧⏎", "⌃K"]);
    }

    #[test]
    fn thinking_exposes_stop_only_in_keylight() {
        let mut state = TuiState::new(10);
        state.awaiting_response = true;
        let k = keys(&state, false);
        assert!(k.contains(&"⎋"));
        assert!(!k.contains(&"?"));
    }

    #[test]
    fn streaming_adds_message_navigation_shortcuts() {
        let mut state = TuiState::new(10);
        state.awaiting_response = true;
        state.start_ms = Some(0);
        state.first_output_ms = Some(5);
        let k = keys(&state, false);
        assert!(k.contains(&"⎋"));
        assert!(k.contains(&"["));
        assert!(k.contains(&"]"));
    }

    #[test]
    fn error_state_shows_recovery_actions() {
        let mut state = TuiState::new(10);
        state.last_error_seq = Some(3);
        let k = keys(&state, false);
        assert_eq!(k, vec!["r", "c", "x", "⎋"]);
    }

    #[test]
    fn palette_overlay_shows_palette_controls() {
        let mut state = TuiState::new(10);
        state.open_palette(
            crate::PaletteMode::Command,
            crate::PaletteOrigin::TopCenter,
            Vec::new(),
            "no results",
            false,
            "",
        );
        let k = keys(&state, false);
        assert_eq!(k, vec!["↑↓", "⏎", "⇥", "⎋"]);
    }

    #[test]
    fn generic_overlay_shows_close_scroll_raw() {
        let mut state = TuiState::new(10);
        state.set_overlay(Overlay::Debug);
        let k = keys(&state, false);
        assert_eq!(k, vec!["⎋", "↑↓", "x"]);
    }

    #[test]
    fn gutter_lead_swaps_to_breath_only_when_fully_idle() {
        let theme = ThemeStyles::for_theme(crate::ThemeId::DefaultDark);
        let buffer = TextArea::default();

        // Fully idle → breath glyph.
        let idle = TuiState::new(10);
        assert_eq!(gutter_lead(&idle, &theme, &buffer).0, "·");

        // Streaming → prompt glyph stays put so the input zone still
        // reads as an input affordance.
        let mut streaming = TuiState::new(10);
        streaming.awaiting_response = true;
        assert_eq!(gutter_lead(&streaming, &theme, &buffer).0, "›");

        // Non-empty buffer → prompt glyph (the user is typing).
        let mut typing_input = TextArea::default();
        typing_input.insert_str("hi");
        assert_eq!(gutter_lead(&idle, &theme, &typing_input).0, "›");
    }

    #[test]
    fn breath_phase_wraps_every_cycle() {
        // Phase 0: quiet (the canonical zero-phase for snapshots).
        assert_eq!(breath_phase(0), BreathPhase::Quiet);
        assert_eq!(breath_phase(400), BreathPhase::Quiet);
        // 800..1600 → muted (middle of the breath).
        assert_eq!(breath_phase(800), BreathPhase::Muted);
        assert_eq!(breath_phase(1200), BreathPhase::Muted);
        // 1600..2400 → back to quiet.
        assert_eq!(breath_phase(1600), BreathPhase::Quiet);
        // Wraps after 2400ms.
        assert_eq!(breath_phase(2400), BreathPhase::Quiet);
        assert_eq!(breath_phase(3200), BreathPhase::Muted);
    }

    #[test]
    fn fit_keylight_never_drops_below_two_entries() {
        let items = vec![
            ("⏎", "send"),
            ("⌃K", "palette"),
            ("⌥M", "models"),
            ("⌃G", "go to"),
        ];
        let fit = fit_keylight(&items, 0);
        assert_eq!(fit.len(), 2);
        let full = fit_keylight(&items, 80);
        assert_eq!(full.len(), 4);
    }
}
