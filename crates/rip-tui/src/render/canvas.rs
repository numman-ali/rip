use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::canvas::{
    Block as CanvasBlock, CachedText, CanvasMessage, ContextLifecycle, JobLifecycle, NoticeLevel,
    TaskCardStatus, ToolCardStatus,
};
use crate::TuiState;

use super::activity::render_activity_rail;
use super::input::render_input;
use super::status_bar::render_status_bar;
use super::theme::ThemeStyles;
use super::util::{canvas_scroll_offset, truncate};

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
    input: &str,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_status_bar(frame, state, theme, chunks[0]);
    render_canvas_body(frame, state, theme, chunks[1]);
    render_footer_strip(frame, state, theme, chunks[2]);
    render_input(frame, theme, chunks[3], input);
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

/// Bottom strip above the input. Shows the activity chip summary when
/// anything is happening (tools running, tasks queued, error present);
/// otherwise blank. `build_chips_line` was carrying this before C.1 —
/// keeping the content the same so we only move chrome here, not
/// semantics. C.2 rewrites this as the activity *strip* proper
/// (auto-hide when idle + transcript bottom, colored by worst state).
fn render_footer_strip(frame: &mut Frame<'_>, state: &TuiState, theme: &ThemeStyles, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let chips = build_chips_line(state, area.width as usize);
    let widget = Paragraph::new(Line::from(chips)).style(theme.muted);
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
#[derive(Clone, Copy)]
struct RenderCtx<'a> {
    theme_id: crate::ThemeId,
    styles: &'a ThemeStyles,
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

/// Non-card messages: a single gutter glyph + body line-rows.
fn append_simple_message(
    lines: &mut Vec<Line<'static>>,
    message: &CanvasMessage,
    ctx: &RenderCtx<'_>,
    focused: bool,
) {
    let (glyph, glyph_style) = message_glyph(message, ctx.styles);
    let body_lines = message_body_lines(message, ctx);

    for (row, body_line) in body_lines.into_iter().enumerate() {
        let mut line = Line::default();
        push_gutter(&mut line, row, glyph, glyph_style, focused);
        line.spans.extend(body_line.spans);
        lines.push(line);
    }
}

/// Render a tool/task card as `╭─ title ─ meta ─╮` + body rows + `╰───╯`,
/// with the gutter glyph tacked onto the top-line and a focus `▎` accent
/// when the card is the focused message.
fn append_card_message(
    lines: &mut Vec<Line<'static>>,
    message: &CanvasMessage,
    theme: &ThemeStyles,
    focused: bool,
    card_width: usize,
) {
    let (glyph, glyph_style) = message_glyph(message, theme);
    let (title, meta, status_style, expanded, body_sections, artifact_count) =
        card_descriptor(message);
    let border_style = if focused {
        focus_accent()
    } else {
        muted_style()
    };

    // Top line: gutter glyph + `╭─ title ─── meta ─╮`
    let mut top = Line::default();
    push_gutter(&mut top, 0, glyph, glyph_style, focused);
    top.spans.push(Span::styled(
        format_card_top_line(&title, meta.as_deref(), card_width),
        border_style,
    ));
    lines.push(top);

    // Collapsed summary or expanded sections.
    if !expanded {
        let summary = collapsed_hint(artifact_count);
        push_card_body_line(lines, " ", status_style);
        push_card_body_line(lines, &summary, status_style);
    } else {
        let mut first = true;
        for (label, body_lines) in body_sections {
            if !first {
                push_card_body_line(lines, "", theme.chrome);
            }
            first = false;
            if !label.is_empty() {
                push_card_body_line(lines, label, theme.header);
            }
            for body in body_lines {
                push_card_body_line_styled(lines, body);
            }
        }
        if first {
            // No sections — keep at least one body row so the card isn't
            // just `╭─╮ \n ╰─╯`.
            push_card_body_line(lines, "(no detail yet)", muted_style());
        }
    }

    // Bottom line.
    let mut bottom = Line::default();
    bottom.spans.push(Span::raw(" ".repeat(GUTTER_WIDTH)));
    bottom.spans.push(Span::styled(
        format_card_bottom_line(card_width),
        border_style,
    ));
    lines.push(bottom);
}

fn push_gutter(
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

fn push_card_body_line(lines: &mut Vec<Line<'static>>, text: &str, style: Style) {
    let mut line = Line::default();
    line.spans.push(Span::raw(" ".repeat(GUTTER_WIDTH)));
    line.spans.push(Span::raw(" ".repeat(CARD_BODY_INDENT)));
    line.spans.push(Span::styled(text.to_string(), style));
    lines.push(line);
}

fn push_card_body_line_styled(lines: &mut Vec<Line<'static>>, body_line: Line<'static>) {
    let mut line = Line::default();
    line.spans.push(Span::raw(" ".repeat(GUTTER_WIDTH)));
    line.spans.push(Span::raw(" ".repeat(CARD_BODY_INDENT)));
    line.spans.extend(body_line.spans);
    lines.push(line);
}

/// Descriptor extracted from a card message. Returns
/// `(title, meta, status_style, expanded, body_sections, artifact_count)`.
/// `body_sections` is `(label, Vec<Line>)` — sections are only consulted
/// when `expanded` is true; collapsed cards use the artifact count.
#[allow(clippy::type_complexity)]
fn card_descriptor(
    message: &CanvasMessage,
) -> (
    String,
    Option<String>,
    Style,
    bool,
    Vec<(&'static str, Vec<Line<'static>>)>,
    usize,
) {
    match message {
        CanvasMessage::ToolCard {
            tool_name,
            args_block,
            status,
            body,
            expanded,
            artifact_ids,
            ..
        } => {
            let meta = tool_status_meta(status);
            let status_style = tool_status_style(status);
            let mut sections = Vec::new();
            let args_lines = block_as_lines(args_block);
            if !args_lines.is_empty() {
                sections.push(("args", args_lines));
            }
            let stdout_lines =
                blocks_filter_lines(body, |b| matches!(b, CanvasBlock::ToolStdout(_)));
            if !stdout_lines.is_empty() {
                sections.push(("stdout", stdout_lines));
            }
            let stderr_lines =
                blocks_filter_lines(body, |b| matches!(b, CanvasBlock::ToolStderr(_)));
            if !stderr_lines.is_empty() {
                sections.push(("stderr", stderr_lines));
            }
            let artifact_lines = artifact_chip_lines(artifact_ids);
            if !artifact_lines.is_empty() {
                sections.push(("artifacts", artifact_lines));
            }
            (
                tool_name.clone(),
                Some(meta),
                status_style,
                *expanded,
                sections,
                artifact_ids.len(),
            )
        }
        CanvasMessage::TaskCard {
            tool_name,
            title,
            status,
            body,
            expanded,
            artifact_ids,
            ..
        } => {
            let label = match title.as_deref() {
                Some(t) if !t.is_empty() => format!("{tool_name} · {t}"),
                _ => tool_name.clone(),
            };
            let meta = task_status_meta(status);
            let status_style = task_status_style(status);
            let mut sections = Vec::new();
            let stdout_lines =
                blocks_filter_lines(body, |b| matches!(b, CanvasBlock::ToolStdout(_)));
            if !stdout_lines.is_empty() {
                sections.push(("output", stdout_lines));
            }
            let stderr_lines =
                blocks_filter_lines(body, |b| matches!(b, CanvasBlock::ToolStderr(_)));
            if !stderr_lines.is_empty() {
                sections.push(("stderr", stderr_lines));
            }
            let artifact_lines = artifact_chip_lines(artifact_ids);
            if !artifact_lines.is_empty() {
                sections.push(("artifacts", artifact_lines));
            }
            (
                label,
                Some(meta),
                status_style,
                *expanded,
                sections,
                artifact_ids.len(),
            )
        }
        _ => (String::new(), None, Style::default(), false, Vec::new(), 0),
    }
}

fn collapsed_hint(artifact_count: usize) -> String {
    if artifact_count > 0 {
        format!("{artifact_count} artifacts · ⏎ expand · x raw")
    } else {
        "⏎ expand · x raw".to_string()
    }
}

fn tool_status_meta(status: &ToolCardStatus) -> String {
    match status {
        ToolCardStatus::Running => "running".to_string(),
        ToolCardStatus::Succeeded { duration_ms, .. } => format!("✓ {duration_ms}ms"),
        ToolCardStatus::Failed { error } => {
            let trimmed = truncate(error, 28);
            format!("✕ {trimmed}")
        }
    }
}

fn tool_status_style(status: &ToolCardStatus) -> Style {
    match status {
        ToolCardStatus::Running => running_style(),
        ToolCardStatus::Succeeded { .. } => success_style(),
        ToolCardStatus::Failed { .. } => error_style(),
    }
}

fn task_status_meta(status: &TaskCardStatus) -> String {
    match status {
        TaskCardStatus::Queued => "queued".to_string(),
        TaskCardStatus::Running => "running".to_string(),
        TaskCardStatus::Exited { exit_code } => match exit_code {
            Some(code) => format!("exited {code}"),
            None => "exited".to_string(),
        },
        TaskCardStatus::Cancelled => "cancelled".to_string(),
        TaskCardStatus::Failed { error } => match error.as_deref() {
            Some(err) => format!("✕ {}", truncate(err, 24)),
            None => "✕ failed".to_string(),
        },
    }
}

fn task_status_style(status: &TaskCardStatus) -> Style {
    match status {
        TaskCardStatus::Queued | TaskCardStatus::Running => running_style(),
        TaskCardStatus::Exited {
            exit_code: Some(0) | None,
        } => success_style(),
        TaskCardStatus::Exited { .. } => warn_style(),
        TaskCardStatus::Cancelled => muted_style(),
        TaskCardStatus::Failed { .. } => error_style(),
    }
}

fn block_as_lines(block: &CanvasBlock) -> Vec<Line<'static>> {
    match block {
        CanvasBlock::Paragraph(text)
        | CanvasBlock::Markdown(text)
        | CanvasBlock::Heading { text, .. }
        | CanvasBlock::CodeFence { text, .. }
        | CanvasBlock::ToolArgsJson(text)
        | CanvasBlock::ToolStdout(text)
        | CanvasBlock::ToolStderr(text) => cached_text_lines(text),
        CanvasBlock::Thematic => vec![Line::from("────".to_string())],
        CanvasBlock::ArtifactChip { artifact_id, .. } => {
            let short: String = artifact_id.chars().take(8).collect();
            vec![Line::from(format!("⧉ {short}"))]
        }
        CanvasBlock::BlockQuote(_) | CanvasBlock::List { .. } => Vec::new(),
    }
}

/// Render a block tree as styled lines. This is the B.6 entry point
/// the AgentTurn body uses. Each block type picks an appropriate
/// presentation — headings get a bold leading hash, lists indent and
/// prefix bullets / numbers, block quotes prefix `│ `, code fences
/// run through syntect (B.7) for per-token highlighting, and
/// thematic breaks draw a muted rule.
fn render_blocks(blocks: &[CanvasBlock], ctx: &RenderCtx<'_>) -> Vec<Line<'static>> {
    let base = ctx.styles.chrome;
    let mut out = Vec::new();
    for block in blocks {
        render_block_into(block, base, 0, ctx, &mut out);
    }
    out
}

fn render_block_into(
    block: &CanvasBlock,
    base: Style,
    indent: usize,
    ctx: &RenderCtx<'_>,
    out: &mut Vec<Line<'static>>,
) {
    let pad = " ".repeat(indent);
    match block {
        CanvasBlock::Paragraph(cached) | CanvasBlock::Markdown(cached) => {
            for line in &cached.text.lines {
                out.push(prefixed_line(&pad, line, base));
            }
        }
        CanvasBlock::Heading { level, text } => {
            // ATX-style leading hashes keep the heading legible even
            // without styling (NO_COLOR / 16-color fallbacks) while
            // still feeling restrained. Body renders bold.
            let hashes = "#".repeat((*level as usize).clamp(1, 6));
            let heading_style = base.add_modifier(Modifier::BOLD);
            for (i, line) in text.text.lines.iter().enumerate() {
                let prefix = if i == 0 {
                    format!("{pad}{hashes} ")
                } else {
                    format!("{pad}{}", " ".repeat(hashes.chars().count() + 1))
                };
                out.push(prefixed_line(&prefix, line, heading_style));
            }
        }
        CanvasBlock::CodeFence { lang, text } => {
            // Top rule doubles as the language label; dimmed so it
            // reads as chrome rather than content.
            let label = match lang {
                Some(l) if !l.is_empty() => format!("```{l}"),
                _ => "```".to_string(),
            };
            out.push(Line::from(Span::styled(
                format!("{pad}{label}"),
                muted_style(),
            )));
            // Syntect highlighting (B.7). The parser stored the raw
            // source as `CachedText` lines — flatten back to a string
            // so syntect can scan it as one body; we re-emit one
            // `Line` per source line with per-token spans.
            let source = cached_source(text);
            let highlighted =
                super::syntax::highlight_fence(&source, lang.as_deref(), ctx.theme_id, ctx.styles);
            for line in highlighted {
                out.push(prefixed_line(&pad, &line, base));
            }
            out.push(Line::from(Span::styled(format!("{pad}```"), muted_style())));
        }
        CanvasBlock::BlockQuote(inner) => {
            // `│ ` gutter on every line. We render the child blocks
            // into a temporary buffer and then rewrite their prefix
            // so nested headings/lists keep working.
            let mut inner_lines = Vec::new();
            for child in inner {
                render_block_into(child, base, 0, ctx, &mut inner_lines);
            }
            for mut line in inner_lines {
                let mut spans = Vec::with_capacity(line.spans.len() + 1);
                spans.push(Span::styled(format!("{pad}│ "), muted_style()));
                spans.append(&mut line.spans);
                out.push(Line::from(spans));
            }
        }
        CanvasBlock::List { ordered, items } => {
            for (idx, item) in items.iter().enumerate() {
                let marker = if *ordered {
                    format!("{}. ", idx + 1)
                } else {
                    "• ".to_string()
                };
                let item_indent = indent + marker.chars().count();
                let mut item_lines = Vec::new();
                for (bi, block) in item.iter().enumerate() {
                    render_block_into(block, base, 0, ctx, &mut item_lines);
                    // Blank line between blocks inside the same
                    // item — skipped for the last block so lists
                    // stay compact.
                    if bi + 1 < item.len() {
                        item_lines.push(Line::default());
                    }
                }
                for (li, line) in item_lines.into_iter().enumerate() {
                    let prefix = if li == 0 {
                        format!("{pad}{marker}")
                    } else {
                        " ".repeat(item_indent)
                    };
                    let prefix_style = if li == 0 { muted_style() } else { base };
                    let mut spans = Vec::with_capacity(line.spans.len() + 1);
                    spans.push(Span::styled(prefix, prefix_style));
                    for span in line.spans {
                        spans.push(span);
                    }
                    out.push(Line::from(spans));
                }
            }
        }
        CanvasBlock::Thematic => {
            out.push(Line::from(Span::styled(
                format!("{pad}────"),
                muted_style(),
            )));
        }
        CanvasBlock::ArtifactChip { artifact_id, .. } => {
            let short: String = artifact_id.chars().take(8).collect();
            out.push(Line::from(Span::styled(
                format!("{pad}⧉ {short}"),
                muted_style(),
            )));
        }
        CanvasBlock::ToolArgsJson(cached)
        | CanvasBlock::ToolStdout(cached)
        | CanvasBlock::ToolStderr(cached) => {
            // Tool-card bodies use these; AgentTurn bodies won't
            // normally see them, but keep the match exhaustive.
            for line in &cached.text.lines {
                out.push(prefixed_line(&pad, line, base));
            }
        }
    }
}

fn prefixed_line(prefix: &str, line: &Line<'_>, style: Style) -> Line<'static> {
    let mut spans = Vec::with_capacity(line.spans.len() + 1);
    if !prefix.is_empty() {
        spans.push(Span::raw(prefix.to_string()));
    }
    for span in &line.spans {
        // Paint spans that haven't picked up any style with the
        // caller's base; pre-styled spans (future B.7 syntect tokens)
        // pass through untouched.
        let content = span.content.clone().into_owned();
        let merged = if span.style == Style::default() {
            style
        } else {
            span.style
        };
        spans.push(Span::styled(content, merged));
    }
    if spans.is_empty() {
        spans.push(Span::styled(String::new(), style));
    }
    Line::from(spans)
}

fn blocks_filter_lines(
    blocks: &[CanvasBlock],
    keep: impl Fn(&CanvasBlock) -> bool,
) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    for block in blocks.iter().filter(|b| keep(b)) {
        for line in block_as_lines(block) {
            out.push(line);
        }
    }
    out
}

fn artifact_chip_lines(ids: &[String]) -> Vec<Line<'static>> {
    ids.iter()
        .map(|id| {
            let short: String = id.chars().take(8).collect();
            Line::from(format!("⧉ {short}"))
        })
        .collect()
}

fn cached_text_lines(cached: &CachedText) -> Vec<Line<'static>> {
    cached.text.lines.to_vec()
}

fn message_glyph(message: &CanvasMessage, theme: &ThemeStyles) -> (&'static str, Style) {
    match message {
        CanvasMessage::UserTurn { .. } => ("›", theme.prompt_label),
        CanvasMessage::AgentTurn { streaming, .. } => {
            let base = theme.chrome.add_modifier(Modifier::BOLD);
            if *streaming {
                ("◎", base)
            } else {
                ("◉", base)
            }
        }
        CanvasMessage::ToolCard { .. } => ("⟡", muted_style()),
        CanvasMessage::TaskCard { .. } => ("⧉", muted_style()),
        CanvasMessage::JobNotice { .. } => ("⧉", muted_style()),
        CanvasMessage::SystemNotice { level, .. } => match level {
            NoticeLevel::Danger => ("▲", error_style()),
            NoticeLevel::Warn => ("▲", warn_style()),
            _ => ("·", muted_style()),
        },
        CanvasMessage::ContextNotice { .. } => ("⌖", muted_style()),
        CanvasMessage::CompactionCheckpoint { .. } => ("·", muted_style()),
        CanvasMessage::ExtensionPanel { .. } => ("◈", theme.chrome),
    }
}

fn message_body_lines(message: &CanvasMessage, ctx: &RenderCtx<'_>) -> Vec<Line<'static>> {
    let theme = ctx.styles;
    match message {
        CanvasMessage::UserTurn { blocks, .. } => {
            let text = paragraph_source(blocks);
            style_block_lines(&text, theme.prompt)
        }
        CanvasMessage::AgentTurn {
            blocks,
            streaming_tail,
            ..
        } => {
            // Stable blocks → structured markdown rendering (B.6).
            // Code fences pass through syntect (B.7) for per-token
            // highlighting. In-flight tail → plain text shown beneath;
            // it hasn't crossed a block boundary yet so we can't parse
            // it safely. Once the boundary arrives the collector hands
            // it off as real blocks.
            let mut lines = render_blocks(blocks, ctx);
            if !streaming_tail.is_empty() {
                for segment in streaming_tail.split('\n') {
                    lines.push(Line::from(Span::styled(segment.to_string(), theme.chrome)));
                }
            }
            lines
        }
        CanvasMessage::JobNotice {
            job_kind, status, ..
        } => {
            let summary = format!("{job_kind} · {}", job_status_label(status));
            style_block_lines(&summary, muted_style())
        }
        CanvasMessage::SystemNotice { text, level, .. } => {
            let style = match level {
                NoticeLevel::Danger => error_style(),
                NoticeLevel::Warn => warn_style(),
                _ => muted_style(),
            };
            style_block_lines(text, style)
        }
        CanvasMessage::ContextNotice {
            strategy, status, ..
        } => {
            let label = match status {
                ContextLifecycle::Selecting => "selecting",
                ContextLifecycle::Compiled => "compiled",
            };
            style_block_lines(&format!("context {strategy} · {label}"), muted_style())
        }
        CanvasMessage::CompactionCheckpoint {
            from_seq, to_seq, ..
        } => style_block_lines(
            &format!("compaction checkpoint · seq {from_seq}…{to_seq}"),
            muted_style(),
        ),
        CanvasMessage::ExtensionPanel { title, .. } => {
            style_block_lines(&format!("extension: {title}"), theme.chrome)
        }
        CanvasMessage::ToolCard { .. } | CanvasMessage::TaskCard { .. } => Vec::new(),
    }
}

fn paragraph_source(blocks: &[CanvasBlock]) -> String {
    let mut out = String::new();
    for block in blocks {
        let piece = match block {
            CanvasBlock::Paragraph(text) | CanvasBlock::Markdown(text) => cached_source(text),
            CanvasBlock::Heading { text, .. } => cached_source(text),
            CanvasBlock::CodeFence { text, .. } => cached_source(text),
            CanvasBlock::ToolArgsJson(text) => cached_source(text),
            CanvasBlock::ToolStdout(text) | CanvasBlock::ToolStderr(text) => cached_source(text),
            CanvasBlock::ArtifactChip { artifact_id, .. } => {
                let short: String = artifact_id.chars().take(8).collect();
                format!("⧉ {short}")
            }
            CanvasBlock::Thematic => "────".to_string(),
            CanvasBlock::BlockQuote(_) | CanvasBlock::List { .. } => String::new(),
        };
        if piece.is_empty() {
            continue;
        }
        out.push_str(&piece);
    }
    out
}

fn cached_source(text: &CachedText) -> String {
    let mut out = String::new();
    for (i, line) in text.text.lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        for span in &line.spans {
            out.push_str(span.content.as_ref());
        }
    }
    out
}

fn style_block_lines(source: &str, style: Style) -> Vec<Line<'static>> {
    if source.is_empty() {
        return vec![Line::default()];
    }
    source
        .split('\n')
        .map(|line| Line::from(Span::styled(line.to_string(), style)))
        .collect()
}

fn format_card_top_line(title: &str, meta: Option<&str>, card_width: usize) -> String {
    // `╭─ title ──── meta ─╮`
    // Char count: `╭─ ` (3) + title + ` ` (1) + fill + ` ` (if meta) + meta + ` ─╮` (3)
    let prefix = format!("╭─ {title} ");
    let suffix = match meta {
        Some(m) if !m.is_empty() => format!(" {m} ─╮"),
        _ => "─╮".to_string(),
    };
    let prefix_len = prefix.chars().count();
    let suffix_len = suffix.chars().count();
    let min_total = prefix_len + suffix_len;
    if min_total >= card_width {
        return format!("{prefix}{suffix}");
    }
    let fill = "─".repeat(card_width - min_total);
    format!("{prefix}{fill}{suffix}")
}

fn format_card_bottom_line(card_width: usize) -> String {
    if card_width < 2 {
        return "╰╯".to_string();
    }
    format!("╰{}╯", "─".repeat(card_width - 2))
}

fn job_status_label(status: &JobLifecycle) -> &'static str {
    match status {
        JobLifecycle::Running => "running",
        JobLifecycle::Succeeded { .. } => "succeeded",
        JobLifecycle::Failed { .. } => "failed",
        JobLifecycle::Cancelled => "cancelled",
    }
}

/// Flatten a styled `Text` back to a newline-joined string so
/// `canvas_scroll_offset` can compute wrapped-line counts without needing
/// to know about styles.
fn plain_text(text: &Text<'_>) -> String {
    let mut out = String::new();
    for (i, line) in text.lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        for span in &line.spans {
            out.push_str(span.content.as_ref());
        }
    }
    out
}

pub(super) fn build_chips_line(state: &TuiState, max_width: usize) -> String {
    let mut chips: Vec<String> = Vec::new();

    if state.awaiting_response {
        let waiting = if state.openresponses_first_provider_event_ms.is_some() {
            "working"
        } else if state.openresponses_request_started_ms.is_some() {
            "waiting"
        } else {
            "sending"
        };
        chips.push(format!("[◔ {waiting}]"));
    }

    let running_tools: Vec<&str> = state.running_tool_ids().collect();
    if !running_tools.is_empty() {
        let name = state
            .tools
            .get(running_tools[0])
            .map(|t| t.name.as_str())
            .unwrap_or("tool");
        chips.push(format!("[⟳ {name}]"));
        if running_tools.len() > 1 {
            chips.push(format!("[+{}]", running_tools.len() - 1));
        }
    } else if let Some(tool) = state.tools.values().max_by_key(|tool| tool.started_seq) {
        let chip = match &tool.status {
            crate::ToolStatus::Ended { .. } => format!("[✓ {}]", tool.name),
            crate::ToolStatus::Failed { .. } => format!("[✕ {}]", tool.name),
            crate::ToolStatus::Running => format!("[⟳ {}]", tool.name),
        };
        chips.push(chip);
    }

    let running_tasks = state.running_task_ids().count();
    if running_tasks > 0 {
        chips.push(format!("[tasks:{running_tasks}/{}]", state.tasks.len()));
    } else if !state.tasks.is_empty() {
        chips.push(format!("[tasks:{}]", state.tasks.len()));
    }

    let running_jobs = state.running_job_ids().count();
    if running_jobs > 0 {
        chips.push(format!("[jobs:{running_jobs}/{}]", state.jobs.len()));
    } else if !state.jobs.is_empty() {
        chips.push(format!("[jobs:{}]", state.jobs.len()));
    }

    if let Some(ctx) = state.context.as_ref() {
        let status = match ctx.status {
            crate::ContextStatus::Selecting => "ctx:selecting",
            crate::ContextStatus::Compiled => "ctx:compiled",
        };
        chips.push(format!("[⚙ {status}]"));
    }

    if !state.artifacts.is_empty() {
        chips.push(format!("[📄{}]", state.artifacts.len()));
    }

    if state.is_stalled(5_000) {
        chips.push("[⏸ stalled]".to_string());
    }

    if state.has_error() {
        chips.push("[⚠ error]".to_string());
    }

    let mut out = String::from("chips: ");
    out.push_str(&chips.join(" "));
    if out.chars().count() > max_width {
        out = truncate(&out, max_width);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_card_top_line_fills_dashes_when_meta_and_title_fit() {
        let top = format_card_top_line("write", Some("✓ 120ms"), 40);
        assert!(top.starts_with("╭─ write "));
        assert!(top.ends_with(" ✓ 120ms ─╮"));
        assert_eq!(top.chars().count(), 40);
    }

    #[test]
    fn format_card_top_line_degrades_gracefully_when_too_narrow() {
        let top = format_card_top_line("tool_with_long_name", Some("✓ 120ms"), 10);
        // Too narrow to fill — just concatenate.
        assert!(top.starts_with("╭─ tool_with_long_name "));
        assert!(top.ends_with("─╮"));
    }

    #[test]
    fn format_card_bottom_line_matches_width() {
        let bot = format_card_bottom_line(10);
        assert_eq!(bot, "╰────────╯");
        assert_eq!(bot.chars().count(), 10);
    }
}
