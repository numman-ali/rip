//! Error recovery overlay (Phase C.10).
//!
//! Surfaced automatically when a provider-error frame lands for a
//! run; carries the erroring `seq` so the `x` action can open the
//! X-ray window scoped to that error. Actions map to capabilities:
//!
//! - `r` → `thread.post_message` (re-posts the last user turn; the
//!   kernel spawns the retry run per the capability contract).
//! - `c` → `thread.provider_cursor.rotate`.
//! - `m` → opens the Models palette so the operator can switch
//!   before retrying.
//! - `x` → X-ray overlay scoped to this seq.
//! - `⎋` → dismiss (error chip stays in the activity strip).
//!
//! The overlay itself is render-only; the driver (`rip-cli`)
//! interprets the key events and dispatches the corresponding
//! capability call.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::TuiState;

use super::super::theme::ThemeStyles;
use super::super::util::truncate;

pub(super) fn render_error_recovery_overlay(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
    seq: u64,
) {
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Recover from provider error")
        .style(theme.danger);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width = inner.width as usize;

    let error_summary = state
        .frames
        .iter()
        .find(|frame| frame.seq == seq)
        .map(crate::summary::event_summary)
        .unwrap_or_else(|| format!("provider error at seq {seq}"));

    let mut lines = vec![
        Line::from(vec![
            Span::styled("▲  ", theme.danger),
            Span::styled(
                truncate(&error_summary, width.saturating_sub(5)),
                theme.danger,
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            truncate(
                "The current run is paused. Pick a recovery action:",
                width.saturating_sub(2),
            ),
            theme.chrome,
        )),
        Line::from(""),
    ];

    let action = |key: &str, label: &str, desc: &str| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("  {key}  "), theme.accent),
            Span::styled(label.to_string(), theme.chrome),
            Span::styled(
                truncate(
                    &format!("   {desc}"),
                    width
                        .saturating_sub(4 + key.len() + label.len())
                        .saturating_sub(3),
                ),
                theme.muted,
            ),
        ])
    };

    lines.push(action("r", "retry turn", "re-posts last user message"));
    lines.push(action(
        "c",
        "rotate cursor",
        "thread.provider_cursor.rotate",
    ));
    lines.push(action("m", "switch model", "opens Models palette"));
    lines.push(action("x", "open X-ray", "raw frames around this error"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        truncate(
            "⎋ dismiss — error chip stays on the activity strip",
            width.saturating_sub(2),
        ),
        theme.quiet,
    )));

    let paragraph = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .style(theme.chrome);
    frame.render_widget(paragraph, inner);
}
