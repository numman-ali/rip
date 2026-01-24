mod checkpoints;
mod compaction_summary;
mod context_bundle;
mod context_compiler;
mod continuities;
mod continuity_seek_index;
mod continuity_stream_cache;
mod handoff_context_bundle;
mod provider_openresponses;
mod runner;
mod server;
mod session;
mod tasks;
mod workspace_lock;

pub use continuities::{
    CompactionCheckpointCumulativeV1Request, ContinuityMeta, ContinuityRunLink, ContinuityStore,
    ToolSideEffects,
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
