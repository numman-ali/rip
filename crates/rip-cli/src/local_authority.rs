use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use reqwest::Client;

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

    let mut lock_without_meta_since: Option<std::time::Instant> = None;
    let mut meta_unreachable_since: Option<std::time::Instant> = None;
    let mut spawned_since: Option<std::time::Instant> = None;

    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        let meta = ripd::read_authority_meta(&data_dir).map_err(anyhow::Error::msg)?;
        if let Some(meta) = meta {
            if meta.workspace_root != workspace_root.to_string_lossy() {
                anyhow::bail!(
                    "store authority workspace mismatch: authority_root={} current_root={}",
                    meta.workspace_root,
                    workspace_root.display()
                );
            }
            if ping(&client, &meta.endpoint).await {
                return Ok(meta.endpoint);
            }
            meta_unreachable_since.get_or_insert(std::time::Instant::now());
        } else {
            meta_unreachable_since = None;
        }

        let lock_path = ripd::authority_lock_path(&data_dir);
        if lock_path.exists() {
            lock_without_meta_since.get_or_insert(std::time::Instant::now());

            let lock_has_meta = ripd::authority_meta_path(&data_dir).exists();
            if !lock_has_meta {
                let grace = Duration::from_secs(3);
                if lock_without_meta_since
                    .map(|since| since.elapsed() > grace)
                    .unwrap_or(false)
                {
                    cleanup_stale_lock(&data_dir);
                    lock_without_meta_since = None;
                }
            } else if meta_unreachable_since
                .map(|since| since.elapsed() > Duration::from_secs(1))
                .unwrap_or(false)
            {
                cleanup_stale_lock(&data_dir);
                meta_unreachable_since = None;
            }
        } else {
            lock_without_meta_since = None;
            meta_unreachable_since = None;
            if spawned_since
                .map(|since| since.elapsed() > Duration::from_millis(200))
                .unwrap_or(true)
            {
                spawn_local_authority(&data_dir, &workspace_root)?;
                spawned_since = Some(std::time::Instant::now());
            }
        }

        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out waiting for local authority (store={})",
                data_dir.display()
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
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

fn cleanup_stale_lock(data_dir: &Path) {
    let _ = std::fs::remove_file(ripd::authority_meta_path(data_dir));
    let _ = std::fs::remove_file(ripd::authority_lock_path(data_dir));
}
