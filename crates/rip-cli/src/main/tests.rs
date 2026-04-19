use super::run_impl::{
    ensure_thread, post_thread_message, render_message, run_interactive_remote,
    run_local_with_engine, stream_events, stream_events_with_writer,
};
use super::*;
use httpmock::Method::{GET, POST};
use httpmock::MockServer;
use std::sync::{Mutex, OnceLock};

fn fixture_path(rel: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn session_started_frame() -> String {
    serde_json::json!({
        "id": "e1",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 0,
        "type": "session_started",
        "input": "hi"
    })
    .to_string()
}

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock")
}

fn capture_env(keys: &[&str]) -> Vec<(String, Option<String>)> {
    keys.iter()
        .map(|key| ((*key).to_string(), std::env::var(*key).ok()))
        .collect()
}

fn restore_env(saved: Vec<(String, Option<String>)>) {
    for (key, value) in saved {
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}

fn with_clean_env<F: FnOnce()>(f: F) {
    let _guard = env_lock();
    let keys = [
        "RIP_DATA_DIR",
        "RIP_WORKSPACE_ROOT",
        "RIP_OPENRESPONSES_ENDPOINT",
        "RIP_OPENRESPONSES_MODEL",
        "RIP_OPENRESPONSES_STATELESS_HISTORY",
        "RIP_OPENRESPONSES_PARALLEL_TOOL_CALLS",
        "RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE",
        "RIP_OPENRESPONSES_API_KEY",
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
    ];
    let saved = capture_env(&keys);
    for key in keys {
        std::env::remove_var(key);
    }
    f();
    restore_env(saved);
}

#[allow(clippy::await_holding_lock)]
async fn with_clean_env_async<F, Fut>(f: F)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    // Keep the env lock held across await to avoid test env races.
    let _guard = env_lock();
    let keys = [
        "RIP_DATA_DIR",
        "RIP_WORKSPACE_ROOT",
        "RIP_OPENRESPONSES_ENDPOINT",
        "RIP_OPENRESPONSES_MODEL",
        "RIP_OPENRESPONSES_STATELESS_HISTORY",
        "RIP_OPENRESPONSES_PARALLEL_TOOL_CALLS",
        "RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE",
        "RIP_OPENRESPONSES_API_KEY",
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
    ];
    let saved = capture_env(&keys);
    for key in keys {
        std::env::remove_var(key);
    }
    f().await;
    restore_env(saved);
}

#[tokio::test]
async fn ensure_thread_success() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST).path("/threads/ensure");
        then.status(200)
            .header("content-type", "application/json")
            .body(r#"{"thread_id":"t1"}"#);
    });

    let client = Client::new();
    let thread_id = ensure_thread(&client, &server.base_url()).await.unwrap();
    assert_eq!(thread_id, "t1");
    mock.assert();
}

#[tokio::test]
async fn ensure_thread_failure() {
    let server = MockServer::start();
    let _mock = server.mock(|when, then| {
        when.method(POST).path("/threads/ensure");
        then.status(500);
    });

    let client = Client::new();
    let err = ensure_thread(&client, &server.base_url())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("ensure thread failed"));
}

#[tokio::test]
async fn ensure_thread_parse_error() {
    let server = MockServer::start();
    let _mock = server.mock(|when, then| {
        when.method(POST).path("/threads/ensure");
        then.status(200)
            .header("content-type", "application/json")
            .body(r#"{"thread":"missing_id"}"#);
    });

    let client = Client::new();
    let err = ensure_thread(&client, &server.base_url())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("missing field"));
}

#[tokio::test]
async fn post_thread_message_failure() {
    let server = MockServer::start();
    let _mock = server.mock(|when, then| {
        when.method(POST).path("/threads/t1/messages");
        then.status(404);
    });

    let client = Client::new();
    let err = post_thread_message(&client, &server.base_url(), "t1", "hi", "user", "cli", None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("post message failed"));
}

#[tokio::test]
async fn post_thread_message_success_includes_openresponses_payload() {
    let server = MockServer::start();
    let _mock = server.mock(|when, then| {
        when.method(POST)
            .path("/threads/t1/messages")
            .body_contains(r#""content":"hi""#)
            .body_contains(r#""actor_id":"user""#)
            .body_contains(r#""origin":"cli""#)
            .body_contains(
                r#""openresponses":{"model":"openai/gpt-oss-20b","parallel_tool_calls":true}"#,
            );
        then.status(202)
            .header("content-type", "application/json")
            .body(r#"{"thread_id":"t1","message_id":"m1","session_id":"s1"}"#);
    });

    let client = Client::new();
    let response = post_thread_message(
        &client,
        &server.base_url(),
        "t1",
        "hi",
        "user",
        "cli",
        Some(serde_json::json!({
            "model": "openai/gpt-oss-20b",
            "parallel_tool_calls": true
        })),
    )
    .await
    .expect("post message");
    assert_eq!(response.session_id, "s1");
}

#[tokio::test]
async fn post_thread_message_success() {
    let server = MockServer::start();
    let _mock = server.mock(|when, then| {
        when.method(POST).path("/threads/t1/messages");
        then.status(202)
            .header("content-type", "application/json")
            .body(r#"{"thread_id":"t1","message_id":"m1","session_id":"s1"}"#);
    });

    let client = Client::new();
    let response =
        post_thread_message(&client, &server.base_url(), "t1", "hi", "user", "cli", None)
            .await
            .expect("post message");
    assert_eq!(response.session_id, "s1");
}

#[tokio::test]
async fn stream_events_reads_messages() {
    let server = MockServer::start();
    let payload = session_started_frame();
    let _mock = server.mock(|when, then| {
        when.method(GET).path("/sessions/s1/events");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(format!("data: {payload}\n\n"));
    });
    let client = Client::new();
    let result = stream_events(&client, &server.base_url(), "s1", OutputView::Raw).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn stream_events_renders_output_view() {
    let server = MockServer::start();
    let output_payload = serde_json::json!({
        "id": "e1",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 0,
        "type": "output_text_delta",
        "delta": "hi"
    })
    .to_string();
    let reasoning_payload = serde_json::json!({
        "id": "e2",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 1,
        "type": "provider_event",
        "provider": "openresponses",
        "status": "event",
        "event_name": "response.reasoning.delta",
        "data": {"type": "response.reasoning.delta", "delta": "step"},
        "raw": null,
        "errors": [],
        "response_errors": []
    })
    .to_string();
    let tool_payload = serde_json::json!({
        "id": "e3",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 2,
        "type": "provider_event",
        "provider": "openresponses",
        "status": "event",
        "event_name": "response.function_call_arguments.delta",
        "data": {"type": "response.function_call_arguments.delta", "delta": "{\"arg\":1}"},
        "raw": null,
        "errors": [],
        "response_errors": []
    })
    .to_string();
    let body =
        format!("data: {output_payload}\n\ndata: {reasoning_payload}\n\ndata: {tool_payload}\n\n");
    let _mock = server.mock(|when, then| {
        when.method(GET).path("/sessions/s1/events");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(body);
    });

    let client = Client::new();
    let url = format!("{}/sessions/s1/events", server.base_url());
    let mut stream = client.get(url).eventsource().unwrap();
    let mut buffer = Vec::new();
    stream_events_with_writer(&mut stream, OutputView::Output, &mut buffer)
        .await
        .unwrap();
    let rendered = String::from_utf8(buffer).expect("utf8");
    assert_eq!(rendered.trim_end(), "hi");
}

#[tokio::test]
async fn run_headless_with_interactive_flag() {
    let server = MockServer::start();
    let _ensure = server.mock(|when, then| {
        when.method(POST).path("/threads/ensure");
        then.status(200)
            .header("content-type", "application/json")
            .body(r#"{"thread_id":"t1"}"#);
    });
    let _post = server.mock(|when, then| {
        when.method(POST).path("/threads/t1/messages");
        then.status(202)
            .header("content-type", "application/json")
            .body(r#"{"thread_id":"t1","message_id":"m1","session_id":"abc"}"#);
    });
    let _events = server.mock(|when, then| {
        when.method(GET).path("/sessions/abc/events");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(format!("data: {}\n\n", session_started_frame()));
    });

    let cli = Cli {
        prompt: None,
        server: None,
        session: None,
        task: None,
        command: Some(Commands::Run {
            prompt: "hello".to_string(),
            server: Some(server.base_url()),
            provider: None,
            model: None,
            stateless_history: false,
            parallel_tool_calls: false,
            followup_user_message: None,
            headless: false,
            view: OutputView::Raw,
        }),
    };
    let result = run(cli).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn run_headless_remote() {
    let server = MockServer::start();
    let _ensure = server.mock(|when, then| {
        when.method(POST).path("/threads/ensure");
        then.status(200)
            .header("content-type", "application/json")
            .body(r#"{"thread_id":"t1"}"#);
    });
    let _post = server.mock(|when, then| {
        when.method(POST).path("/threads/t1/messages");
        then.status(202)
            .header("content-type", "application/json")
            .body(r#"{"thread_id":"t1","message_id":"m1","session_id":"abc"}"#);
    });
    let _events = server.mock(|when, then| {
        when.method(GET).path("/sessions/abc/events");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(format!("data: {}\n\n", session_started_frame()));
    });

    let cli = Cli {
        prompt: None,
        server: None,
        session: None,
        task: None,
        command: Some(Commands::Run {
            prompt: "hello".to_string(),
            server: Some(server.base_url()),
            provider: None,
            model: None,
            stateless_history: false,
            parallel_tool_calls: false,
            followup_user_message: None,
            headless: true,
            view: OutputView::Raw,
        }),
    };
    let result = run(cli).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn run_interactive_remote_smoke() {
    let server = MockServer::start();
    let _ensure = server.mock(|when, then| {
        when.method(POST).path("/threads/ensure");
        then.status(200)
            .header("content-type", "application/json")
            .body(r#"{"thread_id":"t1"}"#);
    });
    let _post = server.mock(|when, then| {
        when.method(POST).path("/threads/t1/messages");
        then.status(202)
            .header("content-type", "application/json")
            .body(r#"{"thread_id":"t1","message_id":"m1","session_id":"abc"}"#);
    });
    let _events = server.mock(|when, then| {
        when.method(GET).path("/sessions/abc/events");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(format!("data: {}\n\n", session_started_frame()));
    });

    let result = run_interactive_remote(
        "hello".to_string(),
        server.base_url(),
        OutputView::Raw,
        None,
    )
    .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn run_interactive_remote_forwards_openresponses_overrides() {
    let server = MockServer::start();
    let _ensure = server.mock(|when, then| {
        when.method(POST).path("/threads/ensure");
        then.status(200)
            .header("content-type", "application/json")
            .body(r#"{"thread_id":"t1"}"#);
    });
    let _post = server.mock(|when, then| {
        when.method(POST)
            .path("/threads/t1/messages")
            .body_contains(r#""openresponses":{"endpoint":"https://openrouter.ai/api/v1/responses","model":"nvidia/nemotron-3-nano-30b-a3b:free","stateless_history":true}"#);
        then.status(202)
            .header("content-type", "application/json")
            .body(r#"{"thread_id":"t1","message_id":"m1","session_id":"abc"}"#);
    });
    let _events = server.mock(|when, then| {
        when.method(GET).path("/sessions/abc/events");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(format!("data: {}\n\n", session_started_frame()));
    });

    let result = run_interactive_remote(
        "hello".to_string(),
        server.base_url(),
        OutputView::Raw,
        Some(serde_json::json!({
            "endpoint": "https://openrouter.ai/api/v1/responses",
            "model": "nvidia/nemotron-3-nano-30b-a3b:free",
            "stateless_history": true
        })),
    )
    .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn run_interactive_local_uses_env_paths() {
    with_clean_env_async(|| async {
        let tmp = std::env::temp_dir().join(format!("rip-cli-local-{}", std::process::id()));
        let data_dir = tmp.join("data");
        let workspace_dir = tmp.join("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("workspace");
        std::env::set_var("RIP_DATA_DIR", &data_dir);
        std::env::set_var("RIP_WORKSPACE_ROOT", &workspace_dir);

        let cli = Cli {
            prompt: None,
            server: None,
            session: None,
            task: None,
            command: Some(Commands::Run {
                prompt: "hello".to_string(),
                server: None,
                provider: None,
                model: None,
                stateless_history: false,
                parallel_tool_calls: false,
                followup_user_message: None,
                headless: false,
                view: OutputView::Raw,
            }),
        };
        let result = run(cli).await;
        assert!(result.is_ok());

        let _ = std::fs::remove_dir_all(&tmp);
    })
    .await;
}

#[tokio::test]
async fn run_accepts_openresponses_flags_with_server() {
    let server = MockServer::start();
    let _ensure = server.mock(|when, then| {
        when.method(POST).path("/threads/ensure");
        then.status(200)
            .header("content-type", "application/json")
            .body(r#"{"thread_id":"t1"}"#);
    });
    let _post = server.mock(|when, then| {
        when.method(POST)
            .path("/threads/t1/messages")
            .json_body_partial(r#"{"openresponses":{"endpoint":"https://api.openai.com/v1/responses","model":"gpt-5-nano-2025-08-07","stateless_history":true,"parallel_tool_calls":true,"followup_user_message":"compat"}}"#);
        then.status(202)
            .header("content-type", "application/json")
            .body(r#"{"thread_id":"t1","message_id":"m1","session_id":"abc"}"#);
    });
    let _events = server.mock(|when, then| {
        when.method(GET).path("/sessions/abc/events");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(format!("data: {}\n\n", session_started_frame()));
    });

    let cli = Cli {
        prompt: None,
        server: None,
        session: None,
        task: None,
        command: Some(Commands::Run {
            prompt: "hello".to_string(),
            server: Some(server.base_url()),
            provider: Some(Provider::Openai),
            model: Some("gpt-5-nano-2025-08-07".to_string()),
            stateless_history: true,
            parallel_tool_calls: true,
            followup_user_message: Some("compat".to_string()),
            headless: true,
            view: OutputView::Raw,
        }),
    };
    let result = run(cli).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn run_allows_model_override_without_provider() {
    let server = MockServer::start();
    let _ensure = server.mock(|when, then| {
        when.method(POST).path("/threads/ensure");
        then.status(200)
            .header("content-type", "application/json")
            .body(r#"{"thread_id":"t1"}"#);
    });
    let _post = server.mock(|when, then| {
        when.method(POST)
            .path("/threads/t1/messages")
            .json_body_partial(r#"{"openresponses":{"model":"gpt-5-nano-2025-08-07"}}"#);
        then.status(202)
            .header("content-type", "application/json")
            .body(r#"{"thread_id":"t1","message_id":"m1","session_id":"abc"}"#);
    });
    let _events = server.mock(|when, then| {
        when.method(GET).path("/sessions/abc/events");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(format!("data: {}\n\n", session_started_frame()));
    });

    let cli = Cli {
        prompt: None,
        server: None,
        session: None,
        task: None,
        command: Some(Commands::Run {
            prompt: "hello".to_string(),
            server: Some(server.base_url()),
            provider: None,
            model: Some("gpt-5-nano-2025-08-07".to_string()),
            stateless_history: false,
            parallel_tool_calls: false,
            followup_user_message: None,
            headless: true,
            view: OutputView::Raw,
        }),
    };
    let result = run(cli).await;
    assert!(result.is_ok());
}

#[test]
fn cli_parses_run() {
    let cli = Cli::parse_from(["rip", "run", "hello"]);
    assert!(cli.prompt.is_none());
    assert!(cli.server.is_none());
    assert!(cli.session.is_none());
    assert!(cli.task.is_none());
    match cli.command {
        Some(Commands::Run { prompt, server, .. }) => {
            assert_eq!(prompt, "hello");
            assert!(server.is_none());
        }
        Some(Commands::Serve) => panic!("expected run"),
        Some(Commands::Tasks { .. }) => panic!("expected run"),
        Some(Commands::Threads { .. }) => panic!("expected run"),
        Some(Commands::Config { .. }) => panic!("expected run"),
        None => panic!("expected run"),
    }
}

#[test]
fn cli_defaults_headless() {
    let cli = Cli::parse_from(["rip", "run", "hello"]);
    assert!(cli.prompt.is_none());
    assert!(cli.server.is_none());
    assert!(cli.session.is_none());
    assert!(cli.task.is_none());
    match cli.command {
        Some(Commands::Run {
            headless,
            view,
            server,
            ..
        }) => {
            assert!(headless);
            assert_eq!(view, OutputView::Output);
            assert!(server.is_none());
        }
        Some(Commands::Serve) => panic!("expected run"),
        Some(Commands::Tasks { .. }) => panic!("expected run"),
        Some(Commands::Threads { .. }) => panic!("expected run"),
        Some(Commands::Config { .. }) => panic!("expected run"),
        None => panic!("expected run"),
    }
}

#[test]
fn cli_parses_openresponses_flags() {
    let cli = Cli::parse_from([
        "rip",
        "run",
        "hello",
        "--provider",
        "openai",
        "--model",
        "gpt-5-nano-2025-08-07",
        "--stateless-history",
        "--parallel-tool-calls",
        "--followup-user-message",
        "continue",
    ]);
    assert!(cli.prompt.is_none());
    assert!(cli.server.is_none());
    assert!(cli.session.is_none());
    assert!(cli.task.is_none());
    match cli.command {
        Some(Commands::Run {
            provider,
            model,
            stateless_history,
            parallel_tool_calls,
            followup_user_message,
            ..
        }) => {
            assert_eq!(provider, Some(Provider::Openai));
            assert_eq!(model.as_deref(), Some("gpt-5-nano-2025-08-07"));
            assert!(stateless_history);
            assert!(parallel_tool_calls);
            assert_eq!(followup_user_message.as_deref(), Some("continue"));
        }
        Some(Commands::Serve) => panic!("expected run"),
        Some(Commands::Tasks { .. }) => panic!("expected run"),
        Some(Commands::Threads { .. }) => panic!("expected run"),
        Some(Commands::Config { .. }) => panic!("expected run"),
        None => panic!("expected run"),
    }
}

#[test]
fn cli_respects_server_flag() {
    let cli = Cli::parse_from(["rip", "run", "hello", "--server", "http://local"]);
    assert!(cli.prompt.is_none());
    assert!(cli.server.is_none());
    assert!(cli.session.is_none());
    assert!(cli.task.is_none());
    match cli.command {
        Some(Commands::Run { server, .. }) => {
            assert_eq!(server.as_deref(), Some("http://local"))
        }
        Some(Commands::Serve) => panic!("expected run"),
        Some(Commands::Tasks { .. }) => panic!("expected run"),
        Some(Commands::Threads { .. }) => panic!("expected run"),
        Some(Commands::Config { .. }) => panic!("expected run"),
        None => panic!("expected run"),
    }
}

#[tokio::test]
async fn local_run_emits_frames() {
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rip-cli-test-{}-{}", std::process::id(), unique));
    let data_dir = root.join("data");
    let workspace_dir = root.join("workspace");
    std::fs::create_dir_all(&workspace_dir).expect("workspace");

    let engine = ripd::SessionEngine::new(data_dir, workspace_dir, None).expect("engine");
    let mut buffer = Vec::new();
    run_local_with_engine(&engine, "hello".to_string(), OutputView::Raw, &mut buffer)
        .await
        .expect("run");
    let rendered = String::from_utf8(buffer).expect("utf8");
    assert!(rendered.contains("\"type\":\"session_started\""));
    assert!(rendered.contains("\"type\":\"session_ended\""));

    let log_path = root.join("data").join("events.jsonl");
    let log = std::fs::read_to_string(&log_path).expect("event log");
    let events: Vec<rip_kernel::Event> = log
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("event json"))
        .collect();

    let continuity_id = events
        .iter()
        .find_map(|event| match &event.kind {
            rip_kernel::EventKind::ContinuityCreated { .. } => Some(event.session_id.clone()),
            _ => None,
        })
        .expect("continuity created");
    let message_event = events
        .iter()
        .find(|event| {
            event.session_id == continuity_id
                && matches!(
                    &event.kind,
                    rip_kernel::EventKind::ContinuityMessageAppended { content, .. }
                        if content == "hello"
                )
        })
        .expect("continuity message");
    let message_id = message_event.id.clone();
    let run_event = events
        .iter()
        .find(|event| {
            event.session_id == continuity_id
                && matches!(
                    &event.kind,
                    rip_kernel::EventKind::ContinuityRunSpawned { message_id: mid, .. }
                        if mid == &message_id
                )
        })
        .expect("continuity run spawned");
    let rip_kernel::EventKind::ContinuityRunSpawned { run_session_id, .. } = &run_event.kind else {
        unreachable!("continuity run spawned match")
    };

    let session_events: Vec<&rip_kernel::EventKind> = events
        .iter()
        .filter(|event| event.session_id == *run_session_id)
        .map(|event| &event.kind)
        .collect();
    assert!(
        session_events
            .iter()
            .any(|kind| matches!(kind, rip_kernel::EventKind::SessionStarted { .. })),
        "expected linked run to emit session_started"
    );
    assert!(
        session_events
            .iter()
            .any(|kind| matches!(kind, rip_kernel::EventKind::SessionEnded { .. })),
        "expected linked run to emit session_ended"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn renders_raw_payload() {
    let mut buffer = Vec::new();
    let mut state = OutputState::default();
    let payload = session_started_frame();
    render_message(OutputView::Raw, payload.as_str(), &mut buffer, &mut state).expect("render");
    let rendered = String::from_utf8(buffer).expect("utf8");
    assert_eq!(rendered.trim_end(), payload);
}

#[test]
fn raw_view_rejects_invalid_frame() {
    let mut buffer = Vec::new();
    let mut state = OutputState::default();
    let payload = "{\"type\":\"session_started\"}";
    let err = render_message(OutputView::Raw, payload, &mut buffer, &mut state).unwrap_err();
    assert!(err.to_string().contains("invalid event frame"));
}

#[test]
fn renders_output_deltas() {
    let mut buffer = Vec::new();
    let mut state = OutputState::default();
    let payload = serde_json::json!({
        "id": "e1",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 0,
        "type": "output_text_delta",
        "delta": "hi"
    })
    .to_string();
    render_message(OutputView::Output, &payload, &mut buffer, &mut state).expect("render");
    let rendered = String::from_utf8(buffer).expect("utf8");
    assert_eq!(rendered.trim_end(), "hi");
}

#[test]
fn renders_tool_stdout_in_output_view() {
    let mut buffer = Vec::new();
    let mut state = OutputState::default();
    let payload = serde_json::json!({
        "id": "e1",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 0,
        "type": "tool_stdout",
        "tool_id": "t1",
        "chunk": "a.txt"
    })
    .to_string();
    render_message(OutputView::Output, &payload, &mut buffer, &mut state).expect("render");
    let end_payload = serde_json::json!({
        "id": "e2",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 1,
        "type": "session_ended",
        "reason": "completed"
    })
    .to_string();
    render_message(OutputView::Output, &end_payload, &mut buffer, &mut state).expect("render");
    let rendered = String::from_utf8(buffer).expect("utf8");
    assert_eq!(rendered.trim_end(), "a.txt");
}

#[test]
fn renders_trailing_newline_when_missing() {
    let mut buffer = Vec::new();
    let mut state = OutputState::default();
    let payload = serde_json::json!({
        "id": "e1",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 0,
        "type": "output_text_delta",
        "delta": "hi"
    })
    .to_string();
    render_message(OutputView::Output, &payload, &mut buffer, &mut state).expect("render");
    let end_payload = serde_json::json!({
        "id": "e2",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 1,
        "type": "session_ended",
        "reason": "completed"
    })
    .to_string();
    render_message(OutputView::Output, &end_payload, &mut buffer, &mut state).expect("render");
    let rendered = String::from_utf8(buffer).expect("utf8");
    assert!(rendered.ends_with("hi\n"));
}

#[test]
fn renders_reasoning_and_tool_deltas() {
    let mut buffer = Vec::new();
    let mut state = OutputState::default();
    let reasoning_payload = serde_json::json!({
        "id": "e2",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 1,
        "type": "provider_event",
        "provider": "openresponses",
        "status": "event",
        "event_name": "response.reasoning.delta",
        "data": {"type": "response.reasoning.delta", "delta": "step"},
        "raw": null,
        "errors": [],
        "response_errors": []
    })
    .to_string();
    render_message(
        OutputView::Output,
        &reasoning_payload,
        &mut buffer,
        &mut state,
    )
    .expect("render");

    let tool_payload = serde_json::json!({
        "id": "e3",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 2,
        "type": "provider_event",
        "provider": "openresponses",
        "status": "event",
        "event_name": "response.function_call_arguments.delta",
        "data": {"type": "response.function_call_arguments.delta", "delta": "{\"arg\":1}"},
        "raw": null,
        "errors": [],
        "response_errors": []
    })
    .to_string();
    render_message(OutputView::Output, &tool_payload, &mut buffer, &mut state).expect("render");
    let end_payload = serde_json::json!({
        "id": "e4",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 2,
        "type": "session_ended",
        "reason": "completed"
    })
    .to_string();
    render_message(OutputView::Output, &end_payload, &mut buffer, &mut state).expect("render");

    let rendered = String::from_utf8(buffer).expect("utf8");
    assert!(rendered.trim().is_empty());
}

#[test]
fn cli_respects_headless_flag() {
    let cli = Cli::parse_from(["rip", "run", "hello", "--headless", "false"]);
    assert!(cli.prompt.is_none());
    assert!(cli.server.is_none());
    assert!(cli.session.is_none());
    assert!(cli.task.is_none());
    match cli.command {
        Some(Commands::Run { headless, .. }) => assert!(!headless),
        Some(Commands::Serve) => panic!("expected run"),
        Some(Commands::Tasks { .. }) => panic!("expected run"),
        Some(Commands::Threads { .. }) => panic!("expected run"),
        Some(Commands::Config { .. }) => panic!("expected run"),
        None => panic!("expected run"),
    }
}

#[test]
fn cli_parses_serve() {
    let cli = Cli::parse_from(["rip", "serve"]);
    assert!(cli.prompt.is_none());
    assert!(cli.server.is_none());
    assert!(cli.session.is_none());
    assert!(cli.task.is_none());
    match cli.command {
        Some(Commands::Serve) => {}
        Some(Commands::Run { .. }) => panic!("expected serve"),
        Some(Commands::Tasks { .. }) => panic!("expected serve"),
        Some(Commands::Threads { .. }) => panic!("expected serve"),
        Some(Commands::Config { .. }) => panic!("expected serve"),
        None => panic!("expected serve"),
    }
}

#[test]
fn cli_parses_threads_ensure_local() {
    let cli = Cli::parse_from(["rip", "threads", "ensure"]);
    match cli.command {
        Some(Commands::Threads { server, command }) => {
            assert!(server.is_none());
            assert!(matches!(command, threads::ThreadsCommand::Ensure));
        }
        _ => panic!("expected threads ensure"),
    }
}

#[test]
fn cli_parses_threads_list_remote() {
    let cli = Cli::parse_from(["rip", "threads", "--server", "http://local", "list"]);
    match cli.command {
        Some(Commands::Threads { server, command }) => {
            assert_eq!(server.as_deref(), Some("http://local"));
            assert!(matches!(command, threads::ThreadsCommand::List));
        }
        _ => panic!("expected threads list"),
    }
}

#[test]
fn cli_parses_default_interactive_prompt() {
    let cli = Cli::parse_from(["rip", "hello"]);
    assert_eq!(cli.prompt.as_deref(), Some("hello"));
    assert!(cli.server.is_none());
    assert!(cli.session.is_none());
    assert!(cli.task.is_none());
    assert!(cli.command.is_none());
}

#[test]
fn cli_parses_tui_attach_flags() {
    let cli = Cli::parse_from(["rip", "--server", "http://local", "--session", "abc"]);
    assert!(cli.prompt.is_none());
    assert_eq!(cli.server.as_deref(), Some("http://local"));
    assert_eq!(cli.session.as_deref(), Some("abc"));
    assert!(cli.task.is_none());
    assert!(cli.command.is_none());
}

#[tokio::test]
async fn tui_attach_stream_renders_like_basic_snapshot() {
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;
    use rip_tui::{render, RenderMode, TuiState};

    fn buffer_to_string(buffer: &Buffer) -> String {
        let mut out = String::new();
        for y in 0..buffer.area.height {
            let mut line = String::new();
            for x in 0..buffer.area.width {
                let symbol = buffer.cell((x, y)).map(|cell| cell.symbol()).unwrap_or(" ");
                line.push_str(symbol);
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }

    fn render_to_string(width: u16, height: u16, state: &TuiState) -> String {
        use ratatui_textarea::TextArea;
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        let input = TextArea::default();
        terminal
            .draw(|f| render(f, state, RenderMode::Json, &input))
            .expect("draw");
        buffer_to_string(terminal.backend().buffer())
    }

    let fixture = std::fs::read_to_string(fixture_path("../../fixtures/server/attach_basic.sse"))
        .expect("fixture");

    let server = MockServer::start();
    let _events = server.mock(|when, then| {
        when.method(GET).path("/sessions/abc/events");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(fixture.clone());
    });

    let client = Client::new();
    let url = format!("{}/sessions/abc/events", server.base_url());
    let mut stream = client.get(url).eventsource().expect("eventsource");

    let mut state = TuiState::new(10_000);
    while let Some(next) = stream.next().await {
        match next {
            Ok(Event::Open) => {}
            Ok(Event::Message(msg)) => {
                let frame: FrameEvent = serde_json::from_str(&msg.data).expect("frame json");
                state.update(frame);
            }
            Err(EventSourceError::StreamEnded) => break,
            Err(err) => panic!("stream error: {err}"),
        }
    }

    let expected_80 =
        std::fs::read_to_string(fixture_path("../rip-tui/tests/snapshots/basic_80x24.txt"))
            .expect("snapshot");
    assert_eq!(expected_80, render_to_string(80, 24, &state));

    let expected_60 =
        std::fs::read_to_string(fixture_path("../rip-tui/tests/snapshots/basic_60x20.txt"))
            .expect("snapshot");
    assert_eq!(expected_60, render_to_string(60, 20, &state));
}

#[tokio::test]
async fn stream_events_stops_on_stream_end() {
    let mut stream = futures_util::stream::iter(vec![Err(EventSourceError::StreamEnded)]);
    let mut buffer = Vec::new();
    let result = stream_events_with_writer(&mut stream, OutputView::Raw, &mut buffer).await;
    assert!(result.is_ok());
}

#[test]
fn openresponses_overrides_from_env_none_without_endpoint() {
    with_clean_env(|| {
        assert!(openresponses_overrides_from_env().is_none());
    });
}

#[test]
fn parse_env_bool_handles_truthy_and_falsey_values() {
    with_clean_env(|| {
        std::env::set_var("RIP_TEST_BOOL", "yes");
        assert_eq!(parse_env_bool("RIP_TEST_BOOL"), Some(true));

        std::env::set_var("RIP_TEST_BOOL", "off");
        assert_eq!(parse_env_bool("RIP_TEST_BOOL"), Some(false));

        std::env::remove_var("RIP_TEST_BOOL");
        assert_eq!(parse_env_bool("RIP_TEST_BOOL"), None);
    });
}

#[test]
fn openresponses_overrides_from_env_reads_vars() {
    with_clean_env(|| {
        std::env::set_var(
            "RIP_OPENRESPONSES_ENDPOINT",
            "https://openrouter.ai/api/v1/responses",
        );
        std::env::set_var("RIP_OPENRESPONSES_MODEL", "openai/gpt-oss-20b");
        std::env::set_var("RIP_OPENRESPONSES_STATELESS_HISTORY", "yes");
        std::env::set_var("RIP_OPENRESPONSES_PARALLEL_TOOL_CALLS", "true");
        std::env::set_var("RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE", "continue");

        let overrides = openresponses_overrides_from_env().expect("overrides");
        let expected = serde_json::json!({
            "endpoint": "https://openrouter.ai/api/v1/responses",
            "model": "openai/gpt-oss-20b",
            "stateless_history": true,
            "parallel_tool_calls": true,
            "followup_user_message": "continue",
        });
        assert_eq!(overrides, expected);
    });
}

#[test]
fn base64_encode_handles_padding_boundaries() {
    assert_eq!(base64_encode(b""), "");
    assert_eq!(base64_encode(b"f"), "Zg==");
    assert_eq!(base64_encode(b"fo"), "Zm8=");
    assert_eq!(base64_encode(b"foo"), "Zm9v");
    assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
}

#[test]
fn renders_provider_errors_when_no_output() {
    let mut buffer = Vec::new();
    let mut state = OutputState::default();
    let payload = serde_json::json!({
        "id": "e1",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 0,
        "type": "provider_event",
        "provider": "openresponses",
        "status": "invalid_json",
        "event_name": null,
        "data": null,
        "raw": "raw",
        "errors": ["bad json"],
        "response_errors": ["schema"]
    })
    .to_string();
    render_message(OutputView::Output, &payload, &mut buffer, &mut state).expect("render");
    let end_payload = serde_json::json!({
        "id": "e2",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 1,
        "type": "session_ended",
        "reason": "completed"
    })
    .to_string();
    render_message(OutputView::Output, &end_payload, &mut buffer, &mut state).expect("render");
    let rendered = String::from_utf8(buffer).expect("utf8");
    assert!(rendered.contains("provider_errors: bad json"));
    assert!(rendered.contains("provider_response_errors: schema"));
    assert!(rendered.contains("provider_invalid_json: raw"));
}

#[test]
fn renders_tool_stderr_with_newline() {
    let mut buffer = Vec::new();
    let mut state = OutputState::default();
    let stdout_payload = serde_json::json!({
        "id": "e1",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 0,
        "type": "tool_stdout",
        "tool_id": "t1",
        "chunk": "a.txt"
    })
    .to_string();
    render_message(OutputView::Output, &stdout_payload, &mut buffer, &mut state).expect("render");
    let stderr_payload = serde_json::json!({
        "id": "e2",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 1,
        "type": "tool_stderr",
        "tool_id": "t1",
        "chunk": "boom"
    })
    .to_string();
    render_message(OutputView::Output, &stderr_payload, &mut buffer, &mut state).expect("render");
    let end_payload = serde_json::json!({
        "id": "e3",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 2,
        "type": "session_ended",
        "reason": "completed"
    })
    .to_string();
    render_message(OutputView::Output, &end_payload, &mut buffer, &mut state).expect("render");
    let rendered = String::from_utf8(buffer).expect("utf8");
    assert!(rendered.contains("a.txt"));
    assert!(rendered.contains("\nstderr: boom"));
}

#[test]
fn renders_tool_failed_when_no_output() {
    let mut buffer = Vec::new();
    let mut state = OutputState::default();
    let payload = serde_json::json!({
        "id": "e1",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 0,
        "type": "tool_failed",
        "tool_id": "t1",
        "error": "boom"
    })
    .to_string();
    render_message(OutputView::Output, &payload, &mut buffer, &mut state).expect("render");
    let end_payload = serde_json::json!({
        "id": "e2",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 1,
        "type": "session_ended",
        "reason": "completed"
    })
    .to_string();
    render_message(OutputView::Output, &end_payload, &mut buffer, &mut state).expect("render");
    let rendered = String::from_utf8(buffer).expect("utf8");
    assert!(rendered.contains("tool_failed: boom"));
}

#[test]
fn trailing_newline_is_preserved() {
    let mut buffer = Vec::new();
    let mut state = OutputState::default();
    let payload = serde_json::json!({
        "id": "e1",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 0,
        "type": "output_text_delta",
        "delta": "hi\n"
    })
    .to_string();
    render_message(OutputView::Output, &payload, &mut buffer, &mut state).expect("render");
    let end_payload = serde_json::json!({
        "id": "e2",
        "session_id": "s1",
        "timestamp_ms": 0,
        "seq": 1,
        "type": "session_ended",
        "reason": "completed"
    })
    .to_string();
    render_message(OutputView::Output, &end_payload, &mut buffer, &mut state).expect("render");
    let rendered = String::from_utf8(buffer).expect("utf8");
    assert!(rendered.ends_with("hi\n"));
}
