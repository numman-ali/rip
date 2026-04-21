#![cfg(not(windows))]

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use tokio::process::Command;

fn unique_tmp_root(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()))
}

fn rip_bin() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_rip") {
        return PathBuf::from(path);
    }

    let exe = std::env::current_exe().expect("current_exe");
    let debug_dir = exe
        .parent()
        .and_then(|path| path.parent())
        .expect("debug dir");
    let candidate = debug_dir.join("rip");
    assert!(
        candidate.exists(),
        "expected rip binary at {}",
        candidate.display()
    );
    candidate
}

async fn terminate_authority(data_dir: &Path) {
    let Ok(Some(meta)) = ripd::read_authority_meta(data_dir) else {
        return;
    };
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &meta.pid.to_string()])
        .status();
    for _ in 0..50 {
        if matches!(ripd::pid_liveness(meta.pid), ripd::PidLiveness::Dead) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &meta.pid.to_string()])
        .status();
}

#[tokio::test]
async fn rip_config_doctor_reports_layered_sources_and_effective_openresponses() {
    let rip = rip_bin();

    let root = unique_tmp_root("rip-config-doctor-local-first");
    let data_dir = root.join("data");
    let repo_root = root.join("repo");
    let workspace_dir = repo_root.join("apps").join("demo");
    let global_dir = root.join("global");
    let custom_config = root.join("custom.jsonc");

    std::fs::create_dir_all(&workspace_dir).expect("workspace");
    std::fs::create_dir_all(repo_root.join(".git")).expect("git root");
    std::fs::create_dir_all(&global_dir).expect("global dir");

    std::fs::write(
        global_dir.join("config.jsonc"),
        r#"{
  // global defaults
      "provider": {
    "openai": {
      "endpoint": "https://api.openai.com/v1/responses",
      "api_key": { "env": "OPENAI_API_KEY" },
      "headers": { "x-global": "1" },
      "openresponses": {
        "stateless_history": true,
        "reasoning": { "summary": "concise" }
      }
    }
  },
  "roles": {
    "primary": { "provider": "openai", "model": "gpt-5-global", "variant": "fast" }
  }
}
"#,
    )
    .expect("write global config");

    std::fs::write(
        &custom_config,
        r#"{
  "openresponses": { "parallel_tool_calls": false }
}
"#,
    )
    .expect("write custom config");

    std::fs::write(
        repo_root.join("rip.jsonc"),
        r#"{
  "provider": {
    "openai": {
      "headers": { "x-project": "1" },
      "openresponses": { "followup_user_message": "project followup" }
    }
  }
}
"#,
    )
    .expect("write project config");

    let mut cmd = Command::new(&rip);
    cmd.args(["config", "doctor"])
        .env("RIP_DATA_DIR", &data_dir)
        .env("RIP_WORKSPACE_ROOT", &workspace_dir)
        .env("RIP_CONFIG_HOME", &global_dir)
        .env("RIP_CONFIG", &custom_config)
        .env("OPENAI_API_KEY", "sk-test-openai")
        .env("RIP_OPENRESPONSES_MODEL", "gpt-5-env")
        .env("RIP_OPENRESPONSES_STATELESS_HISTORY", "off")
        .env("RIP_OPENRESPONSES_PARALLEL_TOOL_CALLS", "yes")
        .env(
            "RIP_OPENRESPONSES_INCLUDE",
            "reasoning.encrypted_content,message.output_text.logprobs",
        )
        .env("RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE", "env followup")
        .env("RIP_OPENRESPONSES_REASONING_EFFORT", "high")
        .env("RIP_OPENRESPONSES_REASONING_SUMMARY", "detailed")
        .env_remove("RIP_OPENRESPONSES_ENDPOINT")
        .env_remove("RIP_OPENRESPONSES_API_KEY")
        .env_remove("RIP_OPENRESPONSES_TOOL_CHOICE")
        .env_remove("OPENROUTER_API_KEY");
    let out = cmd.output().await.expect("config doctor");
    assert!(
        out.status.success(),
        "expected config doctor exit=0; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let payload: Value = serde_json::from_slice(&out.stdout).expect("config doctor json");
    let sources = payload
        .get("sources")
        .and_then(|value| value.as_array())
        .unwrap_or_else(|| panic!("expected sources array in config doctor payload: {payload}"));
    assert!(
        sources.iter().any(|source| {
            source.get("path").and_then(|value| value.as_str())
                == Some(global_dir.join("config.jsonc").to_string_lossy().as_ref())
                && source.get("status").and_then(|value| value.as_str()) == Some("loaded:global")
        }),
        "expected loaded global config source: {payload}"
    );
    assert!(
        sources.iter().any(|source| {
            source.get("path").and_then(|value| value.as_str())
                == Some(custom_config.to_string_lossy().as_ref())
                && source.get("status").and_then(|value| value.as_str()) == Some("loaded:custom")
        }),
        "expected loaded custom config source: {payload}"
    );
    assert!(
        sources.iter().any(|source| {
            source.get("path").and_then(|value| value.as_str())
                == Some(repo_root.join("rip.jsonc").to_string_lossy().as_ref())
                && source.get("status").and_then(|value| value.as_str()) == Some("loaded:project")
        }),
        "expected loaded project config source: {payload}"
    );

    let openresponses = payload
        .get("openresponses")
        .unwrap_or_else(|| panic!("expected openresponses payload: {payload}"));
    assert_eq!(
        openresponses
            .get("provider_id")
            .and_then(|value| value.as_str()),
        Some("openai")
    );
    assert_eq!(
        openresponses.get("route").and_then(|value| value.as_str()),
        Some("openai/gpt-5-global#fast")
    );
    assert_eq!(
        openresponses
            .get("effective_route")
            .and_then(|value| value.as_str()),
        Some("openai/gpt-5-env")
    );
    assert_eq!(
        openresponses
            .get("endpoint_source")
            .and_then(|value| value.as_str()),
        Some("config:provider.openai.endpoint")
    );
    assert_eq!(
        openresponses.get("model").and_then(|value| value.as_str()),
        Some("gpt-5-env")
    );
    assert_eq!(
        openresponses
            .get("model_source")
            .and_then(|value| value.as_str()),
        Some("env:RIP_OPENRESPONSES_MODEL")
    );
    assert_eq!(
        openresponses
            .get("api_key_source")
            .and_then(|value| value.as_str()),
        Some("env:OPENAI_API_KEY")
    );
    assert_eq!(
        openresponses
            .get("has_api_key")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        openresponses
            .get("stateless_history")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        openresponses
            .get("stateless_history_source")
            .and_then(|value| value.as_str()),
        Some("env:RIP_OPENRESPONSES_STATELESS_HISTORY")
    );
    assert_eq!(
        openresponses
            .get("parallel_tool_calls")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        openresponses
            .get("parallel_tool_calls_source")
            .and_then(|value| value.as_str()),
        Some("env:RIP_OPENRESPONSES_PARALLEL_TOOL_CALLS")
    );
    assert_eq!(
        openresponses
            .get("include")
            .and_then(|value| value.as_array())
            .map(|value| {
                value
                    .iter()
                    .filter_map(|item| item.as_str())
                    .collect::<Vec<_>>()
            }),
        Some(vec![
            "reasoning.encrypted_content",
            "message.output_text.logprobs"
        ])
    );
    assert_eq!(
        openresponses
            .get("include_source")
            .and_then(|value| value.as_str()),
        Some("env:RIP_OPENRESPONSES_INCLUDE")
    );
    assert_eq!(
        openresponses
            .get("followup_user_message")
            .and_then(|value| value.as_str()),
        Some("env followup")
    );
    assert_eq!(
        openresponses
            .get("followup_user_message_source")
            .and_then(|value| value.as_str()),
        Some("env:RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE")
    );
    assert_eq!(
        openresponses
            .get("reasoning")
            .and_then(|value| value.get("effort"))
            .and_then(|value| value.as_str()),
        Some("high")
    );
    assert_eq!(
        openresponses
            .get("reasoning")
            .and_then(|value| value.get("summary"))
            .and_then(|value| value.as_str()),
        Some("detailed")
    );
    assert_eq!(
        openresponses
            .get("reasoning_effort_source")
            .and_then(|value| value.as_str()),
        Some("env:RIP_OPENRESPONSES_REASONING_EFFORT")
    );
    assert_eq!(
        openresponses
            .get("reasoning_summary_source")
            .and_then(|value| value.as_str()),
        Some("env:RIP_OPENRESPONSES_REASONING_SUMMARY")
    );
    assert_eq!(
        openresponses
            .get("compat")
            .and_then(|value| value.get("include"))
            .and_then(|value| value.get("support"))
            .and_then(|value| value.get("request"))
            .and_then(|value| value.as_str()),
        Some("native")
    );
    assert!(openresponses
        .get("compat")
        .and_then(|value| value.get("include"))
        .and_then(|value| value.get("support"))
        .and_then(|value| value.get("native_values"))
        .and_then(|value| value.as_array())
        .is_some_and(|values| values
            .iter()
            .any(|value| value.as_str() == Some("message.output_text.logprobs"))));
    assert_eq!(
        openresponses
            .get("compat")
            .and_then(|value| value.get("conversation"))
            .and_then(|value| value.get("requested"))
            .and_then(|value| value.as_str()),
        Some("previous_response_id")
    );
    assert_eq!(
        openresponses
            .get("compat")
            .and_then(|value| value.get("conversation"))
            .and_then(|value| value.get("effective"))
            .and_then(|value| value.as_str()),
        Some("previous_response_id")
    );
    assert_eq!(
        openresponses
            .get("compat")
            .and_then(|value| value.get("active_conversation_strategy"))
            .and_then(|value| value.as_str()),
        Some("previous_response_id")
    );
    assert_eq!(
        openresponses
            .get("compat")
            .and_then(|value| value.get("effective_validation"))
            .and_then(|value| value.get("missing_item_ids"))
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        openresponses
            .get("compat")
            .and_then(|value| value.get("provider"))
            .and_then(|value| value.get("provider_id"))
            .and_then(|value| value.as_str()),
        Some("openai")
    );
    assert_eq!(
        openresponses
            .get("compat")
            .and_then(|value| value.get("provider"))
            .and_then(|value| value.get("stream_shape"))
            .and_then(|value| value.as_str()),
        Some("native")
    );
    assert_eq!(
        openresponses
            .get("compat")
            .and_then(|value| value.get("reasoning"))
            .and_then(|value| value.get("effective"))
            .and_then(|value| value.get("effort"))
            .and_then(|value| value.as_str()),
        Some("high")
    );
    assert_eq!(
        openresponses
            .get("compat")
            .and_then(|value| value.get("reasoning"))
            .and_then(|value| value.get("support"))
            .and_then(|value| value.get("effort"))
            .and_then(|value| value.as_str()),
        Some("unknown")
    );

    let headers = openresponses
        .get("headers")
        .and_then(|value| value.as_array())
        .unwrap_or_else(|| panic!("expected headers array in config doctor payload: {payload}"));
    assert!(
        headers
            .iter()
            .any(|value| value.as_str() == Some("x-global")),
        "expected global provider header in doctor output: {payload}"
    );
    assert!(
        headers
            .iter()
            .any(|value| value.as_str() == Some("x-project")),
        "expected project provider header in doctor output: {payload}"
    );

    terminate_authority(&data_dir).await;
    let _ = std::fs::remove_dir_all(&root);
}
