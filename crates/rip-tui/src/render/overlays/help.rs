//! Help overlay (Phase C.7).
//!
//! The Help overlay is a searchable keybinding + command reference
//! built from the same metadata that drives the Command palette —
//! categories, titles, `CommandAction` ids. No separate table of
//! hotkeys is maintained; the overlay reads the canonical list and
//! formats it into two columns (title · category). A future phase
//! will layer in the bound shortcut per entry once the keymap knows
//! how to reverse-look-up by command id; until then the overlay
//! still does its job as a discoverable command index.
//!
//! The overlay is *static* — it doesn't take a query today. The
//! intent is to pop into Help, scroll with arrows, Esc to close.
//! Searching lives in the Command palette (`⌃K`) which already
//! filters by query; Help is the "give me the menu" view.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::palette::modes::command::CommandAction;
use crate::TuiState;

use super::super::theme::ThemeStyles;
use super::super::util::truncate;

pub(super) fn render_help_overlay(
    frame: &mut Frame<'_>,
    _state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
) {
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Help")
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let width = inner.width as usize;
    let mut current_category = "";

    lines.push(Line::from(Span::styled(
        truncate(
            "Help is reference. ⌃K opens the action palette. ? opens Help only when the composer is empty.",
            width.saturating_sub(2),
        ),
        theme.chrome,
    )));
    lines.push(Line::from(Span::styled(
        truncate(
            "The top row is clickable: thread opens Threads, agent opens Commands, model opens Models.",
            width.saturating_sub(2),
        ),
        theme.muted,
    )));
    lines.push(Line::from(Span::styled(
        truncate(
            "Mouse wheel scrolls the canvas. Home jumps to the top, End follows the live tail again.",
            width.saturating_sub(2),
        ),
        theme.muted,
    )));
    lines.push(Line::from(Span::styled(
        truncate(
            "Direct shortcuts: ⌥M models, ⌃G go to, ⌃T threads, ⌥O options, ⌃Y copy.",
            width.saturating_sub(2),
        ),
        theme.muted,
    )));
    lines.push(Line::from(Span::styled(
        truncate(
            "In the palette, the bracketed tab is the one Enter will act in.",
            width.saturating_sub(2),
        ),
        theme.muted,
    )));
    lines.push(Line::from(""));

    for action in CommandAction::ALL {
        if action.category() != current_category {
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                action.category().to_string(),
                theme.muted,
            )));
            current_category = action.category();
        }

        let chip = if action.is_available() {
            ""
        } else {
            "  · unavailable"
        };
        let line_text = truncate(
            &format!("{}{}", action.title(), chip),
            width.saturating_sub(4),
        );
        let style = if action.is_available() {
            theme.chrome
        } else {
            theme.quiet
        };
        lines.push(Line::from(vec![
            Span::styled("  ", style),
            Span::styled(line_text, style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        truncate(
            "⎋ close   ⌃K action palette   type in the palette to filter",
            width.saturating_sub(2),
        ),
        theme.quiet,
    )));

    let paragraph = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .style(theme.chrome);
    frame.render_widget(paragraph, inner);
}
