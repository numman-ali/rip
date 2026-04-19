mod cards;
mod content;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use ratatui_textarea::TextArea;

use crate::canvas::CanvasMessage;
use crate::TuiState;

use super::activity::{build_strip_line, render_activity_rail};
use super::input::render_input;
use super::status_bar::render_status_bar;
use super::theme::ThemeStyles;
use super::util::{canvas_scroll_offset, wrapped_line_count};

use cards::append_card_message;
use content::{append_simple_message, plain_text};

// Re-exported for unit tests in this module tree.
#[cfg(test)]
use cards::{format_card_bottom_line, format_card_top_line};
#[cfg(test)]
use content::subagent_slot;

const GUTTER_WIDTH: usize = 3;
const CARD_BODY_INDENT: usize = 2;

/// Canvas layout (Phase C.1+):
///
/// ```text
///  row 0         hero strip (borderless, 1 row)
///  rows 1..n-2   canvas body (borderless)
///  row n-2       activity strip (borderless, 1 row)
///  rows n-1..n   input (borderless w/ ▎ rule, currently 2 rows)
/// ```
///
/// No outer borders, no titled panes. Rhythm and gutters do the work
/// that boxes used to — see `docs/07_tasks/tui_revamp.md` Part 2.3 /
/// Part 3.1.
pub(super) fn render_canvas_screen(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    input: &TextArea<'static>,
) {
    // Input block grows with multi-line input (C.4). `input_block_rows`
    // reserves enough vertical space for the editor + keylight: always
    // 1 keylight + [1..6] editor rows, capped by the buffer's line
    // count. The activity strip hides when the editor exceeds 2 rows
    // so we never triple-squeeze the canvas.
    let editor_rows = editor_rows_needed(input, frame.area().height);
    let keylight_row = 1u16;
    let input_block = editor_rows + keylight_row;
    let show_activity = editor_rows <= 1;
    let activity_row = if show_activity { 1u16 } else { 0u16 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(activity_row),
            Constraint::Length(input_block),
        ])
        .split(frame.area());

    render_status_bar(frame, state, theme, chunks[0]);
    render_canvas_body(frame, state, theme, chunks[1]);
    if show_activity {
        render_footer_strip(frame, state, theme, chunks[2]);
    }
    render_input(frame, state, theme, chunks[3], input);
}

pub fn canvas_hit_message_id(
    state: &TuiState,
    viewport_width: u16,
    viewport_height: u16,
    row: u16,
) -> Option<String> {
    if viewport_width == 0 || viewport_height == 0 {
        return None;
    }

    let width = viewport_width as usize;
    let full_text = build_canvas_text(state, &ThemeStyles::for_theme(state.theme), width);
    let full_plain = plain_text(&full_text);
    let scroll = canvas_scroll_offset(
        state,
        Rect {
            x: 0,
            y: 0,
            width: viewport_width,
            height: viewport_height,
        },
        &full_plain,
    )
    .0 as usize;
    let target_row = scroll.saturating_add(row as usize);
    let focused = state.focused_message_id.as_deref();
    let styles = ThemeStyles::for_theme(state.theme);
    let ctx = RenderCtx {
        theme_id: state.theme,
        styles: &styles,
        motion: MotionCtx::from_state(state),
    };
    let card_width = card_width_for(width);
    let mut cursor = 0usize;

    for (idx, message) in state.canvas.messages.iter().enumerate() {
        if idx > 0 {
            if cursor == target_row {
                return None;
            }
            cursor += 1;
        }
        let mut lines = Vec::new();
        append_message(&mut lines, message, &ctx, focused, card_width);
        let plain = plain_text(&Text::from(lines));
        let wrapped = wrapped_line_count(&plain, width);
        if target_row < cursor.saturating_add(wrapped) {
            return Some(message.message_id().to_string());
        }
        cursor += wrapped;
    }

    None
}

fn editor_rows_needed(input: &TextArea<'static>, available: u16) -> u16 {
    let lines = input.lines().len().max(1) as u16;
    let cap = 6u16;
    let keylight = 1u16;
    // Keep at least 3 rows for the canvas so we never zero it out.
    let max_input_block = available.saturating_sub(3 + keylight);
    lines.min(cap).min(max_input_block.max(1))
}

pub(super) fn render_canvas_body(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
) {
    if state.activity_pinned && area.width >= 100 {
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(40), Constraint::Length(32)])
            .split(area);
        render_canvas(frame, state, theme, panes[0]);
        render_activity_rail(frame, state, theme, panes[1]);
    } else {
        render_canvas(frame, state, theme, area);
    }
}

fn render_canvas(frame: &mut Frame<'_>, state: &TuiState, theme: &ThemeStyles, area: Rect) {
    let canvas_width = area.width as usize;
    let text = build_canvas_text(state, theme, canvas_width);
    let scroll_text = plain_text(&text);
    let widget = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .scroll(canvas_scroll_offset(state, area, &scroll_text))
        .style(theme.chrome);
    frame.render_widget(widget, area);
}

/// Bottom strip above the input. Shows a single-row activity summary:
/// error → stall → running tool → running task → running job → context,
/// truncated right-to-left with an ellipsis. Hidden when there's nothing
/// to show *and* the transcript is at the bottom.
fn render_footer_strip(frame: &mut Frame<'_>, state: &TuiState, theme: &ThemeStyles, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let Some(line) = build_strip_line(state, theme, area.width as usize) else {
        return;
    };
    let widget = Paragraph::new(line);
    frame.render_widget(widget, area);
}

/// Walk `state.canvas.messages` and render each as a gutter + body pair.
/// Replaces the old `output_text` + `prompt_ranges` renderer.
pub(super) fn build_canvas_text(
    state: &TuiState,
    theme: &ThemeStyles,
    width: usize,
) -> Text<'static> {
    if state.canvas.messages.is_empty() {
        return Text::from("<no output yet>");
    }

    let card_width = card_width_for(width);
    let focused = state.focused_message_id.as_deref();
    let ctx = RenderCtx {
        theme_id: state.theme,
        styles: theme,
        motion: MotionCtx::from_state(state),
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (idx, message) in state.canvas.messages.iter().enumerate() {
        if idx > 0 {
            lines.push(Line::default());
        }
        append_message(&mut lines, message, &ctx, focused, card_width);
    }
    Text::from(lines)
}

/// Small bundle of styling + theme-id for the block renderer. The
/// theme id is only needed by the `CodeFence` path (syntect theme
/// selection); keeping it in a context struct means we don't have
/// to change every helper signature.
///
/// `motion` carries the per-frame clock tokens that drive C.9's
/// breath / thinking / streaming motion primitives. `now_ms` is the
/// current tick (from `state.now_ms`, set by the driver each frame);
/// `last_event_ms` is the wall-clock timestamp of the most recent
/// frame ingested (used as a content-driven pulse source). Both
/// default to `0` in tests — that pins the animation to a canonical
/// phase so golden snapshots stay deterministic.
#[derive(Clone, Copy)]
struct RenderCtx<'a> {
    theme_id: crate::ThemeId,
    styles: &'a ThemeStyles,
    motion: MotionCtx,
}

#[derive(Clone, Copy, Default)]
struct MotionCtx {
    now_ms: u64,
    last_event_ms: u64,
}

impl MotionCtx {
    fn from_state(state: &TuiState) -> Self {
        Self {
            now_ms: state.now_ms.unwrap_or(0),
            last_event_ms: state.last_event_ms.unwrap_or(0),
        }
    }

    /// 4-frame thinking cycle (◐ ◓ ◑ ◒) at ~400ms per frame. Pinned
    /// at phase 0 when `now_ms == 0` so tests render `◐`.
    fn thinking_glyph(&self) -> &'static str {
        const FRAMES: [&str; 4] = ["◐", "◓", "◑", "◒"];
        FRAMES[((self.now_ms / 400) % FRAMES.len() as u64) as usize]
    }

    /// Streaming pulse is content-driven — when a token arrived
    /// recently (within 350ms, slightly longer than the thinking
    /// cycle so the pulse feels reactive rather than strobing), the
    /// agent glyph promotes to `fg_primary`; otherwise it relaxes
    /// back to the base accent. If `now_ms == 0` the pulse never
    /// triggers (tests see the base style).
    fn streaming_is_hot(&self) -> bool {
        self.now_ms > 0
            && self.last_event_ms > 0
            && self.now_ms.saturating_sub(self.last_event_ms) < 350
    }
}

/// Card chrome occupies the canvas width minus the 3-col gutter (and
/// leaves at least two columns of slack for wrap safety). The minimum is
/// enough to render the corners and a placeholder label.
fn card_width_for(canvas_width: usize) -> usize {
    let available = canvas_width.saturating_sub(GUTTER_WIDTH);
    available.saturating_sub(1).max(20)
}

/// Colors here are minimal — Phase C expands `ThemeStyles` with semantic
/// muted/error/warn/accent fields driven by the Graphite/Ink token sets.
/// For B.2+ we pick restrained defaults that preserve readability across
/// existing terminals.
fn muted_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn error_style() -> Style {
    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
}

fn warn_style() -> Style {
    Style::default().fg(Color::Yellow)
}

fn success_style() -> Style {
    Style::default().fg(Color::Green)
}

fn running_style() -> Style {
    Style::default().fg(Color::Cyan)
}

fn focus_accent() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn append_message(
    lines: &mut Vec<Line<'static>>,
    message: &CanvasMessage,
    ctx: &RenderCtx<'_>,
    focused: Option<&str>,
    card_width: usize,
) {
    let focused = focused == Some(message.message_id());
    match message {
        CanvasMessage::ToolCard { .. } | CanvasMessage::TaskCard { .. } => {
            append_card_message(lines, message, ctx.styles, focused, card_width);
        }
        _ => {
            append_simple_message(lines, message, ctx, focused);
        }
    }
}

pub(super) fn push_gutter(
    line: &mut Line<'static>,
    row: usize,
    glyph: &'static str,
    glyph_style: Style,
    focused: bool,
) {
    if row == 0 {
        // Col 0: glyph. Col 1: focus rule `▎` (or blank). Col 2: spacer.
        line.spans
            .push(Span::styled(glyph.to_string(), glyph_style));
        let rule = if focused { "▎" } else { " " };
        let rule_style = if focused {
            focus_accent()
        } else {
            Style::default()
        };
        line.spans.push(Span::styled(rule.to_string(), rule_style));
        line.spans.push(Span::raw(" "));
    } else {
        let rule = if focused { "▎" } else { " " };
        let rule_style = if focused {
            focus_accent()
        } else {
            Style::default()
        };
        line.spans.push(Span::raw(" "));
        line.spans.push(Span::styled(rule.to_string(), rule_style));
        line.spans.push(Span::raw(" "));
    }
}

#[cfg(test)]
mod tests;
