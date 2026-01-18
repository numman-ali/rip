use std::io::{self, Write};

use clap::{Parser, Subcommand, ValueEnum};
use futures_util::StreamExt;
use reqwest::Client;
use reqwest_eventsource::{Error as EventSourceError, Event, RequestBuilderExt};
use rip_kernel::Event as FrameEvent;
use rip_provider_openresponses::{
    extract_reasoning_deltas, extract_text_deltas, extract_tool_call_argument_deltas,
};
use serde::Deserialize;

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
        #[arg(long, default_value = "http://127.0.0.1:7341")]
        server: String,
        #[arg(
            long,
            default_value_t = true,
            value_parser = clap::value_parser!(bool),
            action = clap::ArgAction::Set
        )]
        headless: bool,
        #[arg(long, value_enum, default_value_t = OutputView::Raw)]
        view: OutputView,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
enum OutputView {
    Raw,
    Output,
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
            headless,
            view,
        } => {
            if headless {
                run_headless(prompt, server, view).await?;
            } else {
                run_interactive(prompt, server, view).await?;
            }
        }
    }

    Ok(())
}

async fn run_headless(prompt: String, server: String, view: OutputView) -> anyhow::Result<()> {
    let client = Client::new();
    let session_id = create_session(&client, &server).await?;
    send_input(&client, &server, &session_id, &prompt).await?;
    stream_events(&client, &server, &session_id, view).await?;
    Ok(())
}

async fn run_interactive(prompt: String, server: String, view: OutputView) -> anyhow::Result<()> {
    let client = Client::new();
    let session_id = create_session(&client, &server).await?;
    send_input(&client, &server, &session_id, &prompt).await?;
    stream_events(&client, &server, &session_id, view).await?;
    Ok(())
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
    while let Some(next) = stream.next().await {
        match next {
            Ok(Event::Open) => {}
            Ok(Event::Message(msg)) => {
                render_message(view, &msg.data, out)?;
            }
            Err(EventSourceError::StreamEnded) => break,
            Err(err) => return Err(err.into()),
        }
    }

    Ok(())
}

fn render_message(view: OutputView, payload: &str, out: &mut dyn Write) -> anyhow::Result<()> {
    let frame: FrameEvent = serde_json::from_str(payload)
        .map_err(|err| anyhow::anyhow!("invalid event frame: {err}"))?;

    match view {
        OutputView::Raw => {
            writeln!(out, "{payload}")?;
            out.flush()?;
        }
        OutputView::Output => {
            for delta in extract_text_deltas(std::slice::from_ref(&frame)) {
                writeln!(out, "{delta}")?;
            }
            for delta in extract_reasoning_deltas(std::slice::from_ref(&frame)) {
                writeln!(out, "reasoning: {delta}")?;
            }
            for delta in extract_tool_call_argument_deltas(std::slice::from_ref(&frame)) {
                writeln!(out, "tool: {delta}")?;
            }
            out.flush()?;
        }
    }
    Ok(())
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
            "type": "provider_event",
            "provider": "openresponses",
            "status": "event",
            "event_name": "response.output_text.delta",
            "data": {"type": "response.output_text.delta", "delta": "hi"},
            "raw": null,
            "errors": [],
            "response_errors": []
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
        assert_eq!(
            rendered.trim_end(),
            "hi\nreasoning: step\ntool: {\"arg\":1}"
        );
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
                server: server.base_url(),
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
            Commands::Run { prompt, .. } => assert_eq!(prompt, "hello"),
        }
    }

    #[test]
    fn cli_defaults_headless() {
        let cli = Cli::parse_from(["rip", "run", "hello"]);
        match cli.command {
            Commands::Run { headless, view, .. } => {
                assert!(headless);
                assert_eq!(view, OutputView::Raw);
            }
        }
    }

    #[test]
    fn cli_respects_server_flag() {
        let cli = Cli::parse_from(["rip", "run", "hello", "--server", "http://local"]);
        match cli.command {
            Commands::Run { server, .. } => assert_eq!(server, "http://local"),
        }
    }

    #[test]
    fn renders_raw_payload() {
        let mut buffer = Vec::new();
        let payload = session_started_frame();
        render_message(OutputView::Raw, payload.as_str(), &mut buffer).expect("render");
        let rendered = String::from_utf8(buffer).expect("utf8");
        assert_eq!(rendered.trim_end(), payload);
    }

    #[test]
    fn raw_view_rejects_invalid_frame() {
        let mut buffer = Vec::new();
        let payload = "{\"type\":\"session_started\"}";
        let err = render_message(OutputView::Raw, payload, &mut buffer).unwrap_err();
        assert!(err.to_string().contains("invalid event frame"));
    }

    #[test]
    fn renders_output_deltas() {
        let mut buffer = Vec::new();
        let payload = serde_json::json!({
            "id": "e1",
            "session_id": "s1",
            "timestamp_ms": 0,
            "seq": 0,
            "type": "provider_event",
            "provider": "openresponses",
            "status": "event",
            "event_name": "response.output_text.delta",
            "data": {"type": "response.output_text.delta", "delta": "hi"},
            "raw": null,
            "errors": [],
            "response_errors": []
        })
        .to_string();
        render_message(OutputView::Output, &payload, &mut buffer).expect("render");
        let rendered = String::from_utf8(buffer).expect("utf8");
        assert_eq!(rendered.trim_end(), "hi");
    }

    #[test]
    fn renders_reasoning_and_tool_deltas() {
        let mut buffer = Vec::new();
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
        render_message(OutputView::Output, &reasoning_payload, &mut buffer).expect("render");

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
        render_message(OutputView::Output, &tool_payload, &mut buffer).expect("render");

        let rendered = String::from_utf8(buffer).expect("utf8");
        let lines: Vec<&str> = rendered.lines().collect();
        assert!(lines.contains(&"reasoning: step"));
        assert!(lines.contains(&"tool: {\"arg\":1}"));
    }

    #[test]
    fn cli_respects_headless_flag() {
        let cli = Cli::parse_from(["rip", "run", "hello", "--headless", "false"]);
        match cli.command {
            Commands::Run { headless, .. } => assert!(!headless),
        }
    }
}
