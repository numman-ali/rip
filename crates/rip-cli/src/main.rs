use std::io::{self, Write};

use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use reqwest::Client;
use reqwest_eventsource::{Error as EventSourceError, Event, RequestBuilderExt};
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
        #[arg(long, default_value_t = true)]
        headless: bool,
    },
}

#[derive(Deserialize)]
struct SessionCreated {
    session_id: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            prompt,
            server,
            headless,
        } => {
            if !headless {
                eprintln!("interactive mode not implemented; falling back to headless");
            }
            run_headless(prompt, server).await?;
        }
    }

    Ok(())
}

async fn run_headless(prompt: String, server: String) -> anyhow::Result<()> {
    let client = Client::new();
    let session_id = create_session(&client, &server).await?;
    send_input(&client, &server, &session_id, &prompt).await?;
    stream_events(&client, &server, &session_id).await?;
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

async fn stream_events(client: &Client, server: &str, session_id: &str) -> anyhow::Result<()> {
    let url = format!("{server}/sessions/{session_id}/events");
    let mut stream = client.get(url).eventsource()?;
    let stdout = io::stdout();
    let mut handle = stdout.lock();

    while let Some(next) = stream.next().await {
        match next {
            Ok(Event::Open) => {}
            Ok(Event::Message(msg)) => {
                writeln!(handle, "{}", msg.data)?;
                handle.flush()?;
            }
            Err(EventSourceError::StreamEnded) => break,
            Err(err) => return Err(err.into()),
        }
    }

    Ok(())
}
