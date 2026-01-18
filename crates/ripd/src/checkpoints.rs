use std::path::PathBuf;

use rip_tools::{CheckpointHook, CheckpointRecord, CheckpointRequest, CheckpointRewindRecord};
use rip_workspace::Workspace;

pub struct WorkspaceCheckpointHook {
    workspace: Workspace,
}

impl WorkspaceCheckpointHook {
    pub fn new(root: PathBuf) -> std::io::Result<Self> {
        Ok(Self {
            workspace: Workspace::new(root)?,
        })
    }
}

impl CheckpointHook for WorkspaceCheckpointHook {
    #[cfg_attr(test, inline(never))]
    fn create(&self, request: CheckpointRequest) -> Result<CheckpointRecord, String> {
        let checkpoint = self
            .workspace
            .create_checkpoint(&request.session_id, request.label, &request.files)
            .map_err(|err| format!("checkpoint create failed: {err}"))?;
        let files = checkpoint
            .files
            .iter()
            .map(|entry| entry.path.clone())
            .collect();

        Ok(CheckpointRecord {
            id: checkpoint.id,
            label: checkpoint.label,
            created_at_ms: checkpoint.created_at_ms,
            files,
        })
    }

    #[cfg_attr(test, inline(never))]
    fn rewind(
        &self,
        session_id: &str,
        checkpoint_id: &str,
    ) -> Result<CheckpointRewindRecord, String> {
        let checkpoints = self
            .workspace
            .list_checkpoints(session_id)
            .map_err(|err| format!("checkpoint list failed: {err}"))?;
        let checkpoint = checkpoints
            .into_iter()
            .find(|entry| entry.id == checkpoint_id)
            .ok_or_else(|| "checkpoint not found".to_string())?;

        self.workspace
            .rewind_to_checkpoint(session_id, checkpoint_id)
            .map_err(|err| format!("checkpoint rewind failed: {err}"))?;

        let files = checkpoint
            .files
            .iter()
            .map(|entry| entry.path.clone())
            .collect();

        Ok(CheckpointRewindRecord {
            id: checkpoint.id,
            label: checkpoint.label,
            files,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rip_tools::{CheckpointHook, CheckpointRequest};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn create_and_rewind_checkpoint() {
        let dir = tempdir().expect("tmp");
        let root = dir.path().to_path_buf();
        let hook = WorkspaceCheckpointHook::new(root.clone()).expect("hook");
        let hook_ref: &dyn CheckpointHook = &hook;

        let file = root.join("a.txt");
        fs::write(&file, "one").expect("write");

        let record_direct = hook_ref
            .create(CheckpointRequest {
                session_id: "s1".to_string(),
                label: "direct".to_string(),
                files: vec![file.clone()],
                auto: false,
                tool_name: None,
            })
            .expect("create direct");

        let create_fn = CheckpointHook::create;
        let record = create_fn(
            hook_ref,
            CheckpointRequest {
                session_id: "s1".to_string(),
                label: "manual".to_string(),
                files: vec![file.clone()],
                auto: false,
                tool_name: None,
            },
        )
        .expect("create");
        assert!(!record.id.is_empty());
        assert_eq!(record.label, "manual");
        assert_eq!(record.files, vec!["a.txt".to_string()]);

        fs::write(&file, "two").expect("write");
        let _ = hook_ref
            .rewind("s1", &record_direct.id)
            .expect("rewind direct");
        let rewind_fn = CheckpointHook::rewind;
        let rewind = rewind_fn(hook_ref, "s1", &record.id).expect("rewind");
        assert_eq!(rewind.id, record.id);
        assert_eq!(rewind.label, "manual");
        assert_eq!(rewind.files, vec!["a.txt".to_string()]);
        assert_eq!(fs::read_to_string(&file).expect("read"), "one");
    }

    #[test]
    fn rewind_missing_checkpoint_errors() {
        let dir = tempdir().expect("tmp");
        let root = dir.path().to_path_buf();
        let hook = WorkspaceCheckpointHook::new(root).expect("hook");
        let hook_ref: &dyn CheckpointHook = &hook;

        let _ = hook_ref.rewind("s1", "missing").expect_err("error direct");
        let rewind_fn = CheckpointHook::rewind;
        let err = rewind_fn(hook_ref, "s1", "missing").expect_err("error");
        assert!(err.contains("not found"));
    }
}
