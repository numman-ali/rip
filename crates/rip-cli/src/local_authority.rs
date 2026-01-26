use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use reqwest::Client;

fn update_last_state(last_state: &mut Option<String>, backoff_ms: &mut u64, next: String) {
    if last_state.as_deref() != Some(next.as_str()) {
        *last_state = Some(next);
        *backoff_ms = 20;
    }
}

pub(crate) fn default_data_dir() -> PathBuf {
    if let Ok(value) = std::env::var("RIP_DATA_DIR") {
        return PathBuf::from(value);
    }
    PathBuf::from("data")
}

pub(crate) fn default_workspace_root() -> PathBuf {
    if let Ok(value) = std::env::var("RIP_WORKSPACE_ROOT") {
        return PathBuf::from(value);
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub(crate) async fn ensure_local_authority() -> anyhow::Result<String> {
    let data_dir = default_data_dir();
    let workspace_root = default_workspace_root();
    ensure_local_authority_with_paths(data_dir, workspace_root).await
}

async fn ensure_local_authority_with_paths(
    data_dir: PathBuf,
    workspace_root: PathBuf,
) -> anyhow::Result<String> {
    std::fs::create_dir_all(&data_dir)?;

    let client = Client::builder()
        .timeout(Duration::from_millis(250))
        .build()?;

    let mut last_spawned_at: Option<std::time::Instant> = None;
    let mut lock_invalid_since: Option<std::time::Instant> = None;
    let mut backoff_ms: u64 = 20;
    let mut last_state: Option<String> = None;

    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        let workspace_root_str = workspace_root.to_string_lossy().to_string();
        let meta = ripd::read_authority_meta(&data_dir).map_err(anyhow::Error::msg)?;
        if let Some(meta) = meta {
            lock_invalid_since = None;
            if meta.workspace_root != workspace_root_str {
                anyhow::bail!(
                    "store authority workspace mismatch: authority_root={} current_root={}",
                    meta.workspace_root,
                    workspace_root.display()
                );
            }
            if ping(&client, &meta.endpoint).await {
                return Ok(meta.endpoint);
            }

            let pid_liveness = ripd::pid_liveness(meta.pid);
            update_last_state(
                &mut last_state,
                &mut backoff_ms,
                format!(
                    "authority unavailable: endpoint={} pid={} pid_liveness={pid_liveness:?}",
                    meta.endpoint, meta.pid
                ),
            );

            if matches!(pid_liveness, ripd::PidLiveness::Dead) {
                let cleaned = ripd::try_cleanup_stale_authority_files(
                    &data_dir,
                    meta.pid,
                    meta.started_at_ms,
                )
                .map_err(anyhow::Error::msg)?;
                if cleaned {
                    backoff_ms = 20;
                    continue;
                }
            }
        } else {
            let lock_path = ripd::authority_lock_path(&data_dir);
            if lock_path.exists() {
                match ripd::read_authority_lock_record(&data_dir) {
                    Ok(Some(lock)) => {
                        if lock.workspace_root != workspace_root_str {
                            anyhow::bail!(
                                "store authority workspace mismatch: authority_root={} current_root={}",
                                lock.workspace_root,
                                workspace_root.display()
                            );
                        }

                        let pid_liveness = ripd::pid_liveness(lock.pid);
                        update_last_state(
                            &mut last_state,
                            &mut backoff_ms,
                            format!(
                                "authority starting (meta.json missing): pid={} pid_liveness={pid_liveness:?}",
                                lock.pid
                            ),
                        );

                        lock_invalid_since = None;
                        if matches!(pid_liveness, ripd::PidLiveness::Dead) {
                            let cleaned = ripd::try_cleanup_stale_authority_files(
                                &data_dir,
                                lock.pid,
                                lock.started_at_ms,
                            )
                            .map_err(anyhow::Error::msg)?;
                            if cleaned {
                                backoff_ms = 20;
                                continue;
                            }
                        }
                    }
                    Ok(None) => {
                        update_last_state(
                            &mut last_state,
                            &mut backoff_ms,
                            format!(
                                "authority lock exists but cannot be read: {}",
                                lock_path.display()
                            ),
                        );
                    }
                    Err(err) => {
                        lock_invalid_since.get_or_insert(std::time::Instant::now());
                        update_last_state(
                            &mut last_state,
                            &mut backoff_ms,
                            format!(
                                "authority lock exists but is invalid json (waiting): {} ({err})",
                                lock_path.display()
                            ),
                        );

                        if err.contains("lock json invalid")
                            && lock_invalid_since
                                .map(|since| since.elapsed() > Duration::from_secs(1))
                                .unwrap_or(false)
                        {
                            let cleaned = ripd::try_cleanup_corrupt_lock_file(&data_dir)
                                .map_err(anyhow::Error::msg)?;
                            if cleaned {
                                lock_invalid_since = None;
                                backoff_ms = 20;
                                continue;
                            }
                        }
                    }
                }
            } else {
                lock_invalid_since = None;
                update_last_state(
                    &mut last_state,
                    &mut backoff_ms,
                    "spawning local authority".to_string(),
                );

                let spawn_cooldown = Duration::from_millis(500);
                if last_spawned_at
                    .map(|since| since.elapsed() > spawn_cooldown)
                    .unwrap_or(true)
                {
                    spawn_local_authority(&data_dir, &workspace_root)?;
                    last_spawned_at = Some(std::time::Instant::now());
                    backoff_ms = 20;
                    continue;
                }
            }
        }

        if std::time::Instant::now() >= deadline {
            let lock_path = ripd::authority_lock_path(&data_dir);
            let meta_path = ripd::authority_meta_path(&data_dir);
            let log_path = ripd::authority_dir(&data_dir).join("authority.log");
            anyhow::bail!(
                "timed out waiting for local authority (store={}). last_state={}. lock_path={} meta_path={} log_path={}",
                data_dir.display(),
                last_state.unwrap_or_else(|| "unknown".to_string()),
                lock_path.display(),
                meta_path.display(),
                log_path.display()
            );
        }

        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        backoff_ms = (backoff_ms.saturating_mul(2)).min(200);
    }
}

async fn ping(client: &Client, server: &str) -> bool {
    let url = format!("{server}/openapi.json");
    match client.get(url).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

fn spawn_local_authority(data_dir: &Path, workspace_root: &Path) -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;

    let authority_dir = ripd::authority_dir(data_dir);
    std::fs::create_dir_all(&authority_dir)?;
    let log_path = authority_dir.join("authority.log");
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let log_err = log.try_clone()?;

    let mut cmd = std::process::Command::new(exe);
    cmd.arg("serve")
        .env("RIP_DATA_DIR", data_dir)
        .env("RIP_WORKSPACE_ROOT", workspace_root)
        .env("RIP_SERVER_ADDR", "127.0.0.1:0")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));

    let _child = cmd.spawn()?;
    Ok(())
}
