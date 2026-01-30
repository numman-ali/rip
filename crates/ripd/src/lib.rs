mod checkpoints;
mod compaction_auto_summary;
mod compaction_checkpoint_index;
mod compaction_summary;
mod config;
mod context_bundle;
mod context_compiler;
mod continuities;
mod continuity_seek_index;
mod continuity_stream_cache;
mod handoff_context_bundle;
mod local_authority;
mod message_ordinal_index;
mod openresponses_observability;
mod provider_openresponses;
mod runner;
mod server;
mod session;
mod tasks;
mod workspace_lock;

pub use continuities::{
    CompactionAutoResultCheckpointV1, CompactionAutoScheduleV1Request,
    CompactionAutoScheduleV1Response, CompactionAutoV1Request, CompactionAutoV1Response,
    CompactionCheckpointCumulativeV1Request, CompactionCutPointV1, CompactionCutPointsV1Request,
    CompactionCutPointsV1Response, CompactionPlannedCutPointV1, CompactionStatusV1Request,
    CompactionStatusV1Response, ContextSelectionStatusDecisionV1, ContextSelectionStatusV1Request,
    ContextSelectionStatusV1Response, ContinuityMeta, ContinuityRunLink, ContinuityStore,
    ProviderCursorRotateV1Request, ProviderCursorRotateV1Response, ProviderCursorStatusCursorV1,
    ProviderCursorStatusV1Request, ProviderCursorStatusV1Response, ToolSideEffects,
};
pub use local_authority::{
    authority_dir, authority_lock_path, authority_meta_path, pid_liveness,
    read_authority_lock_record, read_authority_meta, try_cleanup_corrupt_lock_file,
    try_cleanup_stale_authority_files, AuthorityLockGuard, AuthorityLockRecord, AuthorityMeta,
    PidLiveness,
};
pub use runner::{SessionEngine, SessionHandle};

#[cfg(not(test))]
pub async fn serve_default() {
    server::serve(server::data_dir()).await;
}

#[cfg(test)]
mod server_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_are_accessible() {
        let dir = tempfile::tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_root = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_root).expect("workspace");
        let engine = SessionEngine::new(data_dir, workspace_root, None).expect("engine");
        let _handle = engine.create_session();
    }
}
