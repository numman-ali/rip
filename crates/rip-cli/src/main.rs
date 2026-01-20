use std::io::{self, Write};

use clap::{Parser, Subcommand, ValueEnum};
use futures_util::StreamExt;
use reqwest::Client;
use reqwest_eventsource::{Error as EventSourceError, Event, RequestBuilderExt};
use rip_kernel::{Event as FrameEvent, EventKind};
use serde::Deserialize;
use tokio::sync::broadcast;

#[derive(Parser)]
#[command(name = "rip")]
#[command(about = "RIP CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
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
        Commands::Run {
            prompt,
            server,
            provider,
            model,
            stateless_history,
            parallel_tool_calls,
            followup_user_message,
            headless,
            view,
        } => {
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
        Commands::Serve => {
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
            command: Commands::Run {
                prompt: "hello".to_string(),
                server: Some(server.base_url()),
                provider: None,
                model: None,
                stateless_history: false,
                parallel_tool_calls: false,
                followup_user_message: None,
                headless: false,
                view: OutputView::Raw,
            },
        };
        let result = run(cli).await;
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parses_run() {
        let cli = Cli::parse_from(["rip", "run", "hello"]);
        match cli.command {
            Commands::Run { prompt, server, .. } => {
                assert_eq!(prompt, "hello");
                assert!(server.is_none());
            }
            Commands::Serve => panic!("expected run"),
        }
    }

    #[test]
    fn cli_defaults_headless() {
        let cli = Cli::parse_from(["rip", "run", "hello"]);
        match cli.command {
            Commands::Run {
                headless,
                view,
                server,
                ..
            } => {
                assert!(headless);
                assert_eq!(view, OutputView::Output);
                assert!(server.is_none());
            }
            Commands::Serve => panic!("expected run"),
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
        match cli.command {
            Commands::Run {
                provider,
                model,
                stateless_history,
                parallel_tool_calls,
                followup_user_message,
                ..
            } => {
                assert_eq!(provider, Some(Provider::Openai));
                assert_eq!(model.as_deref(), Some("gpt-5-nano-2025-08-07"));
                assert!(stateless_history);
                assert!(parallel_tool_calls);
                assert_eq!(followup_user_message.as_deref(), Some("continue"));
            }
            Commands::Serve => panic!("expected run"),
        }
    }

    #[test]
    fn cli_respects_server_flag() {
        let cli = Cli::parse_from(["rip", "run", "hello", "--server", "http://local"]);
        match cli.command {
            Commands::Run { server, .. } => assert_eq!(server.as_deref(), Some("http://local")),
            Commands::Serve => panic!("expected run"),
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
        match cli.command {
            Commands::Run { headless, .. } => assert!(!headless),
            Commands::Serve => panic!("expected run"),
        }
    }

    #[test]
    fn cli_parses_serve() {
        let cli = Cli::parse_from(["rip", "serve"]);
        match cli.command {
            Commands::Serve => {}
            Commands::Run { .. } => panic!("expected serve"),
        }
    }
}
