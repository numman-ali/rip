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
pub use render::{render, RenderMode};
pub use state::{
    ContextStatus, ContextSummary, JobStatus, JobSummary, OutputViewMode, Overlay, PaletteEntry,
    PaletteMode, PaletteState, TaskSummary, ThemeId, ToolStatus, ToolSummary, TuiState,
};
