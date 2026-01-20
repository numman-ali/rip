use std::sync::Arc;

use rip_kernel::EventKind;
use rip_tools::{
    register_builtin_tools, BuiltinToolConfig, ToolInvocation, ToolRegistry, ToolRunner,
};
use serde_json::json;
use tempfile::tempdir;

fn assert_tool_ended(events: &[rip_kernel::Event]) {
    assert!(events
        .iter()
        .any(|event| matches!(event.kind, EventKind::ToolEnded { .. })));
}

#[tokio::test]
async fn registry_handlers_execute() {
    let dir = tempdir().expect("tmp");
    let registry = Arc::new(ToolRegistry::default());
    register_builtin_tools(
        &registry,
        BuiltinToolConfig {
            workspace_root: dir.path().to_path_buf(),
            ..BuiltinToolConfig::default()
        },
    );

    let runner = ToolRunner::new(registry, 1);
    let mut seq = 0;

    let events = runner
        .run(
            "s1",
            &mut seq,
            ToolInvocation {
                name: "write".to_string(),
                args: json!({"path":"a.txt","content":"hi"}),
                timeout_ms: None,
            },
        )
        .await;
    assert_tool_ended(&events);

    let events = runner
        .run(
            "s1",
            &mut seq,
            ToolInvocation {
                name: "read".to_string(),
                args: json!({"path":"a.txt"}),
                timeout_ms: None,
            },
        )
        .await;
    assert_tool_ended(&events);

    let events = runner
        .run(
            "s1",
            &mut seq,
            ToolInvocation {
                name: "ls".to_string(),
                args: json!({"path":"."}),
                timeout_ms: None,
            },
        )
        .await;
    assert_tool_ended(&events);

    let events = runner
        .run(
            "s1",
            &mut seq,
            ToolInvocation {
                name: "grep".to_string(),
                args: json!({"pattern":"hi","path":"."}),
                timeout_ms: None,
            },
        )
        .await;
    assert_tool_ended(&events);

    let patch = "*** Begin Patch\n*** Add File: b.txt\n+hello\n*** End Patch\n";
    let events = runner
        .run(
            "s1",
            &mut seq,
            ToolInvocation {
                name: "apply_patch".to_string(),
                args: json!({"patch": patch}),
                timeout_ms: None,
            },
        )
        .await;
    assert_tool_ended(&events);

    let events = runner
        .run(
            "s1",
            &mut seq,
            ToolInvocation {
                name: "artifact_fetch".to_string(),
                args: json!({"id":"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"}),
                timeout_ms: None,
            },
        )
        .await;
    assert_tool_ended(&events);

    let events = runner
        .run(
            "s1",
            &mut seq,
            ToolInvocation {
                name: "bash".to_string(),
                args: json!({"command":"echo hi"}),
                timeout_ms: None,
            },
        )
        .await;
    assert_tool_ended(&events);
}
