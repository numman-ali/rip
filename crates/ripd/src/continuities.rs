use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use rip_kernel::{CompactionPlannedCutPoint, Event, EventKind, StreamKind};
use rip_log::EventLog;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::context_compiler::RECENT_MESSAGES_V1_LIMIT;
use crate::continuity_stream_cache::ContinuityStreamCache;
use crate::handoff_context_bundle::HandoffContextBundleV1;
use crate::{
    compaction_auto_summary::{
        render_auto_compaction_summary_markdown_v0_2,
        summary_markdown_is_legacy_metadata_placeholder, AutoSummaryAccumulator,
        RenderAutoSummaryMarkdownParams,
    },
    compaction_summary::COMPACTION_SUMMARY_SCHEMA_V1,
    compaction_summary::{
        read_compaction_summary_v1, write_compaction_summary_v1, CompactionSummaryV1,
        COMPACTION_SUMMARY_KIND_CUMULATIVE_V1,
    },
};

const INDEX_VERSION: u32 = 1;
const EVENT_CHANNEL_CAPACITY: usize = 16_384;
const COMPACTION_JOB_KIND_SUMMARIZER_V1: &str = "compaction_summarizer_v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuityMeta {
    pub continuity_id: String,
    pub created_at_ms: u64,
    pub title: Option<String>,
    pub archived: bool,
}

#[derive(Debug, Clone)]
pub struct ContinuityRunLink {
    pub continuity_id: String,
    pub message_id: String,
    pub actor_id: String,
    pub origin: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ContextCompiledPayload {
    pub(crate) run_session_id: String,
    pub(crate) bundle_artifact_id: String,
    pub(crate) compiler_id: String,
    pub(crate) compiler_strategy: String,
    pub(crate) from_seq: u64,
    pub(crate) from_message_id: Option<String>,
    pub(crate) actor_id: String,
    pub(crate) origin: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ContextSelectionDecidedPayload {
    pub(crate) run_session_id: String,
    pub(crate) message_id: String,
    pub(crate) compiler_id: String,
    pub(crate) compiler_strategy: String,
    pub(crate) limits: serde_json::Value,
    pub(crate) compaction_checkpoint: Option<rip_kernel::ContextSelectionCompactionCheckpointV1>,
    pub(crate) resets: Vec<rip_kernel::ContextSelectionResetV1>,
    pub(crate) reason: Option<serde_json::Value>,
    pub(crate) actor_id: String,
    pub(crate) origin: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ContextCompileInput {
    pub(crate) continuity_events: Vec<Event>,
    pub(crate) from_seq: u64,
    pub(crate) from_message_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct CompactionCheckpointForCompile {
    pub(crate) checkpoint_id: String,
    pub(crate) summary_kind: String,
    pub(crate) summary_artifact_id: String,
    pub(crate) to_seq: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct ProviderCursorUpdatedPayload {
    pub(crate) provider: String,
    pub(crate) endpoint: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) cursor: Option<serde_json::Value>,
    pub(crate) action: String,
    pub(crate) reason: Option<String>,
    pub(crate) run_session_id: Option<String>,
    pub(crate) actor_id: String,
    pub(crate) origin: String,
}

#[derive(Debug, Clone)]
pub struct CompactionCheckpointCumulativeV1Request {
    pub summary_markdown: Option<String>,
    pub summary_artifact_id: Option<String>,
    pub to_message_id: Option<String>,
    pub to_seq: Option<u64>,
    pub stride_messages: Option<u64>,
    pub actor_id: String,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompactionCutPointsV1Request {
    pub stride_messages: Option<u64>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompactionCutPointsV1Response {
    pub thread_id: String,
    pub stride_messages: u64,
    pub message_count: u64,
    pub cut_rule_id: String,
    pub cut_points: Vec<CompactionCutPointV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompactionCutPointV1 {
    pub target_message_ordinal: u64,
    pub to_seq: u64,
    pub to_message_id: String,
    pub already_checkpointed: bool,
    pub latest_checkpoint_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompactionAutoV1Request {
    pub stride_messages: Option<u64>,
    pub max_new_checkpoints: Option<u32>,
    pub dry_run: Option<bool>,
    pub actor_id: String,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompactionAutoScheduleV1Request {
    pub stride_messages: Option<u64>,
    pub max_new_checkpoints: Option<u32>,
    pub block_on_inflight: Option<bool>,
    pub execute: Option<bool>,
    pub dry_run: Option<bool>,
    pub actor_id: String,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompactionAutoV1Response {
    pub thread_id: String,
    pub job_id: Option<String>,
    pub job_kind: Option<String>,
    pub status: String,
    pub stride_messages: u64,
    pub message_count: u64,
    pub cut_rule_id: String,
    pub planned: Vec<CompactionPlannedCutPointV1>,
    pub result: Vec<CompactionAutoResultCheckpointV1>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompactionPlannedCutPointV1 {
    pub target_message_ordinal: u64,
    pub to_seq: u64,
    pub to_message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompactionAutoResultCheckpointV1 {
    pub checkpoint_id: String,
    pub summary_artifact_id: String,
    pub to_seq: u64,
    pub to_message_id: String,
    pub cut_rule_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompactionAutoScheduleV1Response {
    pub thread_id: String,
    pub decision_id: Option<String>,
    pub policy_id: String,
    pub decision: String,
    pub execute: bool,
    pub stride_messages: u64,
    pub max_new_checkpoints: u32,
    pub block_on_inflight: bool,
    pub message_count: u64,
    pub cut_rule_id: String,
    pub planned: Vec<CompactionPlannedCutPointV1>,
    pub job_id: Option<String>,
    pub job_kind: Option<String>,
    pub result: Vec<CompactionAutoResultCheckpointV1>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompactionStatusV1Request {
    pub stride_messages: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompactionStatusV1Response {
    pub thread_id: String,
    pub stride_messages: u64,
    pub message_count: u64,
    pub latest_checkpoint: Option<CompactionStatusCheckpointV1>,
    pub next_cut_point: Option<CompactionPlannedCutPointV1>,
    pub inflight_job_id: Option<String>,
    pub last_schedule_decision: Option<CompactionStatusScheduleDecisionV1>,
    pub last_job_outcome: Option<CompactionStatusJobOutcomeV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderCursorStatusV1Request {}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderCursorStatusCursorV1 {
    pub cursor_event_id: String,
    pub provider: String,
    pub endpoint: Option<String>,
    pub model: Option<String>,
    pub cursor: Option<serde_json::Value>,
    pub action: String,
    pub reason: Option<String>,
    pub run_session_id: Option<String>,
    pub actor_id: String,
    pub origin: String,
    pub seq: u64,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderCursorStatusV1Response {
    pub thread_id: String,
    pub active: Option<ProviderCursorStatusCursorV1>,
    pub cursors: Vec<ProviderCursorStatusCursorV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderCursorRotateV1Request {
    pub provider: Option<String>,
    pub endpoint: Option<String>,
    pub model: Option<String>,
    pub reason: Option<String>,
    pub actor_id: String,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderCursorRotateV1Response {
    pub thread_id: String,
    pub rotated: bool,
    pub provider: Option<String>,
    pub endpoint: Option<String>,
    pub model: Option<String>,
    pub cursor_event_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ContextSelectionStatusV1Request {
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ContextSelectionStatusCheckpointV1 {
    pub checkpoint_id: String,
    pub summary_kind: String,
    pub summary_artifact_id: String,
    pub to_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ContextSelectionStatusResetV1 {
    pub input: String,
    pub action: String,
    pub reason: String,
    #[serde(default, rename = "ref", skip_serializing_if = "Option::is_none")]
    pub ref_: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ContextSelectionStatusDecisionV1 {
    pub decision_event_id: String,
    pub run_session_id: String,
    pub message_id: String,
    pub compiler_id: String,
    pub compiler_strategy: String,
    pub limits: serde_json::Value,
    pub compaction_checkpoint: Option<ContextSelectionStatusCheckpointV1>,
    pub resets: Vec<ContextSelectionStatusResetV1>,
    pub reason: Option<serde_json::Value>,
    pub actor_id: String,
    pub origin: String,
    pub seq: u64,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ContextSelectionStatusV1Response {
    pub thread_id: String,
    pub decisions: Vec<ContextSelectionStatusDecisionV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompactionStatusCheckpointV1 {
    pub checkpoint_id: String,
    pub cut_rule_id: String,
    pub summary_kind: String,
    pub summary_artifact_id: String,
    pub to_seq: u64,
    pub to_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompactionStatusScheduleDecisionV1 {
    pub decision_id: String,
    pub policy_id: String,
    pub decision: String,
    pub execute: bool,
    pub stride_messages: u64,
    pub max_new_checkpoints: u32,
    pub block_on_inflight: bool,
    pub message_count: u64,
    pub cut_rule_id: String,
    pub planned: Vec<CompactionPlannedCutPointV1>,
    pub job_id: Option<String>,
    pub job_kind: Option<String>,
    pub actor_id: String,
    pub origin: String,
    pub seq: u64,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompactionStatusJobOutcomeV1 {
    pub job_id: String,
    pub job_kind: String,
    pub status: String,
    pub error: Option<String>,
    pub created: Vec<CompactionAutoResultCheckpointV1>,
    pub actor_id: String,
    pub origin: String,
    pub seq: u64,
    pub timestamp_ms: u64,
}

struct CompactionCheckpointCreatedPayload {
    cut_rule_id: String,
    summary_kind: String,
    summary_artifact_id: String,
    from_seq: u64,
    from_message_id: Option<String>,
    to_seq: u64,
    to_message_id: Option<String>,
    actor_id: String,
    origin: String,
}

struct CompactionAutoScheduleDecidedPayload {
    decision_id: String,
    policy_id: String,
    decision: String,
    execute: bool,
    stride_messages: u64,
    max_new_checkpoints: u32,
    block_on_inflight: bool,
    message_count: u64,
    cut_rule_id: String,
    planned: Vec<CompactionPlannedCutPoint>,
    job_id: Option<String>,
    job_kind: Option<String>,
    reason: Option<serde_json::Value>,
    actor_id: String,
    origin: String,
}

struct JobEndedPayload {
    job_id: String,
    job_kind: String,
    status: String,
    result: Option<serde_json::Value>,
    error: Option<String>,
    actor_id: String,
    origin: String,
}

#[derive(Debug, Clone)]
pub struct ToolSideEffects {
    pub tool_id: String,
    pub tool_name: String,
    pub affected_paths: Option<Vec<String>>,
    pub checkpoint_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContinuityIndexV1 {
    version: u32,
    /// workspace key -> default continuity id
    workspaces: HashMap<String, String>,
    continuities: HashMap<String, ContinuityMetaV1>,
}

impl Default for ContinuityIndexV1 {
    fn default() -> Self {
        Self {
            version: INDEX_VERSION,
            workspaces: HashMap::new(),
            continuities: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContinuityMetaV1 {
    created_at_ms: u64,
    title: Option<String>,
    archived: bool,
}

pub struct ContinuityStore {
    data_dir: PathBuf,
    workspace_root: PathBuf,
    event_log: Arc<EventLog>,
    stream_cache: ContinuityStreamCache,
    sender: broadcast::Sender<Event>,
    index: Mutex<ContinuityIndexV1>,
    next_seq: Mutex<HashMap<String, u64>>,
}

impl ContinuityStore {
    pub fn new(
        data_dir: PathBuf,
        workspace_root: PathBuf,
        event_log: Arc<EventLog>,
    ) -> Result<Self, String> {
        let index = load_index(&index_path(&data_dir)).unwrap_or_default();
        let (sender, _receiver) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let stream_cache = ContinuityStreamCache::new(&data_dir);
        Ok(Self {
            data_dir,
            workspace_root,
            event_log,
            stream_cache,
            sender,
            index: Mutex::new(index),
            next_seq: Mutex::new(HashMap::new()),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    pub fn replay_events(&self, continuity_id: &str) -> io::Result<Vec<Event>> {
        if let Ok(Some(events)) = self.stream_cache.try_replay(continuity_id) {
            return Ok(events);
        }

        let events = self
            .event_log
            .replay_stream(StreamKind::Continuity, continuity_id)?;
        if !events.is_empty() {
            self.stream_cache
                .rebuild_best_effort(continuity_id, &events);
        }
        Ok(events)
    }

    pub(crate) fn load_context_compile_input_recent_messages_v1(
        &self,
        continuity_id: &str,
        anchor_message_id: &str,
    ) -> Result<ContextCompileInput, String> {
        const INITIAL_TAIL_BYTES: usize = 256 * 1024;
        const MAX_TAIL_BYTES: usize = 8 * 1024 * 1024;
        const MAX_TAIL_EVENTS: usize = 100_000;

        let mut tail_bytes = INITIAL_TAIL_BYTES;
        while tail_bytes <= MAX_TAIL_BYTES {
            match self.stream_cache.scan_tail_messages_runs_v1(
                continuity_id,
                MAX_TAIL_EVENTS,
                tail_bytes,
            ) {
                Ok(Some(tail)) => {
                    if !tail.events.is_empty() {
                        // Prefer the full continuity sidecar's head seq so `from_seq` matches the
                        // truth stream even when the mr sidecar omits non-message events.
                        let head_seq = self
                            .stream_cache
                            .try_read_last_seq(continuity_id)
                            .ok()
                            .flatten()
                            .or_else(|| tail.events.last().map(|event| event.seq))
                            .unwrap_or_default();

                        let mut message_events: Vec<(u64, String)> = Vec::new();
                        for event in &tail.events {
                            if matches!(event.kind, EventKind::ContinuityMessageAppended { .. }) {
                                message_events.push((event.seq, event.id.clone()));
                            }
                        }

                        if let Some((message_seq, from_seq)) =
                            resolve_cutpoint_from_tail(&message_events, head_seq, anchor_message_id)
                        {
                            let message_count = message_events
                                .iter()
                                .filter(|(seq, _)| *seq <= from_seq)
                                .count();

                            if tail.complete || message_count >= RECENT_MESSAGES_V1_LIMIT {
                                return Ok(ContextCompileInput {
                                    continuity_events: tail.events,
                                    from_seq: from_seq.max(message_seq),
                                    from_message_id: Some(anchor_message_id.to_string()),
                                });
                            }
                        } else if tail.complete {
                            return Err(format!(
                                "continuity message not found: {anchor_message_id}"
                            ));
                        }
                    } else if tail.complete {
                        return Err("continuity sidecar is empty".to_string());
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }

            if tail_bytes >= MAX_TAIL_BYTES {
                break;
            }
            tail_bytes = (tail_bytes * 2).min(MAX_TAIL_BYTES);
        }

        // Fall back to a seekable window read when the anchor isn't near the tail.
        if let Ok(Some(window)) = self.stream_cache.window_recent_messages_v1_from_message_id(
            continuity_id,
            anchor_message_id,
            RECENT_MESSAGES_V1_LIMIT,
        ) {
            return Ok(ContextCompileInput {
                continuity_events: window.events,
                from_seq: window.from_seq,
                from_message_id: window.from_message_id,
            });
        }

        // Fall back to full replay when caches are missing/invalid.
        let continuity_events = self
            .replay_events(continuity_id)
            .map_err(|err| format!("continuity replay failed: {err}"))?;
        if continuity_events.is_empty() {
            return Err("continuity stream does not exist".to_string());
        }

        let (from_seq, from_message_id) =
            resolve_context_compile_cutpoint_full(&continuity_events, anchor_message_id)?;
        Ok(ContextCompileInput {
            continuity_events,
            from_seq,
            from_message_id,
        })
    }

    pub(crate) fn latest_compaction_checkpoint_for_compile_v1(
        &self,
        continuity_id: &str,
        from_seq: u64,
    ) -> Result<Option<CompactionCheckpointForCompile>, String> {
        if let Ok(Some(event)) = self
            .stream_cache
            .latest_compaction_checkpoint_before_or_at_seq_v1(continuity_id, from_seq)
        {
            if let EventKind::ContinuityCompactionCheckpointCreated {
                checkpoint_id,
                summary_kind,
                summary_artifact_id,
                to_seq,
                ..
            } = event.kind
            {
                return Ok(Some(CompactionCheckpointForCompile {
                    checkpoint_id,
                    summary_kind,
                    summary_artifact_id,
                    to_seq,
                }));
            }
        }

        let events = self
            .replay_events(continuity_id)
            .map_err(|err| format!("continuity replay failed: {err}"))?;

        let mut best: Option<CompactionCheckpointForCompile> = None;
        for event in &events {
            let EventKind::ContinuityCompactionCheckpointCreated {
                checkpoint_id,
                summary_kind,
                summary_artifact_id,
                to_seq,
                ..
            } = &event.kind
            else {
                continue;
            };
            if *to_seq > from_seq {
                continue;
            }

            let replace = match best.as_ref() {
                None => true,
                Some(current) => *to_seq > current.to_seq,
            };
            if replace {
                best = Some(CompactionCheckpointForCompile {
                    checkpoint_id: checkpoint_id.clone(),
                    summary_kind: summary_kind.clone(),
                    summary_artifact_id: summary_artifact_id.clone(),
                    to_seq: *to_seq,
                });
            } else if let Some(current) = best.as_mut() {
                // Tie-breaker: later continuity frame wins when `to_seq` is equal.
                if *to_seq == current.to_seq {
                    current.checkpoint_id = checkpoint_id.clone();
                    current.summary_kind = summary_kind.clone();
                    current.summary_artifact_id = summary_artifact_id.clone();
                }
            }
        }

        Ok(best)
    }

    pub(crate) fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub fn ensure_default(&self) -> Result<String, String> {
        let workspace = workspace_key(&self.workspace_root);

        if let Some(existing) = self
            .index
            .lock()
            .expect("continuity index mutex")
            .workspaces
            .get(&workspace)
            .cloned()
        {
            return Ok(existing);
        }

        if let Some(existing) = self
            .find_latest_continuity_for_workspace(&workspace)
            .map_err(|err| format!("continuity log scan failed: {err}"))?
        {
            // Backfill the cache index so future calls are O(1).
            {
                let mut index = self.index.lock().expect("continuity index mutex");
                index.workspaces.insert(workspace.clone(), existing.clone());
                let _ = save_index(&index_path(&self.data_dir), &index);
            }
            return Ok(existing);
        }

        let continuity_id = Uuid::new_v4().to_string();
        self.create_continuity(workspace, Some(continuity_id), None, true)
    }

    pub fn branch(
        &self,
        parent_thread_id: &str,
        title: Option<String>,
        from_message_id: Option<String>,
        from_seq: Option<u64>,
        actor_id: String,
        origin: String,
    ) -> Result<(String, u64, Option<String>), String> {
        if from_message_id.is_some() && from_seq.is_some() {
            return Err("branch requires only one of from_message_id or from_seq".to_string());
        }

        let parent_events = self
            .replay_events(parent_thread_id)
            .map_err(|err| format!("branch parent replay failed: {err}"))?;
        if parent_events.is_empty() {
            return Err("branch parent continuity stream does not exist".to_string());
        }

        let head_seq = parent_events
            .last()
            .map(|event| event.seq)
            .unwrap_or_default();

        let (parent_seq, parent_message_id) = if let Some(from_seq) = from_seq {
            if from_seq > head_seq {
                return Err(format!(
                    "branch from_seq out of range: max_seq={head_seq}, got {from_seq}"
                ));
            }
            let last_message = parent_events
                .iter()
                .rev()
                .find(|event| {
                    event.seq <= from_seq
                        && matches!(event.kind, EventKind::ContinuityMessageAppended { .. })
                })
                .map(|event| event.id.clone());
            (from_seq, last_message)
        } else if let Some(message_id) = from_message_id.clone() {
            let mut message_seq: Option<u64> = None;
            let mut max_related_seq: Option<u64> = None;

            for event in &parent_events {
                match &event.kind {
                    EventKind::ContinuityMessageAppended { .. } if event.id == message_id => {
                        message_seq = Some(event.seq);
                        max_related_seq = Some(event.seq);
                    }
                    EventKind::ContinuityRunSpawned {
                        message_id: mid, ..
                    }
                    | EventKind::ContinuityRunEnded {
                        message_id: mid, ..
                    } if mid == &message_id => {
                        max_related_seq = Some(max_related_seq.unwrap_or(0).max(event.seq));
                    }
                    _ => {}
                }
            }

            if message_seq.is_none() {
                return Err(format!("branch from_message_id not found: {message_id}"));
            }
            (max_related_seq.unwrap_or(0), Some(message_id))
        } else {
            let last_message = parent_events
                .iter()
                .rev()
                .find(|event| matches!(event.kind, EventKind::ContinuityMessageAppended { .. }))
                .map(|event| event.id.clone());
            (head_seq, last_message)
        };

        let workspace = workspace_key(&self.workspace_root);
        let thread_id = self.create_continuity(workspace, None, title, false)?;

        let event = Event {
            id: Uuid::new_v4().to_string(),
            session_id: thread_id.clone(),
            timestamp_ms: now_ms(),
            seq: 1,
            kind: EventKind::ContinuityBranched {
                parent_thread_id: parent_thread_id.to_string(),
                parent_seq,
                parent_message_id: parent_message_id.clone(),
                actor_id,
                origin,
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity_branched: {err}"))?;
        self.stream_cache.append_best_effort(&event);
        let _ = self.sender.send(event.clone());

        self.next_seq
            .lock()
            .expect("continuity seq mutex")
            .insert(thread_id.clone(), 2);

        Ok((thread_id, parent_seq, parent_message_id))
    }

    pub fn handoff(
        &self,
        from_thread_id: &str,
        title: Option<String>,
        summary: (Option<String>, Option<String>),
        from_message_id: Option<String>,
        from_seq: Option<u64>,
        provenance: (String, String),
    ) -> Result<(String, u64, Option<String>), String> {
        let (summary_markdown, mut summary_artifact_id) = summary;
        let (actor_id, origin) = provenance;
        if summary_markdown.is_none() && summary_artifact_id.is_none() {
            return Err("handoff requires summary_markdown and/or summary_artifact_id".to_string());
        }
        if from_message_id.is_some() && from_seq.is_some() {
            return Err("handoff requires only one of from_message_id or from_seq".to_string());
        }

        let from_events = self
            .replay_events(from_thread_id)
            .map_err(|err| format!("handoff parent replay failed: {err}"))?;
        if from_events.is_empty() {
            return Err("handoff parent continuity stream does not exist".to_string());
        }

        let head_seq = from_events
            .last()
            .map(|event| event.seq)
            .unwrap_or_default();

        let (from_seq, from_message_id) = if let Some(from_seq) = from_seq {
            if from_seq > head_seq {
                return Err(format!(
                    "handoff from_seq out of range: max_seq={head_seq}, got {from_seq}"
                ));
            }
            let last_message = from_events
                .iter()
                .rev()
                .find(|event| {
                    event.seq <= from_seq
                        && matches!(event.kind, EventKind::ContinuityMessageAppended { .. })
                })
                .map(|event| event.id.clone());
            (from_seq, last_message)
        } else if let Some(message_id) = from_message_id.clone() {
            let mut message_seq: Option<u64> = None;
            let mut max_related_seq: Option<u64> = None;

            for event in &from_events {
                match &event.kind {
                    EventKind::ContinuityMessageAppended { .. } if event.id == message_id => {
                        message_seq = Some(event.seq);
                        max_related_seq = Some(event.seq);
                    }
                    EventKind::ContinuityRunSpawned {
                        message_id: mid, ..
                    }
                    | EventKind::ContinuityRunEnded {
                        message_id: mid, ..
                    } if mid == &message_id => {
                        max_related_seq = Some(max_related_seq.unwrap_or(0).max(event.seq));
                    }
                    _ => {}
                }
            }

            if message_seq.is_none() {
                return Err(format!("handoff from_message_id not found: {message_id}"));
            }
            (max_related_seq.unwrap_or(0), Some(message_id))
        } else {
            let last_message = from_events
                .iter()
                .rev()
                .find(|event| matches!(event.kind, EventKind::ContinuityMessageAppended { .. }))
                .map(|event| event.id.clone());
            (head_seq, last_message)
        };

        let workspace = workspace_key(&self.workspace_root);
        let thread_id = self.create_continuity(workspace, None, title, false)?;

        if summary_artifact_id.is_none() {
            if let Some(markdown) = summary_markdown.as_ref() {
                let bundle = HandoffContextBundleV1::new_source_cut(
                    markdown.clone(),
                    from_thread_id.to_string(),
                    from_seq,
                    from_message_id.clone(),
                );
                summary_artifact_id = Some(crate::handoff_context_bundle::write_bundle_v1(
                    &self.workspace_root,
                    &bundle,
                )?);
            }
        }

        let event = Event {
            id: Uuid::new_v4().to_string(),
            session_id: thread_id.clone(),
            timestamp_ms: now_ms(),
            seq: 1,
            kind: EventKind::ContinuityHandoffCreated {
                from_thread_id: from_thread_id.to_string(),
                from_seq,
                from_message_id: from_message_id.clone(),
                summary_artifact_id,
                summary_markdown,
                actor_id,
                origin,
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity_handoff_created: {err}"))?;
        self.stream_cache.append_best_effort(&event);
        let _ = self.sender.send(event.clone());

        self.next_seq
            .lock()
            .expect("continuity seq mutex")
            .insert(thread_id.clone(), 2);

        Ok((thread_id, from_seq, from_message_id))
    }

    pub fn compaction_checkpoint_cumulative_v1(
        &self,
        thread_id: &str,
        req: CompactionCheckpointCumulativeV1Request,
    ) -> Result<(String, String, u64, String, String), String> {
        let summary_markdown = req.summary_markdown;
        let summary_artifact_id = req.summary_artifact_id;
        let to_message_id = req.to_message_id;
        let to_seq = req.to_seq;
        let stride_messages = req.stride_messages;
        let actor_id = req.actor_id;
        let origin = req.origin;

        if summary_markdown.is_none() && summary_artifact_id.is_none() {
            return Err(
                "compaction checkpoint requires summary_markdown and/or summary_artifact_id"
                    .to_string(),
            );
        }
        if to_message_id.is_some() && to_seq.is_some() {
            return Err(
                "compaction checkpoint requires only one of to_message_id or to_seq".to_string(),
            );
        }

        let events = self
            .replay_events(thread_id)
            .map_err(|err| format!("compaction thread replay failed: {err}"))?;
        if events.is_empty() {
            return Err("compaction thread continuity stream does not exist".to_string());
        }

        let message_events: Vec<(u64, String)> = events
            .iter()
            .filter_map(|event| match &event.kind {
                EventKind::ContinuityMessageAppended { .. } => Some((event.seq, event.id.clone())),
                _ => None,
            })
            .collect();
        if message_events.is_empty() {
            return Err("compaction requires at least one message in the thread".to_string());
        }

        let (to_seq, to_message_id, cut_rule_id) = if let Some(message_id) = to_message_id.clone() {
            let Some((seq, _)) = message_events.iter().find(|(_, id)| id == &message_id) else {
                return Err(format!("compaction to_message_id not found: {message_id}"));
            };
            (*seq, message_id, "manual_v1".to_string())
        } else if let Some(to_seq) = to_seq {
            let Some((_, message_id)) = message_events.iter().find(|(seq, _)| *seq == to_seq)
            else {
                return Err(format!(
                    "compaction to_seq must be a message boundary: seq={to_seq}"
                ));
            };
            (to_seq, message_id.clone(), "manual_v1".to_string())
        } else {
            let stride = stride_messages.unwrap_or(10_000);
            if stride == 0 {
                return Err("compaction stride_messages must be > 0".to_string());
            }
            let message_count = message_events.len() as u64;
            let target = (message_count / stride) * stride;
            if target == 0 {
                return Err(format!(
                    "compaction stride_messages not reached: stride={stride}, messages={message_count}"
                ));
            }
            let idx = (target - 1) as usize;
            let (seq, message_id) = message_events
                .get(idx)
                .ok_or_else(|| "compaction stride cutpoint out of range".to_string())?;
            (
                *seq,
                message_id.clone(),
                format!("stride_messages_v1/{stride}"),
            )
        };

        // Best-effort basis: the most recent prior cumulative checkpoint (by `to_seq`).
        let mut base_summary_artifact_id: Option<String> = None;
        let mut best_checkpoint_to_seq: u64 = 0;
        let mut best_checkpoint_event_seq: u64 = 0;
        for event in &events {
            let EventKind::ContinuityCompactionCheckpointCreated {
                summary_kind,
                summary_artifact_id,
                to_seq: checkpoint_to_seq,
                ..
            } = &event.kind
            else {
                continue;
            };
            if summary_kind != COMPACTION_SUMMARY_KIND_CUMULATIVE_V1 {
                continue;
            }
            if *checkpoint_to_seq >= to_seq {
                continue;
            }
            if *checkpoint_to_seq > best_checkpoint_to_seq
                || (*checkpoint_to_seq == best_checkpoint_to_seq
                    && event.seq > best_checkpoint_event_seq)
            {
                best_checkpoint_to_seq = *checkpoint_to_seq;
                best_checkpoint_event_seq = event.seq;
                base_summary_artifact_id = Some(summary_artifact_id.clone());
            }
        }

        let summary_artifact_id = if let Some(artifact_id) = summary_artifact_id {
            let summary = read_compaction_summary_v1(&self.workspace_root, &artifact_id)?;
            if summary.schema() != COMPACTION_SUMMARY_SCHEMA_V1 {
                return Err(format!(
                    "compaction summary schema mismatch: expected {}, got {}",
                    COMPACTION_SUMMARY_SCHEMA_V1,
                    summary.schema()
                ));
            }
            if summary.kind() != COMPACTION_SUMMARY_KIND_CUMULATIVE_V1 {
                return Err(format!(
                    "compaction summary kind mismatch: expected {}, got {}",
                    COMPACTION_SUMMARY_KIND_CUMULATIVE_V1,
                    summary.kind()
                ));
            }
            if summary.coverage_thread_id() != thread_id {
                return Err("compaction summary thread_id mismatch".to_string());
            }
            if summary.coverage_to_seq() != to_seq {
                return Err("compaction summary to_seq mismatch".to_string());
            }
            artifact_id
        } else {
            let markdown = summary_markdown.expect("checked");
            let summary = CompactionSummaryV1::new_cumulative_source_cut(
                crate::compaction_summary::NewCumulativeCompactionSummaryV1 {
                    thread_id: thread_id.to_string(),
                    to_seq,
                    to_message_id: Some(to_message_id.clone()),
                    actor_id: actor_id.clone(),
                    origin: origin.clone(),
                    produced_by: Some(("manual".to_string(), "rip-cli".to_string())),
                    base_summary_artifact_id,
                    basis_note: None,
                    summary_markdown: markdown,
                },
            );
            write_compaction_summary_v1(&self.workspace_root, &summary)?
        };

        let checkpoint_id = self.append_compaction_checkpoint_created(
            thread_id,
            CompactionCheckpointCreatedPayload {
                cut_rule_id: cut_rule_id.clone(),
                summary_kind: COMPACTION_SUMMARY_KIND_CUMULATIVE_V1.to_string(),
                summary_artifact_id: summary_artifact_id.clone(),
                from_seq: 0,
                from_message_id: None,
                to_seq,
                to_message_id: Some(to_message_id.clone()),
                actor_id,
                origin,
            },
        )?;

        Ok((
            checkpoint_id,
            summary_artifact_id,
            to_seq,
            to_message_id,
            cut_rule_id,
        ))
    }

    pub fn compaction_cut_points_v1(
        &self,
        thread_id: &str,
        req: CompactionCutPointsV1Request,
    ) -> Result<CompactionCutPointsV1Response, String> {
        let stride = req.stride_messages.unwrap_or(10_000);
        if stride == 0 {
            return Err("invalid_stride".to_string());
        }
        let limit = req.limit.unwrap_or(1).clamp(1, 32) as u64;

        let mut replayed: Option<Vec<Event>> = None;
        let mut message_events: Option<Vec<(u64, String)>> = None;
        let mut checkpoint_index: Option<Vec<(u64, u64, String)>> = None;

        let message_count = match self.stream_cache.message_count_messages_runs_v1(thread_id) {
            Ok(Some(count)) => count,
            _ => {
                let events = self
                    .replay_events(thread_id)
                    .map_err(|err| format!("continuity replay failed: {err}"))?;
                if events.is_empty() {
                    return Err("thread_not_found".to_string());
                }
                replayed = Some(events);
                match self.stream_cache.message_count_messages_runs_v1(thread_id) {
                    Ok(Some(count)) => count,
                    _ => {
                        let events = replayed.as_ref().expect("set");
                        let msgs: Vec<(u64, String)> = events
                            .iter()
                            .filter_map(|event| match &event.kind {
                                EventKind::ContinuityMessageAppended { .. } => {
                                    Some((event.seq, event.id.clone()))
                                }
                                _ => None,
                            })
                            .collect();
                        let count = msgs.len() as u64;
                        message_events = Some(msgs);
                        count
                    }
                }
            }
        };

        let cut_rule_id = format!("stride_messages_v1/{stride}");

        let mut cut_points: Vec<CompactionCutPointV1> = Vec::new();
        let latest_multiple = (message_count / stride) * stride;
        if latest_multiple == 0 {
            return Ok(CompactionCutPointsV1Response {
                thread_id: thread_id.to_string(),
                stride_messages: stride,
                message_count,
                cut_rule_id,
                cut_points,
            });
        }

        for i in 0..limit {
            let ordinal = latest_multiple.saturating_sub(i.saturating_mul(stride));
            if ordinal == 0 {
                break;
            }

            let resolved = self
                .stream_cache
                .message_by_ordinal_messages_runs_v1(thread_id, ordinal)
                .ok()
                .flatten()
                .or_else(|| {
                    message_events.as_ref().and_then(|events| {
                        let idx = (ordinal - 1) as usize;
                        let (seq, id) = events.get(idx)?.clone();
                        Some((seq, id))
                    })
                });
            let (to_seq, to_message_id) = match resolved {
                Some((to_seq, to_message_id)) => (to_seq, to_message_id),
                None => {
                    // Fallback: build message_events from replay when caches are missing/invalid.
                    if replayed.is_none() {
                        let events = self
                            .replay_events(thread_id)
                            .map_err(|err| format!("continuity replay failed: {err}"))?;
                        if events.is_empty() {
                            return Err("thread_not_found".to_string());
                        }
                        replayed = Some(events);
                    }
                    if message_events.is_none() {
                        let events = replayed.as_ref().expect("set");
                        let msgs: Vec<(u64, String)> = events
                            .iter()
                            .filter_map(|event| match &event.kind {
                                EventKind::ContinuityMessageAppended { .. } => {
                                    Some((event.seq, event.id.clone()))
                                }
                                _ => None,
                            })
                            .collect();
                        message_events = Some(msgs);
                    }
                    let msgs = message_events.as_ref().expect("set");
                    let idx = (ordinal - 1) as usize;
                    let Some((to_seq, to_message_id)) = msgs.get(idx).cloned() else {
                        continue;
                    };
                    (to_seq, to_message_id)
                }
            };

            let mut best_checkpoint_to_seq: Option<u64> = None;
            let mut best_checkpoint_seq: u64 = 0;
            let mut best_checkpoint_id: Option<String> = None;

            let cache_best = self
                .stream_cache
                .latest_compaction_checkpoint_before_or_at_seq_v1(thread_id, to_seq);
            match cache_best {
                Ok(Some(event)) => {
                    if let EventKind::ContinuityCompactionCheckpointCreated {
                        checkpoint_id,
                        to_seq: checkpoint_to_seq,
                        ..
                    } = &event.kind
                    {
                        best_checkpoint_to_seq = Some(*checkpoint_to_seq);
                        best_checkpoint_seq = event.seq;
                        best_checkpoint_id = Some(checkpoint_id.clone());
                    }
                }
                Ok(None) => {
                    // When the full continuity sidecar is absent, the checkpoint cache cannot be
                    // rebuilt locally; fall back to the truth log for correctness.
                    if self
                        .stream_cache
                        .try_read_last_seq(thread_id)
                        .ok()
                        .flatten()
                        .is_none()
                        && replayed.is_none()
                    {
                        let events = self
                            .replay_events(thread_id)
                            .map_err(|err| format!("continuity replay failed: {err}"))?;
                        if events.is_empty() {
                            return Err("thread_not_found".to_string());
                        }
                        replayed = Some(events);
                    }
                }
                Err(_) => {
                    if replayed.is_none() {
                        let events = self
                            .replay_events(thread_id)
                            .map_err(|err| format!("continuity replay failed: {err}"))?;
                        if events.is_empty() {
                            return Err("thread_not_found".to_string());
                        }
                        replayed = Some(events);
                    }
                }
            }

            if best_checkpoint_to_seq.is_none() {
                if let Some(events) = replayed.as_ref() {
                    let idx = checkpoint_index.get_or_insert_with(|| {
                        events
                            .iter()
                            .filter_map(|event| match &event.kind {
                                EventKind::ContinuityCompactionCheckpointCreated {
                                    checkpoint_id,
                                    to_seq,
                                    ..
                                } => Some((*to_seq, event.seq, checkpoint_id.clone())),
                                _ => None,
                            })
                            .collect()
                    });
                    for (checkpoint_to_seq, checkpoint_seq, checkpoint_id) in idx.iter() {
                        if *checkpoint_to_seq > to_seq {
                            continue;
                        }
                        match best_checkpoint_to_seq {
                            None => {
                                best_checkpoint_to_seq = Some(*checkpoint_to_seq);
                                best_checkpoint_seq = *checkpoint_seq;
                                best_checkpoint_id = Some(checkpoint_id.clone());
                            }
                            Some(current_to_seq) => {
                                if *checkpoint_to_seq > current_to_seq
                                    || (*checkpoint_to_seq == current_to_seq
                                        && *checkpoint_seq > best_checkpoint_seq)
                                {
                                    best_checkpoint_to_seq = Some(*checkpoint_to_seq);
                                    best_checkpoint_seq = *checkpoint_seq;
                                    best_checkpoint_id = Some(checkpoint_id.clone());
                                }
                            }
                        }
                    }
                }
            }

            let already_checkpointed = best_checkpoint_to_seq == Some(to_seq);
            let latest_checkpoint_id = if already_checkpointed {
                best_checkpoint_id.clone()
            } else {
                None
            };

            cut_points.push(CompactionCutPointV1 {
                target_message_ordinal: ordinal,
                to_seq,
                to_message_id,
                already_checkpointed,
                latest_checkpoint_id,
            });
        }

        Ok(CompactionCutPointsV1Response {
            thread_id: thread_id.to_string(),
            stride_messages: stride,
            message_count,
            cut_rule_id,
            cut_points,
        })
    }

    pub fn compaction_status_v1(
        &self,
        thread_id: &str,
        req: CompactionStatusV1Request,
    ) -> Result<CompactionStatusV1Response, String> {
        let stride = req.stride_messages.unwrap_or(10_000);
        if stride == 0 {
            return Err("invalid_stride".to_string());
        }

        let cut_points = self.compaction_cut_points_v1(
            thread_id,
            CompactionCutPointsV1Request {
                stride_messages: Some(stride),
                limit: Some(32),
            },
        )?;

        let next_cut_point = cut_points
            .cut_points
            .iter()
            .find(|cp| !cp.already_checkpointed)
            .map(|cp| CompactionPlannedCutPointV1 {
                target_message_ordinal: cp.target_message_ordinal,
                to_seq: cp.to_seq,
                to_message_id: cp.to_message_id.clone(),
            });

        let inflight_job_id = self.find_inflight_compaction_job_id_best_effort_v1(thread_id);

        let mut latest_checkpoint: Option<CompactionStatusCheckpointV1> = None;
        let mut last_schedule_decision: Option<CompactionStatusScheduleDecisionV1> = None;
        let mut last_job_outcome: Option<CompactionStatusJobOutcomeV1> = None;

        if let Ok(Some(event)) = self
            .stream_cache
            .latest_compaction_checkpoint_before_or_at_seq_v1(thread_id, u64::MAX)
        {
            if let EventKind::ContinuityCompactionCheckpointCreated {
                checkpoint_id,
                cut_rule_id,
                summary_kind,
                summary_artifact_id,
                to_seq,
                to_message_id,
                ..
            } = &event.kind
            {
                latest_checkpoint = Some(CompactionStatusCheckpointV1 {
                    checkpoint_id: checkpoint_id.clone(),
                    cut_rule_id: cut_rule_id.clone(),
                    summary_kind: summary_kind.clone(),
                    summary_artifact_id: summary_artifact_id.clone(),
                    to_seq: *to_seq,
                    to_message_id: to_message_id.clone(),
                });
            }
        }

        const INITIAL_TAIL_BYTES: usize = 256 * 1024;
        const MAX_TAIL_BYTES: usize = 8 * 1024 * 1024;
        const MAX_TAIL_EVENTS: usize = 10_000;

        let mut tail_bytes = INITIAL_TAIL_BYTES;
        while tail_bytes <= MAX_TAIL_BYTES
            && (last_schedule_decision.is_none() || last_job_outcome.is_none())
        {
            match self
                .stream_cache
                .scan_tail(thread_id, MAX_TAIL_EVENTS, tail_bytes)
            {
                Ok(Some(tail)) => {
                    for event in tail.events.iter().rev() {
                        if last_schedule_decision.is_none() {
                            if let EventKind::ContinuityCompactionAutoScheduleDecided {
                                decision_id,
                                policy_id,
                                decision,
                                execute,
                                stride_messages,
                                max_new_checkpoints,
                                block_on_inflight,
                                message_count,
                                cut_rule_id,
                                planned,
                                job_id,
                                job_kind,
                                actor_id,
                                origin,
                                ..
                            } = &event.kind
                            {
                                let planned_v1 = planned
                                    .iter()
                                    .map(|p| CompactionPlannedCutPointV1 {
                                        target_message_ordinal: p.target_message_ordinal,
                                        to_seq: p.to_seq,
                                        to_message_id: p.to_message_id.clone(),
                                    })
                                    .collect();
                                last_schedule_decision = Some(CompactionStatusScheduleDecisionV1 {
                                    decision_id: decision_id.clone(),
                                    policy_id: policy_id.clone(),
                                    decision: decision.clone(),
                                    execute: *execute,
                                    stride_messages: *stride_messages,
                                    max_new_checkpoints: *max_new_checkpoints,
                                    block_on_inflight: *block_on_inflight,
                                    message_count: *message_count,
                                    cut_rule_id: cut_rule_id.clone(),
                                    planned: planned_v1,
                                    job_id: job_id.clone(),
                                    job_kind: job_kind.clone(),
                                    actor_id: actor_id.clone(),
                                    origin: origin.clone(),
                                    seq: event.seq,
                                    timestamp_ms: event.timestamp_ms,
                                });
                            }
                        }

                        if last_job_outcome.is_none() {
                            if let EventKind::ContinuityJobEnded {
                                job_id,
                                job_kind,
                                status,
                                result,
                                error,
                                actor_id,
                                origin,
                            } = &event.kind
                            {
                                if job_kind == COMPACTION_JOB_KIND_SUMMARIZER_V1 {
                                    let created = parse_compaction_job_created_checkpoints(result);
                                    last_job_outcome = Some(CompactionStatusJobOutcomeV1 {
                                        job_id: job_id.clone(),
                                        job_kind: job_kind.clone(),
                                        status: status.clone(),
                                        error: error.clone(),
                                        created,
                                        actor_id: actor_id.clone(),
                                        origin: origin.clone(),
                                        seq: event.seq,
                                        timestamp_ms: event.timestamp_ms,
                                    });
                                }
                            }
                        }

                        if last_schedule_decision.is_some() && last_job_outcome.is_some() {
                            break;
                        }
                    }

                    if tail.complete {
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }

            tail_bytes = (tail_bytes * 2).min(MAX_TAIL_BYTES);
        }

        let need_replay = latest_checkpoint.is_none()
            || last_schedule_decision.is_none()
            || last_job_outcome.is_none();

        if need_replay {
            let events = self
                .replay_events(thread_id)
                .map_err(|err| format!("continuity replay failed: {err}"))?;
            if events.is_empty() {
                return Err("thread_not_found".to_string());
            }

            if latest_checkpoint.is_none() {
                let mut best: Option<CompactionStatusCheckpointV1> = None;
                let mut best_to_seq: u64 = 0;
                let mut best_event_seq: u64 = 0;
                for event in &events {
                    let EventKind::ContinuityCompactionCheckpointCreated {
                        checkpoint_id,
                        cut_rule_id,
                        summary_kind,
                        summary_artifact_id,
                        to_seq,
                        to_message_id,
                        ..
                    } = &event.kind
                    else {
                        continue;
                    };

                    if *to_seq > best_to_seq
                        || (*to_seq == best_to_seq && event.seq > best_event_seq)
                    {
                        best_to_seq = *to_seq;
                        best_event_seq = event.seq;
                        best = Some(CompactionStatusCheckpointV1 {
                            checkpoint_id: checkpoint_id.clone(),
                            cut_rule_id: cut_rule_id.clone(),
                            summary_kind: summary_kind.clone(),
                            summary_artifact_id: summary_artifact_id.clone(),
                            to_seq: *to_seq,
                            to_message_id: to_message_id.clone(),
                        });
                    }
                }
                latest_checkpoint = best;
            }

            for event in events.iter().rev() {
                if last_schedule_decision.is_none() {
                    if let EventKind::ContinuityCompactionAutoScheduleDecided {
                        decision_id,
                        policy_id,
                        decision,
                        execute,
                        stride_messages,
                        max_new_checkpoints,
                        block_on_inflight,
                        message_count,
                        cut_rule_id,
                        planned,
                        job_id,
                        job_kind,
                        actor_id,
                        origin,
                        ..
                    } = &event.kind
                    {
                        let planned_v1 = planned
                            .iter()
                            .map(|p| CompactionPlannedCutPointV1 {
                                target_message_ordinal: p.target_message_ordinal,
                                to_seq: p.to_seq,
                                to_message_id: p.to_message_id.clone(),
                            })
                            .collect();
                        last_schedule_decision = Some(CompactionStatusScheduleDecisionV1 {
                            decision_id: decision_id.clone(),
                            policy_id: policy_id.clone(),
                            decision: decision.clone(),
                            execute: *execute,
                            stride_messages: *stride_messages,
                            max_new_checkpoints: *max_new_checkpoints,
                            block_on_inflight: *block_on_inflight,
                            message_count: *message_count,
                            cut_rule_id: cut_rule_id.clone(),
                            planned: planned_v1,
                            job_id: job_id.clone(),
                            job_kind: job_kind.clone(),
                            actor_id: actor_id.clone(),
                            origin: origin.clone(),
                            seq: event.seq,
                            timestamp_ms: event.timestamp_ms,
                        });
                    }
                }

                if last_job_outcome.is_none() {
                    if let EventKind::ContinuityJobEnded {
                        job_id,
                        job_kind,
                        status,
                        result,
                        error,
                        actor_id,
                        origin,
                    } = &event.kind
                    {
                        if job_kind == COMPACTION_JOB_KIND_SUMMARIZER_V1 {
                            let created = parse_compaction_job_created_checkpoints(result);
                            last_job_outcome = Some(CompactionStatusJobOutcomeV1 {
                                job_id: job_id.clone(),
                                job_kind: job_kind.clone(),
                                status: status.clone(),
                                error: error.clone(),
                                created,
                                actor_id: actor_id.clone(),
                                origin: origin.clone(),
                                seq: event.seq,
                                timestamp_ms: event.timestamp_ms,
                            });
                        }
                    }
                }

                if last_schedule_decision.is_some() && last_job_outcome.is_some() {
                    break;
                }
            }
        }

        Ok(CompactionStatusV1Response {
            thread_id: thread_id.to_string(),
            stride_messages: stride,
            message_count: cut_points.message_count,
            latest_checkpoint,
            next_cut_point,
            inflight_job_id,
            last_schedule_decision,
            last_job_outcome,
        })
    }

    pub fn provider_cursor_status_v1(
        &self,
        thread_id: &str,
        _req: ProviderCursorStatusV1Request,
    ) -> Result<ProviderCursorStatusV1Response, String> {
        if self.get(thread_id).is_none() {
            return Err("not_found".to_string());
        }

        #[derive(Clone, PartialEq, Eq, Hash)]
        struct Key {
            provider: String,
            endpoint: Option<String>,
            model: Option<String>,
        }

        let mut active: Option<ProviderCursorStatusCursorV1> = None;
        let mut by_key: HashMap<Key, ProviderCursorStatusCursorV1> = HashMap::new();

        const INITIAL_TAIL_BYTES: usize = 256 * 1024;
        const MAX_TAIL_BYTES: usize = 8 * 1024 * 1024;
        const MAX_TAIL_EVENTS: usize = 10_000;
        const MAX_KEYS: usize = 32;

        let mut tail_bytes = INITIAL_TAIL_BYTES;
        let mut scanned_sidecar = false;
        while tail_bytes <= MAX_TAIL_BYTES {
            match self
                .stream_cache
                .scan_tail(thread_id, MAX_TAIL_EVENTS, tail_bytes)
            {
                Ok(Some(tail)) => {
                    scanned_sidecar = true;
                    for event in tail.events.iter().rev() {
                        let EventKind::ContinuityProviderCursorUpdated {
                            provider,
                            endpoint,
                            model,
                            cursor,
                            action,
                            reason,
                            run_session_id,
                            actor_id,
                            origin,
                        } = &event.kind
                        else {
                            continue;
                        };

                        let cursor_row = ProviderCursorStatusCursorV1 {
                            cursor_event_id: event.id.clone(),
                            provider: provider.clone(),
                            endpoint: endpoint.clone(),
                            model: model.clone(),
                            cursor: cursor.clone(),
                            action: action.clone(),
                            reason: reason.clone(),
                            run_session_id: run_session_id.clone(),
                            actor_id: actor_id.clone(),
                            origin: origin.clone(),
                            seq: event.seq,
                            timestamp_ms: event.timestamp_ms,
                        };

                        if active.is_none() {
                            active = Some(cursor_row.clone());
                        }

                        let key = Key {
                            provider: provider.clone(),
                            endpoint: endpoint.clone(),
                            model: model.clone(),
                        };
                        by_key.entry(key).or_insert(cursor_row);

                        if by_key.len() >= MAX_KEYS {
                            break;
                        }
                    }

                    if tail.complete || by_key.len() >= MAX_KEYS {
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }

            tail_bytes = (tail_bytes * 2).min(MAX_TAIL_BYTES);
        }

        if !scanned_sidecar {
            let events = self
                .replay_events(thread_id)
                .map_err(|err| format!("continuity replay failed: {err}"))?;

            for event in events.iter().rev() {
                let EventKind::ContinuityProviderCursorUpdated {
                    provider,
                    endpoint,
                    model,
                    cursor,
                    action,
                    reason,
                    run_session_id,
                    actor_id,
                    origin,
                } = &event.kind
                else {
                    continue;
                };

                let cursor_row = ProviderCursorStatusCursorV1 {
                    cursor_event_id: event.id.clone(),
                    provider: provider.clone(),
                    endpoint: endpoint.clone(),
                    model: model.clone(),
                    cursor: cursor.clone(),
                    action: action.clone(),
                    reason: reason.clone(),
                    run_session_id: run_session_id.clone(),
                    actor_id: actor_id.clone(),
                    origin: origin.clone(),
                    seq: event.seq,
                    timestamp_ms: event.timestamp_ms,
                };

                if active.is_none() {
                    active = Some(cursor_row.clone());
                }

                let key = Key {
                    provider: provider.clone(),
                    endpoint: endpoint.clone(),
                    model: model.clone(),
                };
                by_key.entry(key).or_insert(cursor_row);

                if by_key.len() >= MAX_KEYS {
                    break;
                }
            }
        }

        let mut cursors: Vec<ProviderCursorStatusCursorV1> = by_key.into_values().collect();
        cursors.sort_by(|a, b| {
            (
                a.provider.as_str(),
                a.endpoint.as_deref().unwrap_or(""),
                a.model.as_deref().unwrap_or(""),
            )
                .cmp(&(
                    b.provider.as_str(),
                    b.endpoint.as_deref().unwrap_or(""),
                    b.model.as_deref().unwrap_or(""),
                ))
        });

        Ok(ProviderCursorStatusV1Response {
            thread_id: thread_id.to_string(),
            active,
            cursors,
        })
    }

    pub fn provider_cursor_rotate_v1(
        &self,
        thread_id: &str,
        mut req: ProviderCursorRotateV1Request,
    ) -> Result<ProviderCursorRotateV1Response, String> {
        if self.get(thread_id).is_none() {
            return Err("not_found".to_string());
        }

        if req.actor_id.trim().is_empty() {
            req.actor_id = "user".to_string();
        }
        if req.origin.trim().is_empty() {
            req.origin = "unknown".to_string();
        }

        let matches_filter = |provider: &str,
                              endpoint: Option<&str>,
                              model: Option<&str>,
                              req: &ProviderCursorRotateV1Request|
         -> bool {
            if let Some(filter) = req.provider.as_deref() {
                if provider != filter {
                    return false;
                }
            }
            if let Some(filter) = req.endpoint.as_deref() {
                if endpoint != Some(filter) {
                    return false;
                }
            }
            if let Some(filter) = req.model.as_deref() {
                if model != Some(filter) {
                    return false;
                }
            }
            true
        };

        const INITIAL_TAIL_BYTES: usize = 256 * 1024;
        const MAX_TAIL_BYTES: usize = 8 * 1024 * 1024;
        const MAX_TAIL_EVENTS: usize = 10_000;

        let mut target: Option<(String, Option<String>, Option<String>)> = None;
        let mut tail_bytes = INITIAL_TAIL_BYTES;
        while tail_bytes <= MAX_TAIL_BYTES && target.is_none() {
            match self
                .stream_cache
                .scan_tail(thread_id, MAX_TAIL_EVENTS, tail_bytes)
            {
                Ok(Some(tail)) => {
                    for event in tail.events.iter().rev() {
                        let EventKind::ContinuityProviderCursorUpdated {
                            provider,
                            endpoint,
                            model,
                            ..
                        } = &event.kind
                        else {
                            continue;
                        };
                        if !matches_filter(provider, endpoint.as_deref(), model.as_deref(), &req) {
                            continue;
                        }
                        target = Some((provider.clone(), endpoint.clone(), model.clone()));
                        break;
                    }
                    if tail.complete {
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
            tail_bytes = (tail_bytes * 2).min(MAX_TAIL_BYTES);
        }

        if target.is_none() {
            let events = self
                .replay_events(thread_id)
                .map_err(|err| format!("continuity replay failed: {err}"))?;
            for event in events.iter().rev() {
                let EventKind::ContinuityProviderCursorUpdated {
                    provider,
                    endpoint,
                    model,
                    ..
                } = &event.kind
                else {
                    continue;
                };
                if !matches_filter(provider, endpoint.as_deref(), model.as_deref(), &req) {
                    continue;
                }
                target = Some((provider.clone(), endpoint.clone(), model.clone()));
                break;
            }
        }

        let Some((provider, endpoint, model)) = target else {
            return Ok(ProviderCursorRotateV1Response {
                thread_id: thread_id.to_string(),
                rotated: false,
                provider: None,
                endpoint: None,
                model: None,
                cursor_event_id: None,
            });
        };

        let id = self.append_provider_cursor_updated(
            thread_id,
            ProviderCursorUpdatedPayload {
                provider: provider.clone(),
                endpoint: endpoint.clone(),
                model: model.clone(),
                cursor: None,
                action: "rotated".to_string(),
                reason: req.reason.clone(),
                run_session_id: None,
                actor_id: req.actor_id,
                origin: req.origin,
            },
        )?;

        Ok(ProviderCursorRotateV1Response {
            thread_id: thread_id.to_string(),
            rotated: true,
            provider: Some(provider),
            endpoint,
            model,
            cursor_event_id: Some(id),
        })
    }

    pub fn context_selection_status_v1(
        &self,
        thread_id: &str,
        req: ContextSelectionStatusV1Request,
    ) -> Result<ContextSelectionStatusV1Response, String> {
        if self.get(thread_id).is_none() {
            return Err("not_found".to_string());
        }

        const DEFAULT_LIMIT: usize = 10;
        const MAX_LIMIT: usize = 50;
        const INITIAL_TAIL_BYTES: usize = 256 * 1024;
        const MAX_TAIL_BYTES: usize = 8 * 1024 * 1024;
        const MAX_TAIL_EVENTS: usize = 100_000;

        let mut limit = req.limit.unwrap_or(DEFAULT_LIMIT as u32) as usize;
        limit = limit.min(MAX_LIMIT);

        let mut decisions: Vec<ContextSelectionStatusDecisionV1> = Vec::new();

        let mut tail_complete = false;
        let mut scanned_sidecar = false;
        let mut tail_bytes = INITIAL_TAIL_BYTES;
        while tail_bytes <= MAX_TAIL_BYTES && decisions.len() < limit {
            match self
                .stream_cache
                .scan_tail(thread_id, MAX_TAIL_EVENTS, tail_bytes)
            {
                Ok(Some(tail)) => {
                    scanned_sidecar = true;
                    for event in tail.events.iter().rev() {
                        let EventKind::ContinuityContextSelectionDecided {
                            run_session_id,
                            message_id,
                            compiler_id,
                            compiler_strategy,
                            limits,
                            compaction_checkpoint,
                            resets,
                            reason,
                            actor_id,
                            origin,
                        } = &event.kind
                        else {
                            continue;
                        };

                        let checkpoint = compaction_checkpoint.as_ref().map(|ckpt| {
                            ContextSelectionStatusCheckpointV1 {
                                checkpoint_id: ckpt.checkpoint_id.clone(),
                                summary_kind: ckpt.summary_kind.clone(),
                                summary_artifact_id: ckpt.summary_artifact_id.clone(),
                                to_seq: ckpt.to_seq,
                            }
                        });

                        let resets_v1 = resets
                            .iter()
                            .map(|reset| ContextSelectionStatusResetV1 {
                                input: reset.input.clone(),
                                action: reset.action.clone(),
                                reason: reset.reason.clone(),
                                ref_: reset.ref_.clone(),
                            })
                            .collect();

                        decisions.push(ContextSelectionStatusDecisionV1 {
                            decision_event_id: event.id.clone(),
                            run_session_id: run_session_id.clone(),
                            message_id: message_id.clone(),
                            compiler_id: compiler_id.clone(),
                            compiler_strategy: compiler_strategy.clone(),
                            limits: limits.clone(),
                            compaction_checkpoint: checkpoint,
                            resets: resets_v1,
                            reason: reason.clone(),
                            actor_id: actor_id.clone(),
                            origin: origin.clone(),
                            seq: event.seq,
                            timestamp_ms: event.timestamp_ms,
                        });

                        if decisions.len() >= limit {
                            break;
                        }
                    }

                    if tail.complete {
                        tail_complete = true;
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }

            tail_bytes = (tail_bytes * 2).min(MAX_TAIL_BYTES);
        }

        if !scanned_sidecar || (!tail_complete && decisions.len() < limit) {
            let events = self
                .replay_events(thread_id)
                .map_err(|err| format!("continuity replay failed: {err}"))?;

            decisions.clear();
            for event in events.iter().rev() {
                let EventKind::ContinuityContextSelectionDecided {
                    run_session_id,
                    message_id,
                    compiler_id,
                    compiler_strategy,
                    limits,
                    compaction_checkpoint,
                    resets,
                    reason,
                    actor_id,
                    origin,
                } = &event.kind
                else {
                    continue;
                };

                let checkpoint =
                    compaction_checkpoint
                        .as_ref()
                        .map(|ckpt| ContextSelectionStatusCheckpointV1 {
                            checkpoint_id: ckpt.checkpoint_id.clone(),
                            summary_kind: ckpt.summary_kind.clone(),
                            summary_artifact_id: ckpt.summary_artifact_id.clone(),
                            to_seq: ckpt.to_seq,
                        });

                let resets_v1 = resets
                    .iter()
                    .map(|reset| ContextSelectionStatusResetV1 {
                        input: reset.input.clone(),
                        action: reset.action.clone(),
                        reason: reset.reason.clone(),
                        ref_: reset.ref_.clone(),
                    })
                    .collect();

                decisions.push(ContextSelectionStatusDecisionV1 {
                    decision_event_id: event.id.clone(),
                    run_session_id: run_session_id.clone(),
                    message_id: message_id.clone(),
                    compiler_id: compiler_id.clone(),
                    compiler_strategy: compiler_strategy.clone(),
                    limits: limits.clone(),
                    compaction_checkpoint: checkpoint,
                    resets: resets_v1,
                    reason: reason.clone(),
                    actor_id: actor_id.clone(),
                    origin: origin.clone(),
                    seq: event.seq,
                    timestamp_ms: event.timestamp_ms,
                });

                if decisions.len() >= limit {
                    break;
                }
            }
        }

        Ok(ContextSelectionStatusV1Response {
            thread_id: thread_id.to_string(),
            decisions,
        })
    }

    pub fn compaction_auto_v1(
        &self,
        thread_id: &str,
        req: CompactionAutoV1Request,
    ) -> Result<CompactionAutoV1Response, String> {
        let mut response = self.compaction_auto_spawn_job_v1(thread_id, req.clone())?;
        if response.status != "spawned" {
            return Ok(response);
        }

        let job_id = response
            .job_id
            .clone()
            .ok_or_else(|| "compaction auto spawned without job_id".to_string())?;

        match self.compaction_auto_run_spawned_job_v1(
            thread_id,
            &job_id,
            response.stride_messages,
            &response.cut_rule_id,
            &response.planned,
            (req.actor_id.as_str(), req.origin.as_str()),
        ) {
            Ok(result) => {
                response.status = "completed".to_string();
                response.result = result;
            }
            Err(err) => {
                response.status = "failed".to_string();
                response.error = Some(err);
            }
        }

        Ok(response)
    }

    pub fn compaction_auto_schedule_v1(
        &self,
        thread_id: &str,
        req: CompactionAutoScheduleV1Request,
    ) -> Result<CompactionAutoScheduleV1Response, String> {
        let mut response = self.compaction_auto_schedule_spawn_job_v1(thread_id, req.clone())?;
        if response.decision != "scheduled" || !response.execute {
            return Ok(response);
        }

        let job_id = response
            .job_id
            .clone()
            .ok_or_else(|| "compaction auto schedule spawned without job_id".to_string())?;

        match self.compaction_auto_run_spawned_job_v1(
            thread_id,
            &job_id,
            response.stride_messages,
            &response.cut_rule_id,
            &response.planned,
            (req.actor_id.as_str(), req.origin.as_str()),
        ) {
            Ok(result) => {
                response.decision = "completed".to_string();
                response.result = result;
            }
            Err(err) => {
                response.decision = "failed".to_string();
                response.error = Some(err);
            }
        }

        Ok(response)
    }

    pub(crate) fn compaction_auto_schedule_spawn_job_v1(
        &self,
        thread_id: &str,
        req: CompactionAutoScheduleV1Request,
    ) -> Result<CompactionAutoScheduleV1Response, String> {
        let stride = req.stride_messages.unwrap_or(10_000);
        if stride == 0 {
            return Err("invalid_stride".to_string());
        }
        let max_new_checkpoints = req.max_new_checkpoints.unwrap_or(1).clamp(1, 32);
        let block_on_inflight = req.block_on_inflight.unwrap_or(true);
        let execute = req.execute.unwrap_or(true);
        let dry_run = req.dry_run.unwrap_or(false);

        let cut_rule_id = format!("stride_messages_v1/{stride}");
        let policy_id = format!(
            "compaction_auto_schedule_v1/stride_messages_v1/{stride}/max_new_checkpoints_v1/{max_new_checkpoints}/block_on_inflight_v1/{block_on_inflight}"
        );

        // Plan latest-first cut points, skipping already checkpointed ones.
        let cut_points = self.compaction_cut_points_v1(
            thread_id,
            CompactionCutPointsV1Request {
                stride_messages: Some(stride),
                limit: Some(32),
            },
        )?;

        let mut planned: Vec<CompactionPlannedCutPointV1> = Vec::new();
        for cp in &cut_points.cut_points {
            if planned.len() as u32 >= max_new_checkpoints {
                break;
            }
            if cp.already_checkpointed {
                continue;
            }
            planned.push(CompactionPlannedCutPointV1 {
                target_message_ordinal: cp.target_message_ordinal,
                to_seq: cp.to_seq,
                to_message_id: cp.to_message_id.clone(),
            });
        }

        if planned.is_empty() {
            return Ok(CompactionAutoScheduleV1Response {
                thread_id: thread_id.to_string(),
                decision_id: None,
                policy_id,
                decision: "noop".to_string(),
                execute,
                stride_messages: stride,
                max_new_checkpoints,
                block_on_inflight,
                message_count: cut_points.message_count,
                cut_rule_id,
                planned,
                job_id: None,
                job_kind: None,
                result: Vec::new(),
                error: None,
            });
        }

        if dry_run {
            return Ok(CompactionAutoScheduleV1Response {
                thread_id: thread_id.to_string(),
                decision_id: None,
                policy_id,
                decision: "dry_run".to_string(),
                execute,
                stride_messages: stride,
                max_new_checkpoints,
                block_on_inflight,
                message_count: cut_points.message_count,
                cut_rule_id,
                planned,
                job_id: None,
                job_kind: None,
                result: Vec::new(),
                error: None,
            });
        }

        let inflight_job_id = if block_on_inflight {
            self.find_inflight_compaction_job_id_best_effort_v1(thread_id)
        } else {
            None
        };

        if let Some(job_id) = inflight_job_id {
            let decision_id = Uuid::new_v4().to_string();
            let planned_frame = planned
                .iter()
                .map(|p| CompactionPlannedCutPoint {
                    target_message_ordinal: p.target_message_ordinal,
                    to_seq: p.to_seq,
                    to_message_id: p.to_message_id.clone(),
                })
                .collect();
            self.append_compaction_auto_schedule_decided(
                thread_id,
                CompactionAutoScheduleDecidedPayload {
                    decision_id: decision_id.clone(),
                    policy_id: policy_id.clone(),
                    decision: "skipped_inflight".to_string(),
                    execute,
                    stride_messages: stride,
                    max_new_checkpoints,
                    block_on_inflight,
                    message_count: cut_points.message_count,
                    cut_rule_id: cut_rule_id.clone(),
                    planned: planned_frame,
                    job_id: None,
                    job_kind: None,
                    reason: Some(serde_json::json!({
                        "kind": "inflight_job",
                        "job_id": job_id,
                    })),
                    actor_id: req.actor_id.clone(),
                    origin: req.origin.clone(),
                },
            )?;

            return Ok(CompactionAutoScheduleV1Response {
                thread_id: thread_id.to_string(),
                decision_id: Some(decision_id),
                policy_id,
                decision: "skipped_inflight".to_string(),
                execute,
                stride_messages: stride,
                max_new_checkpoints,
                block_on_inflight,
                message_count: cut_points.message_count,
                cut_rule_id,
                planned,
                job_id: None,
                job_kind: None,
                result: Vec::new(),
                error: None,
            });
        }

        let spawned = self.compaction_auto_spawn_job_v1(
            thread_id,
            CompactionAutoV1Request {
                stride_messages: Some(stride),
                max_new_checkpoints: Some(max_new_checkpoints),
                dry_run: Some(false),
                actor_id: req.actor_id.clone(),
                origin: req.origin.clone(),
            },
        )?;

        if spawned.status != "spawned" {
            // The stream may have changed between planning and spawn; treat this as a no-op.
            return Ok(CompactionAutoScheduleV1Response {
                thread_id: thread_id.to_string(),
                decision_id: None,
                policy_id,
                decision: "noop".to_string(),
                execute,
                stride_messages: stride,
                max_new_checkpoints,
                block_on_inflight,
                message_count: cut_points.message_count,
                cut_rule_id,
                planned,
                job_id: None,
                job_kind: None,
                result: Vec::new(),
                error: None,
            });
        }

        let decision_id = Uuid::new_v4().to_string();
        let planned_frame = planned
            .iter()
            .map(|p| CompactionPlannedCutPoint {
                target_message_ordinal: p.target_message_ordinal,
                to_seq: p.to_seq,
                to_message_id: p.to_message_id.clone(),
            })
            .collect();
        self.append_compaction_auto_schedule_decided(
            thread_id,
            CompactionAutoScheduleDecidedPayload {
                decision_id: decision_id.clone(),
                policy_id: policy_id.clone(),
                decision: "scheduled".to_string(),
                execute,
                stride_messages: stride,
                max_new_checkpoints,
                block_on_inflight,
                message_count: cut_points.message_count,
                cut_rule_id: cut_rule_id.clone(),
                planned: planned_frame,
                job_id: spawned.job_id.clone(),
                job_kind: spawned.job_kind.clone(),
                reason: None,
                actor_id: req.actor_id.clone(),
                origin: req.origin.clone(),
            },
        )?;

        Ok(CompactionAutoScheduleV1Response {
            thread_id: thread_id.to_string(),
            decision_id: Some(decision_id),
            policy_id,
            decision: "scheduled".to_string(),
            execute,
            stride_messages: stride,
            max_new_checkpoints,
            block_on_inflight,
            message_count: cut_points.message_count,
            cut_rule_id,
            planned,
            job_id: spawned.job_id,
            job_kind: spawned.job_kind,
            result: Vec::new(),
            error: None,
        })
    }

    pub(crate) fn compaction_auto_spawn_job_v1(
        &self,
        thread_id: &str,
        req: CompactionAutoV1Request,
    ) -> Result<CompactionAutoV1Response, String> {
        let stride = req.stride_messages.unwrap_or(10_000);
        if stride == 0 {
            return Err("invalid_stride".to_string());
        }
        let max_new = req.max_new_checkpoints.unwrap_or(1).clamp(1, 32) as u64;
        let dry_run = req.dry_run.unwrap_or(false);
        let cut_rule_id = format!("stride_messages_v1/{stride}");

        // Plan latest-first cut points, skipping already checkpointed ones.
        let cut_points = self.compaction_cut_points_v1(
            thread_id,
            CompactionCutPointsV1Request {
                stride_messages: Some(stride),
                limit: Some(32),
            },
        )?;

        let mut planned: Vec<CompactionPlannedCutPointV1> = Vec::new();
        for cp in &cut_points.cut_points {
            if planned.len() as u64 >= max_new {
                break;
            }
            if cp.already_checkpointed {
                continue;
            }
            planned.push(CompactionPlannedCutPointV1 {
                target_message_ordinal: cp.target_message_ordinal,
                to_seq: cp.to_seq,
                to_message_id: cp.to_message_id.clone(),
            });
        }

        if planned.is_empty() || dry_run {
            return Ok(CompactionAutoV1Response {
                thread_id: thread_id.to_string(),
                job_id: None,
                job_kind: None,
                status: "noop".to_string(),
                stride_messages: stride,
                message_count: cut_points.message_count,
                cut_rule_id,
                planned,
                result: Vec::new(),
                error: None,
            });
        }

        let job_id = Uuid::new_v4().to_string();
        let details_cut_rule_id = cut_rule_id.clone();
        let details_planned = planned.clone();
        let details = serde_json::json!({
            "schema": "rip.job.compaction_summarizer.v1",
            "cut_rule_id": details_cut_rule_id,
            "stride_messages": stride,
            "planned": details_planned,
        });

        self.append_job_spawned(
            thread_id,
            &job_id,
            COMPACTION_JOB_KIND_SUMMARIZER_V1,
            Some(details),
            req.actor_id.clone(),
            req.origin.clone(),
        )?;

        Ok(CompactionAutoV1Response {
            thread_id: thread_id.to_string(),
            job_id: Some(job_id),
            job_kind: Some(COMPACTION_JOB_KIND_SUMMARIZER_V1.to_string()),
            status: "spawned".to_string(),
            stride_messages: stride,
            message_count: cut_points.message_count,
            cut_rule_id,
            planned,
            result: Vec::new(),
            error: None,
        })
    }

    pub(crate) fn compaction_auto_run_spawned_job_v1(
        &self,
        thread_id: &str,
        job_id: &str,
        stride_messages: u64,
        cut_rule_id: &str,
        planned: &[CompactionPlannedCutPointV1],
        provenance: (&str, &str),
    ) -> Result<Vec<CompactionAutoResultCheckpointV1>, String> {
        let (actor_id, origin) = provenance;
        let mut created: Vec<CompactionAutoResultCheckpointV1> = Vec::new();

        let continuity_events = self
            .replay_events(thread_id)
            .map_err(|err| format!("continuity replay failed: {err}"))?;
        if continuity_events.is_empty() {
            return Err("thread_not_found".to_string());
        }

        struct MessageRef<'a> {
            seq: u64,
            id: &'a str,
            actor_id: &'a str,
            content: &'a str,
        }

        let mut messages: Vec<MessageRef<'_>> = Vec::new();
        for event in &continuity_events {
            let EventKind::ContinuityMessageAppended {
                actor_id, content, ..
            } = &event.kind
            else {
                continue;
            };
            messages.push(MessageRef {
                seq: event.seq,
                id: event.id.as_str(),
                actor_id,
                content,
            });
        }

        fn upper_bound_message_seq(messages: &[MessageRef<'_>], target_seq: u64) -> usize {
            match messages.binary_search_by(|m| m.seq.cmp(&target_seq)) {
                Ok(idx) => idx.saturating_add(1),
                Err(idx) => idx,
            }
        }

        let run_result: Result<(), String> = (|| {
            let mut planned_sorted: Vec<CompactionPlannedCutPointV1> = planned.to_vec();
            planned_sorted.sort_by(|a, b| {
                a.to_seq
                    .cmp(&b.to_seq)
                    .then(a.to_message_id.cmp(&b.to_message_id))
            });

            for cut in &planned_sorted {
                // Best-effort basis: most recent prior cumulative checkpoint (by `to_seq`).
                let mut base_summary_artifact_id: Option<String> = None;
                let mut base_to_seq: u64 = 0;
                if cut.to_seq > 0 {
                    let mut search_max = cut.to_seq.saturating_sub(1);
                    while search_max > 0 {
                        let cache_best = self
                            .stream_cache
                            .latest_compaction_checkpoint_before_or_at_seq_v1(
                                thread_id, search_max,
                            );

                        match cache_best {
                            Ok(Some(event)) => {
                                let EventKind::ContinuityCompactionCheckpointCreated {
                                    summary_kind,
                                    summary_artifact_id,
                                    to_seq: checkpoint_to_seq,
                                    ..
                                } = &event.kind
                                else {
                                    break;
                                };
                                if summary_kind == COMPACTION_SUMMARY_KIND_CUMULATIVE_V1 {
                                    base_summary_artifact_id = Some(summary_artifact_id.clone());
                                    base_to_seq = *checkpoint_to_seq;
                                    break;
                                }
                                // Skip all checkpoints at this `to_seq` and keep searching.
                                search_max = checkpoint_to_seq.saturating_sub(1);
                            }
                            Ok(None) | Err(_) => {
                                let mut best_to_seq: u64 = 0;
                                let mut best_event_seq: u64 = 0;
                                for event in &continuity_events {
                                    let EventKind::ContinuityCompactionCheckpointCreated {
                                        summary_kind,
                                        summary_artifact_id,
                                        to_seq: checkpoint_to_seq,
                                        ..
                                    } = &event.kind
                                    else {
                                        continue;
                                    };
                                    if summary_kind != COMPACTION_SUMMARY_KIND_CUMULATIVE_V1 {
                                        continue;
                                    }
                                    if *checkpoint_to_seq >= cut.to_seq {
                                        continue;
                                    }
                                    if *checkpoint_to_seq > best_to_seq
                                        || (*checkpoint_to_seq == best_to_seq
                                            && event.seq > best_event_seq)
                                    {
                                        best_to_seq = *checkpoint_to_seq;
                                        best_event_seq = event.seq;
                                        base_summary_artifact_id =
                                            Some(summary_artifact_id.clone());
                                    }
                                }
                                base_to_seq = best_to_seq;
                                break;
                            }
                        }
                    }
                }

                let mut base_summary_markdown: Option<String> = None;
                let mut bootstrap = base_summary_artifact_id.is_none();
                let mut basis_note: Option<String> = None;

                if let Some(base_id) = base_summary_artifact_id.as_deref() {
                    match read_compaction_summary_v1(&self.workspace_root, base_id) {
                        Ok(summary) => {
                            let markdown = summary.summary_markdown().to_string();
                            if summary_markdown_is_legacy_metadata_placeholder(&markdown) {
                                bootstrap = true;
                                basis_note =
                                    Some("bootstrap_from_truth_v0.2/legacy_base".to_string());
                            } else {
                                base_summary_markdown = Some(markdown);
                            }
                        }
                        Err(err) => {
                            bootstrap = true;
                            basis_note =
                                Some(format!("bootstrap_from_truth_v0.2/base_read_failed: {err}"));
                        }
                    }
                }

                let start_seq_exclusive = if bootstrap { 0 } else { base_to_seq };
                let start_idx = upper_bound_message_seq(&messages, start_seq_exclusive);
                let end_idx = upper_bound_message_seq(&messages, cut.to_seq);
                let Some(last) = messages.get(end_idx.saturating_sub(1)) else {
                    return Err(format!(
                        "compaction cut point message not found: to_seq={} to_message_id={}",
                        cut.to_seq, cut.to_message_id
                    ));
                };
                if last.seq != cut.to_seq || last.id != cut.to_message_id {
                    return Err(format!(
                        "compaction cut point message mismatch: expected to_seq={} to_message_id={}, got to_seq={} to_message_id={}",
                        cut.to_seq,
                        cut.to_message_id,
                        last.seq,
                        last.id
                    ));
                }

                let mut acc = AutoSummaryAccumulator::default();
                for msg in messages[start_idx..end_idx].iter() {
                    acc.observe_message(msg.actor_id, msg.content);
                }
                let delta = acc.finish();

                let summary_markdown =
                    render_auto_compaction_summary_markdown_v0_2(RenderAutoSummaryMarkdownParams {
                        thread_id,
                        cut_rule_id,
                        stride_messages,
                        target_message_ordinal: cut.target_message_ordinal,
                        to_seq: cut.to_seq,
                        to_message_id: &cut.to_message_id,
                        base_summary_artifact_id: base_summary_artifact_id.as_deref(),
                        base_summary_markdown: base_summary_markdown.as_deref(),
                        basis_note: basis_note.as_deref(),
                        delta,
                        bootstrap,
                    });

                let summary = CompactionSummaryV1::new_cumulative_source_cut(
                    crate::compaction_summary::NewCumulativeCompactionSummaryV1 {
                        thread_id: thread_id.to_string(),
                        to_seq: cut.to_seq,
                        to_message_id: Some(cut.to_message_id.clone()),
                        actor_id: actor_id.to_string(),
                        origin: origin.to_string(),
                        produced_by: Some(("job".to_string(), job_id.to_string())),
                        base_summary_artifact_id,
                        basis_note,
                        summary_markdown,
                    },
                );
                let summary_artifact_id =
                    write_compaction_summary_v1(&self.workspace_root, &summary)?;

                let checkpoint_id = self.append_compaction_checkpoint_created(
                    thread_id,
                    CompactionCheckpointCreatedPayload {
                        cut_rule_id: cut_rule_id.to_string(),
                        summary_kind: COMPACTION_SUMMARY_KIND_CUMULATIVE_V1.to_string(),
                        summary_artifact_id: summary_artifact_id.clone(),
                        from_seq: 0,
                        from_message_id: None,
                        to_seq: cut.to_seq,
                        to_message_id: Some(cut.to_message_id.clone()),
                        actor_id: actor_id.to_string(),
                        origin: origin.to_string(),
                    },
                )?;

                created.push(CompactionAutoResultCheckpointV1 {
                    checkpoint_id,
                    summary_artifact_id,
                    to_seq: cut.to_seq,
                    to_message_id: cut.to_message_id.clone(),
                    cut_rule_id: cut_rule_id.to_string(),
                });
            }
            Ok(())
        })();

        match run_result {
            Ok(()) => {
                let result_created = created.clone();
                let result = serde_json::json!({
                    "schema": "rip.job_result.compaction_summarizer.v1",
                    "created": result_created,
                });
                self.append_job_ended(
                    thread_id,
                    JobEndedPayload {
                        job_id: job_id.to_string(),
                        job_kind: COMPACTION_JOB_KIND_SUMMARIZER_V1.to_string(),
                        status: "completed".to_string(),
                        result: Some(result),
                        error: None,
                        actor_id: actor_id.to_string(),
                        origin: origin.to_string(),
                    },
                )?;
                Ok(created)
            }
            Err(err) => {
                let result_created = created.clone();
                let result = serde_json::json!({
                    "schema": "rip.job_result.compaction_summarizer.v1",
                    "created": result_created,
                });
                let _ = self.append_job_ended(
                    thread_id,
                    JobEndedPayload {
                        job_id: job_id.to_string(),
                        job_kind: COMPACTION_JOB_KIND_SUMMARIZER_V1.to_string(),
                        status: "failed".to_string(),
                        result: Some(result),
                        error: Some(err.clone()),
                        actor_id: actor_id.to_string(),
                        origin: origin.to_string(),
                    },
                );
                Err(err)
            }
        }
    }

    pub fn list(&self) -> Vec<ContinuityMeta> {
        let index = self.index.lock().expect("continuity index mutex");
        index
            .continuities
            .iter()
            .map(|(id, meta)| ContinuityMeta {
                continuity_id: id.clone(),
                created_at_ms: meta.created_at_ms,
                title: meta.title.clone(),
                archived: meta.archived,
            })
            .collect()
    }

    pub fn get(&self, continuity_id: &str) -> Option<ContinuityMeta> {
        let index = self.index.lock().expect("continuity index mutex");
        let meta = index.continuities.get(continuity_id)?;
        Some(ContinuityMeta {
            continuity_id: continuity_id.to_string(),
            created_at_ms: meta.created_at_ms,
            title: meta.title.clone(),
            archived: meta.archived,
        })
    }

    pub fn append_message(
        &self,
        continuity_id: &str,
        actor_id: String,
        origin: String,
        content: String,
    ) -> Result<String, String> {
        let mut next_seq = self.next_seq.lock().expect("continuity seq mutex");
        let seq = match next_seq.get(continuity_id).cloned() {
            Some(seq) => seq,
            None => {
                let seq = self
                    .load_next_seq_for(continuity_id)
                    .map_err(|err| format!("resolve continuity seq: {err}"))?;
                next_seq.insert(continuity_id.to_string(), seq);
                seq
            }
        };

        let message_id = Uuid::new_v4().to_string();
        let event = Event {
            id: message_id.clone(),
            session_id: continuity_id.to_string(),
            timestamp_ms: now_ms(),
            seq,
            kind: EventKind::ContinuityMessageAppended {
                actor_id,
                origin,
                content,
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity message: {err}"))?;
        self.stream_cache.append_best_effort(&event);
        let _ = self.sender.send(event.clone());

        // Only advance after a successful append to avoid gaps in the truth log.
        next_seq.insert(continuity_id.to_string(), seq + 1);
        Ok(message_id)
    }

    pub fn append_run_spawned(
        &self,
        continuity_id: &str,
        message_id: &str,
        session_id: &str,
        actor_id: String,
        origin: String,
    ) -> Result<String, String> {
        let mut next_seq = self.next_seq.lock().expect("continuity seq mutex");
        let seq = match next_seq.get(continuity_id).cloned() {
            Some(seq) => seq,
            None => {
                let seq = self
                    .load_next_seq_for(continuity_id)
                    .map_err(|err| format!("resolve continuity seq: {err}"))?;
                next_seq.insert(continuity_id.to_string(), seq);
                seq
            }
        };

        let id = Uuid::new_v4().to_string();
        let event = Event {
            id: id.clone(),
            session_id: continuity_id.to_string(),
            timestamp_ms: now_ms(),
            seq,
            kind: EventKind::ContinuityRunSpawned {
                run_session_id: session_id.to_string(),
                message_id: message_id.to_string(),
                actor_id: Some(actor_id),
                origin: Some(origin),
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity run spawned: {err}"))?;
        self.stream_cache.append_best_effort(&event);
        let _ = self.sender.send(event.clone());

        next_seq.insert(continuity_id.to_string(), seq + 1);
        Ok(id)
    }

    pub(crate) fn append_context_selection_decided(
        &self,
        continuity_id: &str,
        payload: ContextSelectionDecidedPayload,
    ) -> Result<String, String> {
        let mut next_seq = self.next_seq.lock().expect("continuity seq mutex");
        let seq = match next_seq.get(continuity_id).cloned() {
            Some(seq) => seq,
            None => {
                let seq = self
                    .load_next_seq_for(continuity_id)
                    .map_err(|err| format!("resolve continuity seq: {err}"))?;
                next_seq.insert(continuity_id.to_string(), seq);
                seq
            }
        };

        let id = Uuid::new_v4().to_string();
        let event = Event {
            id: id.clone(),
            session_id: continuity_id.to_string(),
            timestamp_ms: now_ms(),
            seq,
            kind: EventKind::ContinuityContextSelectionDecided {
                run_session_id: payload.run_session_id,
                message_id: payload.message_id,
                compiler_id: payload.compiler_id,
                compiler_strategy: payload.compiler_strategy,
                limits: payload.limits,
                compaction_checkpoint: payload.compaction_checkpoint,
                resets: payload.resets,
                reason: payload.reason,
                actor_id: payload.actor_id,
                origin: payload.origin,
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity context selection decided: {err}"))?;
        self.stream_cache.append_best_effort(&event);
        let _ = self.sender.send(event.clone());

        next_seq.insert(continuity_id.to_string(), seq + 1);
        Ok(id)
    }

    pub(crate) fn append_context_compiled(
        &self,
        continuity_id: &str,
        payload: ContextCompiledPayload,
    ) -> Result<String, String> {
        let mut next_seq = self.next_seq.lock().expect("continuity seq mutex");
        let seq = match next_seq.get(continuity_id).cloned() {
            Some(seq) => seq,
            None => {
                let seq = self
                    .load_next_seq_for(continuity_id)
                    .map_err(|err| format!("resolve continuity seq: {err}"))?;
                next_seq.insert(continuity_id.to_string(), seq);
                seq
            }
        };

        let id = Uuid::new_v4().to_string();
        let event = Event {
            id: id.clone(),
            session_id: continuity_id.to_string(),
            timestamp_ms: now_ms(),
            seq,
            kind: EventKind::ContinuityContextCompiled {
                run_session_id: payload.run_session_id,
                bundle_artifact_id: payload.bundle_artifact_id,
                compiler_id: payload.compiler_id,
                compiler_strategy: payload.compiler_strategy,
                from_seq: payload.from_seq,
                from_message_id: payload.from_message_id,
                actor_id: payload.actor_id,
                origin: payload.origin,
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity context compiled: {err}"))?;
        self.stream_cache.append_best_effort(&event);
        let _ = self.sender.send(event.clone());

        next_seq.insert(continuity_id.to_string(), seq + 1);
        Ok(id)
    }

    pub(crate) fn append_provider_cursor_updated(
        &self,
        continuity_id: &str,
        payload: ProviderCursorUpdatedPayload,
    ) -> Result<String, String> {
        let mut next_seq = self.next_seq.lock().expect("continuity seq mutex");
        let seq = match next_seq.get(continuity_id).cloned() {
            Some(seq) => seq,
            None => {
                let seq = self
                    .load_next_seq_for(continuity_id)
                    .map_err(|err| format!("resolve continuity seq: {err}"))?;
                next_seq.insert(continuity_id.to_string(), seq);
                seq
            }
        };

        let id = Uuid::new_v4().to_string();
        let event = Event {
            id: id.clone(),
            session_id: continuity_id.to_string(),
            timestamp_ms: now_ms(),
            seq,
            kind: EventKind::ContinuityProviderCursorUpdated {
                provider: payload.provider,
                endpoint: payload.endpoint,
                model: payload.model,
                cursor: payload.cursor,
                action: payload.action,
                reason: payload.reason,
                run_session_id: payload.run_session_id,
                actor_id: payload.actor_id,
                origin: payload.origin,
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity provider cursor updated: {err}"))?;
        self.stream_cache.append_best_effort(&event);
        let _ = self.sender.send(event.clone());

        next_seq.insert(continuity_id.to_string(), seq + 1);
        Ok(id)
    }

    fn append_compaction_checkpoint_created(
        &self,
        continuity_id: &str,
        payload: CompactionCheckpointCreatedPayload,
    ) -> Result<String, String> {
        let mut next_seq = self.next_seq.lock().expect("continuity seq mutex");
        let seq = match next_seq.get(continuity_id).cloned() {
            Some(seq) => seq,
            None => {
                let seq = self
                    .load_next_seq_for(continuity_id)
                    .map_err(|err| format!("resolve continuity seq: {err}"))?;
                next_seq.insert(continuity_id.to_string(), seq);
                seq
            }
        };

        let checkpoint_id = Uuid::new_v4().to_string();
        let event = Event {
            id: checkpoint_id.clone(),
            session_id: continuity_id.to_string(),
            timestamp_ms: now_ms(),
            seq,
            kind: EventKind::ContinuityCompactionCheckpointCreated {
                checkpoint_id: checkpoint_id.clone(),
                cut_rule_id: payload.cut_rule_id,
                summary_kind: payload.summary_kind,
                summary_artifact_id: payload.summary_artifact_id,
                from_seq: payload.from_seq,
                from_message_id: payload.from_message_id,
                to_seq: payload.to_seq,
                to_message_id: payload.to_message_id,
                actor_id: payload.actor_id,
                origin: payload.origin,
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity compaction checkpoint: {err}"))?;
        self.stream_cache.append_best_effort(&event);
        let _ = self.sender.send(event.clone());

        next_seq.insert(continuity_id.to_string(), seq + 1);
        Ok(checkpoint_id)
    }

    fn append_compaction_auto_schedule_decided(
        &self,
        continuity_id: &str,
        payload: CompactionAutoScheduleDecidedPayload,
    ) -> Result<String, String> {
        let mut next_seq = self.next_seq.lock().expect("continuity seq mutex");
        let seq = match next_seq.get(continuity_id).cloned() {
            Some(seq) => seq,
            None => {
                let seq = self
                    .load_next_seq_for(continuity_id)
                    .map_err(|err| format!("resolve continuity seq: {err}"))?;
                next_seq.insert(continuity_id.to_string(), seq);
                seq
            }
        };

        let id = payload.decision_id.clone();
        let event = Event {
            id: id.clone(),
            session_id: continuity_id.to_string(),
            timestamp_ms: now_ms(),
            seq,
            kind: EventKind::ContinuityCompactionAutoScheduleDecided {
                decision_id: payload.decision_id,
                policy_id: payload.policy_id,
                decision: payload.decision,
                execute: payload.execute,
                stride_messages: payload.stride_messages,
                max_new_checkpoints: payload.max_new_checkpoints,
                block_on_inflight: payload.block_on_inflight,
                message_count: payload.message_count,
                cut_rule_id: payload.cut_rule_id,
                planned: payload.planned,
                job_id: payload.job_id,
                job_kind: payload.job_kind,
                reason: payload.reason,
                actor_id: payload.actor_id,
                origin: payload.origin,
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity compaction schedule decided: {err}"))?;
        self.stream_cache.append_best_effort(&event);
        let _ = self.sender.send(event.clone());

        next_seq.insert(continuity_id.to_string(), seq + 1);
        Ok(id)
    }

    fn append_job_spawned(
        &self,
        continuity_id: &str,
        job_id: &str,
        job_kind: &str,
        details: Option<serde_json::Value>,
        actor_id: String,
        origin: String,
    ) -> Result<String, String> {
        let mut next_seq = self.next_seq.lock().expect("continuity seq mutex");
        let seq = match next_seq.get(continuity_id).cloned() {
            Some(seq) => seq,
            None => {
                let seq = self
                    .load_next_seq_for(continuity_id)
                    .map_err(|err| format!("resolve continuity seq: {err}"))?;
                next_seq.insert(continuity_id.to_string(), seq);
                seq
            }
        };

        let id = Uuid::new_v4().to_string();
        let event = Event {
            id: id.clone(),
            session_id: continuity_id.to_string(),
            timestamp_ms: now_ms(),
            seq,
            kind: EventKind::ContinuityJobSpawned {
                job_id: job_id.to_string(),
                job_kind: job_kind.to_string(),
                details,
                actor_id,
                origin,
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity job spawned: {err}"))?;
        self.stream_cache.append_best_effort(&event);
        let _ = self.sender.send(event.clone());

        next_seq.insert(continuity_id.to_string(), seq + 1);
        Ok(id)
    }

    fn append_job_ended(
        &self,
        continuity_id: &str,
        payload: JobEndedPayload,
    ) -> Result<String, String> {
        let mut next_seq = self.next_seq.lock().expect("continuity seq mutex");
        let seq = match next_seq.get(continuity_id).cloned() {
            Some(seq) => seq,
            None => {
                let seq = self
                    .load_next_seq_for(continuity_id)
                    .map_err(|err| format!("resolve continuity seq: {err}"))?;
                next_seq.insert(continuity_id.to_string(), seq);
                seq
            }
        };

        let id = Uuid::new_v4().to_string();
        let event = Event {
            id: id.clone(),
            session_id: continuity_id.to_string(),
            timestamp_ms: now_ms(),
            seq,
            kind: EventKind::ContinuityJobEnded {
                job_id: payload.job_id,
                job_kind: payload.job_kind,
                status: payload.status,
                result: payload.result,
                error: payload.error,
                actor_id: payload.actor_id,
                origin: payload.origin,
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity job ended: {err}"))?;
        self.stream_cache.append_best_effort(&event);
        let _ = self.sender.send(event.clone());

        next_seq.insert(continuity_id.to_string(), seq + 1);
        Ok(id)
    }

    fn find_inflight_compaction_job_id_best_effort_v1(
        &self,
        continuity_id: &str,
    ) -> Option<String> {
        const MAX_TAIL_EVENTS: usize = 512;
        const MAX_TAIL_BYTES: usize = 512 * 1024;

        let tail = self
            .stream_cache
            .scan_tail(continuity_id, MAX_TAIL_EVENTS, MAX_TAIL_BYTES)
            .ok()
            .flatten()?;

        let mut ended: std::collections::HashSet<String> = std::collections::HashSet::new();
        for event in tail.events.iter().rev() {
            match &event.kind {
                EventKind::ContinuityJobEnded {
                    job_id, job_kind, ..
                } => {
                    if job_kind == COMPACTION_JOB_KIND_SUMMARIZER_V1 {
                        ended.insert(job_id.clone());
                    }
                }
                EventKind::ContinuityJobSpawned {
                    job_id, job_kind, ..
                } => {
                    if job_kind == COMPACTION_JOB_KIND_SUMMARIZER_V1 && !ended.contains(job_id) {
                        return Some(job_id.clone());
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub fn append_run_ended(
        &self,
        continuity_id: &str,
        message_id: &str,
        session_id: &str,
        reason: String,
        actor_id: String,
        origin: String,
    ) -> Result<String, String> {
        let mut next_seq = self.next_seq.lock().expect("continuity seq mutex");
        let seq = match next_seq.get(continuity_id).cloned() {
            Some(seq) => seq,
            None => {
                let seq = self
                    .load_next_seq_for(continuity_id)
                    .map_err(|err| format!("resolve continuity seq: {err}"))?;
                next_seq.insert(continuity_id.to_string(), seq);
                seq
            }
        };

        let id = Uuid::new_v4().to_string();
        let event = Event {
            id: id.clone(),
            session_id: continuity_id.to_string(),
            timestamp_ms: now_ms(),
            seq,
            kind: EventKind::ContinuityRunEnded {
                run_session_id: session_id.to_string(),
                message_id: message_id.to_string(),
                reason,
                actor_id: Some(actor_id),
                origin: Some(origin),
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity run ended: {err}"))?;
        self.stream_cache.append_best_effort(&event);
        let _ = self.sender.send(event.clone());

        next_seq.insert(continuity_id.to_string(), seq + 1);
        Ok(id)
    }

    pub fn append_tool_side_effects(
        &self,
        run: &ContinuityRunLink,
        run_session_id: &str,
        effects: ToolSideEffects,
    ) -> Result<String, String> {
        let continuity_id = run.continuity_id.as_str();
        let mut next_seq = self.next_seq.lock().expect("continuity seq mutex");
        let seq = match next_seq.get(continuity_id).cloned() {
            Some(seq) => seq,
            None => {
                let seq = self
                    .load_next_seq_for(continuity_id)
                    .map_err(|err| format!("resolve continuity seq: {err}"))?;
                next_seq.insert(continuity_id.to_string(), seq);
                seq
            }
        };

        let id = Uuid::new_v4().to_string();
        let event = Event {
            id: id.clone(),
            session_id: continuity_id.to_string(),
            timestamp_ms: now_ms(),
            seq,
            kind: EventKind::ContinuityToolSideEffects {
                run_session_id: run_session_id.to_string(),
                tool_id: effects.tool_id,
                tool_name: effects.tool_name,
                affected_paths: effects.affected_paths,
                checkpoint_id: effects.checkpoint_id,
                actor_id: run.actor_id.clone(),
                origin: run.origin.clone(),
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity tool side effects: {err}"))?;
        self.stream_cache.append_best_effort(&event);
        let _ = self.sender.send(event.clone());

        next_seq.insert(continuity_id.to_string(), seq + 1);
        Ok(id)
    }

    fn load_next_seq_for(&self, continuity_id: &str) -> Result<u64, io::Error> {
        if let Ok(Some(last_seq)) = self.stream_cache.try_read_last_seq(continuity_id) {
            return Ok(last_seq.saturating_add(1));
        }

        let events = self.replay_events(continuity_id)?;
        let last = events.last().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "continuity stream does not exist")
        })?;
        Ok(last.seq.saturating_add(1))
    }

    fn find_latest_continuity_for_workspace(
        &self,
        workspace: &str,
    ) -> Result<Option<String>, io::Error> {
        let events = self.event_log.replay_validated()?;
        let mut best: Option<(u64, String)> = None;
        for event in events {
            let EventKind::ContinuityCreated { workspace: w, .. } = event.kind else {
                continue;
            };
            if w != workspace {
                continue;
            }
            let id = event.session_id;
            match best {
                Some((ts, _)) if ts >= event.timestamp_ms => {}
                _ => best = Some((event.timestamp_ms, id)),
            }
        }
        Ok(best.map(|(_, id)| id))
    }

    fn create_continuity(
        &self,
        workspace: String,
        continuity_id: Option<String>,
        title: Option<String>,
        set_as_default: bool,
    ) -> Result<String, String> {
        let continuity_id = continuity_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let timestamp_ms = now_ms();
        let created = Event {
            id: Uuid::new_v4().to_string(),
            session_id: continuity_id.clone(),
            timestamp_ms,
            seq: 0,
            kind: EventKind::ContinuityCreated {
                workspace: workspace.clone(),
                title: title.clone(),
            },
        };
        self.event_log
            .append(&created)
            .map_err(|err| format!("append continuity_created: {err}"))?;
        self.stream_cache.append_best_effort(&created);
        let _ = self.sender.send(created.clone());

        {
            let mut index = self.index.lock().expect("continuity index mutex");
            if set_as_default {
                index.workspaces.insert(workspace, continuity_id.clone());
            }
            index.continuities.insert(
                continuity_id.clone(),
                ContinuityMetaV1 {
                    created_at_ms: timestamp_ms,
                    title,
                    archived: false,
                },
            );
            save_index(&index_path(&self.data_dir), &index)
                .map_err(|err| format!("save continuity index: {err}"))?;
        }

        self.next_seq
            .lock()
            .expect("continuity seq mutex")
            .insert(continuity_id.clone(), 1);

        Ok(continuity_id)
    }
}

fn parse_compaction_job_created_checkpoints(
    result: &Option<serde_json::Value>,
) -> Vec<CompactionAutoResultCheckpointV1> {
    let Some(value) = result.as_ref() else {
        return Vec::new();
    };
    let Some(created) = value.get("created").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for item in created {
        let Some(checkpoint_id) = item.get("checkpoint_id").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(summary_artifact_id) = item.get("summary_artifact_id").and_then(|v| v.as_str())
        else {
            continue;
        };
        let Some(to_seq) = item.get("to_seq").and_then(|v| v.as_u64()) else {
            continue;
        };
        let Some(to_message_id) = item.get("to_message_id").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(cut_rule_id) = item.get("cut_rule_id").and_then(|v| v.as_str()) else {
            continue;
        };

        out.push(CompactionAutoResultCheckpointV1 {
            checkpoint_id: checkpoint_id.to_string(),
            summary_artifact_id: summary_artifact_id.to_string(),
            to_seq,
            to_message_id: to_message_id.to_string(),
            cut_rule_id: cut_rule_id.to_string(),
        });
    }
    out
}

fn resolve_cutpoint_from_tail(
    message_events: &[(u64, String)],
    head_seq: u64,
    anchor_message_id: &str,
) -> Option<(u64, u64)> {
    let anchor_idx = message_events
        .iter()
        .position(|(_, id)| id == anchor_message_id)?;
    let message_seq = message_events.get(anchor_idx).map(|(seq, _)| *seq)?;
    let next_message_seq = message_events.get(anchor_idx + 1).map(|(seq, _)| *seq);
    let from_seq = match next_message_seq {
        Some(next_seq) => next_seq.saturating_sub(1),
        None => head_seq,
    };
    Some((message_seq, from_seq))
}

fn resolve_context_compile_cutpoint_full(
    continuity_events: &[Event],
    message_id: &str,
) -> Result<(u64, Option<String>), String> {
    let head_seq = continuity_events
        .last()
        .map(|event| event.seq)
        .unwrap_or_default();

    let mut message_seq: Option<u64> = None;
    let mut next_message_seq: Option<u64> = None;

    for event in continuity_events {
        if !matches!(event.kind, EventKind::ContinuityMessageAppended { .. }) {
            continue;
        }

        if message_seq.is_none() {
            if event.id == message_id {
                message_seq = Some(event.seq);
            }
            continue;
        }

        // First message after the anchor.
        next_message_seq = Some(event.seq);
        break;
    }

    let Some(message_seq) = message_seq else {
        return Err(format!("continuity message not found: {message_id}"));
    };

    let from_seq = match next_message_seq {
        Some(next_seq) => next_seq.saturating_sub(1),
        None => head_seq,
    };

    // Invariants: always include the anchor message itself.
    Ok((from_seq.max(message_seq), Some(message_id.to_string())))
}

fn index_path(data_dir: &Path) -> PathBuf {
    data_dir.join("continuities").join("index.json")
}

fn load_index(path: &Path) -> io::Result<ContinuityIndexV1> {
    let bytes = fs::read(path)?;
    let parsed: ContinuityIndexV1 = serde_json::from_slice(&bytes)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    if parsed.version != INDEX_VERSION {
        return Ok(ContinuityIndexV1::default());
    }
    Ok(parsed)
}

fn save_index(path: &Path, index: &ContinuityIndexV1) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_string_pretty(index)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, payload)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn workspace_key(workspace_root: &Path) -> String {
    workspace_root.to_string_lossy().to_string()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_compiler::{
        compile_recent_messages_v1, compile_summaries_recent_messages_v1,
        CompileRecentMessagesV1Request, CompileSummariesRecentMessagesV1Request,
    };
    use rip_kernel::StreamKind;
    use rip_log::write_snapshot;
    use tempfile::tempdir;

    fn store_for(dir: &tempfile::TempDir) -> (Arc<EventLog>, ContinuityStore, PathBuf) {
        let data_dir = dir.path().join("data");
        let workspace_root = dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace");
        let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
        let store = ContinuityStore::new(data_dir.clone(), workspace_root, event_log.clone())
            .expect("store");
        (event_log, store, data_dir)
    }

    #[test]
    fn ensure_default_creates_and_is_idempotent() {
        let dir = tempdir().expect("tmp");
        let (event_log, store, data_dir) = store_for(&dir);

        let first = store.ensure_default().expect("ensure");
        let second = store.ensure_default().expect("ensure");
        assert_eq!(first, second);

        let index = fs::read_to_string(index_path(&data_dir)).expect("index file");
        assert!(index.contains(&first));

        let events = event_log
            .replay_stream(StreamKind::Continuity, &first)
            .expect("replay");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].seq, 0);
        match &events[0].kind {
            EventKind::ContinuityCreated { workspace, .. } => {
                assert!(!workspace.is_empty());
            }
            other => panic!("expected continuity_created, got {other:?}"),
        }
    }

    #[test]
    fn provider_cursor_status_survives_sidecar_rotation() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, data_dir) = store_for(&dir);

        let thread_id = store.ensure_default().expect("ensure");
        store
            .append_provider_cursor_updated(
                &thread_id,
                ProviderCursorUpdatedPayload {
                    provider: "openresponses".to_string(),
                    endpoint: Some("http://example.test/v1/responses".to_string()),
                    model: Some("fixture-model".to_string()),
                    cursor: Some(serde_json::json!({
                        "previous_response_id": "resp_1"
                    })),
                    action: "set".to_string(),
                    reason: Some("test".to_string()),
                    run_session_id: Some("session-1".to_string()),
                    actor_id: "user".to_string(),
                    origin: "test".to_string(),
                },
            )
            .expect("append cursor");

        let first = store
            .provider_cursor_status_v1(&thread_id, ProviderCursorStatusV1Request {})
            .expect("status");
        assert_eq!(first.thread_id, thread_id);
        let active = first.active.expect("active");
        assert_eq!(active.action, "set");
        assert_eq!(
            active
                .cursor
                .as_ref()
                .and_then(|value| value.get("previous_response_id"))
                .and_then(|value| value.as_str()),
            Some("resp_1")
        );

        // Simulate cache loss / rotation: delete the continuity sidecar caches.
        let _ = std::fs::remove_dir_all(data_dir.join("continuity_streams"));

        let second = store
            .provider_cursor_status_v1(&thread_id, ProviderCursorStatusV1Request {})
            .expect("status after cache delete");
        let active = second.active.expect("active");
        assert_eq!(active.action, "set");
        assert_eq!(
            active
                .cursor
                .as_ref()
                .and_then(|value| value.get("previous_response_id"))
                .and_then(|value| value.as_str()),
            Some("resp_1")
        );

        let rotated = store
            .provider_cursor_rotate_v1(
                &thread_id,
                ProviderCursorRotateV1Request {
                    provider: None,
                    endpoint: None,
                    model: None,
                    reason: Some("manual".to_string()),
                    actor_id: "user".to_string(),
                    origin: "test".to_string(),
                },
            )
            .expect("rotate");
        assert!(rotated.rotated);

        let status = store
            .provider_cursor_status_v1(&thread_id, ProviderCursorStatusV1Request {})
            .expect("status after rotate");
        let active = status.active.expect("active");
        assert_eq!(active.action, "rotated");
        assert!(active.cursor.is_none());
    }

    #[test]
    fn context_selection_status_survives_sidecar_rotation_and_orders_latest_first() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, data_dir) = store_for(&dir);

        let thread_id = store.ensure_default().expect("ensure");

        let m1 = store
            .append_message(
                &thread_id,
                "alice".to_string(),
                "cli".to_string(),
                "m1".to_string(),
            )
            .expect("append message");
        store
            .append_run_spawned(
                &thread_id,
                &m1,
                "session-1",
                "alice".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");
        store
            .append_context_selection_decided(
                &thread_id,
                ContextSelectionDecidedPayload {
                    run_session_id: "session-1".to_string(),
                    message_id: m1.clone(),
                    compiler_id: "rip.context_compiler.v1".to_string(),
                    compiler_strategy: "recent_messages_v1".to_string(),
                    limits: serde_json::json!({ "recent_messages_v1_limit": 16 }),
                    compaction_checkpoint: None,
                    resets: Vec::new(),
                    reason: Some(serde_json::json!({
                        "selected": "recent_messages_v1",
                        "cause": "test",
                    })),
                    actor_id: "alice".to_string(),
                    origin: "cli".to_string(),
                },
            )
            .expect("selection decided");

        let m2 = store
            .append_message(
                &thread_id,
                "alice".to_string(),
                "cli".to_string(),
                "m2".to_string(),
            )
            .expect("append message");
        store
            .append_run_spawned(
                &thread_id,
                &m2,
                "session-2",
                "alice".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");
        store
            .append_context_selection_decided(
                &thread_id,
                ContextSelectionDecidedPayload {
                    run_session_id: "session-2".to_string(),
                    message_id: m2.clone(),
                    compiler_id: "rip.context_compiler.v1".to_string(),
                    compiler_strategy: "summaries_recent_messages_v1".to_string(),
                    limits: serde_json::json!({ "recent_messages_v1_limit": 16 }),
                    compaction_checkpoint: Some(
                        rip_kernel::ContextSelectionCompactionCheckpointV1 {
                            checkpoint_id: "ckpt-1".to_string(),
                            summary_kind: "cumulative_v1".to_string(),
                            summary_artifact_id: "artifact-1".to_string(),
                            to_seq: 1,
                        },
                    ),
                    resets: Vec::new(),
                    reason: Some(serde_json::json!({
                        "selected": "summaries_recent_messages_v1",
                        "cause": "compaction_checkpoint",
                    })),
                    actor_id: "alice".to_string(),
                    origin: "cli".to_string(),
                },
            )
            .expect("selection decided");

        let status = store
            .context_selection_status_v1(
                &thread_id,
                ContextSelectionStatusV1Request { limit: Some(2) },
            )
            .expect("status");
        assert_eq!(status.thread_id, thread_id);
        assert_eq!(status.decisions.len(), 2);
        assert_eq!(status.decisions[0].run_session_id, "session-2");
        assert_eq!(status.decisions[0].message_id, m2);
        assert_eq!(
            status.decisions[0].compiler_strategy,
            "summaries_recent_messages_v1"
        );
        assert!(status.decisions[0].compaction_checkpoint.is_some());
        assert_eq!(status.decisions[1].run_session_id, "session-1");
        assert_eq!(status.decisions[1].message_id, m1);
        assert_eq!(status.decisions[1].compiler_strategy, "recent_messages_v1");

        // Simulate cache loss / rotation: delete the continuity sidecar caches.
        let _ = std::fs::remove_dir_all(data_dir.join("continuity_streams"));

        let status = store
            .context_selection_status_v1(
                &thread_id,
                ContextSelectionStatusV1Request { limit: Some(1) },
            )
            .expect("status after cache delete");
        assert_eq!(status.decisions.len(), 1);
        assert_eq!(status.decisions[0].run_session_id, "session-2");
    }

    #[test]
    fn continuity_sidecar_contains_appended_frames_and_is_preferred_for_replay() {
        use std::io::Write;

        let dir = tempdir().expect("tmp");
        let (_event_log, store, data_dir) = store_for(&dir);

        let continuity_id = store.ensure_default().expect("ensure");
        let message_id = store
            .append_message(
                &continuity_id,
                "alice".to_string(),
                "cli".to_string(),
                "hello".to_string(),
            )
            .expect("append message");
        store
            .append_run_spawned(
                &continuity_id,
                &message_id,
                "session-1",
                "alice".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");
        store
            .append_context_selection_decided(
                &continuity_id,
                ContextSelectionDecidedPayload {
                    run_session_id: "session-1".to_string(),
                    message_id: message_id.clone(),
                    compiler_id: "rip.context_compiler.v1".to_string(),
                    compiler_strategy: "recent_messages_v1".to_string(),
                    limits: serde_json::json!({ "recent_messages_v1_limit": 16 }),
                    compaction_checkpoint: None,
                    resets: Vec::new(),
                    reason: Some(serde_json::json!({
                        "selected": "recent_messages_v1",
                        "cause": "test",
                    })),
                    actor_id: "alice".to_string(),
                    origin: "cli".to_string(),
                },
            )
            .expect("context selection decided");
        store
            .append_context_compiled(
                &continuity_id,
                ContextCompiledPayload {
                    run_session_id: "session-1".to_string(),
                    bundle_artifact_id: "artifact-1".to_string(),
                    compiler_id: "rip.context_compiler.v1".to_string(),
                    compiler_strategy: "recent_messages_v1".to_string(),
                    from_seq: 1,
                    from_message_id: Some(message_id.clone()),
                    actor_id: "alice".to_string(),
                    origin: "cli".to_string(),
                },
            )
            .expect("context compiled");
        store
            .append_run_ended(
                &continuity_id,
                &message_id,
                "session-1",
                "completed".to_string(),
                "alice".to_string(),
                "cli".to_string(),
            )
            .expect("run ended");

        let sidecar_path = data_dir
            .join("continuity_streams")
            .join(format!("{continuity_id}.jsonl"));
        assert!(sidecar_path.exists(), "expected continuity sidecar file");
        let sidecar = fs::read_to_string(&sidecar_path).expect("read sidecar");
        assert!(
            sidecar.contains("continuity_context_compiled"),
            "expected continuity_context_compiled in sidecar"
        );
        assert!(
            sidecar.contains("continuity_context_selection_decided"),
            "expected continuity_context_selection_decided in sidecar"
        );

        // Corrupt the global log so a replay_stream() scan would fail if used.
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(data_dir.join("events.jsonl"))
            .expect("open global log");
        writeln!(file, "not json").expect("write corrupt line");

        let events = store.replay_events(&continuity_id).expect("replay");
        assert!(
            events
                .iter()
                .any(|event| matches!(event.kind, EventKind::ContinuityContextCompiled { .. })),
            "expected continuity_context_compiled in replay"
        );
        assert!(
            events.iter().any(|event| matches!(
                event.kind,
                EventKind::ContinuityContextSelectionDecided { .. }
            )),
            "expected continuity_context_selection_decided in replay"
        );
    }

    #[test]
    fn ensure_default_recovers_from_missing_index() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, data_dir) = store_for(&dir);

        let first = store.ensure_default().expect("ensure");
        fs::remove_file(index_path(&data_dir)).expect("remove index");

        let (_event_log2, store2, _data_dir2) = store_for(&dir);
        let second = store2.ensure_default().expect("ensure");
        assert_eq!(first, second);
    }

    #[test]
    fn append_message_increments_seq() {
        let dir = tempdir().expect("tmp");
        let (event_log, store, _data_dir) = store_for(&dir);

        let continuity_id = store.ensure_default().expect("ensure");
        let m1 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "hello".to_string(),
            )
            .expect("append");
        let m2 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "world".to_string(),
            )
            .expect("append");
        assert_ne!(m1, m2);

        let events = event_log
            .replay_stream(StreamKind::Continuity, &continuity_id)
            .expect("replay");
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].seq, 0);
        assert_eq!(events[1].seq, 1);
        assert_eq!(events[2].seq, 2);
        match &events[2].kind {
            EventKind::ContinuityMessageAppended { content, .. } => assert_eq!(content, "world"),
            other => panic!("expected message, got {other:?}"),
        }
    }

    #[test]
    fn append_run_spawned_advances_seq() {
        let dir = tempdir().expect("tmp");
        let (event_log, store, _data_dir) = store_for(&dir);

        let continuity_id = store.ensure_default().expect("ensure");
        let message_id = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "hello".to_string(),
            )
            .expect("append");
        store
            .append_run_spawned(
                &continuity_id,
                &message_id,
                "session-1",
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");

        let events = event_log
            .replay_stream(StreamKind::Continuity, &continuity_id)
            .expect("replay");
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].seq, 0);
        assert_eq!(events[1].seq, 1);
        assert_eq!(events[2].seq, 2);
        match &events[2].kind {
            EventKind::ContinuityRunSpawned {
                run_session_id,
                actor_id,
                origin,
                ..
            } => {
                assert_eq!(run_session_id, "session-1");
                assert_eq!(actor_id.as_deref(), Some("user"));
                assert_eq!(origin.as_deref(), Some("cli"));
            }
            other => panic!("expected run_spawned, got {other:?}"),
        }
    }

    #[test]
    fn append_run_ended_advances_seq() {
        let dir = tempdir().expect("tmp");
        let (event_log, store, _data_dir) = store_for(&dir);

        let continuity_id = store.ensure_default().expect("ensure");
        let message_id = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "hello".to_string(),
            )
            .expect("append");
        store
            .append_run_spawned(
                &continuity_id,
                &message_id,
                "session-1",
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");
        store
            .append_run_ended(
                &continuity_id,
                &message_id,
                "session-1",
                "completed".to_string(),
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run ended");

        let events = event_log
            .replay_stream(StreamKind::Continuity, &continuity_id)
            .expect("replay");
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].seq, 0);
        assert_eq!(events[1].seq, 1);
        assert_eq!(events[2].seq, 2);
        assert_eq!(events[3].seq, 3);
        match &events[3].kind {
            EventKind::ContinuityRunEnded {
                run_session_id,
                message_id: mid,
                reason,
                actor_id,
                origin,
            } => {
                assert_eq!(run_session_id, "session-1");
                assert_eq!(mid, &message_id);
                assert_eq!(reason, "completed");
                assert_eq!(actor_id.as_deref(), Some("user"));
                assert_eq!(origin.as_deref(), Some("cli"));
            }
            other => panic!("expected run_ended, got {other:?}"),
        }
    }

    #[test]
    fn append_message_recovers_seq_from_sidecar_when_next_seq_cache_missing() {
        use std::io::Write;

        let dir = tempdir().expect("tmp");
        let (_event_log, store, data_dir) = store_for(&dir);

        let continuity_id = store.ensure_default().expect("ensure");
        store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "first".to_string(),
            )
            .expect("append");

        // Simulate a fresh process (in-memory seq cache cleared) with a corrupted global log.
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(data_dir.join("events.jsonl"))
            .expect("open global log");
        writeln!(file, "not json").expect("corrupt global log");

        let (_event_log2, store2, _data_dir2) = store_for(&dir);
        store2
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "second".to_string(),
            )
            .expect("append after restart");
    }

    #[test]
    fn compaction_checkpoint_cumulative_v1_writes_artifact_and_appends_frame() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, _data_dir) = store_for(&dir);

        let continuity_id = store.ensure_default().expect("ensure");
        let m1 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "hello".to_string(),
            )
            .expect("append");
        let _m2 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "world".to_string(),
            )
            .expect("append");

        let (checkpoint_id, summary_artifact_id, to_seq, to_message_id, cut_rule_id) = store
            .compaction_checkpoint_cumulative_v1(
                &continuity_id,
                CompactionCheckpointCumulativeV1Request {
                    summary_markdown: Some("summary".to_string()),
                    summary_artifact_id: None,
                    to_message_id: Some(m1.clone()),
                    to_seq: None,
                    stride_messages: None,
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
            )
            .expect("checkpoint");

        assert_eq!(to_message_id, m1);
        assert_eq!(cut_rule_id, "manual_v1");

        let blob_path = store
            .workspace_root()
            .join(".rip")
            .join("artifacts")
            .join("blobs")
            .join(&summary_artifact_id);
        assert!(blob_path.exists(), "summary artifact blob should exist");

        let events = store.replay_events(&continuity_id).expect("replay");
        let checkpoint_event = events
            .iter()
            .find(|event| event.id == checkpoint_id)
            .expect("checkpoint event");
        match &checkpoint_event.kind {
            EventKind::ContinuityCompactionCheckpointCreated {
                checkpoint_id: cid,
                cut_rule_id: rule,
                summary_kind,
                summary_artifact_id: aid,
                from_seq,
                to_seq: t_seq,
                to_message_id: t_mid,
                actor_id,
                origin,
                ..
            } => {
                assert_eq!(cid, &checkpoint_id);
                assert_eq!(rule, "manual_v1");
                assert_eq!(summary_kind, COMPACTION_SUMMARY_KIND_CUMULATIVE_V1);
                assert_eq!(aid, &summary_artifact_id);
                assert_eq!(*from_seq, 0);
                assert_eq!(*t_seq, to_seq);
                assert_eq!(t_mid.as_deref(), Some(to_message_id.as_str()));
                assert_eq!(actor_id, "user");
                assert_eq!(origin, "cli");
            }
            other => panic!("expected compaction checkpoint frame, got {other:?}"),
        }

        let latest = store
            .latest_compaction_checkpoint_for_compile_v1(&continuity_id, checkpoint_event.seq)
            .expect("lookup")
            .expect("latest");
        assert_eq!(latest.summary_artifact_id, summary_artifact_id);
        assert_eq!(latest.to_seq, to_seq);
    }

    #[test]
    fn latest_compaction_checkpoint_for_compile_tie_breaks_by_stream_order() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, _data_dir) = store_for(&dir);

        let continuity_id = store.ensure_default().expect("ensure");
        let m1 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "hello".to_string(),
            )
            .expect("append");
        let _m2 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "world".to_string(),
            )
            .expect("append");

        let (_checkpoint1_id, summary1_artifact_id, to_seq1, _to_mid1, _cut_rule_id1) = store
            .compaction_checkpoint_cumulative_v1(
                &continuity_id,
                CompactionCheckpointCumulativeV1Request {
                    summary_markdown: Some("summary-1".to_string()),
                    summary_artifact_id: None,
                    to_message_id: Some(m1.clone()),
                    to_seq: None,
                    stride_messages: None,
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
            )
            .expect("checkpoint1");
        let (_checkpoint2_id, summary2_artifact_id, to_seq2, _to_mid2, _cut_rule_id2) = store
            .compaction_checkpoint_cumulative_v1(
                &continuity_id,
                CompactionCheckpointCumulativeV1Request {
                    summary_markdown: Some("summary-2".to_string()),
                    summary_artifact_id: None,
                    to_message_id: Some(m1.clone()),
                    to_seq: None,
                    stride_messages: None,
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
            )
            .expect("checkpoint2");

        assert_eq!(to_seq1, to_seq2);
        assert_ne!(summary1_artifact_id, summary2_artifact_id);

        let events = store.replay_events(&continuity_id).expect("replay");
        let from_seq = events.last().map(|event| event.seq).unwrap_or_default();

        let latest = store
            .latest_compaction_checkpoint_for_compile_v1(&continuity_id, from_seq)
            .expect("lookup")
            .expect("some");
        assert_eq!(latest.to_seq, to_seq1);
        assert_eq!(latest.summary_artifact_id, summary2_artifact_id);
    }

    #[test]
    fn compaction_cut_points_v1_falls_back_when_ordinal_index_missing() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, data_dir) = store_for(&dir);

        let continuity_id = store.ensure_default().expect("ensure");
        let _m1 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "m1".to_string(),
            )
            .expect("append");
        let m2 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "m2".to_string(),
            )
            .expect("append");
        let _m3 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "m3".to_string(),
            )
            .expect("append");
        let m4 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "m4".to_string(),
            )
            .expect("append");

        let req = CompactionCutPointsV1Request {
            stride_messages: Some(2),
            limit: Some(2),
        };
        let first = store
            .compaction_cut_points_v1(&continuity_id, req.clone())
            .expect("cut points");
        assert_eq!(first.message_count, 4);
        assert_eq!(first.cut_points.len(), 2);
        assert_eq!(first.cut_points[0].target_message_ordinal, 4);
        assert_eq!(first.cut_points[0].to_message_id, m4);
        assert_eq!(first.cut_points[1].target_message_ordinal, 2);
        assert_eq!(first.cut_points[1].to_message_id, m2);

        let ord_path = data_dir
            .join("continuity_streams")
            .join(format!("{continuity_id}.mr.msgord.v1.bin"));
        assert!(ord_path.exists(), "expected ordinal index to exist");
        fs::remove_file(&ord_path).expect("remove ordinal index");

        let second = store
            .compaction_cut_points_v1(&continuity_id, req)
            .expect("cut points after deleting ordinal index");
        assert_eq!(second.message_count, 4);
        assert_eq!(second.cut_points.len(), 2);
        assert_eq!(second.cut_points[0].target_message_ordinal, 4);
        assert_eq!(second.cut_points[0].to_message_id, m4);
        assert_eq!(second.cut_points[1].target_message_ordinal, 2);
        assert_eq!(second.cut_points[1].to_message_id, m2);
    }

    #[test]
    fn compaction_auto_schedule_is_replay_safe_under_concurrent_calls() {
        let dir = tempdir().expect("tmp");
        let (event_log, store, _data_dir) = store_for(&dir);
        let store = Arc::new(store);

        let continuity_id = store.ensure_default().expect("ensure");
        let _m1 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "m1".to_string(),
            )
            .expect("append");
        let _m2 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "m2".to_string(),
            )
            .expect("append");

        std::thread::scope(|scope| {
            for _ in 0..4 {
                let store = store.clone();
                let continuity_id = continuity_id.clone();
                scope.spawn(move || {
                    let _ = store.compaction_auto_schedule_spawn_job_v1(
                        &continuity_id,
                        CompactionAutoScheduleV1Request {
                            stride_messages: Some(2),
                            max_new_checkpoints: Some(1),
                            block_on_inflight: Some(true),
                            execute: Some(false),
                            dry_run: Some(false),
                            actor_id: "alice".to_string(),
                            origin: "test".to_string(),
                        },
                    );
                });
            }
        });

        let events = event_log
            .replay_stream(StreamKind::Continuity, &continuity_id)
            .expect("replay");
        assert!(!events.is_empty());
        for (idx, event) in events.iter().enumerate() {
            assert_eq!(event.seq, idx as u64, "expected contiguous seq values");
        }
        assert!(
            events.iter().any(|event| matches!(
                event.kind,
                EventKind::ContinuityCompactionAutoScheduleDecided { .. }
            )),
            "expected at least one continuity_compaction_auto_schedule_decided"
        );
    }

    #[test]
    fn compaction_status_v1_reports_next_cut_point_and_latest_checkpoint() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, _data_dir) = store_for(&dir);

        let continuity_id = store.ensure_default().expect("ensure");
        let _m1 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "m1".to_string(),
            )
            .expect("append");
        let m2 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "m2".to_string(),
            )
            .expect("append");

        let first = store
            .compaction_status_v1(
                &continuity_id,
                CompactionStatusV1Request {
                    stride_messages: Some(1),
                },
            )
            .expect("status");
        assert_eq!(first.thread_id, continuity_id);
        assert_eq!(first.message_count, 2);
        assert!(first.latest_checkpoint.is_none());
        assert_eq!(
            first
                .next_cut_point
                .as_ref()
                .map(|cp| cp.to_message_id.as_str()),
            Some(m2.as_str())
        );

        store
            .compaction_checkpoint_cumulative_v1(
                &continuity_id,
                CompactionCheckpointCumulativeV1Request {
                    summary_markdown: Some("summary".to_string()),
                    summary_artifact_id: None,
                    to_message_id: Some(m2.clone()),
                    to_seq: None,
                    stride_messages: None,
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
            )
            .expect("checkpoint");

        let second = store
            .compaction_status_v1(
                &continuity_id,
                CompactionStatusV1Request {
                    stride_messages: Some(1),
                },
            )
            .expect("status after checkpoint");
        assert!(second.latest_checkpoint.is_some());
        assert_eq!(
            second
                .latest_checkpoint
                .as_ref()
                .and_then(|c| c.to_message_id.as_deref()),
            Some(m2.as_str())
        );
    }

    #[test]
    fn compaction_auto_summary_bootstraps_from_legacy_placeholder_base() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, _data_dir) = store_for(&dir);

        let continuity_id = store.ensure_default().expect("ensure");
        let _m1 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "m1".to_string(),
            )
            .expect("append");
        let m2 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "m2".to_string(),
            )
            .expect("append");
        let _m3 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "m3".to_string(),
            )
            .expect("append");
        let m4 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "m4".to_string(),
            )
            .expect("append");

        // Seed a legacy metadata-only summary as the base checkpoint for ordinal=2.
        let legacy_markdown = format!(
            "# Compaction summary (auto)\n\n- kind: {kind}\n- cut_rule_id: stride_messages_v1/2\n- stride_messages: 2\n- target_message_ordinal: 2\n- to_seq: 2\n- to_message_id: {m2}\n",
            kind = COMPACTION_SUMMARY_KIND_CUMULATIVE_V1
        );
        assert!(
            crate::compaction_auto_summary::summary_markdown_is_legacy_metadata_placeholder(
                &legacy_markdown
            ),
            "expected legacy placeholder detector to match"
        );
        store
            .compaction_checkpoint_cumulative_v1(
                &continuity_id,
                CompactionCheckpointCumulativeV1Request {
                    summary_markdown: Some(legacy_markdown),
                    summary_artifact_id: None,
                    to_message_id: Some(m2.clone()),
                    to_seq: None,
                    stride_messages: None,
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
            )
            .expect("seed legacy checkpoint");

        let resp = store
            .compaction_auto_v1(
                &continuity_id,
                CompactionAutoV1Request {
                    stride_messages: Some(2),
                    max_new_checkpoints: Some(1),
                    dry_run: Some(false),
                    actor_id: "alice".to_string(),
                    origin: "test".to_string(),
                },
            )
            .expect("compaction auto");
        assert_eq!(resp.status, "completed");
        assert_eq!(resp.result.len(), 1);
        let artifact_id = resp.result[0].summary_artifact_id.clone();

        let summary = crate::compaction_summary::read_compaction_summary_v1(
            store.workspace_root(),
            &artifact_id,
        )
        .expect("read summary artifact");
        let markdown = summary.summary_markdown();
        assert!(
            markdown.contains("## Cumulative Summary"),
            "expected v0.2 cumulative section"
        );
        assert!(
            markdown.contains("## Recent Delta Highlights"),
            "expected v0.2 highlights section"
        );
        assert!(
            markdown.contains("m4") || markdown.contains(m4.as_str()),
            "expected summary to include message content"
        );
        assert!(
            !crate::compaction_auto_summary::summary_markdown_is_legacy_metadata_placeholder(
                markdown
            ),
            "expected upgraded summary to not be legacy placeholder"
        );
        assert!(
            markdown.chars().count() <= crate::compaction_auto_summary::MAX_SUMMARY_MARKDOWN_CHARS,
            "expected summary_markdown to be bounded"
        );
    }

    #[test]
    fn tail_context_compile_input_matches_full_replay_for_recent_messages_v1() {
        let dir = tempdir().expect("tmp");
        let (event_log, store, _data_dir) = store_for(&dir);
        let snapshot_dir = dir.path().join("snapshots");

        let continuity_id = store.ensure_default().expect("ensure");
        let mut messages: Vec<(String, String)> = Vec::new();

        for idx in 0..20 {
            let message_id = store
                .append_message(
                    &continuity_id,
                    "user".to_string(),
                    "cli".to_string(),
                    format!("m{idx}:{}", "x".repeat(20_000)),
                )
                .expect("append message");
            let session_id = format!("session-{idx}");

            // Minimal session output for the compiler to pick up assistant replies.
            let session_events = vec![
                Event {
                    id: format!("se{idx}-0"),
                    session_id: session_id.clone(),
                    timestamp_ms: 0,
                    seq: 0,
                    kind: EventKind::SessionStarted {
                        input: "hi".to_string(),
                    },
                },
                Event {
                    id: format!("se{idx}-1"),
                    session_id: session_id.clone(),
                    timestamp_ms: 1,
                    seq: 1,
                    kind: EventKind::OutputTextDelta {
                        delta: format!("a{idx}"),
                    },
                },
                Event {
                    id: format!("se{idx}-2"),
                    session_id: session_id.clone(),
                    timestamp_ms: 2,
                    seq: 2,
                    kind: EventKind::SessionEnded {
                        reason: "completed".to_string(),
                    },
                },
            ];
            write_snapshot(&snapshot_dir, &session_id, &session_events).expect("snapshot");

            store
                .append_run_spawned(
                    &continuity_id,
                    &message_id,
                    &session_id,
                    "user".to_string(),
                    "cli".to_string(),
                )
                .expect("run spawned");
            store
                .append_context_compiled(
                    &continuity_id,
                    ContextCompiledPayload {
                        run_session_id: session_id.clone(),
                        bundle_artifact_id: "artifact-1".to_string(),
                        compiler_id: "rip.context_compiler.v1".to_string(),
                        compiler_strategy: "recent_messages_v1".to_string(),
                        from_seq: 0,
                        from_message_id: Some(message_id.clone()),
                        actor_id: "user".to_string(),
                        origin: "cli".to_string(),
                    },
                )
                .expect("context compiled");
            store
                .append_run_ended(
                    &continuity_id,
                    &message_id,
                    &session_id,
                    "completed".to_string(),
                    "user".to_string(),
                    "cli".to_string(),
                )
                .expect("run ended");

            messages.push((message_id, session_id));
        }

        let anchor_message_id = messages
            .last()
            .map(|(mid, _)| mid.clone())
            .expect("messages");

        let full_events = store.replay_events(&continuity_id).expect("replay full");
        let (full_from_seq, full_from_message_id) =
            resolve_context_compile_cutpoint_full(&full_events, &anchor_message_id)
                .expect("cutpoint");

        let full_bundle = compile_recent_messages_v1(CompileRecentMessagesV1Request {
            continuity_id: &continuity_id,
            continuity_events: &full_events,
            event_log: event_log.as_ref(),
            snapshot_dir: &snapshot_dir,
            from_seq: full_from_seq,
            from_message_id: full_from_message_id.clone(),
            run_session_id: "run-session",
            actor_id: "user",
            origin: "cli",
        })
        .expect("compile full");

        let tail_input = store
            .load_context_compile_input_recent_messages_v1(&continuity_id, &anchor_message_id)
            .expect("tail input");
        assert_eq!(tail_input.from_seq, full_from_seq);
        assert_eq!(tail_input.from_message_id, full_from_message_id);

        let tail_bundle = compile_recent_messages_v1(CompileRecentMessagesV1Request {
            continuity_id: &continuity_id,
            continuity_events: &tail_input.continuity_events,
            event_log: event_log.as_ref(),
            snapshot_dir: &snapshot_dir,
            from_seq: tail_input.from_seq,
            from_message_id: tail_input.from_message_id.clone(),
            run_session_id: "run-session",
            actor_id: "user",
            origin: "cli",
        })
        .expect("compile tail");

        let full_json = serde_json::to_value(&full_bundle).expect("full json");
        let tail_json = serde_json::to_value(&tail_bundle).expect("tail json");
        assert_eq!(tail_json, full_json);
    }

    #[test]
    fn window_context_compile_input_matches_full_replay_for_recent_messages_v1_non_tail_anchor() {
        let dir = tempdir().expect("tmp");
        let (event_log, store, _data_dir) = store_for(&dir);
        let snapshot_dir = dir.path().join("snapshots");

        const MSG_LEN: usize = 60_000;
        const MSG_COUNT: usize = 200;

        let continuity_id = store.ensure_default().expect("ensure");
        let mut message_ids: Vec<String> = Vec::new();

        for idx in 0..MSG_COUNT {
            let message_id = store
                .append_message(
                    &continuity_id,
                    "user".to_string(),
                    "cli".to_string(),
                    format!("m{idx}:{}", "x".repeat(MSG_LEN)),
                )
                .expect("append message");
            let session_id = format!("session-{idx}");

            let session_events = vec![
                Event {
                    id: format!("se{idx}-0"),
                    session_id: session_id.clone(),
                    timestamp_ms: 0,
                    seq: 0,
                    kind: EventKind::SessionStarted {
                        input: "hi".to_string(),
                    },
                },
                Event {
                    id: format!("se{idx}-1"),
                    session_id: session_id.clone(),
                    timestamp_ms: 1,
                    seq: 1,
                    kind: EventKind::OutputTextDelta {
                        delta: format!("a{idx}"),
                    },
                },
                Event {
                    id: format!("se{idx}-2"),
                    session_id: session_id.clone(),
                    timestamp_ms: 2,
                    seq: 2,
                    kind: EventKind::SessionEnded {
                        reason: "completed".to_string(),
                    },
                },
            ];
            write_snapshot(&snapshot_dir, &session_id, &session_events).expect("snapshot");

            store
                .append_run_spawned(
                    &continuity_id,
                    &message_id,
                    &session_id,
                    "user".to_string(),
                    "cli".to_string(),
                )
                .expect("run spawned");
            store
                .append_context_compiled(
                    &continuity_id,
                    ContextCompiledPayload {
                        run_session_id: session_id.clone(),
                        bundle_artifact_id: "artifact-1".to_string(),
                        compiler_id: "rip.context_compiler.v1".to_string(),
                        compiler_strategy: "recent_messages_v1".to_string(),
                        from_seq: 0,
                        from_message_id: Some(message_id.clone()),
                        actor_id: "user".to_string(),
                        origin: "cli".to_string(),
                    },
                )
                .expect("context compiled");
            store
                .append_run_ended(
                    &continuity_id,
                    &message_id,
                    &session_id,
                    "completed".to_string(),
                    "user".to_string(),
                    "cli".to_string(),
                )
                .expect("run ended");

            message_ids.push(message_id);
        }

        let anchor_message_id = message_ids.get(40).cloned().expect("anchor message id");

        let full_events = store.replay_events(&continuity_id).expect("replay full");
        let (full_from_seq, full_from_message_id) =
            resolve_context_compile_cutpoint_full(&full_events, &anchor_message_id)
                .expect("cutpoint");

        let full_bundle = compile_recent_messages_v1(CompileRecentMessagesV1Request {
            continuity_id: &continuity_id,
            continuity_events: &full_events,
            event_log: event_log.as_ref(),
            snapshot_dir: &snapshot_dir,
            from_seq: full_from_seq,
            from_message_id: full_from_message_id.clone(),
            run_session_id: "run-session",
            actor_id: "user",
            origin: "cli",
        })
        .expect("compile full");

        let window_input = store
            .load_context_compile_input_recent_messages_v1(&continuity_id, &anchor_message_id)
            .expect("window input");
        assert_eq!(window_input.from_seq, full_from_seq);
        assert_eq!(window_input.from_message_id, full_from_message_id);
        assert!(
            window_input.continuity_events.len() <= 128,
            "expected bounded window, got {} events",
            window_input.continuity_events.len()
        );

        let window_bundle = compile_recent_messages_v1(CompileRecentMessagesV1Request {
            continuity_id: &continuity_id,
            continuity_events: &window_input.continuity_events,
            event_log: event_log.as_ref(),
            snapshot_dir: &snapshot_dir,
            from_seq: window_input.from_seq,
            from_message_id: window_input.from_message_id.clone(),
            run_session_id: "run-session",
            actor_id: "user",
            origin: "cli",
        })
        .expect("compile window");

        let full_json = serde_json::to_value(&full_bundle).expect("full json");
        let window_json = serde_json::to_value(&window_bundle).expect("window json");
        assert_eq!(window_json, full_json);
    }

    #[test]
    fn window_context_compile_input_matches_full_replay_with_dense_tool_side_effects() {
        let dir = tempdir().expect("tmp");
        let (event_log, store, _data_dir) = store_for(&dir);
        let snapshot_dir = dir.path().join("snapshots");

        const MSG_COUNT: usize = 60;
        const TOOL_EVENTS_PER_MESSAGE: usize = 250;

        let continuity_id = store.ensure_default().expect("ensure");
        let mut message_ids: Vec<String> = Vec::new();

        for idx in 0..MSG_COUNT {
            let message_id = store
                .append_message(
                    &continuity_id,
                    "user".to_string(),
                    "cli".to_string(),
                    format!("m{idx}"),
                )
                .expect("append message");
            let session_id = format!("session-{idx}");

            let session_events = vec![
                Event {
                    id: format!("se{idx}-0"),
                    session_id: session_id.clone(),
                    timestamp_ms: 0,
                    seq: 0,
                    kind: EventKind::SessionStarted {
                        input: "hi".to_string(),
                    },
                },
                Event {
                    id: format!("se{idx}-1"),
                    session_id: session_id.clone(),
                    timestamp_ms: 1,
                    seq: 1,
                    kind: EventKind::OutputTextDelta {
                        delta: format!("a{idx}"),
                    },
                },
                Event {
                    id: format!("se{idx}-2"),
                    session_id: session_id.clone(),
                    timestamp_ms: 2,
                    seq: 2,
                    kind: EventKind::SessionEnded {
                        reason: "completed".to_string(),
                    },
                },
            ];
            write_snapshot(&snapshot_dir, &session_id, &session_events).expect("snapshot");

            store
                .append_run_spawned(
                    &continuity_id,
                    &message_id,
                    &session_id,
                    "user".to_string(),
                    "cli".to_string(),
                )
                .expect("run spawned");

            for tool_idx in 0..TOOL_EVENTS_PER_MESSAGE {
                store
                    .append_tool_side_effects(
                        &ContinuityRunLink {
                            continuity_id: continuity_id.clone(),
                            message_id: message_id.clone(),
                            actor_id: "user".to_string(),
                            origin: "cli".to_string(),
                        },
                        &session_id,
                        ToolSideEffects {
                            tool_id: format!("tool-{idx}-{tool_idx}"),
                            tool_name: "write".to_string(),
                            affected_paths: Some(vec![format!("file-{tool_idx}.txt")]),
                            checkpoint_id: None,
                        },
                    )
                    .expect("tool side effects");
            }

            store
                .append_run_ended(
                    &continuity_id,
                    &message_id,
                    &session_id,
                    "completed".to_string(),
                    "user".to_string(),
                    "cli".to_string(),
                )
                .expect("run ended");

            message_ids.push(message_id);
        }

        let anchor_message_id = message_ids.get(20).cloned().expect("anchor message id");

        let full_events = store.replay_events(&continuity_id).expect("replay full");
        let (full_from_seq, full_from_message_id) =
            resolve_context_compile_cutpoint_full(&full_events, &anchor_message_id)
                .expect("cutpoint");

        let full_bundle = compile_recent_messages_v1(CompileRecentMessagesV1Request {
            continuity_id: &continuity_id,
            continuity_events: &full_events,
            event_log: event_log.as_ref(),
            snapshot_dir: &snapshot_dir,
            from_seq: full_from_seq,
            from_message_id: full_from_message_id.clone(),
            run_session_id: "run-session",
            actor_id: "user",
            origin: "cli",
        })
        .expect("compile full");

        let window_input = store
            .load_context_compile_input_recent_messages_v1(&continuity_id, &anchor_message_id)
            .expect("window input");
        assert_eq!(window_input.from_seq, full_from_seq);
        assert_eq!(window_input.from_message_id, full_from_message_id);
        assert!(
            window_input.continuity_events.iter().all(|event| matches!(
                event.kind,
                EventKind::ContinuityMessageAppended { .. } | EventKind::ContinuityRunEnded { .. }
            )),
            "expected message+run-ended-only window events"
        );

        let window_bundle = compile_recent_messages_v1(CompileRecentMessagesV1Request {
            continuity_id: &continuity_id,
            continuity_events: &window_input.continuity_events,
            event_log: event_log.as_ref(),
            snapshot_dir: &snapshot_dir,
            from_seq: window_input.from_seq,
            from_message_id: window_input.from_message_id.clone(),
            run_session_id: "run-session",
            actor_id: "user",
            origin: "cli",
        })
        .expect("compile window");

        let full_json = serde_json::to_value(&full_bundle).expect("full json");
        let window_json = serde_json::to_value(&window_bundle).expect("window json");
        assert_eq!(window_json, full_json);
    }

    #[test]
    fn window_context_compile_input_matches_full_replay_with_dense_tool_side_effects_and_compaction_summary(
    ) {
        let dir = tempdir().expect("tmp");
        let (event_log, store, _data_dir) = store_for(&dir);
        let snapshot_dir = dir.path().join("snapshots");

        const MSG_COUNT: usize = 60;
        const TOOL_EVENTS_PER_MESSAGE: usize = 250;

        let continuity_id = store.ensure_default().expect("ensure");
        let mut message_ids: Vec<String> = Vec::new();

        for idx in 0..MSG_COUNT {
            let message_id = store
                .append_message(
                    &continuity_id,
                    "user".to_string(),
                    "cli".to_string(),
                    format!("m{idx}"),
                )
                .expect("append message");
            let session_id = format!("session-{idx}");

            let session_events = vec![
                Event {
                    id: format!("se{idx}-0"),
                    session_id: session_id.clone(),
                    timestamp_ms: 0,
                    seq: 0,
                    kind: EventKind::SessionStarted {
                        input: "hi".to_string(),
                    },
                },
                Event {
                    id: format!("se{idx}-1"),
                    session_id: session_id.clone(),
                    timestamp_ms: 1,
                    seq: 1,
                    kind: EventKind::OutputTextDelta {
                        delta: format!("a{idx}"),
                    },
                },
                Event {
                    id: format!("se{idx}-2"),
                    session_id: session_id.clone(),
                    timestamp_ms: 2,
                    seq: 2,
                    kind: EventKind::SessionEnded {
                        reason: "completed".to_string(),
                    },
                },
            ];
            write_snapshot(&snapshot_dir, &session_id, &session_events).expect("snapshot");

            store
                .append_run_spawned(
                    &continuity_id,
                    &message_id,
                    &session_id,
                    "user".to_string(),
                    "cli".to_string(),
                )
                .expect("run spawned");

            for tool_idx in 0..TOOL_EVENTS_PER_MESSAGE {
                store
                    .append_tool_side_effects(
                        &ContinuityRunLink {
                            continuity_id: continuity_id.clone(),
                            message_id: message_id.clone(),
                            actor_id: "user".to_string(),
                            origin: "cli".to_string(),
                        },
                        &session_id,
                        ToolSideEffects {
                            tool_id: format!("tool-{idx}-{tool_idx}"),
                            tool_name: "write".to_string(),
                            affected_paths: Some(vec![format!("file-{tool_idx}.txt")]),
                            checkpoint_id: None,
                        },
                    )
                    .expect("tool side effects");
            }

            store
                .append_run_ended(
                    &continuity_id,
                    &message_id,
                    &session_id,
                    "completed".to_string(),
                    "user".to_string(),
                    "cli".to_string(),
                )
                .expect("run ended");

            message_ids.push(message_id);
        }

        // Create a compaction checkpoint at a message boundary well before the compile anchor.
        let cut_message_id = message_ids.get(10).cloned().expect("cut message id");
        store
            .compaction_checkpoint_cumulative_v1(
                &continuity_id,
                CompactionCheckpointCumulativeV1Request {
                    summary_markdown: Some("summary".to_string()),
                    summary_artifact_id: None,
                    to_message_id: Some(cut_message_id.clone()),
                    to_seq: None,
                    stride_messages: None,
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
            )
            .expect("compaction checkpoint");

        let anchor_message_id = message_ids.get(20).cloned().expect("anchor message id");

        let full_events = store.replay_events(&continuity_id).expect("replay full");
        let (full_from_seq, full_from_message_id) =
            resolve_context_compile_cutpoint_full(&full_events, &anchor_message_id)
                .expect("cutpoint");

        // Pick the latest eligible checkpoint using the full replay stream.
        let mut best: Option<(u64, u64, String)> = None; // (to_seq, event_seq, artifact_id)
        for event in &full_events {
            let EventKind::ContinuityCompactionCheckpointCreated {
                summary_kind,
                summary_artifact_id,
                to_seq,
                ..
            } = &event.kind
            else {
                continue;
            };
            if summary_kind != crate::compaction_summary::COMPACTION_SUMMARY_KIND_CUMULATIVE_V1 {
                continue;
            }
            if *to_seq > full_from_seq {
                continue;
            }
            match best.as_ref() {
                Some((best_to_seq, best_event_seq, _))
                    if (*to_seq < *best_to_seq)
                        || (*to_seq == *best_to_seq && event.seq <= *best_event_seq) => {}
                _ => {
                    best = Some((*to_seq, event.seq, summary_artifact_id.clone()));
                }
            }
        }
        let (summary_to_seq, _event_seq, summary_artifact_id) =
            best.expect("expected compaction checkpoint in full replay");

        let full_bundle =
            compile_summaries_recent_messages_v1(CompileSummariesRecentMessagesV1Request {
                continuity_id: &continuity_id,
                continuity_events: &full_events,
                event_log: event_log.as_ref(),
                snapshot_dir: &snapshot_dir,
                from_seq: full_from_seq,
                from_message_id: full_from_message_id.clone(),
                run_session_id: "run-session",
                actor_id: "user",
                origin: "cli",
                summary_artifact_id: &summary_artifact_id,
                summary_to_seq,
            })
            .expect("compile full");

        let window_input = store
            .load_context_compile_input_recent_messages_v1(&continuity_id, &anchor_message_id)
            .expect("window input");
        assert_eq!(window_input.from_seq, full_from_seq);
        assert_eq!(window_input.from_message_id, full_from_message_id);
        assert!(
            window_input.continuity_events.iter().all(|event| matches!(
                event.kind,
                EventKind::ContinuityMessageAppended { .. } | EventKind::ContinuityRunEnded { .. }
            )),
            "expected message+run-ended-only window events"
        );

        let checkpoint = store
            .latest_compaction_checkpoint_for_compile_v1(&continuity_id, window_input.from_seq)
            .expect("checkpoint lookup")
            .expect("checkpoint");
        assert_eq!(checkpoint.summary_artifact_id, summary_artifact_id);
        assert_eq!(checkpoint.to_seq, summary_to_seq);

        let window_bundle =
            compile_summaries_recent_messages_v1(CompileSummariesRecentMessagesV1Request {
                continuity_id: &continuity_id,
                continuity_events: &window_input.continuity_events,
                event_log: event_log.as_ref(),
                snapshot_dir: &snapshot_dir,
                from_seq: window_input.from_seq,
                from_message_id: window_input.from_message_id.clone(),
                run_session_id: "run-session",
                actor_id: "user",
                origin: "cli",
                summary_artifact_id: &checkpoint.summary_artifact_id,
                summary_to_seq: checkpoint.to_seq,
            })
            .expect("compile window");

        let full_json = serde_json::to_value(&full_bundle).expect("full json");
        let window_json = serde_json::to_value(&window_bundle).expect("window json");
        assert_eq!(window_json, full_json);
    }

    #[test]
    fn window_context_compile_input_works_when_global_log_is_corrupt() {
        use std::io::Write;

        let dir = tempdir().expect("tmp");
        let (event_log, store, data_dir) = store_for(&dir);
        let snapshot_dir = dir.path().join("snapshots");

        const MSG_LEN: usize = 60_000;
        const MSG_COUNT: usize = 200;

        let continuity_id = store.ensure_default().expect("ensure");
        let mut message_ids: Vec<String> = Vec::new();

        for idx in 0..MSG_COUNT {
            let message_id = store
                .append_message(
                    &continuity_id,
                    "user".to_string(),
                    "cli".to_string(),
                    format!("m{idx}:{}", "x".repeat(MSG_LEN)),
                )
                .expect("append message");
            let session_id = format!("session-{idx}");

            let session_events = vec![
                Event {
                    id: format!("se{idx}-0"),
                    session_id: session_id.clone(),
                    timestamp_ms: 0,
                    seq: 0,
                    kind: EventKind::SessionStarted {
                        input: "hi".to_string(),
                    },
                },
                Event {
                    id: format!("se{idx}-1"),
                    session_id: session_id.clone(),
                    timestamp_ms: 1,
                    seq: 1,
                    kind: EventKind::OutputTextDelta {
                        delta: format!("a{idx}"),
                    },
                },
                Event {
                    id: format!("se{idx}-2"),
                    session_id: session_id.clone(),
                    timestamp_ms: 2,
                    seq: 2,
                    kind: EventKind::SessionEnded {
                        reason: "completed".to_string(),
                    },
                },
            ];
            write_snapshot(&snapshot_dir, &session_id, &session_events).expect("snapshot");

            store
                .append_run_spawned(
                    &continuity_id,
                    &message_id,
                    &session_id,
                    "user".to_string(),
                    "cli".to_string(),
                )
                .expect("run spawned");
            store
                .append_context_compiled(
                    &continuity_id,
                    ContextCompiledPayload {
                        run_session_id: session_id.clone(),
                        bundle_artifact_id: "artifact-1".to_string(),
                        compiler_id: "rip.context_compiler.v1".to_string(),
                        compiler_strategy: "recent_messages_v1".to_string(),
                        from_seq: 0,
                        from_message_id: Some(message_id.clone()),
                        actor_id: "user".to_string(),
                        origin: "cli".to_string(),
                    },
                )
                .expect("context compiled");
            store
                .append_run_ended(
                    &continuity_id,
                    &message_id,
                    &session_id,
                    "completed".to_string(),
                    "user".to_string(),
                    "cli".to_string(),
                )
                .expect("run ended");

            message_ids.push(message_id);
        }

        let anchor_message_id = message_ids.get(40).cloned().expect("anchor message id");

        let full_events = store.replay_events(&continuity_id).expect("replay full");
        let (full_from_seq, full_from_message_id) =
            resolve_context_compile_cutpoint_full(&full_events, &anchor_message_id)
                .expect("cutpoint");
        let expected_bundle = compile_recent_messages_v1(CompileRecentMessagesV1Request {
            continuity_id: &continuity_id,
            continuity_events: &full_events,
            event_log: event_log.as_ref(),
            snapshot_dir: &snapshot_dir,
            from_seq: full_from_seq,
            from_message_id: full_from_message_id.clone(),
            run_session_id: "run-session",
            actor_id: "user",
            origin: "cli",
        })
        .expect("compile full");
        let expected_json = serde_json::to_value(&expected_bundle).expect("bundle json");

        // Corrupt the global log: window reads should still succeed via sidecar + indexes.
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(data_dir.join("events.jsonl"))
            .expect("open global log");
        writeln!(file, "not json").expect("corrupt global log");

        let (event_log2, store2, _data_dir2) = store_for(&dir);
        let window_input = store2
            .load_context_compile_input_recent_messages_v1(&continuity_id, &anchor_message_id)
            .expect("window input after restart");
        assert_eq!(window_input.from_seq, full_from_seq);
        assert_eq!(window_input.from_message_id, full_from_message_id);
        assert!(
            window_input.continuity_events.len() <= 128,
            "expected bounded window, got {} events",
            window_input.continuity_events.len()
        );

        let window_bundle = compile_recent_messages_v1(CompileRecentMessagesV1Request {
            continuity_id: &continuity_id,
            continuity_events: &window_input.continuity_events,
            // Snapshot-first: event_log is only used as fallback; global log is corrupted here.
            event_log: event_log2.as_ref(),
            snapshot_dir: &snapshot_dir,
            from_seq: window_input.from_seq,
            from_message_id: window_input.from_message_id.clone(),
            run_session_id: "run-session",
            actor_id: "user",
            origin: "cli",
        })
        .expect("compile window");
        let window_json = serde_json::to_value(&window_bundle).expect("bundle json");
        assert_eq!(window_json, expected_json);
    }

    #[test]
    fn append_tool_side_effects_advances_seq() {
        let dir = tempdir().expect("tmp");
        let (event_log, store, _data_dir) = store_for(&dir);

        let continuity_id = store.ensure_default().expect("ensure");
        let message_id = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "hello".to_string(),
            )
            .expect("append");
        store
            .append_run_spawned(
                &continuity_id,
                &message_id,
                "session-1",
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");
        store
            .append_tool_side_effects(
                &ContinuityRunLink {
                    continuity_id: continuity_id.clone(),
                    message_id: message_id.clone(),
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
                "session-1",
                ToolSideEffects {
                    tool_id: "tool-1".to_string(),
                    tool_name: "write".to_string(),
                    affected_paths: Some(vec!["a.txt".to_string()]),
                    checkpoint_id: Some("checkpoint-1".to_string()),
                },
            )
            .expect("tool side effects");

        let events = event_log
            .replay_stream(StreamKind::Continuity, &continuity_id)
            .expect("replay");
        assert_eq!(events.len(), 4);
        assert_eq!(events[3].seq, 3);
        match &events[3].kind {
            EventKind::ContinuityToolSideEffects {
                run_session_id,
                tool_id,
                tool_name,
                affected_paths,
                checkpoint_id,
                actor_id,
                origin,
            } => {
                assert_eq!(run_session_id, "session-1");
                assert_eq!(tool_id, "tool-1");
                assert_eq!(tool_name, "write");
                assert_eq!(affected_paths.as_deref(), Some(&["a.txt".to_string()][..]));
                assert_eq!(checkpoint_id.as_deref(), Some("checkpoint-1"));
                assert_eq!(actor_id, "user");
                assert_eq!(origin, "cli");
            }
            other => panic!("expected tool side effects, got {other:?}"),
        }
    }

    #[test]
    fn branch_creates_child_with_cutpoint_and_provenance() {
        let dir = tempdir().expect("tmp");
        let (event_log, store, _data_dir) = store_for(&dir);

        let parent_thread_id = store.ensure_default().expect("ensure");
        let m1 = store
            .append_message(
                &parent_thread_id,
                "user".to_string(),
                "cli".to_string(),
                "turn1".to_string(),
            )
            .expect("append");
        store
            .append_run_spawned(
                &parent_thread_id,
                &m1,
                "session-1",
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");
        store
            .append_run_ended(
                &parent_thread_id,
                &m1,
                "session-1",
                "completed".to_string(),
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run ended");
        let _m2 = store
            .append_message(
                &parent_thread_id,
                "user".to_string(),
                "cli".to_string(),
                "turn2".to_string(),
            )
            .expect("append");

        let (child_thread_id, parent_seq, parent_message_id) = store
            .branch(
                &parent_thread_id,
                Some("child".to_string()),
                Some(m1.clone()),
                None,
                "alice".to_string(),
                "team".to_string(),
            )
            .expect("branch");

        assert_eq!(parent_seq, 3, "expected cut to include run_ended");
        assert_eq!(parent_message_id.as_deref(), Some(m1.as_str()));

        let child_events = event_log
            .replay_stream(StreamKind::Continuity, &child_thread_id)
            .expect("replay child");
        assert_eq!(child_events.len(), 2);
        assert_eq!(child_events[0].seq, 0);
        assert_eq!(child_events[1].seq, 1);
        match &child_events[1].kind {
            EventKind::ContinuityBranched {
                parent_thread_id: parent_id,
                parent_seq: cut_seq,
                parent_message_id: cut_message_id,
                actor_id,
                origin,
            } => {
                assert_eq!(parent_id, &parent_thread_id);
                assert_eq!(*cut_seq, 3);
                assert_eq!(cut_message_id.as_deref(), Some(m1.as_str()));
                assert_eq!(actor_id, "alice");
                assert_eq!(origin, "team");
            }
            other => panic!("expected continuity_branched, got {other:?}"),
        }

        store
            .append_message(
                &child_thread_id,
                "user".to_string(),
                "cli".to_string(),
                "child turn".to_string(),
            )
            .expect("append child");
        let child_events = event_log
            .replay_stream(StreamKind::Continuity, &child_thread_id)
            .expect("replay child");
        assert_eq!(child_events.len(), 3);
        assert_eq!(
            child_events[2].seq, 2,
            "expected seq to continue after branch"
        );
    }

    #[test]
    fn branch_rejects_conflicting_cut_selectors() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, _data_dir) = store_for(&dir);

        let parent_thread_id = store.ensure_default().expect("ensure");
        let err = store
            .branch(
                &parent_thread_id,
                None,
                Some("m1".to_string()),
                Some(1),
                "user".to_string(),
                "cli".to_string(),
            )
            .expect_err("expected error");
        assert!(err.contains("from_message_id") && err.contains("from_seq"));
    }

    #[test]
    fn handoff_creates_child_with_cutpoint_provenance_and_summary() {
        let dir = tempdir().expect("tmp");
        let (event_log, store, _data_dir) = store_for(&dir);

        let from_thread_id = store.ensure_default().expect("ensure");
        let m1 = store
            .append_message(
                &from_thread_id,
                "user".to_string(),
                "cli".to_string(),
                "turn1".to_string(),
            )
            .expect("append");
        store
            .append_run_spawned(
                &from_thread_id,
                &m1,
                "session-1",
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");
        store
            .append_run_ended(
                &from_thread_id,
                &m1,
                "session-1",
                "completed".to_string(),
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run ended");
        let _m2 = store
            .append_message(
                &from_thread_id,
                "user".to_string(),
                "cli".to_string(),
                "turn2".to_string(),
            )
            .expect("append");

        let (child_thread_id, from_seq, from_message_id) = store
            .handoff(
                &from_thread_id,
                Some("handoff".to_string()),
                (Some("summary".to_string()), None),
                Some(m1.clone()),
                None,
                ("alice".to_string(), "team".to_string()),
            )
            .expect("handoff");

        assert_eq!(from_seq, 3, "expected cut to include run_ended");
        assert_eq!(from_message_id.as_deref(), Some(m1.as_str()));

        let child_events = event_log
            .replay_stream(StreamKind::Continuity, &child_thread_id)
            .expect("replay child");
        assert_eq!(child_events.len(), 2);
        assert_eq!(child_events[0].seq, 0);
        assert_eq!(child_events[1].seq, 1);
        let artifact_id = match &child_events[1].kind {
            EventKind::ContinuityHandoffCreated {
                from_thread_id: event_from_id,
                from_seq: cut_seq,
                from_message_id: cut_message_id,
                summary_artifact_id,
                summary_markdown,
                actor_id,
                origin,
            } => {
                assert_eq!(event_from_id, &from_thread_id);
                assert_eq!(*cut_seq, 3);
                assert_eq!(cut_message_id.as_deref(), Some(m1.as_str()));
                let artifact_id = summary_artifact_id.as_deref().expect("summary_artifact_id");
                assert_eq!(artifact_id.len(), 64);
                assert_eq!(summary_markdown.as_deref(), Some("summary"));
                assert_eq!(actor_id, "alice");
                assert_eq!(origin, "team");
                artifact_id.to_string()
            }
            other => panic!("expected continuity_handoff_created, got {other:?}"),
        };

        let blob_path = dir
            .path()
            .join("workspace")
            .join(".rip")
            .join("artifacts")
            .join("blobs")
            .join(&artifact_id);
        let bytes = fs::read(&blob_path).expect("read bundle artifact");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("bundle json");
        assert_eq!(
            json.get("schema").and_then(|v| v.as_str()),
            Some("rip.handoff_context_bundle.v1")
        );
        assert_eq!(
            json.get("summary_markdown").and_then(|v| v.as_str()),
            Some("summary")
        );
        let thread_refs = json
            .get("refs")
            .and_then(|v| v.get("threads"))
            .and_then(|v| v.as_array())
            .expect("thread refs");
        assert_eq!(thread_refs.len(), 1);
        assert_eq!(
            thread_refs[0].get("thread_id").and_then(|v| v.as_str()),
            Some(from_thread_id.as_str())
        );
        assert_eq!(thread_refs[0].get("seq").and_then(|v| v.as_u64()), Some(3));
        assert_eq!(
            thread_refs[0].get("message_id").and_then(|v| v.as_str()),
            Some(m1.as_str())
        );

        store
            .append_message(
                &child_thread_id,
                "user".to_string(),
                "cli".to_string(),
                "child turn".to_string(),
            )
            .expect("append child");
        let child_events = event_log
            .replay_stream(StreamKind::Continuity, &child_thread_id)
            .expect("replay child");
        assert_eq!(child_events.len(), 3);
        assert_eq!(
            child_events[2].seq, 2,
            "expected seq to continue after handoff"
        );
    }

    #[test]
    fn handoff_rejects_missing_summary() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, _data_dir) = store_for(&dir);

        let from_thread_id = store.ensure_default().expect("ensure");
        let err = store
            .handoff(
                &from_thread_id,
                None,
                (None, None),
                None,
                None,
                ("user".to_string(), "cli".to_string()),
            )
            .expect_err("expected error");
        assert!(err.contains("summary"), "expected summary validation");
    }

    #[test]
    fn handoff_rejects_conflicting_cut_selectors() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, _data_dir) = store_for(&dir);

        let from_thread_id = store.ensure_default().expect("ensure");
        let err = store
            .handoff(
                &from_thread_id,
                None,
                (Some("summary".to_string()), None),
                Some("m1".to_string()),
                Some(1),
                ("user".to_string(), "cli".to_string()),
            )
            .expect_err("expected error");
        assert!(err.contains("from_message_id") && err.contains("from_seq"));
    }

    #[test]
    fn list_and_get_reflect_created_thread() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, _data_dir) = store_for(&dir);

        let continuity_id = store.ensure_default().expect("ensure");

        let all = store.list();
        assert!(all.iter().any(|meta| meta.continuity_id == continuity_id));

        let meta = store.get(&continuity_id).expect("meta");
        assert_eq!(meta.continuity_id, continuity_id);
        assert!(!meta.archived);
    }

    #[test]
    fn append_message_unknown_continuity_is_error() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, _data_dir) = store_for(&dir);

        let err = store
            .append_message(
                "missing-thread-id",
                "user".to_string(),
                "cli".to_string(),
                "hello".to_string(),
            )
            .expect_err("expected error");
        assert!(err.contains("continuity stream does not exist"));
    }

    #[test]
    fn append_run_spawned_unknown_continuity_is_error() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, _data_dir) = store_for(&dir);

        let err = store
            .append_run_spawned(
                "missing-thread-id",
                "message-1",
                "session-1",
                "user".to_string(),
                "cli".to_string(),
            )
            .expect_err("expected error");
        assert!(err.contains("continuity stream does not exist"));
    }

    #[test]
    fn new_ignores_invalid_index_json() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_root = dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace");

        let path = index_path(&data_dir);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(&path, b"not json").expect("write");

        let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
        let store =
            ContinuityStore::new(data_dir.clone(), workspace_root, event_log).expect("store");

        let continuity_id = store.ensure_default().expect("ensure");
        assert!(!continuity_id.is_empty());
    }

    #[test]
    fn new_resets_index_on_version_mismatch() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_root = dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace");

        let legacy_id = "legacy-thread-id";
        let legacy = serde_json::json!({
            "version": 0,
            "workspaces": {
                workspace_key(&workspace_root): legacy_id,
            },
            "continuities": {
                legacy_id: {
                    "created_at_ms": 0,
                    "title": null,
                    "archived": false,
                }
            }
        });
        let path = index_path(&data_dir);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(&path, legacy.to_string()).expect("write");

        let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
        let store =
            ContinuityStore::new(data_dir.clone(), workspace_root, event_log).expect("store");

        let continuity_id = store.ensure_default().expect("ensure");
        assert_ne!(continuity_id, legacy_id);
    }

    #[test]
    fn ensure_default_errors_when_index_parent_is_file() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_root = dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace");
        fs::create_dir_all(&data_dir).expect("data");
        fs::write(data_dir.join("continuities"), "file").expect("continuities file");

        let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
        let store =
            ContinuityStore::new(data_dir.clone(), workspace_root, event_log).expect("store");

        let err = store.ensure_default().expect_err("expected error");
        assert!(err.contains("save continuity index"));
    }
}
