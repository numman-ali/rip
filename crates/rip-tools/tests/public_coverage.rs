use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rip_kernel::EventKind;
use rip_tools::{
    register_builtin_tools, BuiltinToolConfig, CheckpointHook, CheckpointRecord, CheckpointRequest,
    CheckpointRewindRecord, ToolInvocation, ToolRegistry, ToolRunner,
};
use serde_json::json;
use tempfile::tempdir;

#[derive(Default)]
struct RecordingCheckpointHook {
    requests: Mutex<Vec<CheckpointRequest>>,
}

impl CheckpointHook for RecordingCheckpointHook {
    fn create(&self, request: CheckpointRequest) -> Result<CheckpointRecord, String> {
        self.requests
            .lock()
            .expect("requests")
            .push(request.clone());
        Ok(CheckpointRecord {
            id: "ckpt-1".to_string(),
            label: request.label,
            created_at_ms: 1,
            files: request
                .files
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
        })
    }

    fn rewind(
        &self,
        _session_id: &str,
        checkpoint_id: &str,
    ) -> Result<CheckpointRewindRecord, String> {
        Ok(CheckpointRewindRecord {
            id: checkpoint_id.to_string(),
            label: "rewind".to_string(),
            files: vec!["note.txt".to_string()],
        })
    }
}

#[tokio::test]
async fn builtin_registry_and_runner_cover_write_grep_and_bash_paths() {
    let dir = tempdir().expect("tmp");
    let workspace_root = dir.path().to_path_buf();
    let config = BuiltinToolConfig {
        workspace_root: workspace_root.clone(),
        artifact_max_bytes: 64,
        max_bytes: 64,
        max_results: 10,
        max_depth: 4,
        follow_symlinks: false,
        include_hidden: false,
    };

    let registry = Arc::new(ToolRegistry::default());
    register_builtin_tools(&registry, config.clone());
    let hook = Arc::new(RecordingCheckpointHook::default());
    let runner = ToolRunner::with_checkpoint_hook(registry, 2, hook.clone());

    let mut seq = 0;
    let write_events = runner
        .run(
            "session-1",
            &mut seq,
            ToolInvocation {
                name: "write".to_string(),
                args: json!({"path": "note.txt", "content": "hello from rip"}),
                timeout_ms: None,
            },
        )
        .await;
    assert!(write_events
        .iter()
        .any(|event| matches!(event.kind, EventKind::CheckpointCreated { auto: true, .. })));
    assert!(workspace_root.join("note.txt").exists());

    std::fs::write(
        workspace_root.join("search.txt"),
        "alpha\nmatch me\nomega\n",
    )
    .expect("write search file");
    let grep_events = runner
        .run(
            "session-1",
            &mut seq,
            ToolInvocation {
                name: "grep".to_string(),
                args: json!({"pattern": "match", "path": ".", "regex": false}),
                timeout_ms: None,
            },
        )
        .await;
    assert!(grep_events.iter().any(|event| matches!(
        &event.kind,
        EventKind::ToolStdout { chunk, .. } if chunk.contains("search.txt:2:match me")
    )));

    let bash_events = runner
        .run(
            "session-1",
            &mut seq,
            ToolInvocation {
                name: "bash".to_string(),
                args: json!({"command": "printf '1234567890'", "max_bytes": 4}),
                timeout_ms: None,
            },
        )
        .await;
    assert!(bash_events.iter().any(|event| matches!(
        &event.kind,
        EventKind::ToolEnded { artifacts: Some(artifacts), .. }
            if artifacts
                .get("stdout")
                .and_then(|stdout| stdout.get("artifact"))
                .and_then(|artifact| artifact.get("path"))
                .and_then(|path| path.as_str())
                .map(|path| path.contains(".rip/artifacts/blobs"))
                .unwrap_or(false)
    )));

    let requests = hook.requests.lock().expect("requests");
    assert!(requests.iter().any(|request| {
        request.tool_name.as_deref() == Some("write")
            && request.files == vec![PathBuf::from("note.txt")]
            && request.auto
    }));
}
