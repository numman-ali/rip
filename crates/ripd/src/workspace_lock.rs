use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Clone)]
pub(crate) struct WorkspaceLock {
    semaphore: Arc<Semaphore>,
}

pub(crate) struct WorkspaceGuard {
    _permit: OwnedSemaphorePermit,
}

impl WorkspaceLock {
    pub(crate) fn new() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(1)),
        }
    }

    pub(crate) async fn acquire(&self) -> WorkspaceGuard {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("workspace lock semaphore closed");
        WorkspaceGuard { _permit: permit }
    }
}

pub(crate) fn requires_workspace_lock(tool_name: &str) -> bool {
    !matches!(tool_name, "read" | "ls" | "grep" | "artifact_fetch")
}
