//! Block and message-body rendering for canvas items.
//!
//! This module turns structured `CanvasMessage` content (blocks, inline
//! text, artifact chips, streaming tails) into `Vec<Line<'static>>` ready
//! for the canvas `Paragraph`. The card-chrome rules (corners, meta,
//! status styling) live in the sibling `cards` module; everything here
//! stays pure and style-only.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};

use crate::canvas::{
    AgentRole, Block as CanvasBlock, CachedText, CanvasMessage, ContextLifecycle, JobLifecycle,
    NoticeLevel,
};

use super::super::theme::ThemeStyles;
use super::{error_style, muted_style, warn_style, MotionCtx, RenderCtx};

/// Non-card messages: a single gutter glyph + body line-rows.
pub(super) fn append_simple_message(
    lines: &mut Vec<Line<'static>>,
    message: &CanvasMessage,
    ctx: &RenderCtx<'_>,
    focused: bool,
) {
    let (glyph, glyph_style) = message_glyph(message, ctx.styles, ctx.motion);
    let body_lines = message_body_lines(message, ctx);

    for (row, body_line) in body_lines.into_iter().enumerate() {
        let mut line = Line::default();
        super::push_gutter(&mut line, row, glyph, glyph_style, focused);
        line.spans.extend(body_line.spans);
        lines.push(line);
    }
}

pub(super) fn block_as_lines(block: &CanvasBlock) -> Vec<Line<'static>> {
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
pub(super) fn render_blocks(blocks: &[CanvasBlock], ctx: &RenderCtx<'_>) -> Vec<Line<'static>> {
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
            let label = match lang {
                Some(l) if !l.is_empty() => format!("```{l}"),
                _ => "```".to_string(),
            };
            out.push(Line::from(Span::styled(
                format!("{pad}{label}"),
                muted_style(),
            )));
            let source = cached_source(text);
            let highlighted = super::super::syntax::highlight_fence(
                &source,
                lang.as_deref(),
                ctx.theme_id,
                ctx.styles,
            );
            for line in highlighted {
                out.push(prefixed_line(&pad, &line, base));
            }
            out.push(Line::from(Span::styled(format!("{pad}```"), muted_style())));
        }
        CanvasBlock::BlockQuote(inner) => {
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

pub(super) fn blocks_filter_lines(
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

pub(super) fn artifact_chip_lines(ids: &[String]) -> Vec<Line<'static>> {
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

pub(super) fn message_glyph(
    message: &CanvasMessage,
    theme: &ThemeStyles,
    motion: MotionCtx,
) -> (&'static str, Style) {
    match message {
        CanvasMessage::UserTurn { .. } => ("›", theme.prompt_label),
        CanvasMessage::AgentTurn {
            streaming,
            role,
            blocks,
            streaming_tail,
            ..
        } => match role {
            AgentRole::Primary => {
                let base = theme.header;
                if !*streaming {
                    return ("◉", base);
                }
                let awaiting_first_token = blocks.is_empty() && streaming_tail.is_empty();
                if awaiting_first_token {
                    (motion.thinking_glyph(), base)
                } else if motion.streaming_is_hot() {
                    ("◎", base.add_modifier(Modifier::BOLD))
                } else {
                    ("◎", base)
                }
            }
            AgentRole::Subagent { parent_run_id } => ("◈", subagent_style(theme, parent_run_id)),
            AgentRole::Reviewer { .. } => ("◌", theme.reviewer),
            AgentRole::Extension { .. } => ("◈", theme.extension),
        },
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

fn subagent_style(theme: &ThemeStyles, parent_run_id: &str) -> Style {
    theme.subagent_accents[subagent_slot(parent_run_id)]
}

pub(super) fn subagent_slot(parent_run_id: &str) -> usize {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    parent_run_id.hash(&mut hasher);
    (hasher.finish() as usize) % 4
}

pub(super) fn message_body_lines(
    message: &CanvasMessage,
    ctx: &RenderCtx<'_>,
) -> Vec<Line<'static>> {
    let theme = ctx.styles;
    match message {
        CanvasMessage::UserTurn { blocks, .. } => {
            let text = paragraph_source(blocks);
            style_block_lines(&text, theme.prompt)
        }
        CanvasMessage::AgentTurn {
            reasoning_text,
            reasoning_summary,
            blocks,
            streaming_tail,
            streaming,
            ..
        } => {
            let mut lines = Vec::new();
            if ctx.reasoning_visible {
                lines.extend(reasoning_lines(
                    reasoning_text,
                    reasoning_summary,
                    *streaming,
                    blocks.is_empty() && streaming_tail.is_empty(),
                    theme,
                ));
                if !lines.is_empty() && (!blocks.is_empty() || !streaming_tail.is_empty()) {
                    lines.push(Line::default());
                }
            }
            lines.extend(render_blocks(blocks, ctx));
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

fn reasoning_lines(
    reasoning_text: &str,
    reasoning_summary: &str,
    streaming: bool,
    awaiting_first_token: bool,
    theme: &ThemeStyles,
) -> Vec<Line<'static>> {
    let body = if !reasoning_summary.trim().is_empty() {
        Some(("reasoning summary", reasoning_summary))
    } else if !reasoning_text.trim().is_empty() {
        Some(("thinking", reasoning_text))
    } else {
        None
    };

    let Some((label, body)) = body else {
        return Vec::new();
    };

    let label = if streaming && awaiting_first_token {
        "thinking"
    } else {
        label
    };

    let mut out = Vec::new();
    out.push(Line::from(Span::styled(
        label.to_string(),
        theme.quiet.add_modifier(Modifier::BOLD),
    )));
    for segment in body.split('\n') {
        out.push(Line::from(vec![
            Span::styled("│ ".to_string(), theme.quiet),
            Span::styled(segment.to_string(), theme.muted),
        ]));
    }
    out
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
pub(super) fn plain_text(text: &Text<'_>) -> String {
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
