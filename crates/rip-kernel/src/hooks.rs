use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEventKind {
    SessionStarted,
    Output,
    SessionEnded,
}

#[derive(Debug, Clone)]
pub struct HookContext {
    pub session_id: String,
    pub seq: u64,
    pub timestamp_ms: u64,
    pub event: HookEventKind,
    pub output: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookOutcome {
    Continue,
    Abort { reason: String },
}

pub type HookHandler = Arc<dyn Fn(&HookContext) -> HookOutcome + Send + Sync>;

#[derive(Clone)]
pub struct Hook {
    pub name: String,
    pub event: HookEventKind,
    pub handler: HookHandler,
}

impl Hook {
    pub fn new(
        name: impl Into<String>,
        event: HookEventKind,
        handler: HookHandler,
    ) -> Self {
        Self {
            name: name.into(),
            event,
            handler,
        }
    }
}

#[derive(Default)]
pub struct HookEngine {
    hooks: Mutex<Vec<Hook>>,
}

impl HookEngine {
    pub fn new() -> Self {
        Self {
            hooks: Mutex::new(Vec::new()),
        }
    }

    pub fn register(&self, hook: Hook) {
        let mut hooks = self.hooks.lock().expect("hook engine mutex");
        hooks.push(hook);
    }

    pub fn run(&self, ctx: &HookContext) -> HookOutcome {
        let hooks = self.hooks.lock().expect("hook engine mutex");
        for hook in hooks.iter().filter(|hook| hook.event == ctx.event) {
            match (hook.handler)(ctx) {
                HookOutcome::Continue => {}
                HookOutcome::Abort { reason } => {
                    return HookOutcome::Abort { reason };
                }
            }
        }
        HookOutcome::Continue
    }
}
