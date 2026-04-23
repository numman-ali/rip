use clap::{Parser, Subcommand, ValueEnum};
use futures_util::StreamExt;
use reqwest::Client;
use reqwest_eventsource::{Error as EventSourceError, Event, RequestBuilderExt};
use rip_kernel::{Event as FrameEvent, EventKind};
use serde_json::Value;

#[cfg(test)]
#[path = "main/tests.rs"]
mod tests;

#[path = "main/run.rs"]
mod run_impl;

mod fullscreen;
mod local_authority;
mod metrics;
mod tasks_watch;
#[cfg(test)]
mod test_env;
mod threads;

#[derive(Parser)]
#[command(name = "rip")]
#[command(about = "RIP CLI", long_about = None)]
struct Cli {
    /// Optional initial prompt for the interactive terminal UI (when no subcommand is used).
    prompt: Option<String>,
    /// Server base URL for TUI attach mode (requires `--session` and no subcommand).
    #[arg(long)]
    server: Option<String>,
    /// Existing session id for TUI attach mode.
    #[arg(long)]
    session: Option<String>,
    /// Existing task id for TUI attach mode.
    #[arg(long)]
    task: Option<String>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Run {
        prompt: String,
        #[arg(long)]
        server: Option<String>,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        detach: bool,
        #[arg(long, value_enum)]
        provider: Option<Provider>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        stateless_history: bool,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        parallel_tool_calls: bool,
        #[arg(long = "include")]
        include: Vec<String>,
        #[arg(long)]
        followup_user_message: Option<String>,
        #[arg(long, action = clap::ArgAction::SetTrue, conflicts_with = "no_web_search")]
        web_search: bool,
        #[arg(long = "no-web-search", action = clap::ArgAction::SetTrue, conflicts_with = "web_search")]
        no_web_search: bool,
        #[arg(long, value_enum)]
        web_search_context_size: Option<SearchContextSizeArg>,
        #[arg(long)]
        web_search_external_web_access: Option<bool>,
        #[arg(long, value_enum)]
        reasoning_effort: Option<ReasoningEffortArg>,
        #[arg(long, value_enum)]
        reasoning_summary: Option<ReasoningSummaryArg>,
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
    Tasks {
        /// Server base URL for remote mode. If omitted, auto-start/auto-attach the local authority.
        #[arg(long)]
        server: Option<String>,
        #[command(subcommand)]
        command: TaskCommand,
    },
    Threads {
        #[arg(long)]
        server: Option<String>,
        #[command(subcommand)]
        command: threads::ThreadsCommand,
    },
    Config {
        /// Server base URL for remote mode. If omitted, auto-start/auto-attach the local authority.
        #[arg(long)]
        server: Option<String>,
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
enum OutputView {
    Raw,
    Output,
    Metrics,
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
    metrics: metrics::RunMetrics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Provider {
    Openai,
    Openrouter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum ReasoningEffortArg {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

impl ReasoningEffortArg {
    const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum ReasoningSummaryArg {
    Concise,
    Detailed,
    Auto,
}

impl ReasoningSummaryArg {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Concise => "concise",
            Self::Detailed => "detailed",
            Self::Auto => "auto",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum SearchContextSizeArg {
    Low,
    Medium,
    High,
}

impl SearchContextSizeArg {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Subcommand)]
enum TaskCommand {
    Spawn {
        #[arg(long)]
        tool: String,
        /// Tool args as JSON.
        #[arg(long)]
        args: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, value_enum, default_value_t = TaskExecutionMode::Pipes)]
        execution_mode: TaskExecutionMode,
    },
    List,
    Status {
        id: String,
    },
    Cancel {
        id: String,
        #[arg(long)]
        reason: Option<String>,
    },
    Stdin {
        id: String,
        /// UTF-8 text to send to stdin (encoded to base64 for transport).
        #[arg(long, conflicts_with = "chunk_b64")]
        text: Option<String>,
        /// Raw stdin bytes (base64) to send.
        #[arg(long, conflicts_with = "text")]
        chunk_b64: Option<String>,
        /// If using --text, do not append a trailing newline.
        #[arg(long, requires = "text")]
        no_newline: bool,
    },
    Resize {
        id: String,
        #[arg(long)]
        rows: u16,
        #[arg(long)]
        cols: u16,
    },
    Signal {
        id: String,
        signal: String,
    },
    Output {
        id: String,
        #[arg(long, value_enum, default_value_t = TaskStream::Stdout)]
        stream: TaskStream,
        #[arg(long, default_value_t = 0)]
        offset_bytes: u64,
        #[arg(long)]
        max_bytes: Option<usize>,
    },
    Events {
        id: String,
    },
    Watch {
        #[arg(long, default_value_t = 1000)]
        interval_ms: u64,
    },
}

#[derive(Subcommand)]
enum ConfigCommand {
    Doctor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum TaskStream {
    Stdout,
    Stderr,
    Pty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum TaskExecutionMode {
    Pipes,
    Pty,
}

#[cfg(not(test))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run(Cli::parse()).await
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        None => match (cli.server, cli.session, cli.task) {
            (None, None, None) => {
                let server = local_authority::ensure_local_authority().await?;
                let openresponses_overrides = openresponses_overrides_from_env();
                fullscreen::run_fullscreen_tui_remote(server, cli.prompt, openresponses_overrides)
                    .await?;
            }
            (Some(server), Some(session_id), None) => {
                if let Some(prompt) = cli.prompt {
                    if !prompt.trim().is_empty() {
                        anyhow::bail!(
                            "unexpected prompt when attaching to a session; omit <prompt>"
                        );
                    }
                }
                fullscreen::run_fullscreen_tui_attach(server, session_id).await?;
            }
            (Some(server), None, Some(task_id)) => {
                if let Some(prompt) = cli.prompt {
                    if !prompt.trim().is_empty() {
                        anyhow::bail!("unexpected prompt when attaching to a task; omit <prompt>");
                    }
                }
                fullscreen::run_fullscreen_tui_attach_task(server, task_id).await?;
            }
            (Some(server), None, None) => {
                fullscreen::run_fullscreen_tui_remote(server, cli.prompt, None).await?;
            }
            (Some(_), Some(_), Some(_)) => anyhow::bail!("use exactly one of --session or --task"),
            (None, Some(_), _) | (None, _, Some(_)) => anyhow::bail!("missing --server"),
        },
        Some(Commands::Run {
            prompt,
            server,
            detach,
            provider,
            model,
            stateless_history,
            parallel_tool_calls,
            include,
            followup_user_message,
            web_search,
            no_web_search,
            web_search_context_size,
            web_search_external_web_access,
            reasoning_effort,
            reasoning_summary,
            headless,
            view,
        }) => {
            let has_openresponses_flags = provider.is_some()
                || model.is_some()
                || stateless_history
                || parallel_tool_calls
                || !include.is_empty()
                || followup_user_message.is_some()
                || web_search
                || no_web_search
                || web_search_context_size.is_some()
                || web_search_external_web_access.is_some()
                || reasoning_effort.is_some()
                || reasoning_summary.is_some();
            let openresponses_overrides = if has_openresponses_flags {
                let mut obj = serde_json::Map::new();
                if let Some(provider) = provider {
                    let endpoint = match provider {
                        Provider::Openai => "https://api.openai.com/v1/responses",
                        Provider::Openrouter => "https://openrouter.ai/api/v1/responses",
                    };
                    obj.insert("endpoint".to_string(), Value::String(endpoint.to_string()));
                }
                if let Some(model) = model.clone() {
                    if !model.trim().is_empty() {
                        obj.insert("model".to_string(), Value::String(model));
                    }
                }
                if stateless_history {
                    obj.insert("stateless_history".to_string(), Value::Bool(true));
                }
                if parallel_tool_calls {
                    obj.insert("parallel_tool_calls".to_string(), Value::Bool(true));
                }
                insert_include_overrides(&mut obj, &include)?;
                if let Some(message) = followup_user_message.clone() {
                    if !message.trim().is_empty() {
                        obj.insert("followup_user_message".to_string(), Value::String(message));
                    }
                }
                insert_web_search_overrides(
                    &mut obj,
                    web_search,
                    no_web_search,
                    web_search_context_size,
                    web_search_external_web_access,
                )?;
                insert_reasoning_overrides(&mut obj, reasoning_effort, reasoning_summary);
                Some(Value::Object(obj))
            } else if server.is_none() {
                // Local-only compat: allow env changes in the client to be forwarded as per-run
                // overrides so the authority does not need restarting.
                openresponses_overrides_from_env()
            } else {
                None
            };
            if let Some(server) = server {
                if headless {
                    run_impl::run_headless_remote(
                        prompt,
                        server,
                        view,
                        openresponses_overrides,
                        detach,
                    )
                    .await?;
                } else {
                    run_impl::run_interactive_remote(
                        prompt,
                        server,
                        view,
                        openresponses_overrides,
                        detach,
                    )
                    .await?;
                }
            } else {
                #[cfg(test)]
                {
                    let _openresponses_overrides = openresponses_overrides;
                    if headless {
                        run_impl::run_headless_local(prompt, view, detach).await?;
                    } else {
                        run_impl::run_interactive_local(prompt, view, detach).await?;
                    }
                }
                #[cfg(not(test))]
                {
                    let server = local_authority::ensure_local_authority().await?;
                    if headless {
                        run_impl::run_headless_remote(
                            prompt,
                            server,
                            view,
                            openresponses_overrides,
                            detach,
                        )
                        .await?;
                    } else {
                        run_impl::run_interactive_remote(
                            prompt,
                            server,
                            view,
                            openresponses_overrides,
                            detach,
                        )
                        .await?;
                    }
                }
            }
        }
        Some(Commands::Serve) => {
            ripd::serve_default().await;
        }
        Some(Commands::Tasks { server, command }) => {
            let server = match server {
                Some(server) => server,
                None => local_authority::ensure_local_authority().await?,
            };
            let client = Client::new();
            match command {
                TaskCommand::Spawn {
                    tool,
                    args,
                    title,
                    execution_mode,
                } => {
                    let args_value: Value = serde_json::from_str(&args)
                        .map_err(|err| anyhow::anyhow!("invalid --args json: {err}"))?;
                    let url = format!("{server}/tasks");
                    let execution_mode_str = match execution_mode {
                        TaskExecutionMode::Pipes => "pipes",
                        TaskExecutionMode::Pty => "pty",
                    };
                    let response = client
                        .post(url)
                        .json(&serde_json::json!({
                            "tool": tool,
                            "args": args_value,
                            "title": title,
                            "execution_mode": execution_mode_str
                        }))
                        .send()
                        .await?;
                    let status = response.status();
                    if !status.is_success() {
                        anyhow::bail!("task spawn failed: {status}");
                    }
                    let body = response.text().await?;
                    println!("{body}");
                }
                TaskCommand::List => {
                    let url = format!("{server}/tasks");
                    let response = client.get(url).send().await?;
                    let status = response.status();
                    if !status.is_success() {
                        anyhow::bail!("task list failed: {status}");
                    }
                    let body = response.text().await?;
                    println!("{body}");
                }
                TaskCommand::Status { id } => {
                    let url = format!("{server}/tasks/{id}");
                    let response = client.get(url).send().await?;
                    let status = response.status();
                    if !status.is_success() {
                        anyhow::bail!("task status failed: {status}");
                    }
                    let body = response.text().await?;
                    println!("{body}");
                }
                TaskCommand::Cancel { id, reason } => {
                    let url = format!("{server}/tasks/{id}/cancel");
                    let response = client
                        .post(url)
                        .json(&serde_json::json!({"reason": reason}))
                        .send()
                        .await?;
                    let status = response.status();
                    if !status.is_success() {
                        anyhow::bail!("task cancel failed: {status}");
                    }
                }
                TaskCommand::Stdin {
                    id,
                    text,
                    chunk_b64,
                    no_newline,
                } => {
                    let url = format!("{server}/tasks/{id}/stdin");
                    let chunk_b64 = match (text, chunk_b64) {
                        (Some(text), None) => {
                            let payload = if no_newline {
                                text
                            } else {
                                format!("{text}\n")
                            };
                            base64_encode(payload.as_bytes())
                        }
                        (None, Some(chunk_b64)) => chunk_b64,
                        _ => anyhow::bail!("use exactly one of --text or --chunk-b64"),
                    };
                    let response = client
                        .post(url)
                        .json(&serde_json::json!({"chunk_b64": chunk_b64}))
                        .send()
                        .await?;
                    let status = response.status();
                    if !status.is_success() {
                        anyhow::bail!("task stdin failed: {status}");
                    }
                }
                TaskCommand::Resize { id, rows, cols } => {
                    let url = format!("{server}/tasks/{id}/resize");
                    let response = client
                        .post(url)
                        .json(&serde_json::json!({"rows": rows, "cols": cols}))
                        .send()
                        .await?;
                    let status = response.status();
                    if !status.is_success() {
                        anyhow::bail!("task resize failed: {status}");
                    }
                }
                TaskCommand::Signal { id, signal } => {
                    let url = format!("{server}/tasks/{id}/signal");
                    let response = client
                        .post(url)
                        .json(&serde_json::json!({"signal": signal}))
                        .send()
                        .await?;
                    let status = response.status();
                    if !status.is_success() {
                        anyhow::bail!("task signal failed: {status}");
                    }
                }
                TaskCommand::Output {
                    id,
                    stream,
                    offset_bytes,
                    max_bytes,
                } => {
                    let stream_str = match stream {
                        TaskStream::Stdout => "stdout",
                        TaskStream::Stderr => "stderr",
                        TaskStream::Pty => "pty",
                    };
                    let mut url = format!(
                        "{server}/tasks/{id}/output?stream={stream_str}&offset_bytes={offset_bytes}"
                    );
                    if let Some(max_bytes) = max_bytes {
                        url.push_str(&format!("&max_bytes={max_bytes}"));
                    }
                    let response = client.get(url).send().await?;
                    let status = response.status();
                    if !status.is_success() {
                        anyhow::bail!("task output failed: {status}");
                    }
                    let body = response.text().await?;
                    println!("{body}");
                }
                TaskCommand::Events { id } => {
                    let url = format!("{server}/tasks/{id}/events");
                    let mut stream = client.get(url).eventsource()?;
                    while let Some(next) = stream.next().await {
                        match next {
                            Ok(Event::Open) => {}
                            Ok(Event::Message(msg)) => {
                                let frame: Option<FrameEvent> =
                                    serde_json::from_str(&msg.data).ok();
                                println!("{}", msg.data);
                                if let Some(frame) = frame {
                                    if matches!(
                                        frame.kind,
                                        EventKind::ToolTaskStatus {
                                            status: rip_kernel::ToolTaskStatus::Exited
                                                | rip_kernel::ToolTaskStatus::Cancelled
                                                | rip_kernel::ToolTaskStatus::Failed,
                                            ..
                                        }
                                    ) {
                                        break;
                                    }
                                }
                            }
                            Err(EventSourceError::StreamEnded) => break,
                            Err(err) => return Err(err.into()),
                        }
                    }
                }
                TaskCommand::Watch { interval_ms } => {
                    tasks_watch::run_tasks_watch(server.clone(), interval_ms).await?;
                }
            }
        }
        Some(Commands::Threads { server, command }) => {
            threads::run_threads(server, command).await?;
        }
        Some(Commands::Config { server, command }) => {
            let server = match server {
                Some(server) => server,
                None => local_authority::ensure_local_authority().await?,
            };

            match command {
                ConfigCommand::Doctor => {
                    let url = format!("{server}/config/doctor");
                    let client = Client::new();
                    let response = client.get(url).send().await?;
                    let status = response.status();
                    if !status.is_success() {
                        let body = response.text().await.unwrap_or_default();
                        anyhow::bail!("config doctor failed: {status}: {body}");
                    }
                    let value: Value = response.json().await?;
                    println!("{}", serde_json::to_string_pretty(&value)?);
                }
            }
        }
    }

    Ok(())
}

fn openresponses_overrides_from_env() -> Option<Value> {
    let endpoint = std::env::var("RIP_OPENRESPONSES_ENDPOINT")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;

    let mut obj = serde_json::Map::new();
    obj.insert("endpoint".to_string(), Value::String(endpoint));

    if let Ok(value) = std::env::var("RIP_OPENRESPONSES_MODEL") {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            obj.insert("model".to_string(), Value::String(trimmed));
        }
    }

    if let Some(stateless_history) = parse_env_bool("RIP_OPENRESPONSES_STATELESS_HISTORY") {
        obj.insert(
            "stateless_history".to_string(),
            Value::Bool(stateless_history),
        );
    }

    if let Some(parallel_tool_calls) = parse_env_bool("RIP_OPENRESPONSES_PARALLEL_TOOL_CALLS") {
        obj.insert(
            "parallel_tool_calls".to_string(),
            Value::Bool(parallel_tool_calls),
        );
    }

    if let Ok(value) = std::env::var("RIP_OPENRESPONSES_INCLUDE") {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            let include = match ripd::parse_openresponses_include_list(&trimmed) {
                Ok(include) => include,
                Err(err) => {
                    eprintln!("invalid RIP_OPENRESPONSES_INCLUDE={trimmed:?}: {err}; ignoring");
                    Vec::new()
                }
            };
            if !include.is_empty() {
                obj.insert(
                    "include".to_string(),
                    Value::Array(
                        include
                            .into_iter()
                            .map(|value| serde_json::to_value(value).expect("include serializes"))
                            .collect(),
                    ),
                );
            }
        }
    }

    if let Ok(value) = std::env::var("RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE") {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            obj.insert("followup_user_message".to_string(), Value::String(trimmed));
        }
    }
    insert_web_search_overrides_from_env(&mut obj);

    let mut reasoning = serde_json::Map::new();
    if let Ok(value) = std::env::var("RIP_OPENRESPONSES_REASONING_EFFORT") {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            reasoning.insert("effort".to_string(), Value::String(trimmed));
        }
    }
    if let Ok(value) = std::env::var("RIP_OPENRESPONSES_REASONING_SUMMARY") {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            reasoning.insert("summary".to_string(), Value::String(trimmed));
        }
    }
    if !reasoning.is_empty() {
        obj.insert("reasoning".to_string(), Value::Object(reasoning));
    }

    Some(Value::Object(obj))
}

fn insert_web_search_overrides(
    obj: &mut serde_json::Map<String, Value>,
    web_search: bool,
    no_web_search: bool,
    web_search_context_size: Option<SearchContextSizeArg>,
    web_search_external_web_access: Option<bool>,
) -> anyhow::Result<()> {
    if !web_search
        && !no_web_search
        && web_search_context_size.is_none()
        && web_search_external_web_access.is_none()
    {
        return Ok(());
    }

    let mut web = serde_json::Map::new();
    if web_search {
        web.insert("enabled".to_string(), Value::Bool(true));
    } else if no_web_search {
        web.insert("enabled".to_string(), Value::Bool(false));
    }
    if let Some(value) = web_search_context_size {
        web.insert(
            "search_context_size".to_string(),
            Value::String(value.as_str().to_string()),
        );
    }
    if let Some(value) = web_search_external_web_access {
        web.insert("external_web_access".to_string(), Value::Bool(value));
    }
    if !web.is_empty() {
        obj.insert("web_search".to_string(), Value::Object(web));
    }
    Ok(())
}

fn insert_web_search_overrides_from_env(obj: &mut serde_json::Map<String, Value>) {
    let mut web = serde_json::Map::new();
    if let Some(enabled) = parse_env_bool("RIP_OPENRESPONSES_WEB_SEARCH") {
        web.insert("enabled".to_string(), Value::Bool(enabled));
    }
    if let Ok(value) = std::env::var("RIP_OPENRESPONSES_WEB_SEARCH_CONTEXT_SIZE") {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            match ripd::parse_search_context_size(&trimmed) {
                Ok(parsed) => {
                    web.insert(
                        "search_context_size".to_string(),
                        Value::String(
                            match parsed {
                                ripd::SearchContextSize::Low => "low",
                                ripd::SearchContextSize::Medium => "medium",
                                ripd::SearchContextSize::High => "high",
                            }
                            .to_string(),
                        ),
                    );
                }
                Err(err) => eprintln!(
                    "invalid RIP_OPENRESPONSES_WEB_SEARCH_CONTEXT_SIZE={trimmed:?}: {err}; ignoring"
                ),
            }
        }
    }
    if let Some(enabled) = parse_env_bool("RIP_OPENRESPONSES_WEB_SEARCH_EXTERNAL_WEB_ACCESS") {
        web.insert("external_web_access".to_string(), Value::Bool(enabled));
    }
    if !web.is_empty() {
        obj.insert("web_search".to_string(), Value::Object(web));
    }
}

fn insert_reasoning_overrides(
    obj: &mut serde_json::Map<String, Value>,
    reasoning_effort: Option<ReasoningEffortArg>,
    reasoning_summary: Option<ReasoningSummaryArg>,
) {
    let mut reasoning = serde_json::Map::new();
    if let Some(value) = reasoning_effort {
        reasoning.insert(
            "effort".to_string(),
            Value::String(value.as_str().to_string()),
        );
    }
    if let Some(value) = reasoning_summary {
        reasoning.insert(
            "summary".to_string(),
            Value::String(value.as_str().to_string()),
        );
    }
    if !reasoning.is_empty() {
        obj.insert("reasoning".to_string(), Value::Object(reasoning));
    }
}

fn insert_include_overrides(
    obj: &mut serde_json::Map<String, Value>,
    include: &[String],
) -> anyhow::Result<()> {
    if include.is_empty() {
        return Ok(());
    }

    let mut parsed = Vec::new();
    for raw in include {
        let value = ripd::parse_openresponses_include(raw)
            .map_err(|err| anyhow::anyhow!("invalid --include {raw:?}: {err}"))?;
        if !parsed.contains(&value) {
            parsed.push(value);
        }
    }

    obj.insert(
        "include".to_string(),
        Value::Array(
            parsed
                .into_iter()
                .map(|value| serde_json::to_value(value).expect("include serializes"))
                .collect(),
        ),
    );
    Ok(())
}

fn parse_env_bool(key: &str) -> Option<bool> {
    std::env::var(key).ok().map(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity((bytes.len().saturating_add(2) / 3) * 4);
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
        out.push(TABLE[(n & 0x3f) as usize] as char);
        i += 3;
    }

    match bytes.len().saturating_sub(i) {
        0 => {}
        1 => {
            let n = (bytes[i] as u32) << 16;
            out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
            out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
            out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
            out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
            out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
            out.push('=');
        }
        _ => unreachable!("len mod 3 is always 0..=2"),
    }

    out
}

async fn ensure_thread(client: &Client, server: &str) -> anyhow::Result<String> {
    run_impl::ensure_thread(client, server).await
}

async fn post_thread_message(
    client: &Client,
    server: &str,
    thread_id: &str,
    content: &str,
    actor_id: &str,
    origin: &str,
    openresponses_overrides: Option<Value>,
) -> anyhow::Result<threads::ThreadPostMessageResponse> {
    run_impl::post_thread_message(
        client,
        server,
        thread_id,
        content,
        actor_id,
        origin,
        openresponses_overrides,
    )
    .await
}
