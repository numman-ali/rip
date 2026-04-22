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
use ratatui::layout::Rect;
use ratatui_textarea::TextArea;
use rip_tui::{
    canvas_hit_message_id, canvas_screen_regions, hero_click_target, overlay_mouse_target,
    HeroClickTarget, OverlayMouseTarget, TuiState,
};

use super::{move_selected, UiAction};

pub(in crate::fullscreen) fn handle_mouse_event(
    mouse: MouseEvent,
    state: &mut TuiState,
    input: &TextArea<'static>,
) -> UiAction {
    let (width, height) = match terminal_size() {
        Ok(size) => size,
        Err(_) => return UiAction::None,
    };

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
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return match overlay_mouse_target(
                state,
                Rect {
                    x: 0,
                    y: 0,
                    width,
                    height,
                },
                mouse.column,
                mouse.row,
            ) {
                OverlayMouseTarget::PaletteEntry(selected) => {
                    state.palette_set_selected(selected);
                    UiAction::ApplyPalette
                }
                OverlayMouseTarget::Outside => {
                    state.close_overlay();
                    UiAction::None
                }
                _ => UiAction::None,
            };
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
            return match overlay_mouse_target(
                state,
                Rect {
                    x: 0,
                    y: 0,
                    width,
                    height,
                },
                mouse.column,
                mouse.row,
            ) {
                OverlayMouseTarget::ThreadPickerEntry(selected) => {
                    state.thread_picker_set_selected(selected);
                    UiAction::ApplyThreadPicker
                }
                OverlayMouseTarget::Outside => {
                    state.close_overlay();
                    UiAction::None
                }
                _ => UiAction::None,
            };
        }
        return UiAction::None;
    }

    if state.overlay_owns_input() {
        return match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => match overlay_mouse_target(
                state,
                Rect {
                    x: 0,
                    y: 0,
                    width,
                    height,
                },
                mouse.column,
                mouse.row,
            ) {
                OverlayMouseTarget::Outside => {
                    state.close_overlay();
                    UiAction::None
                }
                _ => UiAction::None,
            },
            MouseEventKind::ScrollUp => {
                if state.overlay_is_scrollable() {
                    state.scroll_overlay_up(3);
                }
                UiAction::None
            }
            MouseEventKind::ScrollDown => {
                if state.overlay_is_scrollable() {
                    state.scroll_overlay_down(3);
                }
                UiAction::None
            }
            _ => UiAction::None,
        };
    }

    if mouse.row == 0 && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        return match hero_click_target(state, width, mouse.column) {
            Some(HeroClickTarget::Thread) => UiAction::OpenPaletteThreads,
            Some(HeroClickTarget::Agent) => UiAction::TogglePalette,
            Some(HeroClickTarget::Model) => UiAction::OpenPaletteModels,
            None => UiAction::None,
        };
    }

    if mouse_hits_activity_surface(state, input, width, height, mouse.column, mouse.row) {
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
            mouse_canvas_hit_geometry(state, input, width, height, mouse.column, mouse.row)
        {
            if let Some(message_id) =
                canvas_hit_message_id(state, viewport_width, viewport_height, row_in_canvas)
            {
                state.set_focused_message(message_id);
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
    input: &TextArea<'static>,
    width: u16,
    height: u16,
    column: u16,
    row: u16,
) -> bool {
    let regions = canvas_screen_regions(
        state,
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        },
        input,
    );

    if let Some(activity_rail) = regions.activity_rail {
        if column >= activity_rail.x
            && column < activity_rail.x.saturating_add(activity_rail.width)
            && row >= activity_rail.y
            && row < activity_rail.y.saturating_add(activity_rail.height)
        {
            return true;
        }
    }

    regions
        .activity_footer
        .is_some_and(|activity_footer| row == activity_footer.y)
}

#[cfg(test)]
pub(in crate::fullscreen) fn mouse_footer_activity_row(height: u16) -> Option<u16> {
    (height >= 4).then_some(height.saturating_sub(3))
}

pub(in crate::fullscreen) fn mouse_canvas_hit_geometry(
    state: &TuiState,
    input: &TextArea<'static>,
    width: u16,
    height: u16,
    column: u16,
    row: u16,
) -> Option<(u16, u16, u16)> {
    let regions = canvas_screen_regions(
        state,
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        },
        input,
    );
    let canvas = regions.canvas;
    if canvas.height == 0
        || row < canvas.y
        || row >= canvas.y.saturating_add(canvas.height)
        || column < canvas.x
        || column >= canvas.x.saturating_add(canvas.width)
    {
        return None;
    }

    Some((canvas.width, canvas.height, row.saturating_sub(canvas.y)))
}
