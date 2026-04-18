use ratatui::layout::Rect;

use crate::TuiState;

pub(super) fn fmt_ms(value: Option<u64>) -> String {
    value
        .map(|ms| format!("{ms}ms"))
        .unwrap_or_else(|| "-".to_string())
}

pub(super) fn truncate(input: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }
    if input.chars().count() <= max_len {
        return input.to_string();
    }
    input
        .chars()
        .take(max_len.saturating_sub(1))
        .collect::<String>()
        + "…"
}

pub(super) fn wrapped_line_count(text: &str, width: usize) -> usize {
    let width = width.max(1);
    let mut total = 0usize;
    for line in text.split('\n') {
        let char_count = line.chars().count();
        total += if char_count == 0 {
            1
        } else {
            ((char_count - 1) / width) + 1
        };
    }
    total.max(1)
}

pub(super) fn canvas_scroll_offset(state: &TuiState, area: Rect, text: &str) -> (u16, u16) {
    let width = area.width.max(1) as usize;
    let height = area.height.max(1) as usize;
    let total_lines = wrapped_line_count(text, width);
    let max_scroll = total_lines.saturating_sub(height);
    let scroll = max_scroll.saturating_sub(state.canvas_scroll_from_bottom as usize);
    (scroll.min(u16::MAX as usize) as u16, 0)
}
