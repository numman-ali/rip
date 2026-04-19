//! Tool / task card chrome rendering.
//!
//! Builds `╭─ title ─── meta ─╮` headers, collapsed-hint footers,
//! expanded sections, and maps `ToolCardStatus` / `TaskCardStatus`
//! to metadata strings + accent styles. The block-level rendering
//! that populates the card body lives in the sibling `content`
//! module.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::canvas::{Block as CanvasBlock, CanvasMessage, TaskCardStatus, ToolCardStatus};

use super::super::theme::ThemeStyles;
use super::super::util::truncate;
use super::content::{artifact_chip_lines, block_as_lines, blocks_filter_lines, message_glyph};
use super::{
    error_style, focus_accent, muted_style, running_style, success_style, warn_style, MotionCtx,
    CARD_BODY_INDENT, GUTTER_WIDTH,
};

/// Render a tool/task card as `╭─ title ─ meta ─╮` + body rows + `╰───╯`,
/// with the gutter glyph tacked onto the top-line and a focus `▎` accent
/// when the card is the focused message.
pub(super) fn append_card_message(
    lines: &mut Vec<Line<'static>>,
    message: &CanvasMessage,
    theme: &ThemeStyles,
    focused: bool,
    card_width: usize,
) {
    let (glyph, glyph_style) = message_glyph(message, theme, MotionCtx::default());
    let (title, meta, status_style, expanded, body_sections, artifact_count) =
        card_descriptor(message);
    let border_style = if focused {
        focus_accent()
    } else {
        muted_style()
    };

    let mut top = Line::default();
    super::push_gutter(&mut top, 0, glyph, glyph_style, focused);
    top.spans.push(Span::styled(
        format_card_top_line(&title, meta.as_deref(), card_width),
        border_style,
    ));
    lines.push(top);

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
            push_card_body_line(lines, "(no detail yet)", muted_style());
        }
    }

    let mut bottom = Line::default();
    bottom.spans.push(Span::raw(" ".repeat(GUTTER_WIDTH)));
    bottom.spans.push(Span::styled(
        format_card_bottom_line(card_width),
        border_style,
    ));
    lines.push(bottom);
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

pub(super) fn format_card_top_line(title: &str, meta: Option<&str>, card_width: usize) -> String {
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

pub(super) fn format_card_bottom_line(card_width: usize) -> String {
    if card_width < 2 {
        return "╰╯".to_string();
    }
    format!("╰{}╯", "─".repeat(card_width - 2))
}
