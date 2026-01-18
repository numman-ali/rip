use std::sync::{Arc, Mutex};

use rip_kernel::{CheckpointAction, EventKind};
use rip_tools::{
    register_builtin_tools, BuiltinToolConfig, CheckpointHook, CheckpointRecord, CheckpointRequest,
    CheckpointRewindRecord, ToolInvocation, ToolRegistry, ToolRunner,
};
use serde_json::json;
use tempfile::tempdir;

#[derive(Default)]
struct HookSpy {
    requests: Mutex<Vec<CheckpointRequest>>,
    rewinds: Mutex<Vec<String>>,
}

impl CheckpointHook for HookSpy {
    fn create(&self, request: CheckpointRequest) -> Result<CheckpointRecord, String> {
        let files = request
            .files
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        self.requests.lock().expect("lock").push(request.clone());
        Ok(CheckpointRecord {
            id: "cp-1".to_string(),
            label: request.label,
            created_at_ms: 123,
            files,
        })
    }

    fn rewind(
        &self,
        _session_id: &str,
        checkpoint_id: &str,
    ) -> Result<CheckpointRewindRecord, String> {
        self.rewinds
            .lock()
            .expect("lock")
            .push(checkpoint_id.to_string());
        Ok(CheckpointRewindRecord {
            id: checkpoint_id.to_string(),
            label: "manual".to_string(),
            files: vec!["a.txt".to_string()],
        })
    }
}

#[tokio::test]
async fn auto_checkpoint_emits_before_tool_started() {
    let dir = tempdir().expect("tmp");
    let root = dir.path().to_path_buf();
    let registry = Arc::new(ToolRegistry::default());
    register_builtin_tools(
        &registry,
        BuiltinToolConfig {
            workspace_root: root,
            ..BuiltinToolConfig::default()
        },
    );

    let hook = Arc::new(HookSpy::default());
    let runner = ToolRunner::with_checkpoint_hook(registry, 1, hook.clone());
    let mut seq = 0;

    let events = runner
        .run(
            "session-1",
            &mut seq,
            ToolInvocation {
                name: "write".to_string(),
                args: json!({"path": "a.txt", "content": "hello"}),
                timeout_ms: None,
            },
        )
        .await;

    match &events[0].kind {
        EventKind::CheckpointCreated {
            checkpoint_id,
            label,
            created_at_ms,
            files,
            auto,
            tool_name,
        } => {
            assert_eq!(checkpoint_id, "cp-1");
            assert_eq!(label, "auto:write");
            assert_eq!(*created_at_ms, 123);
            assert_eq!(files, &vec!["a.txt".to_string()]);
            assert!(*auto);
            assert_eq!(tool_name.as_deref(), Some("write"));
        }
        other => panic!("expected checkpoint_created, got {other:?}"),
    }

    assert!(matches!(events[1].kind, EventKind::ToolStarted { .. }));
    let recorded = hook.requests.lock().expect("lock");
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].tool_name.as_deref(), Some("write"));
    assert_eq!(recorded[0].files.len(), 1);
}

#[tokio::test]
async fn checkpoint_parse_failure_emits_failed() {
    let dir = tempdir().expect("tmp");
    let registry = Arc::new(ToolRegistry::default());
    register_builtin_tools(
        &registry,
        BuiltinToolConfig {
            workspace_root: dir.path().to_path_buf(),
            ..BuiltinToolConfig::default()
        },
    );

    let hook = Arc::new(HookSpy::default());
    let runner = ToolRunner::with_checkpoint_hook(registry, 1, hook.clone());
    let mut seq = 0;

    let events = runner
        .run(
            "session-1",
            &mut seq,
            ToolInvocation {
                name: "write".to_string(),
                args: json!({"content": "missing path"}),
                timeout_ms: None,
            },
        )
        .await;

    assert!(matches!(
        events[0].kind,
        EventKind::CheckpointFailed {
            action: CheckpointAction::Create,
            ..
        }
    ));
    assert!(hook.requests.lock().expect("lock").is_empty());
}

#[test]
fn rewind_checkpoint_emits_event() {
    let registry = Arc::new(ToolRegistry::default());
    let hook = Arc::new(HookSpy::default());
    let runner = ToolRunner::with_checkpoint_hook(registry, 1, hook.clone());
    let mut seq = 0;

    let events = runner.rewind_checkpoint("session-1", &mut seq, "cp-9");
    match &events[0].kind {
        EventKind::CheckpointRewound {
            checkpoint_id,
            label,
            files,
        } => {
            assert_eq!(checkpoint_id, "cp-9");
            assert_eq!(label, "manual");
            assert_eq!(files, &vec!["a.txt".to_string()]);
        }
        other => panic!("expected checkpoint_rewound, got {other:?}"),
    }
}

#[test]
fn rewind_checkpoint_reports_missing_hook() {
    let registry = Arc::new(ToolRegistry::default());
    let runner = ToolRunner::new(registry, 1);
    let mut seq = 0;

    let events = runner.rewind_checkpoint("session-1", &mut seq, "cp-0");
    assert!(matches!(
        events[0].kind,
        EventKind::CheckpointFailed {
            action: CheckpointAction::Rewind,
            ..
        }
    ));
}

#[test]
fn create_checkpoint_emits_event() {
    let registry = Arc::new(ToolRegistry::default());
    let hook = Arc::new(HookSpy::default());
    let runner = ToolRunner::with_checkpoint_hook(registry, 1, hook);
    let mut seq = 0;

    let events = runner.create_checkpoint(
        "session-1",
        &mut seq,
        "manual".to_string(),
        vec![std::path::PathBuf::from("a.txt")],
    );
    match &events[0].kind {
        EventKind::CheckpointCreated {
            label,
            auto,
            tool_name,
            ..
        } => {
            assert_eq!(label, "manual");
            assert!(!(*auto));
            assert!(tool_name.is_none());
        }
        other => panic!("expected checkpoint_created, got {other:?}"),
    }
}

#[test]
fn create_checkpoint_reports_missing_hook() {
    let registry = Arc::new(ToolRegistry::default());
    let runner = ToolRunner::new(registry, 1);
    let mut seq = 0;

    let events = runner.create_checkpoint(
        "session-1",
        &mut seq,
        "manual".to_string(),
        vec![std::path::PathBuf::from("a.txt")],
    );
    assert!(matches!(
        events[0].kind,
        EventKind::CheckpointFailed {
            action: CheckpointAction::Create,
            ..
        }
    ));
}
