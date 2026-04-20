//! Structured canvas message model (Phase B.1).
//!
//! The old canvas was a `String` + byte-range `prompt_ranges`; this module
//! replaces it with a typed list of `CanvasMessage`s. B.1 populates messages
//! alongside the old `output_text`; B.2 deletes the string path and walks
//! `messages` to render. Later phases (B.5 StreamCollector, B.6 pulldown-cmark,
//! B.7 syntect) fill in `Block` richness without changing the outer shape.

use std::hash::{Hash, Hasher};

use ratatui::text::{Line, Text};
use rip_kernel::ToolTaskExecutionMode;
use serde_json::Value;

use super::stream_collector::StreamCollector;

/// Pre-rendered ratatui `Text` with a hash of its source for cache
/// invalidation.
///
/// **Theme invariant (B.8).** `CachedText` MUST NOT contain
/// theme-dependent styling. The markdown parser stores only
/// `Span::raw` content here; per-token colors (including syntect
/// highlighting for `Block::CodeFence`) are applied at render time.
/// This means toggling the theme (`TuiState::toggle_theme`) only
/// needs the next frame to repaint — no cache invalidation, no
/// re-parse, no block rewrite. Any future phase that wants to cache
/// *styled* spans needs to either (a) re-cache on theme swap or
/// (b) carry a theme tag in `source_hash` so stale cache is
/// detectable.
#[derive(Debug, Clone)]
pub struct CachedText {
    pub text: Text<'static>,
    pub source_hash: u64,
}

impl CachedText {
    pub fn plain(source: &str) -> Self {
        let lines = source
            .split('\n')
            .map(|line| Line::from(line.to_string()))
            .collect::<Vec<_>>();
        Self {
            text: Text::from(lines),
            source_hash: hash_str(source),
        }
    }

    pub fn empty() -> Self {
        Self::plain("")
    }
}

fn hash_str(source: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

/// Semantic blocks inside a canvas message. B.1 populates `Paragraph`,
/// `ToolArgsJson`, `ToolStdout`, `ToolStderr`, and `ArtifactChip`; the
/// markdown primitives land in B.6.
#[derive(Debug, Clone)]
pub enum Block {
    Paragraph(CachedText),
    Heading {
        level: u8,
        text: CachedText,
    },
    Markdown(CachedText),
    CodeFence {
        lang: Option<String>,
        text: CachedText,
    },
    BlockQuote(Vec<Block>),
    List {
        ordered: bool,
        items: Vec<Vec<Block>>,
    },
    Thematic,
    ToolArgsJson(CachedText),
    ToolStdout(CachedText),
    ToolStderr(CachedText),
    ArtifactChip {
        artifact_id: String,
        bytes: Option<u64>,
    },
}

/// Which actor produced this contribution. Required (never optional) so the
/// multi-actor canvas never collapses into "one user + one agent".
#[derive(Debug, Clone)]
pub enum AgentRole {
    Primary,
    Subagent { parent_run_id: String },
    Reviewer { target_message_id: String },
    Extension { kind: String },
}

#[derive(Debug, Clone)]
pub enum ToolCardStatus {
    Running,
    Succeeded { duration_ms: u64, exit_code: i32 },
    Failed { error: String },
}

#[derive(Debug, Clone)]
pub enum TaskCardStatus {
    Queued,
    Running,
    Exited { exit_code: Option<i32> },
    Cancelled,
    Failed { error: Option<String> },
}

#[derive(Debug, Clone)]
pub enum JobLifecycle {
    Running,
    Succeeded { result: Option<Value> },
    Failed { error: Option<String> },
    Cancelled,
}

#[derive(Debug, Clone)]
pub enum ContextLifecycle {
    Selecting,
    Compiled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeLevel {
    Quiet,
    Info,
    Warn,
    Danger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelPlacement {
    Inline,
    Overlay,
    ActivityChip,
}

#[derive(Debug, Clone)]
pub struct StyledLine {
    pub text: String,
    pub accent: Option<String>,
}

/// A single canvas entry. Ordered append-only; never mutated except the
/// streaming tail (which flips `streaming: false` on `SessionEnded`) and
/// cards whose status advances (`Running → Succeeded/Failed`).
#[derive(Debug, Clone)]
pub enum CanvasMessage {
    UserTurn {
        message_id: String,
        actor_id: String,
        origin: String,
        blocks: Vec<Block>,
        submitted_at_ms: u64,
    },
    AgentTurn {
        message_id: String,
        run_session_id: String,
        agent_id: Option<String>,
        role: AgentRole,
        actor_id: String,
        model: Option<String>,
        reasoning_text: String,
        reasoning_summary: String,
        blocks: Vec<Block>,
        /// Transient in-flight text held by the StreamCollector (B.5).
        /// The renderer shows this beneath `blocks` while streaming;
        /// `SessionEnded` flushes it into a final `Block::Paragraph`.
        /// Populated only while `streaming == true`.
        streaming_tail: String,
        /// Incremental markdown scan state for the in-flight tail.
        /// Keeps streaming code blocks from rescanning the whole tail
        /// on every delta.
        streaming_collector: StreamCollector,
        streaming: bool,
        started_at_ms: u64,
        ended_at_ms: Option<u64>,
    },
    ToolCard {
        message_id: String,
        tool_id: String,
        tool_name: String,
        args_block: Block,
        status: ToolCardStatus,
        body: Vec<Block>,
        expanded: bool,
        artifact_ids: Vec<String>,
        started_seq: u64,
        started_at_ms: u64,
    },
    TaskCard {
        message_id: String,
        task_id: String,
        tool_name: String,
        title: Option<String>,
        execution_mode: ToolTaskExecutionMode,
        status: TaskCardStatus,
        body: Vec<Block>,
        expanded: bool,
        artifact_ids: Vec<String>,
        started_at_ms: Option<u64>,
    },
    JobNotice {
        message_id: String,
        job_id: String,
        job_kind: String,
        details: Option<Value>,
        status: JobLifecycle,
        actor_id: String,
        origin: String,
        started_at_ms: Option<u64>,
        ended_at_ms: Option<u64>,
    },
    SystemNotice {
        message_id: String,
        level: NoticeLevel,
        text: String,
        origin_event_kind: String,
        seq: u64,
    },
    ContextNotice {
        message_id: String,
        run_session_id: String,
        strategy: String,
        status: ContextLifecycle,
        bundle_artifact_id: Option<String>,
        contributed_artifact_ids: Vec<String>,
    },
    CompactionCheckpoint {
        message_id: String,
        checkpoint_id: String,
        from_seq: u64,
        to_seq: u64,
        summary_artifact_id: String,
    },
    ExtensionPanel {
        message_id: String,
        panel_id: String,
        extension_id: String,
        title: String,
        placement: PanelPlacement,
        lines: Vec<StyledLine>,
        keys: Vec<(String, String)>,
        artifact_ids: Vec<String>,
    },
}

impl CanvasMessage {
    pub fn message_id(&self) -> &str {
        match self {
            CanvasMessage::UserTurn { message_id, .. }
            | CanvasMessage::AgentTurn { message_id, .. }
            | CanvasMessage::ToolCard { message_id, .. }
            | CanvasMessage::TaskCard { message_id, .. }
            | CanvasMessage::JobNotice { message_id, .. }
            | CanvasMessage::SystemNotice { message_id, .. }
            | CanvasMessage::ContextNotice { message_id, .. }
            | CanvasMessage::CompactionCheckpoint { message_id, .. }
            | CanvasMessage::ExtensionPanel { message_id, .. } => message_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_text_plain_preserves_lines_and_hashes_source() {
        let a = CachedText::plain("hello\nworld");
        let b = CachedText::plain("hello\nworld");
        assert_eq!(a.text.lines.len(), 2);
        assert_eq!(a.source_hash, b.source_hash);
        let c = CachedText::plain("hello\nearth");
        assert_ne!(a.source_hash, c.source_hash);
    }

    #[test]
    fn message_id_returns_the_right_field_for_each_variant() {
        let cases: &[CanvasMessage] = &[
            CanvasMessage::UserTurn {
                message_id: "u1".into(),
                actor_id: "user".into(),
                origin: "tui".into(),
                blocks: Vec::new(),
                submitted_at_ms: 0,
            },
            CanvasMessage::SystemNotice {
                message_id: "s1".into(),
                level: NoticeLevel::Info,
                text: "hi".into(),
                origin_event_kind: "test".into(),
                seq: 0,
            },
        ];
        assert_eq!(cases[0].message_id(), "u1");
        assert_eq!(cases[1].message_id(), "s1");
    }
}
