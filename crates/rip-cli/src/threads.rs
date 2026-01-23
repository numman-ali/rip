use std::io::{self, Write};

use clap::Subcommand;
use futures_util::StreamExt;
use reqwest::Client;
use reqwest_eventsource::{Error as EventSourceError, Event, RequestBuilderExt};
use rip_kernel::Event as FrameEvent;
use serde::{Deserialize, Serialize};

#[derive(Subcommand)]
pub(crate) enum ThreadsCommand {
    /// Ensure the default continuity exists and print `{"thread_id": ...}`.
    Ensure,
    /// List known continuities and print a JSON array.
    List,
    /// Get continuity metadata and print JSON.
    Get {
        /// Thread id (continuity id).
        id: String,
    },
    /// Create a new thread branched from a parent.
    Branch {
        /// Parent thread id (continuity id).
        id: String,
        /// Optional title/name for the new thread.
        #[arg(long)]
        title: Option<String>,
        /// Branch from a specific message id (turn anchor).
        #[arg(long)]
        from_message_id: Option<String>,
        /// Branch from an explicit parent continuity seq (power/debug).
        #[arg(long)]
        from_seq: Option<u64>,
        /// Actor id (provenance).
        #[arg(long)]
        actor_id: Option<String>,
        /// Origin (provenance).
        #[arg(long)]
        origin: Option<String>,
    },
    /// Create a new thread as a handoff from a parent, carrying curated context.
    Handoff {
        /// Source thread id (continuity id).
        id: String,
        /// Optional title/name for the new thread.
        #[arg(long)]
        title: Option<String>,
        /// Curated summary/context bundle as markdown.
        #[arg(long)]
        summary_markdown: Option<String>,
        /// Curated summary/context bundle as an artifact id.
        #[arg(long)]
        summary_artifact_id: Option<String>,
        /// Handoff from a specific message id (turn anchor).
        #[arg(long)]
        from_message_id: Option<String>,
        /// Handoff from an explicit parent continuity seq (power/debug).
        #[arg(long)]
        from_seq: Option<u64>,
        /// Actor id (provenance).
        #[arg(long)]
        actor_id: Option<String>,
        /// Origin (provenance).
        #[arg(long)]
        origin: Option<String>,
    },
    /// Append a message to a continuity and start a run; print linkage JSON.
    PostMessage {
        /// Thread id (continuity id).
        id: String,
        /// Message content.
        #[arg(long)]
        content: String,
        /// Actor id (provenance).
        #[arg(long)]
        actor_id: Option<String>,
        /// Origin (provenance).
        #[arg(long)]
        origin: Option<String>,
    },
    /// Stream continuity event frames as JSONL (past + live).
    Events {
        /// Thread id (continuity id).
        id: String,
        /// Stop after printing N frames (useful for tests/fixtures).
        #[arg(long)]
        max_events: Option<usize>,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ThreadEnsureResponse {
    pub(crate) thread_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ThreadMeta {
    pub(crate) thread_id: String,
    pub(crate) created_at_ms: u64,
    pub(crate) title: Option<String>,
    pub(crate) archived: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ThreadPostMessageResponse {
    pub(crate) thread_id: String,
    pub(crate) message_id: String,
    pub(crate) session_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ThreadBranchResponse {
    pub(crate) thread_id: String,
    pub(crate) parent_thread_id: String,
    pub(crate) parent_seq: u64,
    pub(crate) parent_message_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ThreadHandoffResponse {
    pub(crate) thread_id: String,
    pub(crate) from_thread_id: String,
    pub(crate) from_seq: u64,
    pub(crate) from_message_id: Option<String>,
}

pub(crate) async fn run_threads(
    server: Option<String>,
    command: ThreadsCommand,
) -> anyhow::Result<()> {
    match server {
        Some(server) => run_threads_remote(server, command).await,
        None => run_threads_local(command).await,
    }
}

async fn run_threads_remote(server: String, command: ThreadsCommand) -> anyhow::Result<()> {
    let client = Client::new();
    match command {
        ThreadsCommand::Ensure => {
            let url = format!("{server}/threads/ensure");
            let response = client.post(url).send().await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread ensure failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::List => {
            let url = format!("{server}/threads");
            let response = client.get(url).send().await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread list failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::Get { id } => {
            let url = format!("{server}/threads/{id}");
            let response = client.get(url).send().await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread get failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::Branch {
            id,
            title,
            from_message_id,
            from_seq,
            actor_id,
            origin,
        } => {
            let url = format!("{server}/threads/{id}/branch");
            let response = client
                .post(url)
                .json(&serde_json::json!({
                    "title": title,
                    "from_message_id": from_message_id,
                    "from_seq": from_seq,
                    "actor_id": actor_id,
                    "origin": origin
                }))
                .send()
                .await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread branch failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::Handoff {
            id,
            title,
            summary_markdown,
            summary_artifact_id,
            from_message_id,
            from_seq,
            actor_id,
            origin,
        } => {
            let url = format!("{server}/threads/{id}/handoff");
            let response = client
                .post(url)
                .json(&serde_json::json!({
                    "title": title,
                    "summary_markdown": summary_markdown,
                    "summary_artifact_id": summary_artifact_id,
                    "from_message_id": from_message_id,
                    "from_seq": from_seq,
                    "actor_id": actor_id,
                    "origin": origin
                }))
                .send()
                .await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread handoff failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::PostMessage {
            id,
            content,
            actor_id,
            origin,
        } => {
            let url = format!("{server}/threads/{id}/messages");
            let response = client
                .post(url)
                .json(&serde_json::json!({
                    "content": content,
                    "actor_id": actor_id,
                    "origin": origin
                }))
                .send()
                .await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread post_message failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::Events { id, max_events } => {
            let url = format!("{server}/threads/{id}/events");
            let mut stream = client.get(url).eventsource()?;
            let stdout = io::stdout();
            let mut out = stdout.lock();
            stream_frames_sse(&mut stream, max_events, &mut out).await?;
        }
    }

    Ok(())
}

async fn run_threads_local(command: ThreadsCommand) -> anyhow::Result<()> {
    let engine =
        ripd::SessionEngine::new_default().map_err(|err| anyhow::anyhow!("engine init: {err}"))?;
    run_threads_local_with_engine(&engine, command).await
}

async fn run_threads_local_with_engine(
    engine: &ripd::SessionEngine,
    command: ThreadsCommand,
) -> anyhow::Result<()> {
    let store = engine.continuities();
    match command {
        ThreadsCommand::Ensure => {
            let thread_id = store
                .ensure_default()
                .map_err(|err| anyhow::anyhow!("thread ensure: {err}"))?;
            let payload = ThreadEnsureResponse { thread_id };
            println!("{}", serde_json::to_string(&payload)?);
        }
        ThreadsCommand::List => {
            let mut out = Vec::new();
            for meta in store.list() {
                out.push(ThreadMeta {
                    thread_id: meta.continuity_id,
                    created_at_ms: meta.created_at_ms,
                    title: meta.title,
                    archived: meta.archived,
                });
            }
            println!("{}", serde_json::to_string(&out)?);
        }
        ThreadsCommand::Get { id } => match store.get(&id) {
            Some(meta) => {
                let payload = ThreadMeta {
                    thread_id: id,
                    created_at_ms: meta.created_at_ms,
                    title: meta.title,
                    archived: meta.archived,
                };
                println!("{}", serde_json::to_string(&payload)?);
            }
            None => anyhow::bail!("thread get failed: not found"),
        },
        ThreadsCommand::Branch {
            id,
            title,
            from_message_id,
            from_seq,
            actor_id,
            origin,
        } => {
            let actor_id = actor_id.unwrap_or_else(|| "user".to_string());
            let origin = origin.unwrap_or_else(|| "cli".to_string());
            let (thread_id, parent_seq, parent_message_id) = store
                .branch(&id, title, from_message_id, from_seq, actor_id, origin)
                .map_err(|err| anyhow::anyhow!("thread branch failed: {err}"))?;
            let payload = ThreadBranchResponse {
                thread_id,
                parent_thread_id: id,
                parent_seq,
                parent_message_id,
            };
            println!("{}", serde_json::to_string(&payload)?);
        }
        ThreadsCommand::Handoff {
            id,
            title,
            summary_markdown,
            summary_artifact_id,
            from_message_id,
            from_seq,
            actor_id,
            origin,
        } => {
            let actor_id = actor_id.unwrap_or_else(|| "user".to_string());
            let origin = origin.unwrap_or_else(|| "cli".to_string());
            let (thread_id, from_seq, from_message_id) = store
                .handoff(
                    &id,
                    title,
                    (summary_markdown, summary_artifact_id),
                    from_message_id,
                    from_seq,
                    (actor_id, origin),
                )
                .map_err(|err| anyhow::anyhow!("thread handoff failed: {err}"))?;
            let payload = ThreadHandoffResponse {
                thread_id,
                from_thread_id: id,
                from_seq,
                from_message_id,
            };
            println!("{}", serde_json::to_string(&payload)?);
        }
        ThreadsCommand::PostMessage {
            id,
            content,
            actor_id,
            origin,
        } => {
            let actor_id = actor_id.unwrap_or_else(|| "user".to_string());
            let origin = origin.unwrap_or_else(|| "cli".to_string());
            let message_id = store
                .append_message(&id, actor_id.clone(), origin.clone(), content.clone())
                .map_err(|err| anyhow::anyhow!("thread post_message failed: {err}"))?;

            let handle = engine.create_session();
            let session_id = handle.session_id.clone();

            let run_link = ripd::ContinuityRunLink {
                continuity_id: id.clone(),
                message_id: message_id.clone(),
                actor_id: actor_id.clone(),
                origin: origin.clone(),
            };
            store
                .append_run_spawned(&id, &message_id, &session_id, actor_id, origin)
                .map_err(|err| anyhow::anyhow!("thread post_message run link failed: {err}"))?;

            engine.spawn_session(handle, content, Some(run_link));

            let payload = ThreadPostMessageResponse {
                thread_id: id,
                message_id,
                session_id,
            };
            println!("{}", serde_json::to_string(&payload)?);
        }
        ThreadsCommand::Events { id, max_events } => {
            let past = store
                .replay_events(&id)
                .map_err(|err| anyhow::anyhow!("thread events replay failed: {err}"))?;
            if past.is_empty() {
                anyhow::bail!("thread events failed: not found");
            }

            let mut receiver = store.subscribe();
            let stdout = io::stdout();
            let mut out = stdout.lock();
            stream_frames_local(&id, past, &mut receiver, max_events, &mut out).await?;
        }
    }

    Ok(())
}

async fn stream_frames_sse(
    stream: &mut (impl futures_util::Stream<Item = Result<Event, EventSourceError>> + Unpin),
    max_events: Option<usize>,
    out: &mut dyn Write,
) -> anyhow::Result<()> {
    let mut seen = 0usize;
    while let Some(next) = stream.next().await {
        match next {
            Ok(Event::Open) => {}
            Ok(Event::Message(msg)) => {
                serde_json::from_str::<FrameEvent>(&msg.data)
                    .map_err(|err| anyhow::anyhow!("invalid event frame: {err}"))?;
                writeln!(out, "{}", msg.data)?;
                out.flush()?;
                seen = seen.saturating_add(1);
                if max_events.map(|limit| seen >= limit).unwrap_or(false) {
                    break;
                }
            }
            Err(EventSourceError::StreamEnded) => break,
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

async fn stream_frames_local(
    thread_id: &str,
    past: Vec<FrameEvent>,
    receiver: &mut tokio::sync::broadcast::Receiver<FrameEvent>,
    max_events: Option<usize>,
    out: &mut dyn Write,
) -> anyhow::Result<()> {
    let mut seen = 0usize;
    let mut last_seq = past.last().map(|event| event.seq);

    for event in past {
        let json =
            serde_json::to_string(&event).map_err(|err| anyhow::anyhow!("event json: {err}"))?;
        writeln!(out, "{json}")?;
        out.flush()?;
        seen = seen.saturating_add(1);
        if max_events.map(|limit| seen >= limit).unwrap_or(false) {
            return Ok(());
        }
    }

    loop {
        match receiver.recv().await {
            Ok(event) => {
                if event.session_id != thread_id {
                    continue;
                }
                if last_seq.map(|seq| event.seq <= seq).unwrap_or(false) {
                    continue;
                }
                last_seq = Some(event.seq);
                let json = serde_json::to_string(&event)
                    .map_err(|err| anyhow::anyhow!("event json: {err}"))?;
                writeln!(out, "{json}")?;
                out.flush()?;
                seen = seen.saturating_add(1);
                if max_events.map(|limit| seen >= limit).unwrap_or(false) {
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use rip_kernel::EventKind;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::sync::broadcast;

    fn unique_tmp_root(prefix: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()))
    }

    #[tokio::test]
    async fn stream_frames_local_respects_max_events() {
        let root = unique_tmp_root("rip-cli-threads-stream");
        let data_dir = root.join("data");
        let workspace_dir = root.join("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("workspace");

        let engine =
            ripd::SessionEngine::new(data_dir, workspace_dir.clone(), None).expect("engine");
        let store = engine.continuities();
        let thread_id = store.ensure_default().expect("ensure");
        let message_id = store
            .append_message(
                &thread_id,
                "user".to_string(),
                "sdk-ts".to_string(),
                "hello".to_string(),
            )
            .expect("append message");
        let handle = engine.create_session();
        store
            .append_run_spawned(
                &thread_id,
                &message_id,
                &handle.session_id,
                "user".to_string(),
                "sdk-ts".to_string(),
            )
            .expect("append run spawned");

        let past = store.replay_events(&thread_id).expect("replay");
        assert_eq!(
            past.len(),
            3,
            "expected continuity_created + message + run spawned"
        );

        let mut receiver = store.subscribe();
        let mut buffer = Vec::new();
        stream_frames_local(&thread_id, past, &mut receiver, Some(3), &mut buffer)
            .await
            .expect("stream");

        let rendered = String::from_utf8(buffer).expect("utf8");
        let lines: Vec<&str> = rendered
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        assert_eq!(lines.len(), 3);

        let frames: Vec<FrameEvent> = lines
            .iter()
            .map(|line| serde_json::from_str(line).expect("frame json"))
            .collect();
        assert!(
            frames
                .iter()
                .any(|frame| matches!(frame.kind, EventKind::ContinuityCreated { .. })),
            "expected continuity_created"
        );
        assert!(
            frames
                .iter()
                .any(|frame| matches!(frame.kind, EventKind::ContinuityMessageAppended { .. })),
            "expected continuity_message_appended"
        );
        assert!(
            frames
                .iter()
                .any(|frame| matches!(frame.kind, EventKind::ContinuityRunSpawned { .. })),
            "expected continuity_run_spawned"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn run_threads_local_smoke() {
        let root = unique_tmp_root("rip-cli-threads-local");
        let data_dir = root.join("data");
        let workspace_dir = root.join("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("workspace");

        let engine = ripd::SessionEngine::new(data_dir.clone(), workspace_dir.clone(), None)
            .expect("engine");

        run_threads_local_with_engine(&engine, ThreadsCommand::Ensure)
            .await
            .expect("thread ensure");
        run_threads_local_with_engine(&engine, ThreadsCommand::List)
            .await
            .expect("thread list");

        let thread_id = engine
            .continuities()
            .ensure_default()
            .expect("default thread id");

        run_threads_local_with_engine(
            &engine,
            ThreadsCommand::Get {
                id: thread_id.clone(),
            },
        )
        .await
        .expect("thread get");

        // Simulate a separate CLI invocation (fresh seq cache) so we exercise the
        // `load_next_seq_for` path.
        let engine2 =
            ripd::SessionEngine::new(data_dir.clone(), workspace_dir, None).expect("engine2");
        run_threads_local_with_engine(
            &engine2,
            ThreadsCommand::PostMessage {
                id: thread_id.clone(),
                content: "hello".to_string(),
                actor_id: Some("user".to_string()),
                origin: Some("sdk-ts".to_string()),
            },
        )
        .await
        .expect("thread post_message");

        run_threads_local_with_engine(
            &engine2,
            ThreadsCommand::Events {
                id: thread_id,
                max_events: Some(1),
            },
        )
        .await
        .expect("thread events");

        // Avoid deleting the temp dirs while the background run is still writing frames.
        let log_path = data_dir.join("events.jsonl");
        for _ in 0..100 {
            if let Ok(log) = std::fs::read_to_string(&log_path) {
                let events: Vec<FrameEvent> = log
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .filter_map(|line| serde_json::from_str(line).ok())
                    .collect();
                let run_session_id = events.iter().find_map(|event| match &event.kind {
                    EventKind::ContinuityRunSpawned { run_session_id, .. } => {
                        Some(run_session_id.clone())
                    }
                    _ => None,
                });
                if let Some(run_session_id) = run_session_id {
                    if events.iter().any(|event| {
                        event.session_id == run_session_id
                            && matches!(event.kind, EventKind::SessionEnded { .. })
                    }) {
                        break;
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn run_threads_local_unknown_thread_errors() {
        let root = unique_tmp_root("rip-cli-threads-local-missing");
        let data_dir = root.join("data");
        let workspace_dir = root.join("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("workspace");
        let engine =
            ripd::SessionEngine::new(data_dir, workspace_dir.clone(), None).expect("engine");

        let err = run_threads_local_with_engine(
            &engine,
            ThreadsCommand::Get {
                id: "missing-thread".to_string(),
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("thread get failed"));

        let err = run_threads_local_with_engine(
            &engine,
            ThreadsCommand::Events {
                id: "missing-thread".to_string(),
                max_events: Some(1),
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("thread events"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn run_threads_local_post_message_defaults_provenance() {
        let root = unique_tmp_root("rip-cli-threads-local-provenance");
        let data_dir = root.join("data");
        let workspace_dir = root.join("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("workspace");

        let engine =
            ripd::SessionEngine::new(data_dir.clone(), workspace_dir, None).expect("engine");
        let store = engine.continuities();
        let thread_id = store.ensure_default().expect("ensure");

        let mut receiver = store.subscribe();

        run_threads_local_with_engine(
            &engine,
            ThreadsCommand::PostMessage {
                id: thread_id.clone(),
                content: "hello".to_string(),
                actor_id: None,
                origin: None,
            },
        )
        .await
        .expect("thread post_message");

        let mut saw_message = false;
        let mut saw_run_spawned = false;
        let mut run_session_id: Option<String> = None;
        for _ in 0..2 {
            let event = receiver.recv().await.expect("recv");
            match event.kind {
                EventKind::ContinuityMessageAppended {
                    actor_id,
                    origin,
                    content,
                } => {
                    if content == "hello" {
                        assert_eq!(actor_id, "user");
                        assert_eq!(origin, "cli");
                        saw_message = true;
                    }
                }
                EventKind::ContinuityRunSpawned {
                    run_session_id: id,
                    actor_id,
                    origin,
                    ..
                } => {
                    assert_eq!(actor_id.as_deref(), Some("user"));
                    assert_eq!(origin.as_deref(), Some("cli"));
                    run_session_id = Some(id);
                    saw_run_spawned = true;
                }
                _ => {}
            }
        }
        assert!(saw_message, "expected continuity_message_appended");
        assert!(saw_run_spawned, "expected continuity_run_spawned");

        let log_path = data_dir.join("events.jsonl");
        if let Some(run_session_id) = run_session_id {
            for _ in 0..100 {
                if let Ok(log) = std::fs::read_to_string(&log_path) {
                    let events: Vec<FrameEvent> = log
                        .lines()
                        .filter(|line| !line.trim().is_empty())
                        .filter_map(|line| serde_json::from_str(line).ok())
                        .collect();
                    if events.iter().any(|event| {
                        event.session_id == run_session_id
                            && matches!(event.kind, EventKind::SessionEnded { .. })
                    }) {
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn run_threads_local_branch_defaults_provenance() {
        let root = unique_tmp_root("rip-cli-threads-local-branch");
        let data_dir = root.join("data");
        let workspace_dir = root.join("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("workspace");

        let engine = ripd::SessionEngine::new(data_dir, workspace_dir, None).expect("engine");
        let store = engine.continuities();
        let parent_thread_id = store.ensure_default().expect("ensure");
        let m1 = store
            .append_message(
                &parent_thread_id,
                "user".to_string(),
                "cli".to_string(),
                "hello".to_string(),
            )
            .expect("append");
        store
            .append_run_spawned(
                &parent_thread_id,
                &m1,
                "session-1",
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");
        store
            .append_run_ended(
                &parent_thread_id,
                &m1,
                "session-1",
                "completed".to_string(),
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run ended");

        let mut receiver = store.subscribe();

        run_threads_local_with_engine(
            &engine,
            ThreadsCommand::Branch {
                id: parent_thread_id.clone(),
                title: Some("child".to_string()),
                from_message_id: Some(m1.clone()),
                from_seq: None,
                actor_id: None,
                origin: None,
            },
        )
        .await
        .expect("thread branch");

        let mut saw_created = false;
        let mut saw_branched = false;
        for _ in 0..2 {
            let event = receiver.recv().await.expect("recv");
            match event.kind {
                EventKind::ContinuityCreated { title, .. } => {
                    if title.as_deref() == Some("child") {
                        saw_created = true;
                    }
                }
                EventKind::ContinuityBranched {
                    parent_thread_id: event_parent_id,
                    parent_message_id,
                    actor_id,
                    origin,
                    ..
                } => {
                    assert_eq!(event_parent_id, parent_thread_id);
                    assert_eq!(parent_message_id.as_deref(), Some(m1.as_str()));
                    assert_eq!(actor_id, "user");
                    assert_eq!(origin, "cli");
                    saw_branched = true;
                }
                _ => {}
            }
        }
        assert!(saw_created, "expected continuity_created for branch");
        assert!(saw_branched, "expected continuity_branched");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn run_threads_local_handoff_defaults_provenance() {
        let root = unique_tmp_root("rip-cli-threads-local-handoff");
        let data_dir = root.join("data");
        let workspace_dir = root.join("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("workspace");

        let engine =
            ripd::SessionEngine::new(data_dir, workspace_dir.clone(), None).expect("engine");
        let store = engine.continuities();
        let parent_thread_id = store.ensure_default().expect("ensure");
        let m1 = store
            .append_message(
                &parent_thread_id,
                "user".to_string(),
                "cli".to_string(),
                "hello".to_string(),
            )
            .expect("append");
        store
            .append_run_spawned(
                &parent_thread_id,
                &m1,
                "session-1",
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");
        store
            .append_run_ended(
                &parent_thread_id,
                &m1,
                "session-1",
                "completed".to_string(),
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run ended");

        let mut receiver = store.subscribe();

        run_threads_local_with_engine(
            &engine,
            ThreadsCommand::Handoff {
                id: parent_thread_id.clone(),
                title: Some("handoff".to_string()),
                summary_markdown: Some("summary".to_string()),
                summary_artifact_id: None,
                from_message_id: Some(m1.clone()),
                from_seq: None,
                actor_id: None,
                origin: None,
            },
        )
        .await
        .expect("thread handoff");

        let mut saw_created = false;
        let mut saw_handoff = false;
        let mut bundle_artifact_id: Option<String> = None;
        for _ in 0..2 {
            let event = receiver.recv().await.expect("recv");
            match event.kind {
                EventKind::ContinuityCreated { title, .. } => {
                    if title.as_deref() == Some("handoff") {
                        saw_created = true;
                    }
                }
                EventKind::ContinuityHandoffCreated {
                    from_thread_id: event_from_id,
                    from_seq,
                    from_message_id,
                    summary_markdown,
                    summary_artifact_id,
                    actor_id,
                    origin,
                    ..
                } => {
                    assert_eq!(event_from_id, parent_thread_id);
                    assert_eq!(from_seq, 3);
                    assert_eq!(from_message_id.as_deref(), Some(m1.as_str()));
                    assert_eq!(summary_markdown.as_deref(), Some("summary"));
                    let artifact_id = summary_artifact_id.as_deref().expect("summary_artifact_id");
                    assert_eq!(artifact_id.len(), 64);
                    bundle_artifact_id = Some(artifact_id.to_string());
                    assert_eq!(actor_id, "user");
                    assert_eq!(origin, "cli");
                    saw_handoff = true;
                }
                _ => {}
            }
        }
        assert!(saw_created, "expected continuity_created for handoff");
        assert!(saw_handoff, "expected continuity_handoff_created");
        let artifact_id = bundle_artifact_id.expect("bundle artifact id");
        let blob_path = workspace_dir
            .join(".rip")
            .join("artifacts")
            .join("blobs")
            .join(&artifact_id);
        let bytes = std::fs::read(&blob_path).expect("read bundle artifact");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("bundle json");
        assert_eq!(
            json.get("schema").and_then(|v| v.as_str()),
            Some("rip.handoff_context_bundle.v1")
        );
        assert_eq!(
            json.get("summary_markdown").and_then(|v| v.as_str()),
            Some("summary")
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    fn continuity_created_frame(thread_id: &str, seq: u64) -> FrameEvent {
        FrameEvent {
            id: format!("e{seq}"),
            session_id: thread_id.to_string(),
            timestamp_ms: 0,
            seq,
            kind: EventKind::ContinuityCreated {
                workspace: "w".to_string(),
                title: None,
            },
        }
    }

    #[tokio::test]
    async fn run_threads_remote_events_stops_on_stream_end() {
        let server = MockServer::start();
        let _events = server.mock(|when, then| {
            when.method(GET).path("/threads/t1/events");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body("");
        });

        run_threads(
            Some(server.base_url()),
            ThreadsCommand::Events {
                id: "t1".to_string(),
                max_events: None,
            },
        )
        .await
        .expect("remote events");
    }

    #[tokio::test]
    async fn run_threads_remote_events_errors_on_invalid_json() {
        let server = MockServer::start();
        let _events = server.mock(|when, then| {
            when.method(GET).path("/threads/t1/events");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body("data: not json\n\n");
        });

        let err = run_threads(
            Some(server.base_url()),
            ThreadsCommand::Events {
                id: "t1".to_string(),
                max_events: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("invalid event frame"));
    }

    #[tokio::test]
    async fn run_threads_remote_events_writes_frames_until_stream_end() {
        let server = MockServer::start();
        let payload = serde_json::to_string(&continuity_created_frame("t1", 0)).expect("json");
        let _events = server.mock(|when, then| {
            when.method(GET).path("/threads/t1/events");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(format!("data: {payload}\n\n"));
        });

        run_threads(
            Some(server.base_url()),
            ThreadsCommand::Events {
                id: "t1".to_string(),
                max_events: None,
            },
        )
        .await
        .expect("remote events");
    }

    #[tokio::test]
    async fn stream_frames_local_filters_and_stops_after_limit() {
        let past = vec![continuity_created_frame("t1", 0)];
        let (sender, mut receiver) = broadcast::channel(16);

        let _ = sender.send(continuity_created_frame("other", 1));
        let _ = sender.send(continuity_created_frame("t1", 0));
        let _ = sender.send(continuity_created_frame("t1", 1));

        let mut buffer = Vec::new();
        stream_frames_local("t1", past, &mut receiver, Some(2), &mut buffer)
            .await
            .expect("stream");
        let rendered = String::from_utf8(buffer).expect("utf8");
        let lines: Vec<&str> = rendered
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        assert_eq!(lines.len(), 2);
        let frames: Vec<FrameEvent> = lines
            .iter()
            .map(|line| serde_json::from_str(line).expect("frame"))
            .collect();
        assert_eq!(frames[0].seq, 0);
        assert_eq!(frames[1].seq, 1);
        assert_eq!(frames[1].session_id, "t1");
    }

    #[tokio::test]
    async fn stream_frames_local_exits_on_closed_channel() {
        let past = vec![continuity_created_frame("t1", 0)];
        let (sender, mut receiver) = broadcast::channel(1);
        drop(sender);

        let mut buffer = Vec::new();
        stream_frames_local("t1", past, &mut receiver, None, &mut buffer)
            .await
            .expect("stream");
        let rendered = String::from_utf8(buffer).expect("utf8");
        assert_eq!(rendered.lines().count(), 1);
    }

    #[tokio::test]
    async fn run_threads_remote_smoke() {
        let server = MockServer::start();

        let _ensure = server.mock(|when, then| {
            when.method(POST).path("/threads/ensure");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"thread_id":"t1"}"#);
        });
        let _list = server.mock(|when, then| {
            when.method(GET).path("/threads");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"[{"thread_id":"t1","created_at_ms":0,"title":null,"archived":false}]"#);
        });
        let _get = server.mock(|when, then| {
            when.method(GET).path("/threads/t1");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"thread_id":"t1","created_at_ms":0,"title":null,"archived":false}"#);
        });
        let _post = server.mock(|when, then| {
            when.method(POST).path("/threads/t1/messages");
            then.status(202)
                .header("content-type", "application/json")
                .body(r#"{"thread_id":"t1","message_id":"m1","session_id":"s1"}"#);
        });
        let payload = serde_json::json!({
            "id": "e1",
            "session_id": "t1",
            "stream_kind": "continuity",
            "stream_id": "t1",
            "timestamp_ms": 0,
            "seq": 0,
            "type": "continuity_created",
            "workspace": "w",
            "title": null
        })
        .to_string();
        let _events = server.mock(|when, then| {
            when.method(GET).path("/threads/t1/events");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(format!("data: {payload}\n\n"));
        });
        let _branch = server.mock(|when, then| {
            when.method(POST).path("/threads/t1/branch");
            then.status(201)
                .header("content-type", "application/json")
                .body(r#"{"thread_id":"t2","parent_thread_id":"t1","parent_seq":0}"#);
        });
        let _handoff = server.mock(|when, then| {
            when.method(POST).path("/threads/t1/handoff");
            then.status(201)
                .header("content-type", "application/json")
                .body(r#"{"thread_id":"t3","from_thread_id":"t1","from_seq":0}"#);
        });

        run_threads(Some(server.base_url()), ThreadsCommand::Ensure)
            .await
            .expect("remote ensure");
        run_threads(Some(server.base_url()), ThreadsCommand::List)
            .await
            .expect("remote list");
        run_threads(
            Some(server.base_url()),
            ThreadsCommand::Get {
                id: "t1".to_string(),
            },
        )
        .await
        .expect("remote get");
        run_threads(
            Some(server.base_url()),
            ThreadsCommand::PostMessage {
                id: "t1".to_string(),
                content: "hi".to_string(),
                actor_id: Some("user".to_string()),
                origin: Some("sdk-ts".to_string()),
            },
        )
        .await
        .expect("remote post_message");
        run_threads(
            Some(server.base_url()),
            ThreadsCommand::Events {
                id: "t1".to_string(),
                max_events: Some(1),
            },
        )
        .await
        .expect("remote events");
        run_threads(
            Some(server.base_url()),
            ThreadsCommand::Branch {
                id: "t1".to_string(),
                title: Some("child".to_string()),
                from_message_id: None,
                from_seq: None,
                actor_id: Some("user".to_string()),
                origin: Some("sdk-ts".to_string()),
            },
        )
        .await
        .expect("remote branch");
        run_threads(
            Some(server.base_url()),
            ThreadsCommand::Handoff {
                id: "t1".to_string(),
                title: Some("handoff".to_string()),
                summary_markdown: Some("summary".to_string()),
                summary_artifact_id: None,
                from_message_id: None,
                from_seq: None,
                actor_id: Some("user".to_string()),
                origin: Some("sdk-ts".to_string()),
            },
        )
        .await
        .expect("remote handoff");
    }

    #[tokio::test]
    async fn run_threads_remote_errors_on_non_success_status() {
        let server = MockServer::start();
        let _get = server.mock(|when, then| {
            when.method(GET).path("/threads/missing");
            then.status(404);
        });

        let err = run_threads(
            Some(server.base_url()),
            ThreadsCommand::Get {
                id: "missing".to_string(),
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("thread get failed"));
    }

    #[tokio::test]
    async fn run_threads_remote_errors_on_non_success_status_for_other_routes() {
        let server = MockServer::start();

        let _ensure = server.mock(|when, then| {
            when.method(POST).path("/threads/ensure");
            then.status(500);
        });
        let err = run_threads(Some(server.base_url()), ThreadsCommand::Ensure)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("thread ensure failed"));

        let _list = server.mock(|when, then| {
            when.method(GET).path("/threads");
            then.status(500);
        });
        let err = run_threads(Some(server.base_url()), ThreadsCommand::List)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("thread list failed"));

        let _post = server.mock(|when, then| {
            when.method(POST).path("/threads/t1/messages");
            then.status(404);
        });
        let err = run_threads(
            Some(server.base_url()),
            ThreadsCommand::PostMessage {
                id: "t1".to_string(),
                content: "hi".to_string(),
                actor_id: None,
                origin: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("thread post_message failed"));

        let _branch = server.mock(|when, then| {
            when.method(POST).path("/threads/t1/branch");
            then.status(500);
        });
        let err = run_threads(
            Some(server.base_url()),
            ThreadsCommand::Branch {
                id: "t1".to_string(),
                title: None,
                from_message_id: None,
                from_seq: None,
                actor_id: None,
                origin: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("thread branch failed"));

        let _handoff = server.mock(|when, then| {
            when.method(POST).path("/threads/t1/handoff");
            then.status(500);
        });
        let err = run_threads(
            Some(server.base_url()),
            ThreadsCommand::Handoff {
                id: "t1".to_string(),
                title: None,
                summary_markdown: Some("summary".to_string()),
                summary_artifact_id: None,
                from_message_id: None,
                from_seq: None,
                actor_id: None,
                origin: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("thread handoff failed"));
    }
}
