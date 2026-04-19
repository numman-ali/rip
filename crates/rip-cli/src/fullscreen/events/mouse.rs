//! Mouse event routing + canvas / activity / hero hit geometry.
//!
//! The mouse pipe is the most spatial part of the input layer: every
//! click / drag / scroll must resolve to a `UiAction` (or a silent
//! state mutation) against the current frame layout. All geometry
//! uses `terminal_size()` rather than the last rendered `area` so the
//! user's click lands on what they see *now*, even if the renderer
//! hasn't caught up with a resize yet.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use crossterm::terminal::size as terminal_size;
use rip_tui::{canvas_hit_message_id, hero_click_target, HeroClickTarget, TuiState};

use super::{move_selected, UiAction};

pub(in crate::fullscreen) fn handle_mouse_event(
    mouse: MouseEvent,
    state: &mut TuiState,
) -> UiAction {
    if state.is_palette_open() {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                state.palette_move_selection(-1);
                return UiAction::None;
            }
            MouseEventKind::ScrollDown => {
                state.palette_move_selection(1);
                return UiAction::None;
            }
            _ => {}
        }
        return UiAction::None;
    }

    if state.is_thread_picker_open() {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                state.thread_picker_move_selection(-1);
                return UiAction::None;
            }
            MouseEventKind::ScrollDown => {
                state.thread_picker_move_selection(1);
                return UiAction::None;
            }
            _ => {}
        }
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return UiAction::ApplyThreadPicker;
        }
        return UiAction::None;
    }

    let (width, height) = match terminal_size() {
        Ok(size) => size,
        Err(_) => return UiAction::None,
    };

    if mouse.row == 0 && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        return match hero_click_target(state, width, mouse.column) {
            Some(HeroClickTarget::Thread) => UiAction::OpenPaletteThreads,
            Some(HeroClickTarget::Agent) => UiAction::TogglePalette,
            Some(HeroClickTarget::Model) => UiAction::OpenPaletteModels,
            None => UiAction::None,
        };
    }

    if mouse_hits_activity_surface(state, width, height, mouse.column, mouse.row) {
        return match mouse.kind {
            MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown => {
                state.set_overlay(rip_tui::Overlay::Activity);
                UiAction::None
            }
            _ => UiAction::None,
        };
    }

    if matches!(
        mouse.kind,
        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
    ) {
        if let Some((viewport_width, viewport_height, row_in_canvas)) =
            mouse_canvas_hit_geometry(state, width, height, mouse.column, mouse.row)
        {
            if let Some(message_id) =
                canvas_hit_message_id(state, viewport_width, viewport_height, row_in_canvas)
            {
                state.focused_message_id = Some(message_id);
                state.auto_follow = false;
            }
            return UiAction::None;
        }
    }

    match mouse.kind {
        MouseEventKind::ScrollUp => {
            if state.output_view == rip_tui::OutputViewMode::Rendered {
                UiAction::ScrollCanvasUp
            } else {
                state.auto_follow = false;
                move_selected(state, -6);
                UiAction::None
            }
        }
        MouseEventKind::ScrollDown => {
            if state.output_view == rip_tui::OutputViewMode::Rendered {
                UiAction::ScrollCanvasDown
            } else {
                state.auto_follow = false;
                move_selected(state, 6);
                UiAction::None
            }
        }
        _ => UiAction::None,
    }
}

fn mouse_hits_activity_surface(
    state: &TuiState,
    width: u16,
    height: u16,
    column: u16,
    row: u16,
) -> bool {
    if state.activity_pinned && width >= 100 {
        let rail_width = 32u16;
        let rail_start = width.saturating_sub(rail_width);
        if column >= rail_start && row > 0 && row < height.saturating_sub(2) {
            return true;
        }
    }

    let Some(activity_row) = mouse_footer_activity_row(height) else {
        return false;
    };
    row == activity_row
}

pub(in crate::fullscreen) fn mouse_footer_activity_row(height: u16) -> Option<u16> {
    (height >= 4).then_some(height.saturating_sub(3))
}

pub(in crate::fullscreen) fn mouse_canvas_hit_geometry(
    state: &TuiState,
    width: u16,
    height: u16,
    column: u16,
    row: u16,
) -> Option<(u16, u16, u16)> {
    let body_top = 1u16;
    let bottom_reserved = 3u16;
    let body_height = height.saturating_sub(body_top + bottom_reserved);
    if body_height == 0 || row < body_top || row >= body_top.saturating_add(body_height) {
        return None;
    }

    let viewport_width = if state.activity_pinned && width >= 100 {
        let canvas_width = width.saturating_sub(32);
        if column >= canvas_width {
            return None;
        }
        canvas_width
    } else {
        width
    };

    Some((viewport_width, body_height, row.saturating_sub(body_top)))
}
