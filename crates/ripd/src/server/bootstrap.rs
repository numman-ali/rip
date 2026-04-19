#[cfg(not(test))]
use super::*;

#[cfg(not(test))]
pub(crate) async fn serve(data_dir: std::path::PathBuf) {
    let workspace_root = workspace_root();
    let addr = server_addr_from_env().unwrap_or_else(|| "127.0.0.1:7341".parse().expect("addr"));

    let client = Client::builder()
        .timeout(std::time::Duration::from_millis(250))
        .build()
        .expect("reqwest client");

    let lock = acquire_authority_lock_with_recovery(&client, &data_dir, &workspace_root)
        .await
        .unwrap_or_else(|err| panic!("{err}"));

    let app = build_app_with_workspace_root_and_provider(
        data_dir.clone(),
        workspace_root.clone(),
        OpenResponsesConfig::from_env(),
    );

    let listener = TcpListener::bind(addr).await.expect("bind");
    let local_addr = listener.local_addr().expect("local addr");
    let endpoint = format!("http://{local_addr}");
    eprintln!("ripd listening on {endpoint}");

    lock.write_meta(endpoint)
        .unwrap_or_else(|err| panic!("{err}"));

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        let _ = shutdown_rx.await;
    });

    let mut server_task = tokio::spawn(async move { server.await });
    tokio::select! {
        result = &mut server_task => {
            let result = result.expect("server task");
            result.expect("server");
            return;
        }
        _ = shutdown_signal() => {
            let _ = shutdown_tx.send(());
        }
    }

    match tokio::time::timeout(std::time::Duration::from_secs(2), &mut server_task).await {
        Ok(result) => {
            let result = result.expect("server task");
            result.expect("server");
        }
        Err(_) => {
            eprintln!("server shutdown timed out; forcing exit");
            server_task.abort();
            let _ = server_task.await;
        }
    }
}

#[cfg(not(test))]
async fn ping_openapi(client: &Client, endpoint: &str) -> bool {
    let url = format!("{endpoint}/openapi.json");
    match client.get(url).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

#[cfg(not(test))]
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler");
        let _ = sigterm.recv().await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

#[cfg(not(test))]
async fn acquire_authority_lock_with_recovery(
    client: &Client,
    data_dir: &std::path::Path,
    workspace_root: &std::path::Path,
) -> Result<AuthorityLockGuard, String> {
    let workspace_root_str = workspace_root.to_string_lossy().to_string();

    let mut lock_invalid_since: Option<std::time::Instant> = None;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);

    loop {
        match AuthorityLockGuard::try_acquire(data_dir, workspace_root) {
            Ok(lock) => return Ok(lock),
            Err(err) => {
                let meta = crate::read_authority_meta(data_dir).unwrap_or(None);
                let endpoint_reachable = match &meta {
                    Some(meta) => ping_openapi(client, &meta.endpoint).await,
                    None => false,
                };
                if endpoint_reachable {
                    let Some(meta) = &meta else {
                        return Err(err);
                    };
                    return Err(format!(
                        "store already has an authority (endpoint={} pid={})",
                        meta.endpoint, meta.pid
                    ));
                }

                match crate::read_authority_lock_record(data_dir) {
                    Ok(Some(lock)) => {
                        lock_invalid_since = None;
                        if lock.workspace_root != workspace_root_str {
                            return Err(format!(
                                "store authority workspace mismatch: authority_root={} current_root={}",
                                lock.workspace_root,
                                workspace_root.display()
                            ));
                        }

                        let pid_liveness = crate::pid_liveness(lock.pid);
                        if matches!(pid_liveness, crate::PidLiveness::Dead) && !endpoint_reachable {
                            let cleaned = crate::try_cleanup_stale_authority_files(
                                data_dir,
                                lock.pid,
                                lock.started_at_ms,
                            )?;
                            if cleaned {
                                continue;
                            }
                        }

                        return Err(err);
                    }
                    Ok(None) => {
                        if std::time::Instant::now() >= deadline {
                            return Err(err);
                        }
                    }
                    Err(lock_err) => {
                        let lock_path = crate::authority_lock_path(data_dir);
                        lock_invalid_since.get_or_insert(std::time::Instant::now());

                        if lock_err.contains("lock json invalid")
                            && lock_invalid_since
                                .map(|since| since.elapsed() > std::time::Duration::from_secs(1))
                                .unwrap_or(false)
                        {
                            let cleaned = crate::try_cleanup_corrupt_lock_file(data_dir)?;
                            if cleaned {
                                lock_invalid_since = None;
                                continue;
                            }
                        }

                        if std::time::Instant::now() >= deadline {
                            return Err(format!(
                                "{err} (lock_path={} read_err={lock_err})",
                                lock_path.display()
                            ));
                        }
                    }
                }

                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        }
    }
}
