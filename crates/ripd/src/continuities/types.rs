use rip_kernel::{CompactionPlannedCutPoint, Event};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
    pub(crate) compaction_checkpoints: Vec<rip_kernel::ContextSelectionCompactionCheckpointV1>,
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
    #[serde(rename = "ref")]
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compaction_checkpoints: Vec<ContextSelectionStatusCheckpointV1>,
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

#[derive(Debug, Clone)]
pub(crate) struct CompactionCheckpointCreatedPayload {
    pub(crate) cut_rule_id: String,
    pub(crate) summary_kind: String,
    pub(crate) summary_artifact_id: String,
    pub(crate) from_seq: u64,
    pub(crate) from_message_id: Option<String>,
    pub(crate) to_seq: u64,
    pub(crate) to_message_id: Option<String>,
    pub(crate) actor_id: String,
    pub(crate) origin: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CompactionAutoScheduleDecidedPayload {
    pub(crate) decision_id: String,
    pub(crate) policy_id: String,
    pub(crate) decision: String,
    pub(crate) execute: bool,
    pub(crate) stride_messages: u64,
    pub(crate) max_new_checkpoints: u32,
    pub(crate) block_on_inflight: bool,
    pub(crate) message_count: u64,
    pub(crate) cut_rule_id: String,
    pub(crate) planned: Vec<CompactionPlannedCutPoint>,
    pub(crate) job_id: Option<String>,
    pub(crate) job_kind: Option<String>,
    pub(crate) reason: Option<serde_json::Value>,
    pub(crate) actor_id: String,
    pub(crate) origin: String,
}

#[derive(Debug, Clone)]
pub(crate) struct JobEndedPayload {
    pub(crate) job_id: String,
    pub(crate) job_kind: String,
    pub(crate) status: String,
    pub(crate) result: Option<serde_json::Value>,
    pub(crate) error: Option<String>,
    pub(crate) actor_id: String,
    pub(crate) origin: String,
}

#[derive(Debug, Clone)]
pub struct ToolSideEffects {
    pub tool_id: String,
    pub tool_name: String,
    pub affected_paths: Option<Vec<String>>,
    pub checkpoint_id: Option<String>,
}
