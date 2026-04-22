use std::io::{self, Write};

use super::*;
#[cfg(test)]
use tokio::sync::broadcast;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(super) struct DetachedRunInfo {
    pub thread_id: String,
    pub message_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attach_command: Option<String>,
}

pub(super) async fn run_headless_remote(
    prompt: String,
    server: String,
    view: OutputView,
    openresponses_overrides: Option<Value>,
    detach: bool,
) -> anyhow::Result<()> {
    run_remote(prompt, server, view, openresponses_overrides, detach).await
}

pub(super) async fn run_interactive_remote(
    prompt: String,
    server: String,
    view: OutputView,
    openresponses_overrides: Option<Value>,
    detach: bool,
) -> anyhow::Result<()> {
    run_remote(prompt, server, view, openresponses_overrides, detach).await
}

async fn run_remote(
    prompt: String,
    server: String,
    view: OutputView,
    openresponses_overrides: Option<Value>,
    detach: bool,
) -> anyhow::Result<()> {
    let client = Client::new();
    let thread_id = ensure_thread(&client, &server).await?;
    let response = post_thread_message(
        &client,
        &server,
        &thread_id,
        &prompt,
        "user",
        "cli",
        openresponses_overrides,
    )
    .await?;
    if detach {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        let detached = DetachedRunInfo {
            thread_id: response.thread_id,
            message_id: response.message_id,
            session_id: response.session_id.clone(),
            server: Some(server.clone()),
            attach_command: Some(format!(
                "rip --server {server} --session {}",
                response.session_id
            )),
        };
        render_detached_run(view, &mut handle, &detached)?;
        return Ok(());
    }
    stream_events(&client, &server, &response.session_id, view).await?;
    Ok(())
}

#[cfg(test)]
pub(super) async fn run_headless_local(
    prompt: String,
    view: OutputView,
    detach: bool,
) -> anyhow::Result<()> {
    let engine =
        ripd::SessionEngine::new_default().map_err(|err| anyhow::anyhow!("engine init: {err}"))?;
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    if detach {
        run_local_detached_with_engine(&engine, prompt, view, &mut handle).await
    } else {
        run_local_with_engine(&engine, prompt, view, &mut handle).await
    }
}

#[cfg(test)]
pub(super) async fn run_interactive_local(
    prompt: String,
    view: OutputView,
    detach: bool,
) -> anyhow::Result<()> {
    run_headless_local(prompt, view, detach).await
}

pub(super) async fn ensure_thread(client: &Client, server: &str) -> anyhow::Result<String> {
    let url = format!("{server}/threads/ensure");
    let response = client.post(url).send().await?;
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("ensure thread failed: {status}");
    }
    let payload: threads::ThreadEnsureResponse = response.json().await?;
    Ok(payload.thread_id)
}

pub(super) async fn post_thread_message(
    client: &Client,
    server: &str,
    thread_id: &str,
    content: &str,
    actor_id: &str,
    origin: &str,
    openresponses_overrides: Option<Value>,
) -> anyhow::Result<threads::ThreadPostMessageResponse> {
    let url = format!("{server}/threads/{thread_id}/messages");
    let mut payload = serde_json::Map::new();
    payload.insert("content".to_string(), Value::String(content.to_string()));
    payload.insert("actor_id".to_string(), Value::String(actor_id.to_string()));
    payload.insert("origin".to_string(), Value::String(origin.to_string()));
    if let Some(overrides) = openresponses_overrides {
        payload.insert("openresponses".to_string(), overrides);
    }
    let response = client.post(url).json(&payload).send().await?;
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("post message failed: {status}");
    }
    let payload: threads::ThreadPostMessageResponse = response.json().await?;
    Ok(payload)
}

pub(super) async fn stream_events(
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

pub(super) async fn stream_events_with_writer(
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

#[cfg(test)]
pub(super) async fn run_local_with_engine(
    engine: &ripd::SessionEngine,
    prompt: String,
    view: OutputView,
    out: &mut dyn Write,
) -> anyhow::Result<()> {
    let continuities = engine.continuities();
    let continuity_id = continuities
        .ensure_default()
        .map_err(|err| anyhow::anyhow!("continuity ensure: {err}"))?;
    let actor_id = "user".to_string();
    let origin = "cli".to_string();
    let message_id = continuities
        .append_message(
            &continuity_id,
            actor_id.clone(),
            origin.clone(),
            prompt.clone(),
        )
        .map_err(|err| anyhow::anyhow!("continuity post message: {err}"))?;

    let handle = engine.create_session();
    let run_link = ripd::ContinuityRunLink {
        continuity_id: continuity_id.clone(),
        message_id: message_id.clone(),
        actor_id: actor_id.clone(),
        origin: origin.clone(),
    };
    continuities
        .append_run_spawned(
            &continuity_id,
            &message_id,
            &handle.session_id,
            actor_id,
            origin,
        )
        .map_err(|err| anyhow::anyhow!("continuity run spawned: {err}"))?;
    let mut receiver = handle.subscribe();
    engine.spawn_session(handle, prompt, Some(run_link), None);
    stream_events_from_receiver(&mut receiver, view, out).await
}

#[cfg(test)]
pub(super) async fn run_local_detached_with_engine(
    engine: &ripd::SessionEngine,
    prompt: String,
    view: OutputView,
    out: &mut dyn Write,
) -> anyhow::Result<()> {
    let continuities = engine.continuities();
    let thread_id = continuities
        .ensure_default()
        .map_err(|err| anyhow::anyhow!("continuity ensure: {err}"))?;
    let actor_id = "user".to_string();
    let origin = "cli".to_string();
    let message_id = continuities
        .append_message(&thread_id, actor_id.clone(), origin.clone(), prompt.clone())
        .map_err(|err| anyhow::anyhow!("continuity post message: {err}"))?;

    let handle = engine.create_session();
    let run_link = ripd::ContinuityRunLink {
        continuity_id: thread_id.clone(),
        message_id: message_id.clone(),
        actor_id: actor_id.clone(),
        origin: origin.clone(),
    };
    continuities
        .append_run_spawned(
            &thread_id,
            &message_id,
            &handle.session_id,
            actor_id,
            origin,
        )
        .map_err(|err| anyhow::anyhow!("continuity run spawned: {err}"))?;
    let detached = DetachedRunInfo {
        thread_id,
        message_id,
        session_id: handle.session_id.clone(),
        server: None,
        attach_command: None,
    };
    engine.spawn_session(handle, prompt, Some(run_link), None);
    render_detached_run(view, out, &detached)
}

#[cfg(test)]
pub(super) async fn stream_events_from_receiver(
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

pub(super) fn render_message(
    view: OutputView,
    payload: &str,
    out: &mut dyn Write,
    state: &mut OutputState,
) -> anyhow::Result<bool> {
    let frame: FrameEvent = serde_json::from_str(payload)
        .map_err(|err| anyhow::anyhow!("invalid event frame: {err}"))?;
    state.metrics.observe(&frame);
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
        OutputView::Metrics => {
            match &frame.kind {
                EventKind::ToolFailed { error, .. } => state.tool_failed.push(error.clone()),
                EventKind::ProviderEvent {
                    status,
                    errors,
                    response_errors,
                    raw,
                    ..
                } => {
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
                _ => {}
            }

            if should_stop {
                let mut metrics = state.metrics.to_json();
                if let Value::Object(obj) = &mut metrics {
                    obj.insert(
                        "tool_failed".to_string(),
                        Value::Array(
                            state
                                .tool_failed
                                .iter()
                                .cloned()
                                .map(Value::String)
                                .collect(),
                        ),
                    );
                    obj.insert(
                        "provider_errors".to_string(),
                        Value::Array(
                            state
                                .provider_errors
                                .iter()
                                .cloned()
                                .map(Value::String)
                                .collect(),
                        ),
                    );
                    obj.insert(
                        "provider_response_errors".to_string(),
                        Value::Array(
                            state
                                .provider_response_errors
                                .iter()
                                .cloned()
                                .map(Value::String)
                                .collect(),
                        ),
                    );
                    obj.insert(
                        "provider_invalid_json".to_string(),
                        Value::Array(
                            state
                                .provider_invalid_json
                                .iter()
                                .cloned()
                                .map(Value::String)
                                .collect(),
                        ),
                    );
                }
                writeln!(
                    out,
                    "{}",
                    serde_json::to_string(&metrics)
                        .map_err(|err| anyhow::anyhow!("metrics json: {err}"))?
                )?;
            }
            out.flush()?;
        }
    }
    Ok(should_stop)
}

pub(super) fn render_detached_run(
    view: OutputView,
    out: &mut dyn Write,
    detached: &DetachedRunInfo,
) -> anyhow::Result<()> {
    match view {
        OutputView::Raw => {
            let payload = serde_json::to_string(detached)
                .map_err(|err| anyhow::anyhow!("detached run json: {err}"))?;
            writeln!(out, "{payload}")?;
        }
        OutputView::Output | OutputView::Metrics => {
            if let Some(command) = detached.attach_command.as_deref() {
                writeln!(
                    out,
                    "detached session {} on thread {}. reattach with: {command}",
                    detached.session_id, detached.thread_id
                )?;
            } else {
                writeln!(
                    out,
                    "detached session {} on thread {}",
                    detached.session_id, detached.thread_id
                )?;
            }
        }
    }
    out.flush()?;
    Ok(())
}
