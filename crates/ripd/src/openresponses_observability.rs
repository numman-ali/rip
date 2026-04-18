use std::fs;
use std::path::{Path, PathBuf};

use rip_kernel::{Event, EventKind};
use serde_json::Value;
use uuid::Uuid;

const DEFAULT_MAX_BYTES: usize = 1_000_000;

#[derive(Clone, Copy, Debug)]
pub(crate) struct OpenResponsesRequestDumpConfig {
    pub(crate) enabled: bool,
    pub(crate) max_bytes: usize,
}

#[cfg_attr(test, inline(never))]
pub(crate) fn request_dump_config_from_env() -> OpenResponsesRequestDumpConfig {
    let enabled = std::env::var("RIP_OPENRESPONSES_DUMP_REQUEST")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false);

    let max_bytes = std::env::var("RIP_OPENRESPONSES_DUMP_REQUEST_MAX_BYTES")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_BYTES);

    OpenResponsesRequestDumpConfig { enabled, max_bytes }
}

pub(crate) struct OpenResponsesRequestDumpInput<'a> {
    pub(crate) workspace_root: &'a Path,
    pub(crate) session_id: &'a str,
    pub(crate) timestamp_ms: u64,
    pub(crate) seq: u64,
    pub(crate) endpoint: &'a str,
    pub(crate) request_index: u64,
    pub(crate) kind: &'a str,
    pub(crate) body: &'a Value,
}

#[cfg_attr(test, inline(never))]
pub(crate) fn maybe_dump_openresponses_request(
    config: OpenResponsesRequestDumpConfig,
    input: OpenResponsesRequestDumpInput<'_>,
) -> Result<Option<Event>, String> {
    if !config.enabled {
        return Ok(None);
    }

    let bytes =
        serde_json::to_vec(input.body).map_err(|err| format!("request serialize failed: {err}"))?;
    let total_bytes = bytes.len() as u64;

    let mut stored = bytes;
    let mut truncated = false;
    if stored.len() > config.max_bytes {
        stored.truncate(config.max_bytes);
        truncated = true;
    }
    let body_bytes = stored.len() as u64;

    let body_artifact_id = new_artifact_id();
    write_blob_atomic(input.workspace_root, &body_artifact_id, &stored)?;

    let model = input
        .body
        .get("model")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());

    Ok(Some(Event {
        id: Uuid::new_v4().to_string(),
        session_id: input.session_id.to_string(),
        timestamp_ms: input.timestamp_ms,
        seq: input.seq,
        kind: EventKind::OpenResponsesRequest {
            endpoint: input.endpoint.to_string(),
            model,
            request_index: input.request_index,
            kind: input.kind.to_string(),
            body_artifact_id,
            body_bytes,
            total_bytes,
            truncated,
        },
    }))
}

#[cfg_attr(test, inline(never))]
fn artifacts_blobs_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".rip").join("artifacts").join("blobs")
}

#[cfg_attr(test, inline(never))]
fn write_blob_atomic(workspace_root: &Path, artifact_id: &str, bytes: &[u8]) -> Result<(), String> {
    let dir = artifacts_blobs_dir(workspace_root);
    fs::create_dir_all(&dir).map_err(|err| format!("artifact dir create failed: {err}"))?;

    let path = dir.join(artifact_id);
    let tmp = dir.join(format!("{artifact_id}.tmp"));
    fs::write(&tmp, bytes).map_err(|err| format!("artifact write failed: {err}"))?;
    fs::rename(&tmp, &path).map_err(|err| format!("artifact finalize failed: {err}"))?;
    Ok(())
}

#[cfg_attr(test, inline(never))]
fn new_artifact_id() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::ffi::OsString;
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use tempfile::tempdir;

    struct EnvGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = self.previous.take() {
                std::env::set_var(self.key, value);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock")
    }

    fn set_env(key: &'static str, value: impl Into<OsString>) -> EnvGuard {
        let previous = std::env::var_os(key);
        std::env::set_var(key, value.into());
        EnvGuard { key, previous }
    }

    fn remove_env(key: &'static str) -> EnvGuard {
        let previous = std::env::var_os(key);
        std::env::remove_var(key);
        EnvGuard { key, previous }
    }

    #[test]
    fn request_dump_config_from_env_parses_flags_and_limits() {
        let _lock = env_lock();
        let _enabled = set_env("RIP_OPENRESPONSES_DUMP_REQUEST", "YES");
        let _max = set_env("RIP_OPENRESPONSES_DUMP_REQUEST_MAX_BYTES", "321");

        let config = request_dump_config_from_env();
        assert!(config.enabled);
        assert_eq!(config.max_bytes, 321);
    }

    #[test]
    fn request_dump_config_from_env_falls_back_for_invalid_values() {
        let _lock = env_lock();
        let _enabled = set_env("RIP_OPENRESPONSES_DUMP_REQUEST", "nope");
        let _max = set_env("RIP_OPENRESPONSES_DUMP_REQUEST_MAX_BYTES", "0");

        let config = request_dump_config_from_env();
        assert!(!config.enabled);
        assert_eq!(config.max_bytes, DEFAULT_MAX_BYTES);

        drop(_enabled);
        drop(_max);
        let _enabled = remove_env("RIP_OPENRESPONSES_DUMP_REQUEST");
        let _max = set_env("RIP_OPENRESPONSES_DUMP_REQUEST_MAX_BYTES", "bad");
        let config = request_dump_config_from_env();
        assert!(!config.enabled);
        assert_eq!(config.max_bytes, DEFAULT_MAX_BYTES);
    }

    #[test]
    fn maybe_dump_openresponses_request_is_noop_when_disabled() {
        let dir = tempdir().expect("tmp");
        let event = maybe_dump_openresponses_request(
            OpenResponsesRequestDumpConfig {
                enabled: false,
                max_bytes: 16,
            },
            OpenResponsesRequestDumpInput {
                workspace_root: dir.path(),
                session_id: "s1",
                timestamp_ms: 10,
                seq: 2,
                endpoint: "https://api.openai.com/v1/responses",
                request_index: 0,
                kind: "response.create",
                body: &json!({"model": "gpt-5"}),
            },
        )
        .expect("dump");
        assert!(event.is_none());
        assert!(!artifacts_blobs_dir(dir.path()).exists());
    }

    #[test]
    fn maybe_dump_openresponses_request_writes_truncated_blob_and_event() {
        let dir = tempdir().expect("tmp");
        let body = json!({
            "model": "gpt-5",
            "input": "abcdefghijklmnopqrstuvwxyz"
        });
        let event = maybe_dump_openresponses_request(
            OpenResponsesRequestDumpConfig {
                enabled: true,
                max_bytes: 12,
            },
            OpenResponsesRequestDumpInput {
                workspace_root: dir.path(),
                session_id: "s1",
                timestamp_ms: 55,
                seq: 7,
                endpoint: "https://api.openai.com/v1/responses",
                request_index: 3,
                kind: "response.create",
                body: &body,
            },
        )
        .expect("dump")
        .expect("event");

        let EventKind::OpenResponsesRequest {
            endpoint,
            model,
            request_index,
            kind,
            body_artifact_id,
            body_bytes,
            total_bytes,
            truncated,
        } = event.kind
        else {
            panic!("expected openresponses request event");
        };

        assert_eq!(event.session_id, "s1");
        assert_eq!(event.timestamp_ms, 55);
        assert_eq!(event.seq, 7);
        assert_eq!(endpoint, "https://api.openai.com/v1/responses");
        assert_eq!(model.as_deref(), Some("gpt-5"));
        assert_eq!(request_index, 3);
        assert_eq!(kind, "response.create");
        assert_eq!(body_bytes, 12);
        assert!(truncated);
        assert!(total_bytes > body_bytes);
        assert_eq!(body_artifact_id.len(), 64);
        assert!(body_artifact_id.chars().all(|ch| ch.is_ascii_hexdigit()));

        let stored =
            fs::read(artifacts_blobs_dir(dir.path()).join(&body_artifact_id)).expect("blob");
        assert_eq!(stored.len(), body_bytes as usize);
        assert!(stored.len() < serde_json::to_vec(&body).expect("json").len());
    }

    #[test]
    fn maybe_dump_openresponses_request_keeps_full_body_and_handles_missing_model() {
        let dir = tempdir().expect("tmp");
        let body = json!({"input": "hello"});
        let event = maybe_dump_openresponses_request(
            OpenResponsesRequestDumpConfig {
                enabled: true,
                max_bytes: 1024,
            },
            OpenResponsesRequestDumpInput {
                workspace_root: dir.path(),
                session_id: "s2",
                timestamp_ms: 1,
                seq: 9,
                endpoint: "https://example.com/responses",
                request_index: 0,
                kind: "response.patch",
                body: &body,
            },
        )
        .expect("dump")
        .expect("event");

        let EventKind::OpenResponsesRequest {
            model,
            body_artifact_id,
            body_bytes,
            total_bytes,
            truncated,
            ..
        } = event.kind
        else {
            panic!("expected openresponses request event");
        };

        assert_eq!(model, None);
        assert_eq!(body_bytes, total_bytes);
        assert!(!truncated);
        let stored =
            fs::read(artifacts_blobs_dir(dir.path()).join(body_artifact_id)).expect("blob");
        assert_eq!(stored, serde_json::to_vec(&body).expect("json"));
    }

    #[test]
    fn maybe_dump_openresponses_request_surfaces_artifact_dir_failures() {
        let dir = tempdir().expect("tmp");
        let artifact_id = new_artifact_id();
        assert_eq!(artifact_id.len(), 64);
        assert!(artifact_id.chars().all(|ch| ch.is_ascii_hexdigit()));

        fs::create_dir_all(dir.path().join(".rip")).expect("rip dir");
        fs::write(dir.path().join(".rip").join("artifacts"), b"blocked").expect("blocker");

        let err = maybe_dump_openresponses_request(
            OpenResponsesRequestDumpConfig {
                enabled: true,
                max_bytes: 32,
            },
            OpenResponsesRequestDumpInput {
                workspace_root: dir.path(),
                session_id: "s3",
                timestamp_ms: 99,
                seq: 11,
                endpoint: "https://api.openai.com/v1/responses",
                request_index: 1,
                kind: "response.create",
                body: &json!({"input": "hello"}),
            },
        )
        .expect_err("artifact dir failure");
        assert!(err.contains("artifact dir create failed"));
    }

    #[test]
    fn helper_paths_and_blob_writes_round_trip() {
        let dir = tempdir().expect("tmp");
        let blobs_dir = artifacts_blobs_dir(dir.path());
        assert_eq!(
            blobs_dir,
            dir.path().join(".rip").join("artifacts").join("blobs")
        );

        let artifact_id = new_artifact_id();
        assert_eq!(artifact_id.len(), 64);
        assert!(artifact_id.chars().all(|ch| ch.is_ascii_hexdigit()));

        write_blob_atomic(dir.path(), &artifact_id, br#"{"ok":true}"#).expect("write blob");
        let stored = fs::read(blobs_dir.join(&artifact_id)).expect("read blob");
        assert_eq!(stored, br#"{"ok":true}"#);
    }

    #[test]
    fn helper_functions_are_callable_via_function_pointers() {
        let _lock = env_lock();
        let _enabled = set_env("RIP_OPENRESPONSES_DUMP_REQUEST", "true");
        let _max = set_env("RIP_OPENRESPONSES_DUMP_REQUEST_MAX_BYTES", "17");

        let config_fn: fn() -> OpenResponsesRequestDumpConfig = request_dump_config_from_env;
        let config = config_fn();
        assert!(config.enabled);
        assert_eq!(config.max_bytes, 17);

        let dir = tempdir().expect("tmp");
        let blobs_dir_fn: fn(&Path) -> PathBuf = artifacts_blobs_dir;
        let write_blob_fn: fn(&Path, &str, &[u8]) -> Result<(), String> = write_blob_atomic;
        let artifact_id_fn: fn() -> String = new_artifact_id;
        let dump_fn: for<'a> fn(
            OpenResponsesRequestDumpConfig,
            OpenResponsesRequestDumpInput<'a>,
        ) -> Result<Option<Event>, String> = maybe_dump_openresponses_request;

        let artifact_id = artifact_id_fn();
        write_blob_fn(dir.path(), &artifact_id, b"{}").expect("write");
        assert!(blobs_dir_fn(dir.path()).join(&artifact_id).exists());

        let event = dump_fn(
            OpenResponsesRequestDumpConfig {
                enabled: true,
                max_bytes: 128,
            },
            OpenResponsesRequestDumpInput {
                workspace_root: dir.path(),
                session_id: "s4",
                timestamp_ms: 7,
                seq: 3,
                endpoint: "https://api.openai.com/v1/responses",
                request_index: 2,
                kind: "response.create",
                body: &json!({"model": "gpt-5-mini", "input": "hello"}),
            },
        )
        .expect("dump")
        .expect("event");
        assert!(matches!(event.kind, EventKind::OpenResponsesRequest { .. }));
    }
}
