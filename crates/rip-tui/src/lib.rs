pub mod canvas;
mod frame_store;
mod overlay;
pub mod palette;
mod provider_event;
mod render;
mod state;
mod summary;

pub use canvas::{
    AgentRole, Block, CachedText, CanvasMessage, CanvasModel, ContextLifecycle, JobLifecycle,
    NoticeLevel, PanelPlacement, StyledLine, TaskCardStatus, ToolCardStatus,
};
pub use frame_store::FrameStore;
pub use overlay::{OverlayStack, OverlayView};
pub use palette::{ModelRoute, ModelsMode, PaletteSource, ResolvedModelRoute};
pub use render::{
    canvas_hit_message_id, canvas_screen_regions, hero_click_target, overlay_mouse_target, render,
    reveal_focused_canvas_message, CanvasScreenRegions, HeroClickTarget, OverlayMouseTarget,
    RenderMode,
};
pub use state::{
    ContextStatus, ContextSummary, JobStatus, JobSummary, OutputViewMode, Overlay, PaletteEntry,
    PaletteMode, PaletteOrigin, PaletteState, TaskSummary, ThemeId, ThreadPickerEntry,
    ThreadPickerState, ToolStatus, ToolSummary, TuiState, VimMode,
};
