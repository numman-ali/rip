use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rip_kernel::{CompactionPlannedCutPoint, Event, EventKind, StreamKind};
use rip_log::EventLog;
use tokio::sync::broadcast;

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

mod append;
mod branching;
mod compaction_auto;
mod compaction_manual;
mod compaction_status;
mod compile;
mod cursor;
mod index;
#[cfg(test)]
mod tests;
mod types;

use self::index::{
    index_path, load_index, now_ms, save_index, workspace_key, ContinuityIndexV1, ContinuityMetaV1,
};
pub(crate) use self::types::{
    CompactionAutoScheduleDecidedPayload, CompactionCheckpointCreatedPayload,
    ContextCompiledPayload, ContextSelectionDecidedPayload, JobEndedPayload,
    ProviderCursorUpdatedPayload,
};

pub use self::types::{
    CompactionAutoResultCheckpointV1, CompactionAutoScheduleV1Request,
    CompactionAutoScheduleV1Response, CompactionAutoV1Request, CompactionAutoV1Response,
    CompactionCheckpointCumulativeV1Request, CompactionCutPointV1, CompactionCutPointsV1Request,
    CompactionCutPointsV1Response, CompactionPlannedCutPointV1, CompactionStatusCheckpointV1,
    CompactionStatusJobOutcomeV1, CompactionStatusScheduleDecisionV1, CompactionStatusV1Request,
    CompactionStatusV1Response, ContextSelectionStatusCheckpointV1,
    ContextSelectionStatusDecisionV1, ContextSelectionStatusResetV1,
    ContextSelectionStatusV1Request, ContextSelectionStatusV1Response, ContinuityMeta,
    ContinuityRunLink, ProviderCursorRotateV1Request, ProviderCursorRotateV1Response,
    ProviderCursorStatusCursorV1, ProviderCursorStatusV1Request, ProviderCursorStatusV1Response,
    ToolSideEffects,
};
pub(crate) use self::types::{CompactionCheckpointForCompile, ContextCompileInput};

const EVENT_CHANNEL_CAPACITY: usize = 16_384;
const COMPACTION_JOB_KIND_SUMMARIZER_V1: &str = "compaction_summarizer_v1";

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

    pub(crate) fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }
}
