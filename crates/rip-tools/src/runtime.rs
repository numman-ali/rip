use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use futures_util::future::BoxFuture;
use rip_kernel::{CheckpointAction, Event, EventKind};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::Semaphore;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct ToolInvocation {
    pub name: String,
    pub args: Value,
    pub timeout_ms: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct ToolOutput {
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
    pub exit_code: i32,
    pub artifacts: Option<Value>,
}

impl ToolOutput {
    pub fn success(stdout: Vec<String>) -> Self {
        Self {
            stdout,
            stderr: Vec::new(),
            exit_code: 0,
            artifacts: None,
        }
    }

    pub fn failure(stderr: Vec<String>) -> Self {
        Self {
            stdout: Vec::new(),
            stderr,
            exit_code: 1,
            artifacts: None,
        }
    }

    pub fn invalid_args(message: impl Into<String>) -> Self {
        Self {
            stdout: Vec::new(),
            stderr: vec![message.into()],
            exit_code: 2,
            artifacts: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CheckpointRequest {
    pub session_id: String,
    pub label: String,
    pub files: Vec<PathBuf>,
    pub auto: bool,
    pub tool_name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CheckpointRecord {
    pub id: String,
    pub label: String,
    pub created_at_ms: u64,
    pub files: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CheckpointRewindRecord {
    pub id: String,
    pub label: String,
    pub files: Vec<String>,
}

pub trait CheckpointHook: Send + Sync {
    fn create(&self, request: CheckpointRequest) -> Result<CheckpointRecord, String>;
    fn rewind(
        &self,
        session_id: &str,
        checkpoint_id: &str,
    ) -> Result<CheckpointRewindRecord, String>;
}

pub type ToolHandler = Arc<dyn Fn(ToolInvocation) -> BoxFuture<'static, ToolOutput> + Send + Sync>;

#[derive(Default)]
pub struct ToolRegistry {
    tools: Mutex<HashMap<String, ToolHandler>>,
    aliases: Mutex<HashMap<String, String>>,
}

impl ToolRegistry {
    pub fn register(&self, name: impl Into<String>, handler: ToolHandler) {
        let mut tools = self.tools.lock().expect("tool registry mutex");
        tools.insert(name.into(), handler);
    }

    pub fn register_alias(&self, alias: impl Into<String>, target: impl Into<String>) {
        let mut aliases = self.aliases.lock().expect("tool alias mutex");
        aliases.insert(alias.into(), target.into());
    }

    pub fn get(&self, name: &str) -> Option<ToolHandler> {
        let tools = self.tools.lock().expect("tool registry mutex");
        if let Some(handler) = tools.get(name) {
            return Some(handler.clone());
        }
        drop(tools);
        let aliases = self.aliases.lock().expect("tool alias mutex");
        let target = aliases.get(name)?.clone();
        drop(aliases);
        let tools = self.tools.lock().expect("tool registry mutex");
        tools.get(&target).cloned()
    }
}

pub struct ToolRunner {
    registry: Arc<ToolRegistry>,
    semaphore: Arc<Semaphore>,
    checkpoint_hook: Option<Arc<dyn CheckpointHook>>,
}

impl ToolRunner {
    pub fn new(registry: Arc<ToolRegistry>, max_concurrency: usize) -> Self {
        Self {
            registry,
            semaphore: Arc::new(Semaphore::new(max_concurrency.max(1))),
            checkpoint_hook: None,
        }
    }

    pub fn with_checkpoint_hook(
        registry: Arc<ToolRegistry>,
        max_concurrency: usize,
        hook: Arc<dyn CheckpointHook>,
    ) -> Self {
        Self {
            registry,
            semaphore: Arc::new(Semaphore::new(max_concurrency.max(1))),
            checkpoint_hook: Some(hook),
        }
    }

    pub async fn run(
        &self,
        session_id: &str,
        seq: &mut u64,
        invocation: ToolInvocation,
    ) -> Vec<Event> {
        let _permit = self.semaphore.acquire().await.expect("semaphore");
        let tool_id = Uuid::new_v4().to_string();
        let started_at = Instant::now();

        let mut events = Vec::new();
        self.emit_checkpoint_events(session_id, seq, &invocation, &mut events);
        events.push(self.emit(
            session_id,
            seq,
            EventKind::ToolStarted {
                tool_id: tool_id.clone(),
                name: invocation.name.clone(),
                args: invocation.args.clone(),
                timeout_ms: invocation.timeout_ms,
            },
        ));

        let handler = match self.registry.get(&invocation.name) {
            Some(handler) => handler,
            None => {
                events.push(self.emit(
                    session_id,
                    seq,
                    EventKind::ToolFailed {
                        tool_id,
                        error: "unknown tool".to_string(),
                    },
                ));
                return events;
            }
        };

        let output = if let Some(timeout_ms) = invocation.timeout_ms {
            match tokio::time::timeout(
                Duration::from_millis(timeout_ms),
                (handler)(invocation.clone()),
            )
            .await
            {
                Ok(output) => Ok(output),
                Err(_) => Err("timeout".to_string()),
            }
        } else {
            Ok((handler)(invocation.clone()).await)
        };

        match output {
            Ok(output) => {
                for chunk in output.stdout {
                    events.push(self.emit(
                        session_id,
                        seq,
                        EventKind::ToolStdout {
                            tool_id: tool_id.clone(),
                            chunk,
                        },
                    ));
                }
                for chunk in output.stderr {
                    events.push(self.emit(
                        session_id,
                        seq,
                        EventKind::ToolStderr {
                            tool_id: tool_id.clone(),
                            chunk,
                        },
                    ));
                }
                events.push(self.emit(
                    session_id,
                    seq,
                    EventKind::ToolEnded {
                        tool_id,
                        exit_code: output.exit_code,
                        duration_ms: started_at.elapsed().as_millis() as u64,
                        artifacts: output.artifacts,
                    },
                ));
            }
            Err(error) => {
                events.push(self.emit(session_id, seq, EventKind::ToolFailed { tool_id, error }));
            }
        }

        events
    }

    pub fn rewind_checkpoint(
        &self,
        session_id: &str,
        seq: &mut u64,
        checkpoint_id: &str,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        let Some(hook) = &self.checkpoint_hook else {
            events.push(self.emit(
                session_id,
                seq,
                EventKind::CheckpointFailed {
                    action: CheckpointAction::Rewind,
                    error: "checkpoint hook not configured".to_string(),
                },
            ));
            return events;
        };

        match hook.rewind(session_id, checkpoint_id) {
            Ok(record) => events.push(self.emit(
                session_id,
                seq,
                EventKind::CheckpointRewound {
                    checkpoint_id: record.id,
                    label: record.label,
                    files: record.files,
                },
            )),
            Err(error) => events.push(self.emit(
                session_id,
                seq,
                EventKind::CheckpointFailed {
                    action: CheckpointAction::Rewind,
                    error,
                },
            )),
        }

        events
    }

    pub fn create_checkpoint(
        &self,
        session_id: &str,
        seq: &mut u64,
        label: String,
        files: Vec<PathBuf>,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        let Some(hook) = &self.checkpoint_hook else {
            events.push(self.emit(
                session_id,
                seq,
                EventKind::CheckpointFailed {
                    action: CheckpointAction::Create,
                    error: "checkpoint hook not configured".to_string(),
                },
            ));
            return events;
        };

        let request = CheckpointRequest {
            session_id: session_id.to_string(),
            label,
            files,
            auto: false,
            tool_name: None,
        };

        match hook.create(request) {
            Ok(record) => events.push(self.emit(
                session_id,
                seq,
                EventKind::CheckpointCreated {
                    checkpoint_id: record.id,
                    label: record.label,
                    created_at_ms: record.created_at_ms,
                    files: record.files,
                    auto: false,
                    tool_name: None,
                },
            )),
            Err(error) => events.push(self.emit(
                session_id,
                seq,
                EventKind::CheckpointFailed {
                    action: CheckpointAction::Create,
                    error,
                },
            )),
        }

        events
    }

    fn emit_checkpoint_events(
        &self,
        session_id: &str,
        seq: &mut u64,
        invocation: &ToolInvocation,
        events: &mut Vec<Event>,
    ) {
        let Some(hook) = &self.checkpoint_hook else {
            return;
        };
        let files = match files_for_invocation(invocation) {
            Ok(Some(files)) => files,
            Ok(None) => return,
            Err(error) => {
                events.push(self.emit(
                    session_id,
                    seq,
                    EventKind::CheckpointFailed {
                        action: CheckpointAction::Create,
                        error,
                    },
                ));
                return;
            }
        };
        let label = format!("auto:{}", invocation.name);
        let request = CheckpointRequest {
            session_id: session_id.to_string(),
            label,
            files,
            auto: true,
            tool_name: Some(invocation.name.clone()),
        };

        match hook.create(request) {
            Ok(record) => events.push(self.emit(
                session_id,
                seq,
                EventKind::CheckpointCreated {
                    checkpoint_id: record.id,
                    label: record.label,
                    created_at_ms: record.created_at_ms,
                    files: record.files,
                    auto: true,
                    tool_name: Some(invocation.name.clone()),
                },
            )),
            Err(error) => events.push(self.emit(
                session_id,
                seq,
                EventKind::CheckpointFailed {
                    action: CheckpointAction::Create,
                    error,
                },
            )),
        }
    }

    fn emit(&self, session_id: &str, seq: &mut u64, kind: EventKind) -> Event {
        let event = Event {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            timestamp_ms: now_ms(),
            seq: *seq,
            kind,
        };
        *seq += 1;
        event
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Deserialize)]
struct WriteArgs {
    path: String,
}

#[derive(Deserialize)]
struct ApplyPatchArgs {
    patch: String,
}

fn files_for_invocation(invocation: &ToolInvocation) -> Result<Option<Vec<PathBuf>>, String> {
    match invocation.name.as_str() {
        "write" => {
            let args: WriteArgs = serde_json::from_value(invocation.args.clone())
                .map_err(|err| format!("checkpoint args invalid: {err}"))?;
            Ok(Some(vec![PathBuf::from(args.path)]))
        }
        "apply_patch" => {
            let args: ApplyPatchArgs = serde_json::from_value(invocation.args.clone())
                .map_err(|err| format!("checkpoint args invalid: {err}"))?;
            let patch = rip_workspace::Patch::parse(&args.patch).map_err(|err| format!("{err}"))?;
            Ok(Some(patch.affected_paths()))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::future::pending;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn runs_tool_and_streams_output() {
        let registry = Arc::new(ToolRegistry::default());
        registry.register(
            "echo",
            Arc::new(|invocation| {
                Box::pin(async move {
                    ToolOutput {
                        stdout: vec![format!("hi:{}", invocation.args)],
                        stderr: vec!["warn".to_string()],
                        exit_code: 0,
                        artifacts: Some(serde_json::json!({"ok": true})),
                    }
                })
            }),
        );

        let runner = ToolRunner::new(registry, 2);
        let mut seq = 0;
        let events = runner
            .run(
                "session-1",
                &mut seq,
                ToolInvocation {
                    name: "echo".to_string(),
                    args: serde_json::json!("world"),
                    timeout_ms: None,
                },
            )
            .await;

        assert!(matches!(events[0].kind, EventKind::ToolStarted { .. }));
        assert!(events
            .iter()
            .any(|event| matches!(event.kind, EventKind::ToolStdout { .. })));
        assert!(events
            .iter()
            .any(|event| matches!(event.kind, EventKind::ToolStderr { .. })));
        assert!(matches!(
            events.last().map(|event| &event.kind),
            Some(EventKind::ToolEnded { .. })
        ));
    }

    #[tokio::test]
    async fn alias_resolves_to_target() {
        let registry = Arc::new(ToolRegistry::default());
        registry.register(
            "bash",
            Arc::new(|_invocation| {
                Box::pin(async move { ToolOutput::success(vec!["ok".to_string()]) })
            }),
        );
        registry.register_alias("shell", "bash");

        let handler = registry.get("shell").expect("alias");
        let output = handler(ToolInvocation {
            name: "shell".to_string(),
            args: serde_json::json!({}),
            timeout_ms: None,
        })
        .await;

        assert_eq!(output.exit_code, 0);
        assert_eq!(output.stdout, vec!["ok".to_string()]);
    }

    #[tokio::test]
    async fn enforces_timeout() {
        let registry = Arc::new(ToolRegistry::default());
        registry.register(
            "slow",
            Arc::new(|_invocation| Box::pin(async move { pending::<ToolOutput>().await })),
        );

        let runner = ToolRunner::new(registry, 1);
        let mut seq = 0;
        let events = runner
            .run(
                "session-1",
                &mut seq,
                ToolInvocation {
                    name: "slow".to_string(),
                    args: serde_json::json!({}),
                    timeout_ms: Some(10),
                },
            )
            .await;

        assert!(events
            .iter()
            .any(|event| matches!(event.kind, EventKind::ToolFailed { .. })));
    }

    #[tokio::test]
    async fn timeout_allows_fast_tool() {
        let registry = Arc::new(ToolRegistry::default());
        registry.register(
            "fast",
            Arc::new(|_invocation| {
                Box::pin(async move { ToolOutput::success(vec!["ok".to_string()]) })
            }),
        );

        let runner = ToolRunner::new(registry, 1);
        let mut seq = 0;
        let events = runner
            .run(
                "session-1",
                &mut seq,
                ToolInvocation {
                    name: "fast".to_string(),
                    args: serde_json::json!({}),
                    timeout_ms: Some(50),
                },
            )
            .await;

        assert!(events
            .iter()
            .any(|event| matches!(event.kind, EventKind::ToolEnded { .. })));
    }

    #[tokio::test]
    async fn unknown_tool_emits_failure() {
        let registry = Arc::new(ToolRegistry::default());
        let runner = ToolRunner::new(registry, 1);
        let mut seq = 0;
        let events = runner
            .run(
                "session-1",
                &mut seq,
                ToolInvocation {
                    name: "missing".to_string(),
                    args: serde_json::json!({}),
                    timeout_ms: None,
                },
            )
            .await;

        assert!(events
            .iter()
            .any(|event| matches!(event.kind, EventKind::ToolFailed { .. })));
    }

    #[tokio::test]
    async fn limits_concurrency() {
        let registry = Arc::new(ToolRegistry::default());
        let active = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));

        let active_clone = active.clone();
        let max_clone = max_seen.clone();
        registry.register(
            "block",
            Arc::new(move |_invocation| {
                let active = active_clone.clone();
                let max_seen = max_clone.clone();
                Box::pin(async move {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    loop {
                        let prev = max_seen.load(Ordering::SeqCst);
                        if current > prev {
                            if max_seen
                                .compare_exchange(prev, current, Ordering::SeqCst, Ordering::SeqCst)
                                .is_ok()
                            {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    active.fetch_sub(1, Ordering::SeqCst);
                    ToolOutput::success(vec!["ok".to_string()])
                })
            }),
        );

        let runner = ToolRunner::new(registry, 1);
        let mut seq1 = 0;
        let mut seq2 = 0;
        let first = runner.run(
            "session-1",
            &mut seq1,
            ToolInvocation {
                name: "block".to_string(),
                args: serde_json::json!({}),
                timeout_ms: None,
            },
        );
        let second = runner.run(
            "session-1",
            &mut seq2,
            ToolInvocation {
                name: "block".to_string(),
                args: serde_json::json!({}),
                timeout_ms: None,
            },
        );

        let _ = tokio::join!(first, second);
        assert_eq!(max_seen.load(Ordering::SeqCst), 1);
    }
}
