use std::io::{self, Write};

use clap::{Parser, Subcommand, ValueEnum};
use futures_util::StreamExt;
use reqwest::Client;
use reqwest_eventsource::{Error as EventSourceError, Event, RequestBuilderExt};
use rip_kernel::{Event as FrameEvent, EventKind};
use serde::Deserialize;
use tokio::sync::broadcast;

mod fullscreen;

#[derive(Parser)]
#[command(name = "rip")]
#[command(about = "RIP CLI", long_about = None)]
struct Cli {
    /// Optional initial prompt for the interactive terminal UI (when no subcommand is used).
    prompt: Option<String>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Run {
        prompt: String,
        #[arg(long)]
        server: Option<String>,
        #[arg(long, value_enum)]
        provider: Option<Provider>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        stateless_history: bool,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        parallel_tool_calls: bool,
        #[arg(long)]
        followup_user_message: Option<String>,
        #[arg(
            long,
            default_value_t = true,
            value_parser = clap::value_parser!(bool),
            action = clap::ArgAction::Set
        )]
        headless: bool,
        #[arg(long, value_enum, default_value_t = OutputView::Output)]
        view: OutputView,
    },
    Serve,
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
enum OutputView {
    Raw,
    Output,
}

#[derive(Default)]
struct OutputState {
    saw_output: bool,
    trailing_newline: bool,
    tool_stdout: String,
    tool_stderr: String,
    tool_failed: Vec<String>,
    provider_errors: Vec<String>,
    provider_response_errors: Vec<String>,
    provider_invalid_json: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Provider {
    Openai,
    Openrouter,
}

#[derive(Deserialize)]
struct SessionCreated {
    session_id: String,
}

#[cfg(not(test))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run(Cli::parse()).await
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        None => {
            fullscreen::run_fullscreen_tui(cli.prompt).await?;
        }
        Some(Commands::Run {
            prompt,
            server,
            provider,
            model,
            stateless_history,
            parallel_tool_calls,
            followup_user_message,
            headless,
            view,
        }) => {
            let has_openresponses_flags = provider.is_some()
                || model.is_some()
                || stateless_history
                || parallel_tool_calls
                || followup_user_message.is_some();
            if server.is_some() && has_openresponses_flags {
                anyhow::bail!(
                    "openresponses flags are only supported for local runs; configure the server instead"
                );
            }
            if has_openresponses_flags {
                let provider = provider.ok_or_else(|| {
                    anyhow::anyhow!("--provider is required when using openresponses flags")
                })?;
                apply_openresponses_env(
                    provider,
                    model,
                    stateless_history,
                    parallel_tool_calls,
                    followup_user_message,
                )?;
            }
            if let Some(server) = server {
                if headless {
                    run_headless_remote(prompt, server, view).await?;
                } else {
                    run_interactive_remote(prompt, server, view).await?;
                }
            } else if headless {
                run_headless_local(prompt, view).await?;
            } else {
                run_interactive_local(prompt, view).await?;
            }
        }
        Some(Commands::Serve) => {
            ripd::serve_default().await;
        }
    }

    Ok(())
}

fn apply_openresponses_env(
    provider: Provider,
    model: Option<String>,
    stateless_history: bool,
    parallel_tool_calls: bool,
    followup_user_message: Option<String>,
) -> anyhow::Result<()> {
    let endpoint = match provider {
        Provider::Openai => "https://api.openai.com/v1/responses",
        Provider::Openrouter => "https://openrouter.ai/api/v1/responses",
    };
    std::env::set_var("RIP_OPENRESPONSES_ENDPOINT", endpoint);

    if let Some(model) = model {
        std::env::set_var("RIP_OPENRESPONSES_MODEL", model);
    }

    if stateless_history {
        std::env::set_var("RIP_OPENRESPONSES_STATELESS_HISTORY", "1");
    }
    if parallel_tool_calls {
        std::env::set_var("RIP_OPENRESPONSES_PARALLEL_TOOL_CALLS", "1");
    }

    if let Some(message) = followup_user_message {
        std::env::set_var("RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE", message);
    }

    let provider_key = match provider {
        Provider::Openai => std::env::var("OPENAI_API_KEY").ok(),
        Provider::Openrouter => std::env::var("OPENROUTER_API_KEY").ok(),
    };
    let api_key = provider_key.or_else(|| std::env::var("RIP_OPENRESPONSES_API_KEY").ok());
    if let Some(api_key) = api_key {
        std::env::set_var("RIP_OPENRESPONSES_API_KEY", api_key);
        return Ok(());
    }

    let missing_hint = match provider {
        Provider::Openai => "OPENAI_API_KEY",
        Provider::Openrouter => "OPENROUTER_API_KEY",
    };
    anyhow::bail!("missing API key: set {missing_hint} or RIP_OPENRESPONSES_API_KEY")
}

async fn run_headless_remote(
    prompt: String,
    server: String,
    view: OutputView,
) -> anyhow::Result<()> {
    let client = Client::new();
    let session_id = create_session(&client, &server).await?;
    send_input(&client, &server, &session_id, &prompt).await?;
    stream_events(&client, &server, &session_id, view).await?;
    Ok(())
}

async fn run_interactive_remote(
    prompt: String,
    server: String,
    view: OutputView,
) -> anyhow::Result<()> {
    let client = Client::new();
    let session_id = create_session(&client, &server).await?;
    send_input(&client, &server, &session_id, &prompt).await?;
    stream_events(&client, &server, &session_id, view).await?;
    Ok(())
}

async fn run_headless_local(prompt: String, view: OutputView) -> anyhow::Result<()> {
    let engine =
        ripd::SessionEngine::new_default().map_err(|err| anyhow::anyhow!("engine init: {err}"))?;
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    run_local_with_engine(&engine, prompt, view, &mut handle).await
}

async fn run_interactive_local(prompt: String, view: OutputView) -> anyhow::Result<()> {
    run_headless_local(prompt, view).await
}

async fn create_session(client: &Client, server: &str) -> anyhow::Result<String> {
    let url = format!("{server}/sessions");
    let response = client.post(url).send().await?;
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("create session failed: {status}");
    }
    let payload: SessionCreated = response.json().await?;
    Ok(payload.session_id)
}

async fn send_input(
    client: &Client,
    server: &str,
    session_id: &str,
    input: &str,
) -> anyhow::Result<()> {
    let url = format!("{server}/sessions/{session_id}/input");
    let response = client
        .post(url)
        .json(&serde_json::json!({ "input": input }))
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("send input failed: {status}");
    }
    Ok(())
}

async fn stream_events(
    client: &Client,
    server: &str,
    session_id: &str,
    view: OutputView,
) -> anyhow::Result<()> {
    let url = format!("{server}/sessions/{session_id}/events");
    let mut stream = client.get(url).eventsource()?;
    let stdout = io::stdout();
    let mut handle = stdout.lock();

    stream_events_with_writer(&mut stream, view, &mut handle).await
}

async fn stream_events_with_writer(
    stream: &mut (impl futures_util::Stream<Item = Result<Event, EventSourceError>> + Unpin),
    view: OutputView,
    out: &mut dyn Write,
) -> anyhow::Result<()> {
    let mut state = OutputState::default();
    while let Some(next) = stream.next().await {
        match next {
            Ok(Event::Open) => {}
            Ok(Event::Message(msg)) => {
                let should_stop = render_message(view, &msg.data, out, &mut state)?;
                if should_stop {
                    break;
                }
            }
            Err(EventSourceError::StreamEnded) => break,
            Err(err) => return Err(err.into()),
        }
    }

    Ok(())
}

async fn run_local_with_engine(
    engine: &ripd::SessionEngine,
    prompt: String,
    view: OutputView,
    out: &mut dyn Write,
) -> anyhow::Result<()> {
    let handle = engine.create_session();
    let mut receiver = handle.subscribe();
    engine.spawn_session(handle, prompt);
    stream_events_from_receiver(&mut receiver, view, out).await
}

async fn stream_events_from_receiver(
    receiver: &mut broadcast::Receiver<FrameEvent>,
    view: OutputView,
    out: &mut dyn Write,
) -> anyhow::Result<()> {
    let mut state = OutputState::default();
    loop {
        match receiver.recv().await {
            Ok(frame) => {
                let payload = serde_json::to_string(&frame)
                    .map_err(|err| anyhow::anyhow!("event frame json: {err}"))?;
                let should_stop = render_message(view, &payload, out, &mut state)?;
                if should_stop {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Closed) => break,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
        }
    }
    Ok(())
}

fn render_message(
    view: OutputView,
    payload: &str,
    out: &mut dyn Write,
    state: &mut OutputState,
) -> anyhow::Result<bool> {
    let frame: FrameEvent = serde_json::from_str(payload)
        .map_err(|err| anyhow::anyhow!("invalid event frame: {err}"))?;
    let should_stop = matches!(frame.kind, EventKind::SessionEnded { .. });

    match view {
        OutputView::Raw => {
            writeln!(out, "{payload}")?;
            out.flush()?;
        }
        OutputView::Output => {
            match &frame.kind {
                EventKind::OutputTextDelta { delta } => {
                    state.saw_output = true;
                    write!(out, "{delta}")?;
                    if let Some(last) = delta.chars().last() {
                        state.trailing_newline = last == '\n';
                    }
                }
                EventKind::ToolStdout { chunk, .. } => {
                    if !state.saw_output {
                        state.tool_stdout.push_str(chunk);
                    }
                }
                EventKind::ToolStderr { chunk, .. } => {
                    if !state.saw_output {
                        state.tool_stderr.push_str(chunk);
                    }
                }
                EventKind::ToolFailed { error, .. } => {
                    if !state.saw_output {
                        state.tool_failed.push(error.clone());
                    }
                }
                EventKind::ProviderEvent {
                    status,
                    errors,
                    response_errors,
                    raw,
                    ..
                } => {
                    if !state.saw_output {
                        if !errors.is_empty() {
                            state.provider_errors.extend(errors.iter().cloned());
                        }
                        if !response_errors.is_empty() {
                            state
                                .provider_response_errors
                                .extend(response_errors.iter().cloned());
                        }
                        if *status == rip_kernel::ProviderEventStatus::InvalidJson {
                            if let Some(raw) = raw.as_deref() {
                                state.provider_invalid_json.push(raw.to_string());
                            }
                        }
                    }
                }
                _ => {}
            }

            if should_stop {
                if !state.saw_output {
                    if !state.tool_stdout.is_empty() {
                        write!(out, "{}", state.tool_stdout)?;
                    }
                    if !state.tool_stderr.is_empty() {
                        if !state.tool_stdout.ends_with('\n') {
                            writeln!(out)?;
                        }
                        write!(out, "stderr: {}", state.tool_stderr)?;
                    }
                    for error in &state.tool_failed {
                        writeln!(out, "tool_failed: {error}")?;
                    }
                    if !state.provider_errors.is_empty() {
                        writeln!(out, "provider_errors: {}", state.provider_errors.join("; "))?;
                    }
                    if !state.provider_response_errors.is_empty() {
                        writeln!(
                            out,
                            "provider_response_errors: {}",
                            state.provider_response_errors.join("; ")
                        )?;
                    }
                    for raw in &state.provider_invalid_json {
                        writeln!(out, "provider_invalid_json: {raw}")?;
                    }
                } else if !state.trailing_newline {
                    writeln!(out)?;
                }
            }
            out.flush()?;
        }
    }
    Ok(should_stop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::Method::{GET, POST};
    use httpmock::MockServer;
    use std::sync::{Mutex, OnceLock};

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
    async fn create_session_success() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/sessions");
            then.status(201)
                .header("content-type", "application/json")
                .body(r#"{"session_id":"abc"}"#);
        });

        let client = Client::new();
        let session_id = create_session(&client, &server.base_url()).await.unwrap();
        assert_eq!(session_id, "abc");
        mock.assert();
    }

    #[tokio::test]
    async fn create_session_failure() {
        let server = MockServer::start();
        let _mock = server.mock(|when, then| {
            when.method(POST).path("/sessions");
            then.status(500);
        });

        let client = Client::new();
        let err = create_session(&client, &server.base_url())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("create session failed"));
    }

    #[tokio::test]
    async fn send_input_failure() {
        let server = MockServer::start();
        let _mock = server.mock(|when, then| {
            when.method(POST).path("/sessions/s1/input");
            then.status(400);
        });

        let client = Client::new();
        let err = send_input(&client, &server.base_url(), "s1", "hi")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("send input failed"));
    }

    #[tokio::test]
    async fn send_input_success() {
        let server = MockServer::start();
        let _mock = server.mock(|when, then| {
            when.method(POST).path("/sessions/s1/input");
            then.status(202);
        });

        let client = Client::new();
        send_input(&client, &server.base_url(), "s1", "hi")
            .await
            .expect("send");
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
        let body = format!(
            "data: {output_payload}\n\ndata: {reasoning_payload}\n\ndata: {tool_payload}\n\n"
        );
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
        let _create = server.mock(|when, then| {
            when.method(POST).path("/sessions");
            then.status(201)
                .header("content-type", "application/json")
                .body(r#"{"session_id":"abc"}"#);
        });
        let _input = server.mock(|when, then| {
            when.method(POST).path("/sessions/abc/input");
            then.status(202);
        });
        let _events = server.mock(|when, then| {
            when.method(GET).path("/sessions/abc/events");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(format!("data: {}\n\n", session_started_frame()));
        });

        let cli = Cli {
            prompt: None,
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
        let _create = server.mock(|when, then| {
            when.method(POST).path("/sessions");
            then.status(201)
                .header("content-type", "application/json")
                .body(r#"{"session_id":"abc"}"#);
        });
        let _input = server.mock(|when, then| {
            when.method(POST).path("/sessions/abc/input");
            then.status(202);
        });
        let _events = server.mock(|when, then| {
            when.method(GET).path("/sessions/abc/events");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(format!("data: {}\n\n", session_started_frame()));
        });

        let cli = Cli {
            prompt: None,
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
    async fn run_rejects_openresponses_flags_with_server() {
        let cli = Cli {
            prompt: None,
            command: Some(Commands::Run {
                prompt: "hello".to_string(),
                server: Some("http://local".to_string()),
                provider: Some(Provider::Openai),
                model: None,
                stateless_history: false,
                parallel_tool_calls: false,
                followup_user_message: None,
                headless: true,
                view: OutputView::Output,
            }),
        };
        let err = run(cli).await.unwrap_err();
        assert!(err
            .to_string()
            .contains("openresponses flags are only supported for local runs"));
    }

    #[tokio::test]
    async fn run_requires_provider_when_openresponses_flags_set() {
        let cli = Cli {
            prompt: None,
            command: Some(Commands::Run {
                prompt: "hello".to_string(),
                server: None,
                provider: None,
                model: Some("gpt-5-nano-2025-08-07".to_string()),
                stateless_history: false,
                parallel_tool_calls: false,
                followup_user_message: None,
                headless: true,
                view: OutputView::Output,
            }),
        };
        let err = run(cli).await.unwrap_err();
        assert!(err
            .to_string()
            .contains("--provider is required when using openresponses flags"));
    }

    #[test]
    fn cli_parses_run() {
        let cli = Cli::parse_from(["rip", "run", "hello"]);
        assert!(cli.prompt.is_none());
        match cli.command {
            Some(Commands::Run { prompt, server, .. }) => {
                assert_eq!(prompt, "hello");
                assert!(server.is_none());
            }
            Some(Commands::Serve) => panic!("expected run"),
            None => panic!("expected run"),
        }
    }

    #[test]
    fn cli_defaults_headless() {
        let cli = Cli::parse_from(["rip", "run", "hello"]);
        assert!(cli.prompt.is_none());
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
            None => panic!("expected run"),
        }
    }

    #[test]
    fn cli_respects_server_flag() {
        let cli = Cli::parse_from(["rip", "run", "hello", "--server", "http://local"]);
        assert!(cli.prompt.is_none());
        match cli.command {
            Some(Commands::Run { server, .. }) => {
                assert_eq!(server.as_deref(), Some("http://local"))
            }
            Some(Commands::Serve) => panic!("expected run"),
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
        let root =
            std::env::temp_dir().join(format!("rip-cli-test-{}-{}", std::process::id(), unique));
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
        match cli.command {
            Some(Commands::Run { headless, .. }) => assert!(!headless),
            Some(Commands::Serve) => panic!("expected run"),
            None => panic!("expected run"),
        }
    }

    #[test]
    fn cli_parses_serve() {
        let cli = Cli::parse_from(["rip", "serve"]);
        assert!(cli.prompt.is_none());
        match cli.command {
            Some(Commands::Serve) => {}
            Some(Commands::Run { .. }) => panic!("expected serve"),
            None => panic!("expected serve"),
        }
    }

    #[test]
    fn cli_parses_default_interactive_prompt() {
        let cli = Cli::parse_from(["rip", "hello"]);
        assert_eq!(cli.prompt.as_deref(), Some("hello"));
        assert!(cli.command.is_none());
    }

    #[tokio::test]
    async fn stream_events_stops_on_stream_end() {
        let mut stream = futures_util::stream::iter(vec![Err(EventSourceError::StreamEnded)]);
        let mut buffer = Vec::new();
        let result = stream_events_with_writer(&mut stream, OutputView::Raw, &mut buffer).await;
        assert!(result.is_ok());
    }

    #[test]
    fn apply_openresponses_env_sets_openai_vars() {
        with_clean_env(|| {
            std::env::set_var("OPENAI_API_KEY", "test-openai");
            apply_openresponses_env(
                Provider::Openai,
                Some("gpt-5-nano-2025-08-07".to_string()),
                true,
                true,
                Some("continue".to_string()),
            )
            .expect("env");

            assert_eq!(
                std::env::var("RIP_OPENRESPONSES_ENDPOINT").ok().as_deref(),
                Some("https://api.openai.com/v1/responses")
            );
            assert_eq!(
                std::env::var("RIP_OPENRESPONSES_MODEL").ok().as_deref(),
                Some("gpt-5-nano-2025-08-07")
            );
            assert_eq!(
                std::env::var("RIP_OPENRESPONSES_STATELESS_HISTORY")
                    .ok()
                    .as_deref(),
                Some("1")
            );
            assert_eq!(
                std::env::var("RIP_OPENRESPONSES_PARALLEL_TOOL_CALLS")
                    .ok()
                    .as_deref(),
                Some("1")
            );
            assert_eq!(
                std::env::var("RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE")
                    .ok()
                    .as_deref(),
                Some("continue")
            );
            assert_eq!(
                std::env::var("RIP_OPENRESPONSES_API_KEY").ok().as_deref(),
                Some("test-openai")
            );
        });
    }

    #[test]
    fn apply_openresponses_env_sets_openrouter_vars() {
        with_clean_env(|| {
            std::env::set_var("OPENROUTER_API_KEY", "test-openrouter");
            apply_openresponses_env(Provider::Openrouter, None, false, false, None).expect("env");

            assert_eq!(
                std::env::var("RIP_OPENRESPONSES_ENDPOINT").ok().as_deref(),
                Some("https://openrouter.ai/api/v1/responses")
            );
            assert_eq!(
                std::env::var("RIP_OPENRESPONSES_API_KEY").ok().as_deref(),
                Some("test-openrouter")
            );
        });
    }

    #[test]
    fn apply_openresponses_env_uses_fallback_key() {
        with_clean_env(|| {
            std::env::set_var("RIP_OPENRESPONSES_API_KEY", "fallback");
            apply_openresponses_env(Provider::Openai, None, false, false, None).expect("env");
            assert_eq!(
                std::env::var("RIP_OPENRESPONSES_API_KEY").ok().as_deref(),
                Some("fallback")
            );
        });
    }

    #[test]
    fn apply_openresponses_env_missing_key_errors() {
        with_clean_env(|| {
            let err =
                apply_openresponses_env(Provider::Openai, None, false, false, None).unwrap_err();
            assert!(err.to_string().contains("missing API key"));
        });
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
        render_message(OutputView::Output, &stdout_payload, &mut buffer, &mut state)
            .expect("render");
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
        render_message(OutputView::Output, &stderr_payload, &mut buffer, &mut state)
            .expect("render");
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
}
