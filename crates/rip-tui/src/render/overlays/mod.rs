use ratatui::layout::Rect;
use ratatui::Frame;

use crate::{OutputViewMode, Overlay, PaletteOrigin, TuiState};

use super::theme::ThemeStyles;
use super::RenderMode;

pub(super) mod activity;
pub(super) mod debug;
pub(super) mod error;
pub(super) mod error_recovery;
pub(super) mod help;
pub(super) mod palette;
pub(super) mod stall;
pub(super) mod task_detail;
pub(super) mod task_list;
pub(super) mod thread_picker;
pub(super) mod tool_detail;

pub(super) fn render_overlay(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    mode: RenderMode,
) {
    let body = overlay_body_area(frame.area(), state.output_view);
    match state.overlay() {
        Overlay::None => {}
        Overlay::Activity => activity::render_activity_overlay(frame, state, theme, body),
        Overlay::Palette(palette_state) => palette::render_palette_overlay(
            frame,
            state,
            theme,
            overlay_modal_area_for_origin(body, palette_state.origin),
        ),
        Overlay::ThreadPicker(_) => thread_picker::render_thread_picker_overlay(
            frame,
            state,
            theme,
            thread_picker_area(body),
        ),
        Overlay::TaskList => task_list::render_task_list_overlay(frame, state, theme, body),
        Overlay::ToolDetail { tool_id } => tool_detail::render_tool_detail_overlay(
            frame,
            state,
            theme,
            overlay_modal_area(body),
            tool_id,
            mode,
        ),
        Overlay::TaskDetail { task_id } => task_detail::render_task_detail_overlay(
            frame,
            state,
            theme,
            overlay_modal_area(body),
            task_id,
        ),
        Overlay::ErrorDetail { seq } => {
            error::render_error_overlay(frame, state, theme, overlay_modal_area(body), *seq)
        }
        Overlay::StallDetail => {
            stall::render_stall_overlay(frame, state, theme, overlay_modal_area(body))
        }
        Overlay::Debug => {
            debug::render_debug_overlay(frame, state, theme, overlay_modal_area(body))
        }
        Overlay::Help => help::render_help_overlay(frame, state, theme, overlay_modal_area(body)),
        Overlay::ErrorRecovery { seq } => error_recovery::render_error_recovery_overlay(
            frame,
            state,
            theme,
            overlay_modal_area(body),
            *seq,
        ),
    }
}

pub(super) fn overlay_body_area(area: Rect, view: OutputViewMode) -> Rect {
    // After C.1 the outer chrome is borderless: 1-row hero on top, and
    // a 2-row input block on the bottom (editor row + keylight row).
    // The activity strip (1 row between body and input) is not an
    // overlay target — overlays peel over it too so the focus tint
    // reaches the editor.
    let top = 1;
    let bottom = 2;
    let y = area.y.saturating_add(top);
    let height = area.height.saturating_sub(top + bottom).max(1);

    // In X-ray, we allow overlays to cover most of the viewport, but still keep the input visible.
    let _ = view;
    Rect {
        x: area.x,
        y,
        width: area.width.max(1),
        height,
    }
}

pub(super) fn overlay_modal_area(body: Rect) -> Rect {
    let margin_x = (body.width / 10).max(2);
    let margin_y = (body.height / 10).max(1);
    Rect {
        x: body.x.saturating_add(margin_x),
        y: body.y.saturating_add(margin_y),
        width: body.width.saturating_sub(margin_x.saturating_mul(2)).max(1),
        height: body
            .height
            .saturating_sub(margin_y.saturating_mul(2))
            .max(1),
    }
}

pub(super) fn overlay_modal_area_for_origin(body: Rect, origin: PaletteOrigin) -> Rect {
    let base = overlay_modal_area(body);
    let width = (body.width.saturating_mul(3) / 5).clamp(30, body.width.max(1));
    let height = base.height.min(body.height.max(1));
    let centered_x = body.x.saturating_add(body.width.saturating_sub(width) / 2);
    let centered_y = body
        .y
        .saturating_add(body.height.saturating_sub(height) / 2);
    let top_y = body.y.saturating_add(1);
    let bottom_y = body
        .y
        .saturating_add(body.height.saturating_sub(height.saturating_add(1)));
    let left_x = body.x.saturating_add(1);
    let right_x = body
        .x
        .saturating_add(body.width.saturating_sub(width.saturating_add(1)));

    match origin {
        PaletteOrigin::TopCenter => Rect {
            x: centered_x,
            y: top_y,
            width,
            height,
        },
        PaletteOrigin::TopRight => Rect {
            x: right_x,
            y: top_y,
            width,
            height,
        },
        PaletteOrigin::TopLeft => Rect {
            x: left_x,
            y: top_y,
            width,
            height,
        },
        PaletteOrigin::Center => Rect {
            x: centered_x,
            y: centered_y,
            width,
            height,
        },
        PaletteOrigin::BottomCenter => Rect {
            x: centered_x,
            y: bottom_y,
            width,
            height,
        },
    }
}

fn thread_picker_area(body: Rect) -> Rect {
    let margin_x = (body.width / 8).max(2);
    let margin_y = (body.height / 12).max(1);
    Rect {
        x: body.x.saturating_add(margin_x),
        y: body.y.saturating_add(margin_y),
        width: body.width.saturating_sub(margin_x.saturating_mul(2)).max(1),
        height: body
            .height
            .saturating_sub(margin_y.saturating_mul(2))
            .max(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_origin_biases_modal_area() {
        let body = Rect {
            x: 0,
            y: 1,
            width: 100,
            height: 30,
        };

        let top_left = overlay_modal_area_for_origin(body, PaletteOrigin::TopLeft);
        let top_right = overlay_modal_area_for_origin(body, PaletteOrigin::TopRight);
        let bottom = overlay_modal_area_for_origin(body, PaletteOrigin::BottomCenter);

        assert!(top_left.x < top_right.x);
        assert_eq!(top_left.y, top_right.y);
        assert!(bottom.y > top_left.y);
    }
}
