//! Command palette mode (Phase C.5).
//!
//! The Command mode is the workspace's front door — every action
//! discoverable by type-ahead. Entries are grouped semantically
//! (`CANVAS`, `THREADS`, `RUNS`, `OPTIONS`, `DEBUG`, `SYSTEM`) and the
//! palette renderer shows group titles between entry blocks.
//!
//! Each entry carries a stable `value` (the `CommandAction` serialized
//! as a string — `"canvas.scroll-bottom"`, `"threads.new"`, etc.) that
//! the driver maps to an actual capability call. Mapping a string back
//! to a `CommandAction` happens via `CommandAction::from_value` so
//! drivers don't have to re-enumerate the list.
//!
//! **Deferred entries.** Per the revamp plan (Parts 16 + 17), a
//! handful of entries ship *visible but disabled* because their
//! capability isn't in the registry yet (e.g. `threads.new` — no
//! `thread.create` capability today). The palette renderer dims
//! disabled entries and tags them with an `unavailable` chip; the
//! driver should refuse to apply them and surface a one-line toast
//! instead.

use crate::PaletteEntry;

use super::super::PaletteSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAction {
    // Canvas
    ScrollCanvasTop,
    ScrollCanvasBottom,
    FollowTail,
    PrevMessage,
    NextMessage,
    PrevError,
    CopyLastMessage,
    CopySelection,
    ClearSelection,
    // Threads
    NewThread,
    SwitchThread,
    BranchThread,
    HandoffThread,
    RenameThread,
    CompactionRunNow,
    CompactionSchedule,
    CompactionStatus,
    // Runs / Models
    RetryLastTurn,
    StopStreaming,
    SwitchModel,
    RotateProviderCursor,
    ProviderCursorStatus,
    ContextSelectionStatus,
    ConfigDoctor,
    // Options
    ToggleTheme,
    ToggleAutoFollow,
    ToggleReasoningVisibility,
    ToggleVimInputMode,
    ToggleMouseCapture,
    PinActivityRail,
    // Debug
    OpenXrayOnFocused,
    ShowDebugInfo,
    ShowFrameStoreStats,
    CopyLastErrorBreadcrumb,
    // System
    ReloadKeybindings,
    ReloadTheme,
    Quit,
}

impl CommandAction {
    pub const ALL: &'static [CommandAction] = &[
        CommandAction::ScrollCanvasTop,
        CommandAction::ScrollCanvasBottom,
        CommandAction::FollowTail,
        CommandAction::PrevMessage,
        CommandAction::NextMessage,
        CommandAction::PrevError,
        CommandAction::CopyLastMessage,
        CommandAction::CopySelection,
        CommandAction::ClearSelection,
        CommandAction::NewThread,
        CommandAction::SwitchThread,
        CommandAction::BranchThread,
        CommandAction::HandoffThread,
        CommandAction::RenameThread,
        CommandAction::CompactionRunNow,
        CommandAction::CompactionSchedule,
        CommandAction::CompactionStatus,
        CommandAction::RetryLastTurn,
        CommandAction::StopStreaming,
        CommandAction::SwitchModel,
        CommandAction::RotateProviderCursor,
        CommandAction::ProviderCursorStatus,
        CommandAction::ContextSelectionStatus,
        CommandAction::ConfigDoctor,
        CommandAction::ToggleTheme,
        CommandAction::ToggleAutoFollow,
        CommandAction::ToggleReasoningVisibility,
        CommandAction::ToggleVimInputMode,
        CommandAction::ToggleMouseCapture,
        CommandAction::PinActivityRail,
        CommandAction::OpenXrayOnFocused,
        CommandAction::ShowDebugInfo,
        CommandAction::ShowFrameStoreStats,
        CommandAction::CopyLastErrorBreadcrumb,
        CommandAction::ReloadKeybindings,
        CommandAction::ReloadTheme,
        CommandAction::Quit,
    ];

    pub fn id(&self) -> &'static str {
        match self {
            CommandAction::ScrollCanvasTop => "canvas.scroll-top",
            CommandAction::ScrollCanvasBottom => "canvas.scroll-bottom",
            CommandAction::FollowTail => "canvas.follow-tail",
            CommandAction::PrevMessage => "canvas.prev-message",
            CommandAction::NextMessage => "canvas.next-message",
            CommandAction::PrevError => "canvas.prev-error",
            CommandAction::CopyLastMessage => "canvas.copy-last-message",
            CommandAction::CopySelection => "canvas.copy-selection",
            CommandAction::ClearSelection => "canvas.clear-selection",
            CommandAction::NewThread => "threads.new",
            CommandAction::SwitchThread => "threads.switch",
            CommandAction::BranchThread => "threads.branch",
            CommandAction::HandoffThread => "threads.handoff",
            CommandAction::RenameThread => "threads.rename",
            CommandAction::CompactionRunNow => "compaction.run",
            CommandAction::CompactionSchedule => "compaction.schedule",
            CommandAction::CompactionStatus => "compaction.status",
            CommandAction::RetryLastTurn => "runs.retry-turn",
            CommandAction::StopStreaming => "runs.stop-streaming",
            CommandAction::SwitchModel => "runs.switch-model",
            CommandAction::RotateProviderCursor => "runs.rotate-cursor",
            CommandAction::ProviderCursorStatus => "runs.cursor-status",
            CommandAction::ContextSelectionStatus => "runs.context-status",
            CommandAction::ConfigDoctor => "runs.config-doctor",
            CommandAction::ToggleTheme => "options.theme",
            CommandAction::ToggleAutoFollow => "options.auto-follow",
            CommandAction::ToggleReasoningVisibility => "options.reasoning",
            CommandAction::ToggleVimInputMode => "options.vim",
            CommandAction::ToggleMouseCapture => "options.mouse",
            CommandAction::PinActivityRail => "options.pin-activity",
            CommandAction::OpenXrayOnFocused => "debug.xray-focused",
            CommandAction::ShowDebugInfo => "debug.show-info",
            CommandAction::ShowFrameStoreStats => "debug.frame-stats",
            CommandAction::CopyLastErrorBreadcrumb => "debug.copy-error",
            CommandAction::ReloadKeybindings => "system.reload-keys",
            CommandAction::ReloadTheme => "system.reload-theme",
            CommandAction::Quit => "system.quit",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            CommandAction::ScrollCanvasTop => "Scroll to top",
            CommandAction::ScrollCanvasBottom => "Scroll to bottom",
            CommandAction::FollowTail => "Toggle follow-tail",
            CommandAction::PrevMessage => "Previous message",
            CommandAction::NextMessage => "Next message",
            CommandAction::PrevError => "Jump to previous error",
            CommandAction::CopyLastMessage => "Copy last message",
            CommandAction::CopySelection => "Copy selection",
            CommandAction::ClearSelection => "Clear selection",
            CommandAction::NewThread => "New thread",
            CommandAction::SwitchThread => "Switch thread",
            CommandAction::BranchThread => "Branch current thread",
            CommandAction::HandoffThread => "Handoff to new thread",
            CommandAction::RenameThread => "Rename current thread",
            CommandAction::CompactionRunNow => "Run compaction now",
            CommandAction::CompactionSchedule => "Schedule compaction",
            CommandAction::CompactionStatus => "Compaction status",
            CommandAction::RetryLastTurn => "Retry last turn",
            CommandAction::StopStreaming => "Stop streaming",
            CommandAction::SwitchModel => "Switch model",
            CommandAction::RotateProviderCursor => "Rotate provider cursor",
            CommandAction::ProviderCursorStatus => "Provider cursor status",
            CommandAction::ContextSelectionStatus => "Context selection status",
            CommandAction::ConfigDoctor => "Run config doctor",
            CommandAction::ToggleTheme => "Toggle theme (graphite / ink)",
            CommandAction::ToggleAutoFollow => "Toggle auto-follow",
            CommandAction::ToggleReasoningVisibility => "Toggle reasoning visibility",
            CommandAction::ToggleVimInputMode => "Toggle vim input mode",
            CommandAction::ToggleMouseCapture => "Toggle mouse capture",
            CommandAction::PinActivityRail => "Pin activity rail (L only)",
            CommandAction::OpenXrayOnFocused => "Open X-ray on focused item",
            CommandAction::ShowDebugInfo => "Show debug info",
            CommandAction::ShowFrameStoreStats => "Show frame store stats",
            CommandAction::CopyLastErrorBreadcrumb => "Copy last error breadcrumb",
            CommandAction::ReloadKeybindings => "Reload keybindings",
            CommandAction::ReloadTheme => "Reload theme",
            CommandAction::Quit => "Quit",
        }
    }

    pub fn category(&self) -> &'static str {
        match self {
            CommandAction::ScrollCanvasTop
            | CommandAction::ScrollCanvasBottom
            | CommandAction::FollowTail
            | CommandAction::PrevMessage
            | CommandAction::NextMessage
            | CommandAction::PrevError
            | CommandAction::CopyLastMessage
            | CommandAction::CopySelection
            | CommandAction::ClearSelection => "CANVAS",
            CommandAction::NewThread
            | CommandAction::SwitchThread
            | CommandAction::BranchThread
            | CommandAction::HandoffThread
            | CommandAction::RenameThread
            | CommandAction::CompactionRunNow
            | CommandAction::CompactionSchedule
            | CommandAction::CompactionStatus => "THREADS",
            CommandAction::RetryLastTurn
            | CommandAction::StopStreaming
            | CommandAction::SwitchModel
            | CommandAction::RotateProviderCursor
            | CommandAction::ProviderCursorStatus
            | CommandAction::ContextSelectionStatus
            | CommandAction::ConfigDoctor => "RUNS",
            CommandAction::ToggleTheme
            | CommandAction::ToggleAutoFollow
            | CommandAction::ToggleReasoningVisibility
            | CommandAction::ToggleVimInputMode
            | CommandAction::ToggleMouseCapture
            | CommandAction::PinActivityRail => "OPTIONS",
            CommandAction::OpenXrayOnFocused
            | CommandAction::ShowDebugInfo
            | CommandAction::ShowFrameStoreStats
            | CommandAction::CopyLastErrorBreadcrumb => "DEBUG",
            CommandAction::ReloadKeybindings | CommandAction::ReloadTheme | CommandAction::Quit => {
                "SYSTEM"
            }
        }
    }

    /// `true` when the action's backing capability is available today.
    /// Disabled entries ship *visible* with the `unavailable` chip so
    /// users can see what's coming without the palette pretending it
    /// does something it doesn't.
    pub fn is_available(&self) -> bool {
        !matches!(
            self,
            // `thread.create` / `thread.rename` / `thread.ensure_named`
            // are not in the capability registry today (Part 17 of
            // the plan), so these ride as [deferred] until the
            // runtime ships them.
            CommandAction::NewThread | CommandAction::RenameThread
        )
    }

    pub fn from_value(value: &str) -> Option<CommandAction> {
        CommandAction::ALL
            .iter()
            .copied()
            .find(|action| action.id() == value)
    }
}

#[derive(Debug, Clone, Default)]
pub struct CommandMode;

impl CommandMode {
    pub fn new() -> Self {
        Self
    }
}

impl PaletteSource for CommandMode {
    fn id(&self) -> &'static str {
        "command"
    }

    fn label(&self) -> &str {
        "Command"
    }

    fn placeholder(&self) -> &str {
        "search commands"
    }

    fn entries(&self) -> Vec<PaletteEntry> {
        CommandAction::ALL
            .iter()
            .map(|action| PaletteEntry {
                value: action.id().to_string(),
                title: action.title().to_string(),
                subtitle: Some(action.category().to_string()),
                chips: if action.is_available() {
                    Vec::new()
                } else {
                    vec!["unavailable".to_string()]
                },
            })
            .collect()
    }

    fn empty_state(&self) -> &str {
        "No commands match"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_mode_ships_all_declared_entries() {
        let mode = CommandMode::new();
        let entries = mode.entries();
        assert_eq!(entries.len(), CommandAction::ALL.len());
        assert!(entries.len() >= 25, "palette must expose ≥25 commands");
    }

    #[test]
    fn deferred_entries_are_chipped_unavailable() {
        let mode = CommandMode::new();
        let entries = mode.entries();
        let new_thread = entries
            .iter()
            .find(|e| e.value == "threads.new")
            .expect("threads.new present");
        assert!(new_thread.chips.iter().any(|c| c == "unavailable"));
    }

    #[test]
    fn from_value_round_trips_all_actions() {
        for action in CommandAction::ALL {
            let parsed = CommandAction::from_value(action.id()).expect("round trip");
            assert_eq!(parsed, *action);
        }
    }

    #[test]
    fn every_action_carries_category_and_title() {
        for action in CommandAction::ALL {
            assert!(!action.title().is_empty());
            assert!(!action.category().is_empty());
        }
    }
}
