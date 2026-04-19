pub mod canvas;
mod frame_store;
mod overlay;
pub mod palette;
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
pub use render::{canvas_hit_message_id, hero_click_target, render, HeroClickTarget, RenderMode};
pub use state::{
    ContextStatus, ContextSummary, JobStatus, JobSummary, OutputViewMode, Overlay, PaletteEntry,
    PaletteMode, PaletteOrigin, PaletteState, TaskSummary, ThemeId, ThreadPickerEntry,
    ThreadPickerState, ToolStatus, ToolSummary, TuiState, VimMode,
};
