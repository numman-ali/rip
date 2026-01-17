use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use futures_util::future::BoxFuture;
use rip_kernel::{Event, EventKind};
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

pub type ToolHandler = Arc<dyn Fn(ToolInvocation) -> BoxFuture<'static, ToolOutput> + Send + Sync>;

#[derive(Default)]
pub struct ToolRegistry {
    tools: Mutex<HashMap<String, ToolHandler>>,
}

impl ToolRegistry {
    pub fn register(&self, name: impl Into<String>, handler: ToolHandler) {
        let mut tools = self.tools.lock().expect("tool registry mutex");
        tools.insert(name.into(), handler);
    }

    pub fn get(&self, name: &str) -> Option<ToolHandler> {
        let tools = self.tools.lock().expect("tool registry mutex");
        tools.get(name).cloned()
    }
}

pub struct ToolRunner {
    registry: Arc<ToolRegistry>,
    semaphore: Arc<Semaphore>,
}

impl ToolRunner {
    pub fn new(registry: Arc<ToolRegistry>, max_concurrency: usize) -> Self {
        Self {
            registry,
            semaphore: Arc::new(Semaphore::new(max_concurrency.max(1))),
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
