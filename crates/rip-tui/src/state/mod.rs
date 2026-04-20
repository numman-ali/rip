use std::collections::{BTreeMap, BTreeSet};

use rip_kernel::{EventKind, ToolTaskStatus};

use crate::canvas::CanvasModel;
use crate::{FrameStore, OverlayStack};

mod palette;
mod status;
mod thread_picker;
mod update;
mod view;

pub use palette::{PaletteEntry, PaletteMode, PaletteOrigin, PaletteState};
pub use status::{
    ContextStatus, ContextSummary, JobStatus, JobSummary, TaskSummary, ToolStatus, ToolSummary,
};
pub use thread_picker::{ThreadPickerEntry, ThreadPickerState};
pub use view::{OutputViewMode, Overlay, ThemeId, VimMode};

const DEFAULT_MAX_FRAMES: usize = 10_000;
const DEFAULT_MAX_PREVIEW_BYTES: usize = 8_192;

#[derive(Debug, Clone)]
pub struct TuiState {
    pub frames: FrameStore,
    pub selected_seq: Option<u64>,
    pub auto_follow: bool,
    pub canvas_scroll_from_bottom: u16,
    pub output_view: OutputViewMode,
    pub theme: ThemeId,
    /// Opt-in vim bindings for the multi-line editor (D.5). When true,
    /// the driver routes textarea keys through a minimal Normal/Insert
    /// state machine (Esc → Normal, i/a/o → Insert, h/j/k/l/w/b/0/$ in
    /// Normal, etc.). Default off so the textarea behaves like a normal
    /// emacs-ish editor for new users; toggled via the Options palette.
    pub vim_input_mode: bool,
    /// Current vim mode when `vim_input_mode` is enabled. Irrelevant
    /// otherwise. Toggling vim mode on drops the editor into Normal;
    /// toggling it off resets back to Insert so the ambient textarea
    /// behaviour is restored.
    pub vim_mode: VimMode,
    /// Pending prefix key for two-key vim Normal-mode operators (`dd`,
    /// `yy`, `gg`). `None` means no operator is pending; any completed
    /// action or unmatched follow-up clears it. Kept as a plain `char`
    /// because the full vim count/register/motion grammar is
    /// intentionally out of scope for the opt-in.
    pub vim_pending: Option<char>,
    overlay_stack: OverlayStack,
    pub activity_pinned: bool,
    pub now_ms: Option<u64>,
    pub continuity_id: Option<String>,
    pub session_id: Option<String>,
    pub start_ms: Option<u64>,
    pub first_output_ms: Option<u64>,
    pub end_ms: Option<u64>,
    pub openresponses_request_started_ms: Option<u64>,
    pub openresponses_response_headers_ms: Option<u64>,
    pub openresponses_response_first_byte_ms: Option<u64>,
    pub openresponses_first_provider_event_ms: Option<u64>,
    pub openresponses_endpoint: Option<String>,
    pub openresponses_model: Option<String>,
    pub preferred_openresponses_endpoint: Option<String>,
    pub preferred_openresponses_model: Option<String>,
    pub preferred_openresponses_reasoning_effort: Option<String>,
    pub preferred_openresponses_reasoning_summary: Option<String>,
    pub reasoning_visible: bool,
    /// Structured canvas model — the sole source of truth for agent
    /// text, tool cards, notices, and everything else the renderer
    /// walks. Streaming deltas flow through the per-AgentTurn
    /// `StreamCollector` (B.5), so there is no string shadow of the
    /// transcript — the canvas IS the transcript.
    pub canvas: CanvasModel,
    /// Focus ring over canvas messages (Phase B.4). Drives the `▎` accent
    /// rule on cards, `⏎`-expand on `ToolCard`/`TaskCard`, and the `x`
    /// route into the per-item X-ray overlay. `None` means "focus is on
    /// the input editor, nothing on the canvas is selected."
    pub focused_message_id: Option<String>,
    pub pending_prompt: Option<String>,
    pub awaiting_response: bool,
    pub status_message: Option<String>,
    pub clipboard_buffer: Option<String>,
    pub tools: BTreeMap<String, ToolSummary>,
    pub tasks: BTreeMap<String, TaskSummary>,
    pub jobs: BTreeMap<String, JobSummary>,
    pub artifacts: BTreeSet<String>,
    pub context: Option<ContextSummary>,
    pub last_error_seq: Option<u64>,
    pub last_event_ms: Option<u64>,
    max_preview_bytes: usize,
}

impl Default for TuiState {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_FRAMES)
    }
}

impl TuiState {
    pub fn new(max_frames: usize) -> Self {
        Self {
            frames: FrameStore::new(max_frames),
            selected_seq: None,
            auto_follow: true,
            canvas_scroll_from_bottom: 0,
            output_view: OutputViewMode::Rendered,
            theme: ThemeId::DefaultDark,
            vim_input_mode: false,
            vim_mode: VimMode::Insert,
            vim_pending: None,
            overlay_stack: OverlayStack::new(),
            activity_pinned: false,
            now_ms: None,
            continuity_id: None,
            session_id: None,
            start_ms: None,
            first_output_ms: None,
            end_ms: None,
            openresponses_request_started_ms: None,
            openresponses_response_headers_ms: None,
            openresponses_response_first_byte_ms: None,
            openresponses_first_provider_event_ms: None,
            openresponses_endpoint: None,
            openresponses_model: None,
            preferred_openresponses_endpoint: None,
            preferred_openresponses_model: None,
            preferred_openresponses_reasoning_effort: None,
            preferred_openresponses_reasoning_summary: None,
            reasoning_visible: true,
            canvas: CanvasModel::new(),
            focused_message_id: None,
            pending_prompt: None,
            awaiting_response: false,
            status_message: None,
            clipboard_buffer: None,
            tools: BTreeMap::new(),
            tasks: BTreeMap::new(),
            jobs: BTreeMap::new(),
            artifacts: BTreeSet::new(),
            context: None,
            last_error_seq: None,
            last_event_ms: None,
            max_preview_bytes: DEFAULT_MAX_PREVIEW_BYTES,
        }
    }

    pub fn toggle_output_view(&mut self) {
        self.output_view.toggle();
    }

    pub fn toggle_theme(&mut self) {
        self.theme.toggle();
    }

    pub fn overlay(&self) -> &Overlay {
        self.overlay_stack.top()
    }

    pub fn set_overlay(&mut self, overlay: Overlay) {
        self.overlay_stack.set(overlay);
    }

    pub fn push_overlay(&mut self, overlay: Overlay) {
        self.overlay_stack.push(overlay);
    }

    pub fn pop_overlay(&mut self) -> Option<Overlay> {
        self.overlay_stack.pop()
    }

    pub fn overlay_stack(&self) -> &OverlayStack {
        &self.overlay_stack
    }

    pub fn close_overlay(&mut self) {
        self.overlay_stack.clear();
    }

    pub fn toggle_activity_overlay(&mut self) {
        let next = match self.overlay_stack.top() {
            Overlay::Activity => Overlay::None,
            _ => Overlay::Activity,
        };
        self.overlay_stack.set(next);
    }

    pub fn toggle_tasks_overlay(&mut self) {
        let next = match self.overlay_stack.top() {
            Overlay::TaskList => Overlay::None,
            _ => Overlay::TaskList,
        };
        self.overlay_stack.set(next);
    }

    pub fn open_palette(
        &mut self,
        mode: PaletteMode,
        origin: PaletteOrigin,
        entries: Vec<PaletteEntry>,
        empty_message: impl Into<String>,
        allow_custom_value: bool,
        custom_prompt: impl Into<String>,
    ) {
        self.overlay_stack.set(Overlay::Palette(PaletteState::new(
            mode,
            origin,
            entries,
            empty_message.into(),
            allow_custom_value,
            custom_prompt.into(),
        )));
    }

    pub fn open_thread_picker(&mut self, entries: Vec<ThreadPickerEntry>) {
        self.overlay_stack
            .set(Overlay::ThreadPicker(ThreadPickerState::new(entries)));
    }

    pub fn is_palette_open(&self) -> bool {
        matches!(self.overlay_stack.top(), Overlay::Palette(_))
    }

    pub fn palette_move_selection(&mut self, delta: i32) {
        if let Some(Overlay::Palette(palette)) = self.overlay_stack.top_mut() {
            palette.move_selection(delta);
        }
    }

    pub fn palette_push_char(&mut self, ch: char) {
        if let Some(Overlay::Palette(palette)) = self.overlay_stack.top_mut() {
            palette.query.push(ch);
            palette.selected = 0;
            palette.clamp_selected();
        }
    }

    pub fn palette_backspace(&mut self) {
        if let Some(Overlay::Palette(palette)) = self.overlay_stack.top_mut() {
            palette.query.pop();
            palette.selected = 0;
            palette.clamp_selected();
        }
    }

    /// Flatten the canvas's agent-facing text (stable paragraphs + the
    /// in-flight streaming tail) into a single string. Used by the X-ray
    /// "Rendered" pane; no other code should need this — the canvas is
    /// the canonical transcript and normal rendering walks messages
    /// directly.
    pub fn rendered_agent_text(&self) -> String {
        use crate::canvas::{Block, CanvasMessage};
        let mut out = String::new();
        for message in &self.canvas.messages {
            let CanvasMessage::AgentTurn {
                reasoning_text,
                reasoning_summary,
                blocks,
                streaming_tail,
                ..
            } = message
            else {
                continue;
            };
            if self.reasoning_visible {
                let reasoning = if !reasoning_summary.trim().is_empty() {
                    Some(("Reasoning summary", reasoning_summary.as_str()))
                } else if !reasoning_text.trim().is_empty() {
                    Some(("Reasoning", reasoning_text.as_str()))
                } else {
                    None
                };
                if let Some((label, text)) = reasoning {
                    out.push_str(label);
                    out.push('\n');
                    out.push_str(text);
                    out.push('\n');
                    out.push('\n');
                }
            }
            for block in blocks {
                if let Block::Paragraph(cached) = block {
                    for line in &cached.text.lines {
                        for span in &line.spans {
                            out.push_str(&span.content);
                        }
                        out.push('\n');
                    }
                }
            }
            if !streaming_tail.is_empty() {
                out.push_str(streaming_tail);
            }
        }
        out
    }

    pub fn palette_query(&self) -> Option<&str> {
        match self.overlay_stack.top() {
            Overlay::Palette(palette) => Some(palette.query.as_str()),
            _ => None,
        }
    }

    pub fn palette_selected_value(&self) -> Option<String> {
        match self.overlay_stack.top() {
            Overlay::Palette(palette) => palette
                .selected_entry()
                .map(|entry| entry.value.clone())
                .or_else(|| palette.custom_candidate().map(ToOwned::to_owned)),
            _ => None,
        }
    }

    /// Snapshot of the currently-open palette (if any). The driver
    /// uses this in the `ApplyPalette` dispatcher to branch by mode
    /// without borrowing the overlay stack across method calls.
    pub fn palette_state_clone(&self) -> Option<PaletteState> {
        match self.overlay_stack.top() {
            Overlay::Palette(palette) => Some(palette.clone()),
            _ => None,
        }
    }

    pub fn palette_origin(&self) -> Option<PaletteOrigin> {
        match self.overlay_stack.top() {
            Overlay::Palette(palette) => Some(palette.origin),
            _ => None,
        }
    }

    pub fn is_thread_picker_open(&self) -> bool {
        matches!(self.overlay_stack.top(), Overlay::ThreadPicker(_))
    }

    pub fn thread_picker_move_selection(&mut self, delta: i32) {
        if let Some(Overlay::ThreadPicker(picker)) = self.overlay_stack.top_mut() {
            picker.move_selection(delta);
        }
    }

    pub fn thread_picker_selected_value(&self) -> Option<String> {
        match self.overlay_stack.top() {
            Overlay::ThreadPicker(picker) => {
                picker.selected_entry().map(|entry| entry.thread_id.clone())
            }
            _ => None,
        }
    }

    pub fn set_preferred_openresponses_target(
        &mut self,
        endpoint: Option<String>,
        model: Option<String>,
    ) {
        self.preferred_openresponses_endpoint = endpoint.filter(|value| !value.trim().is_empty());
        self.preferred_openresponses_model = model.filter(|value| !value.trim().is_empty());
    }

    pub fn set_preferred_openresponses_reasoning(
        &mut self,
        effort: Option<String>,
        summary: Option<String>,
    ) {
        self.preferred_openresponses_reasoning_effort =
            effort.filter(|value| !value.trim().is_empty());
        self.preferred_openresponses_reasoning_summary =
            summary.filter(|value| !value.trim().is_empty());
    }

    pub fn toggle_reasoning_visibility(&mut self) {
        self.reasoning_visible = !self.reasoning_visible;
    }

    pub fn open_selected_detail(&mut self) {
        // Prefer the most recent error, regardless of selection.
        if let Some(seq) = self.last_error_seq {
            let next = match self.overlay_stack.top() {
                Overlay::ErrorDetail { seq: current } if *current == seq => Overlay::None,
                _ => Overlay::ErrorDetail { seq },
            };
            self.overlay_stack.set(next);
            return;
        }

        let Some(event) = self.selected_event() else {
            return;
        };

        let next = match &event.kind {
            EventKind::ToolStarted { tool_id, .. }
            | EventKind::ToolStdout { tool_id, .. }
            | EventKind::ToolStderr { tool_id, .. }
            | EventKind::ToolEnded { tool_id, .. }
            | EventKind::ToolFailed { tool_id, .. } => Overlay::ToolDetail {
                tool_id: tool_id.clone(),
            },
            EventKind::ToolTaskSpawned { task_id, .. }
            | EventKind::ToolTaskStatus { task_id, .. }
            | EventKind::ToolTaskOutputDelta { task_id, .. }
            | EventKind::ToolTaskCancelRequested { task_id, .. }
            | EventKind::ToolTaskCancelled { task_id, .. } => Overlay::TaskDetail {
                task_id: task_id.clone(),
            },
            _ => Overlay::None,
        };

        let combined = match (self.overlay_stack.top(), next) {
            (Overlay::ToolDetail { tool_id: a }, Overlay::ToolDetail { tool_id: b }) if a == &b => {
                Overlay::None
            }
            (Overlay::TaskDetail { task_id: a }, Overlay::TaskDetail { task_id: b }) if a == &b => {
                Overlay::None
            }
            (_, next) => next,
        };
        self.overlay_stack.set(combined);
    }

    pub fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    pub fn set_continuity_id(&mut self, continuity_id: impl Into<String>) {
        self.continuity_id = Some(continuity_id.into());
    }

    pub fn clear_status_message(&mut self) {
        self.status_message = None;
    }

    /// Reset the UI to a fresh conversation — clears *everything*, ambient
    /// state included. Callers should not reach for this on every submit;
    /// `begin_pending_turn` used to, which meant a task spawned on turn 1
    /// vanished from the Activity strip by turn 3. The revamp plan
    /// (Part 4.3) makes continuity the default: ambient state persists
    /// across turns, only per-run timings reset.
    ///
    /// Today this is intentionally only reachable via tests and explicit
    /// operator resets (Phase C wires a "Reset conversation" palette
    /// command in that will call it).
    pub fn reset_conversation_state(&mut self) {
        self.frames.clear();
        self.selected_seq = None;
        self.auto_follow = true;
        self.canvas_scroll_from_bottom = 0;
        self.overlay_stack.clear();
        self.now_ms = None;
        self.session_id = None;
        self.start_ms = None;
        self.first_output_ms = None;
        self.end_ms = None;
        self.openresponses_request_started_ms = None;
        self.openresponses_response_headers_ms = None;
        self.openresponses_response_first_byte_ms = None;
        self.openresponses_first_provider_event_ms = None;
        self.openresponses_endpoint = None;
        self.openresponses_model = None;
        self.preferred_openresponses_reasoning_effort = None;
        self.preferred_openresponses_reasoning_summary = None;
        self.pending_prompt = None;
        self.awaiting_response = false;
        self.status_message = None;
        self.clipboard_buffer = None;
        self.tools.clear();
        self.tasks.clear();
        self.jobs.clear();
        self.artifacts.clear();
        self.context = None;
        self.last_error_seq = None;
        self.last_event_ms = None;
        self.canvas.clear();
        self.focused_message_id = None;
    }

    /// Prepare TuiState for a new run on the existing conversation.
    /// Per-run fields reset (timings, session id, pending prompt); ambient
    /// state (tools / tasks / jobs / context / canvas / frames) persists.
    fn begin_new_run(&mut self) {
        self.session_id = None;
        self.start_ms = None;
        self.first_output_ms = None;
        self.end_ms = None;
        self.openresponses_request_started_ms = None;
        self.openresponses_response_headers_ms = None;
        self.openresponses_response_first_byte_ms = None;
        self.openresponses_first_provider_event_ms = None;
        self.openresponses_endpoint = None;
        self.openresponses_model = None;
        self.pending_prompt = None;
        self.awaiting_response = false;
        self.status_message = None;
        self.selected_seq = None;
        self.canvas_scroll_from_bottom = 0;
        self.last_error_seq = None;
    }

    pub fn begin_pending_turn(&mut self, input: &str) {
        let prompt = input.trim();
        if prompt.is_empty() {
            return;
        }

        // Ambient state (canvas, tools, tasks, jobs, artifacts, context,
        // frames) persists across turns so "one chat forever" is real;
        // only per-run fields reset.
        self.begin_new_run();
        let submitted_at_ms = self.now_ms.unwrap_or(0);
        self.canvas
            .push_user_turn("user", "tui", prompt, submitted_at_ms);
        self.pending_prompt = Some(prompt.to_string());
        self.awaiting_response = true;
        self.set_status_message("sending...");
    }

    pub fn scroll_canvas_up(&mut self, lines: u16) {
        self.auto_follow = false;
        self.canvas_scroll_from_bottom = self.canvas_scroll_from_bottom.saturating_add(lines);
    }

    pub fn scroll_canvas_down(&mut self, lines: u16) {
        self.canvas_scroll_from_bottom = self.canvas_scroll_from_bottom.saturating_sub(lines);
        if self.canvas_scroll_from_bottom == 0 {
            self.auto_follow = true;
        }
    }

    /// Move the canvas focus to the previous/next focusable message.
    ///
    /// The ring is restricted to items the user can *act on* — cards,
    /// user/agent turns, error notices. Ambient job/context/compaction
    /// notices are skipped so arrow-paging doesn't flood the ring with
    /// non-interactive entries.
    pub fn focus_prev_message(&mut self) {
        self.step_focus(FocusStep::Prev);
    }

    pub fn focus_next_message(&mut self) {
        self.step_focus(FocusStep::Next);
    }

    pub fn focused_message(&self) -> Option<&crate::canvas::CanvasMessage> {
        let id = self.focused_message_id.as_deref()?;
        self.canvas.messages.iter().find(|m| m.message_id() == id)
    }

    pub fn clear_focus(&mut self) {
        self.focused_message_id = None;
    }

    /// `⏎` semantic on a focused tool/task card. Returns `true` when the
    /// focused message is a card (and its `expanded` flag was flipped),
    /// `false` otherwise — so the driver can fall back to "submit input"
    /// when the focus isn't on an expandable item.
    pub fn toggle_focused_card_expanded(&mut self) -> bool {
        let Some(id) = self.focused_message_id.clone() else {
            return false;
        };
        self.canvas.toggle_card_expanded(&id)
    }

    fn step_focus(&mut self, step: FocusStep) {
        let focusable: Vec<&str> = self
            .canvas
            .messages
            .iter()
            .filter(|m| is_focusable(m))
            .map(|m| m.message_id())
            .collect();
        if focusable.is_empty() {
            self.focused_message_id = None;
            return;
        }

        let current = self
            .focused_message_id
            .as_deref()
            .and_then(|id| focusable.iter().position(|candidate| *candidate == id));

        let next_idx = match (current, step) {
            (None, FocusStep::Prev) => focusable.len() - 1,
            (None, FocusStep::Next) => 0,
            (Some(idx), FocusStep::Prev) => {
                if idx == 0 {
                    focusable.len() - 1
                } else {
                    idx - 1
                }
            }
            (Some(idx), FocusStep::Next) => (idx + 1) % focusable.len(),
        };
        self.focused_message_id = Some(focusable[next_idx].to_string());
    }

    pub fn set_now_ms(&mut self, now_ms: u64) {
        self.now_ms = Some(now_ms);
    }

    pub fn is_stalled(&self, threshold_ms: u64) -> bool {
        if self.end_ms.is_some() {
            return false;
        }
        let Some(now_ms) = self.now_ms else {
            return false;
        };
        let Some(last_ms) = self.last_event_ms else {
            return false;
        };
        now_ms.saturating_sub(last_ms) >= threshold_ms
    }

    pub fn has_error(&self) -> bool {
        self.last_error_seq.is_some()
    }

    pub fn running_tool_ids(&self) -> impl Iterator<Item = &str> {
        self.tools.iter().filter_map(|(id, tool)| {
            matches!(tool.status, ToolStatus::Running).then_some(id.as_str())
        })
    }

    pub fn running_task_ids(&self) -> impl Iterator<Item = &str> {
        self.tasks.iter().filter_map(|(id, task)| {
            matches!(
                task.status,
                ToolTaskStatus::Queued | ToolTaskStatus::Running
            )
            .then_some(id.as_str())
        })
    }

    pub fn running_job_ids(&self) -> impl Iterator<Item = &str> {
        self.jobs
            .iter()
            .filter_map(|(id, job)| matches!(job.status, JobStatus::Running).then_some(id.as_str()))
    }
}

#[derive(Debug, Clone, Copy)]
enum FocusStep {
    Prev,
    Next,
}

fn is_focusable(message: &crate::canvas::CanvasMessage) -> bool {
    use crate::canvas::CanvasMessage::*;
    matches!(
        message,
        UserTurn { .. }
            | AgentTurn { .. }
            | ToolCard { .. }
            | TaskCard { .. }
            | SystemNotice { .. }
            | ExtensionPanel { .. }
    )
}

#[cfg(test)]
mod tests;
