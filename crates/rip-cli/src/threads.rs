use clap::Subcommand;
use serde::{Deserialize, Serialize};

#[derive(Subcommand)]
pub(crate) enum ThreadsCommand {
    /// Ensure the default continuity exists and print `{"thread_id": ...}`.
    Ensure,
    /// List known continuities and print a JSON array.
    List,
    /// Get continuity metadata and print JSON.
    Get {
        /// Thread id (continuity id).
        id: String,
    },
    /// Create a new thread branched from a parent.
    Branch {
        /// Parent thread id (continuity id).
        id: String,
        /// Optional title/name for the new thread.
        #[arg(long)]
        title: Option<String>,
        /// Branch from a specific message id (turn anchor).
        #[arg(long)]
        from_message_id: Option<String>,
        /// Branch from an explicit parent continuity seq (power/debug).
        #[arg(long)]
        from_seq: Option<u64>,
        /// Actor id (provenance).
        #[arg(long)]
        actor_id: Option<String>,
        /// Origin (provenance).
        #[arg(long)]
        origin: Option<String>,
    },
    /// Create a new thread as a handoff from a parent, carrying curated context.
    Handoff {
        /// Source thread id (continuity id).
        id: String,
        /// Optional title/name for the new thread.
        #[arg(long)]
        title: Option<String>,
        /// Curated summary/context bundle as markdown.
        #[arg(long)]
        summary_markdown: Option<String>,
        /// Curated summary/context bundle as an artifact id.
        #[arg(long)]
        summary_artifact_id: Option<String>,
        /// Handoff from a specific message id (turn anchor).
        #[arg(long)]
        from_message_id: Option<String>,
        /// Handoff from an explicit parent continuity seq (power/debug).
        #[arg(long)]
        from_seq: Option<u64>,
        /// Actor id (provenance).
        #[arg(long)]
        actor_id: Option<String>,
        /// Origin (provenance).
        #[arg(long)]
        origin: Option<String>,
    },
    /// Append a deterministic compaction checkpoint + summary artifact reference to a thread.
    CompactionCheckpoint {
        /// Thread id (continuity id).
        id: String,
        /// Summary as markdown (written as a `rip.compaction_summary.v1` artifact).
        #[arg(long)]
        summary_markdown: Option<String>,
        /// Existing `rip.compaction_summary.v1` artifact id to reference.
        #[arg(long)]
        summary_artifact_id: Option<String>,
        /// Explicit cut point message id (must be a `continuity_message_appended` event id).
        #[arg(long)]
        to_message_id: Option<String>,
        /// Explicit cut point seq (must be a `continuity_message_appended` seq).
        #[arg(long)]
        to_seq: Option<u64>,
        /// Deterministic cut points by message count stride (default: 10_000 when no cut point is specified).
        #[arg(long)]
        stride_messages: Option<u64>,
        /// Actor id (provenance).
        #[arg(long)]
        actor_id: Option<String>,
        /// Origin (provenance).
        #[arg(long)]
        origin: Option<String>,
    },
    /// Compute deterministic compaction cut points for a thread (stride-based; message boundaries only).
    CompactionCutPoints {
        /// Thread id (continuity id).
        id: String,
        /// Message stride used to compute cut points (default: 10_000).
        #[arg(long)]
        stride_messages: Option<u64>,
        /// Maximum number of cut points returned (latest-first; default: 1).
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Show a truth-derived compaction status projection for a thread.
    CompactionStatus {
        /// Thread id (continuity id).
        id: String,
        /// Message stride used to compute the “next cut point” projection (default: 10_000).
        #[arg(long)]
        stride_messages: Option<u64>,
    },
    /// Show a truth-derived provider cursor cache status projection for a thread.
    ProviderCursorStatus {
        /// Thread id (continuity id).
        id: String,
    },
    /// Rotate/reset the active provider cursor cache for a thread (truth-log only).
    ProviderCursorRotate {
        /// Thread id (continuity id).
        id: String,
        /// Optional stable reason for the rotation/reset (logged as truth).
        #[arg(long)]
        reason: Option<String>,
        /// Actor id (provenance).
        #[arg(long)]
        actor_id: Option<String>,
        /// Origin (provenance).
        #[arg(long)]
        origin: Option<String>,
    },
    /// Show truth-derived context selection strategy decisions for a thread.
    ContextSelectionStatus {
        /// Thread id (continuity id).
        id: String,
        /// Maximum number of decisions returned (latest-first; default: 10).
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Run deterministic auto-compaction: spawn a summarizer job that emits checkpoints.
    CompactionAuto {
        /// Thread id (continuity id).
        id: String,
        /// Message stride used to compute cut points (default: 10_000).
        #[arg(long)]
        stride_messages: Option<u64>,
        /// Maximum number of new checkpoints created per invocation (default: 1).
        #[arg(long)]
        max_new_checkpoints: Option<u32>,
        /// Compute planned cut points but do not write artifacts or append frames.
        #[arg(long, action = clap::ArgAction::SetTrue)]
        dry_run: bool,
        /// Actor id (provenance).
        #[arg(long)]
        actor_id: Option<String>,
        /// Origin (provenance).
        #[arg(long)]
        origin: Option<String>,
    },
    /// Schedule deterministic auto-compaction under an explicit policy (logs the decision).
    CompactionAutoSchedule {
        /// Thread id (continuity id).
        id: String,
        /// Message stride used to compute cut points (default: 10_000).
        #[arg(long)]
        stride_messages: Option<u64>,
        /// Maximum number of new checkpoints created per invocation (default: 1).
        #[arg(long)]
        max_new_checkpoints: Option<u32>,
        /// Allow scheduling even when a compaction job is already in-flight.
        #[arg(long, action = clap::ArgAction::SetTrue)]
        allow_inflight: bool,
        /// Do not execute the spawned job (schedule only).
        #[arg(long, action = clap::ArgAction::SetTrue)]
        no_execute: bool,
        /// Compute the planned cut points but do not emit frames or start jobs.
        #[arg(long, action = clap::ArgAction::SetTrue)]
        dry_run: bool,
        /// Actor id (provenance).
        #[arg(long)]
        actor_id: Option<String>,
        /// Origin (provenance).
        #[arg(long)]
        origin: Option<String>,
    },
    /// Append a message to a continuity and start a run; print linkage JSON.
    PostMessage {
        /// Thread id (continuity id).
        id: String,
        /// Message content.
        #[arg(long)]
        content: String,
        /// Actor id (provenance).
        #[arg(long)]
        actor_id: Option<String>,
        /// Origin (provenance).
        #[arg(long)]
        origin: Option<String>,
    },
    /// Stream continuity event frames as JSONL (past + live).
    Events {
        /// Thread id (continuity id).
        id: String,
        /// Stop after printing N frames (useful for tests/fixtures).
        #[arg(long)]
        max_events: Option<usize>,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ThreadEnsureResponse {
    pub(crate) thread_id: String,
}

#[cfg(test)]
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ThreadMeta {
    pub(crate) thread_id: String,
    pub(crate) created_at_ms: u64,
    pub(crate) title: Option<String>,
    pub(crate) archived: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ThreadPostMessageResponse {
    pub(crate) thread_id: String,
    pub(crate) message_id: String,
    pub(crate) session_id: String,
}

#[cfg(test)]
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ThreadBranchResponse {
    pub(crate) thread_id: String,
    pub(crate) parent_thread_id: String,
    pub(crate) parent_seq: u64,
    pub(crate) parent_message_id: Option<String>,
}

#[cfg(test)]
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ThreadHandoffResponse {
    pub(crate) thread_id: String,
    pub(crate) from_thread_id: String,
    pub(crate) from_seq: u64,
    pub(crate) from_message_id: Option<String>,
}

#[cfg(test)]
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ThreadCompactionCheckpointResponse {
    pub(crate) thread_id: String,
    pub(crate) checkpoint_id: String,
    pub(crate) cut_rule_id: String,
    pub(crate) summary_artifact_id: String,
    pub(crate) to_seq: u64,
    pub(crate) to_message_id: String,
}

pub(crate) async fn run_threads(
    server: Option<String>,
    command: ThreadsCommand,
) -> anyhow::Result<()> {
    match server {
        Some(server) => run_threads_remote(server, command).await,
        None => {
            let server = crate::local_authority::ensure_local_authority().await?;
            run_threads_remote(server, command).await
        }
    }
}

mod exec;
use exec::run_threads_remote;
#[cfg(test)]
use exec::{run_threads_local_with_engine, stream_frames_local};

#[cfg(test)]
mod tests;
