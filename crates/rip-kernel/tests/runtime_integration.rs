use std::sync::Arc;

use rip_kernel::{Command, CommandContext, HookContext, HookEventKind, HookOutcome, Runtime};

#[test]
fn runtime_exposes_hooks_and_commands() {
    let runtime = Runtime::default();

    let hooks = runtime.hooks();
    let ctx = HookContext {
        session_id: "s1".to_string(),
        seq: 0,
        timestamp_ms: 0,
        event: HookEventKind::SessionStarted,
        output: None,
    };
    assert_eq!(hooks.run(&ctx), HookOutcome::Continue);

    let commands = runtime.commands();
    commands
        .register(Command::new(
            "noop",
            "no-op",
            Arc::new(|_ctx| Ok("ok".to_string())),
        ))
        .expect("register");
    let result = commands.execute(
        "noop",
        CommandContext {
            session_id: None,
            args: Vec::new(),
            raw: String::new(),
        },
    );
    assert_eq!(result.expect("execute"), "ok");
}
