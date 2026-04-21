use super::*;

#[test]
fn workspace_root_returns_value() {
    let root = workspace_root();
    let func: fn() -> PathBuf = workspace_root;
    let pointer_root = func();
    assert!(!root.as_os_str().is_empty());
    assert!(!pointer_root.as_os_str().is_empty());
}

#[tokio::test]
async fn config_doctor_serves_resolved_configuration() {
    let dir = tempdir().expect("tmp");
    let data_dir = dir.path().join("data");
    let workspace_dir = dir.path().join("workspace");
    fs::create_dir_all(&workspace_dir).expect("workspace");
    fs::write(
        workspace_dir.join("rip.jsonc"),
        r#"{
  "openresponses": {
    "reasoning": { "summary": "concise" },
    "include": ["reasoning.encrypted_content"]
  },
  "provider": {
    "openai": {
      "endpoint": "https://api.openai.com/v1/responses",
      "api_key": { "env": "OPENAI_API_KEY" }
    }
  },
  "roles": {
    "primary": { "provider": "openai", "model": "gpt-5-mini" }
  }
}
"#,
    )
    .expect("config");

    let previous_api_key = std::env::var_os("OPENAI_API_KEY");
    std::env::set_var("OPENAI_API_KEY", "sk-test-openai");

    let app = build_app_with_workspace_root(data_dir, workspace_dir);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/config/doctor")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    match previous_api_key {
        Some(value) => std::env::set_var("OPENAI_API_KEY", value),
        None => std::env::remove_var("OPENAI_API_KEY"),
    }

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("provider_id"))
            .and_then(|value| value.as_str()),
        Some("openai")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("model"))
            .and_then(|value| value.as_str()),
        Some("gpt-5-mini")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("include"))
            .and_then(|value| value.as_array())
            .and_then(|value| value.first())
            .and_then(|value| value.as_str()),
        Some("reasoning.encrypted_content")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("include_source"))
            .and_then(|value| value.as_str()),
        Some("config:openresponses.include")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("reasoning"))
            .and_then(|value| value.get("summary"))
            .and_then(|value| value.as_str()),
        Some("concise")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("reasoning_summary_source"))
            .and_then(|value| value.as_str()),
        Some("config:openresponses.reasoning.summary")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("compat"))
            .and_then(|value| value.get("provider"))
            .and_then(|value| value.get("provider_id"))
            .and_then(|value| value.as_str()),
        Some("openai")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("compat"))
            .and_then(|value| value.get("include"))
            .and_then(|value| value.get("support"))
            .and_then(|value| value.get("request"))
            .and_then(|value| value.as_str()),
        Some("native")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("compat"))
            .and_then(|value| value.get("active_conversation_strategy"))
            .and_then(|value| value.as_str()),
        Some("previous_response_id")
    );
}

#[tokio::test]
async fn config_doctor_surfaces_openrouter_compat_profile() {
    let dir = tempdir().expect("tmp");
    let data_dir = dir.path().join("data");
    let workspace_dir = dir.path().join("workspace");
    fs::create_dir_all(&workspace_dir).expect("workspace");
    fs::write(
        workspace_dir.join("rip.jsonc"),
        r#"{
  "provider": {
    "openrouter": {
      "endpoint": "https://openrouter.ai/api/v1/responses",
      "api_key": { "env": "OPENROUTER_API_KEY" },
      "openresponses": { "stateless_history": true }
    }
  },
  "roles": {
    "primary": {
      "provider": "openrouter",
      "model": "nvidia/nemotron-3-nano-30b-a3b:free"
    }
  }
}
"#,
    )
    .expect("config");

    let previous_api_key = std::env::var_os("OPENROUTER_API_KEY");
    std::env::set_var("OPENROUTER_API_KEY", "sk-test-openrouter");

    let app = build_app_with_workspace_root(data_dir, workspace_dir);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/config/doctor")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    match previous_api_key {
        Some(value) => std::env::set_var("OPENROUTER_API_KEY", value),
        None => std::env::remove_var("OPENROUTER_API_KEY"),
    }

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("compat"))
            .and_then(|value| value.get("provider"))
            .and_then(|value| value.get("provider_id"))
            .and_then(|value| value.as_str()),
        Some("openrouter")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("compat"))
            .and_then(|value| value.get("active_conversation_strategy"))
            .and_then(|value| value.as_str()),
        Some("stateless_history")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("compat"))
            .and_then(|value| value.get("conversation"))
            .and_then(|value| value.get("requested"))
            .and_then(|value| value.as_str()),
        Some("stateless_history")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("compat"))
            .and_then(|value| value.get("conversation"))
            .and_then(|value| value.get("effective"))
            .and_then(|value| value.as_str()),
        Some("stateless_history")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("compat"))
            .and_then(|value| value.get("effective_validation"))
            .and_then(|value| value.get("missing_response_user"))
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("compat"))
            .and_then(|value| value.get("model"))
            .and_then(|value| value.get("model_id"))
            .and_then(|value| value.as_str()),
        Some("nvidia/nemotron-3-nano-30b-a3b:free")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("compat"))
            .and_then(|value| value.get("reasoning"))
            .and_then(|value| value.get("support"))
            .and_then(|value| value.get("effort"))
            .and_then(|value| value.as_str()),
        Some("native")
    );
}

#[tokio::test]
async fn config_doctor_coerces_unsupported_openrouter_previous_response_id_to_stateless_history() {
    let dir = tempdir().expect("tmp");
    let data_dir = dir.path().join("data");
    let workspace_dir = dir.path().join("workspace");
    fs::create_dir_all(&workspace_dir).expect("workspace");
    fs::write(
        workspace_dir.join("rip.jsonc"),
        r#"{
  "provider": {
    "openrouter": {
      "endpoint": "https://openrouter.ai/api/v1/responses",
      "api_key": { "env": "OPENROUTER_API_KEY" }
    }
  },
  "roles": {
    "primary": {
      "provider": "openrouter",
      "model": "nvidia/nemotron-3-nano-30b-a3b:free"
    }
  }
}
"#,
    )
    .expect("config");

    let previous_api_key = std::env::var_os("OPENROUTER_API_KEY");
    std::env::set_var("OPENROUTER_API_KEY", "sk-test-openrouter");

    let app = build_app_with_workspace_root(data_dir, workspace_dir);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/config/doctor")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    match previous_api_key {
        Some(value) => std::env::set_var("OPENROUTER_API_KEY", value),
        None => std::env::remove_var("OPENROUTER_API_KEY"),
    }

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("stateless_history"))
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("compat"))
            .and_then(|value| value.get("active_conversation_strategy"))
            .and_then(|value| value.as_str()),
        Some("stateless_history")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("compat"))
            .and_then(|value| value.get("conversation"))
            .and_then(|value| value.get("requested"))
            .and_then(|value| value.as_str()),
        Some("previous_response_id")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("compat"))
            .and_then(|value| value.get("conversation"))
            .and_then(|value| value.get("effective"))
            .and_then(|value| value.as_str()),
        Some("stateless_history")
    );
    assert!(payload
        .get("openresponses")
        .and_then(|value| value.get("compat"))
        .and_then(|value| value.get("conversation"))
        .and_then(|value| value.get("warnings"))
        .and_then(|value| value.as_array())
        .is_some_and(|warnings| warnings.iter().any(|warning| warning
            .as_str()
            .is_some_and(|warning| warning.contains("does not support previous_response_id")))));
}

#[tokio::test]
async fn config_doctor_prefers_provider_id_over_noncanonical_endpoint_for_openrouter() {
    let dir = tempdir().expect("tmp");
    let data_dir = dir.path().join("data");
    let workspace_dir = dir.path().join("workspace");
    fs::create_dir_all(&workspace_dir).expect("workspace");
    fs::write(
        workspace_dir.join("rip.jsonc"),
        r#"{
  "provider": {
    "openrouter": {
      "endpoint": "http://127.0.0.1:4010/v1/responses",
      "api_key": { "env": "OPENROUTER_API_KEY" },
      "openresponses": { "stateless_history": true }
    }
  },
  "roles": {
    "primary": {
      "provider": "openrouter",
      "model": "nvidia/nemotron-3-nano-30b-a3b:free"
    }
  }
}
"#,
    )
    .expect("config");

    let previous_api_key = std::env::var_os("OPENROUTER_API_KEY");
    std::env::set_var("OPENROUTER_API_KEY", "sk-test-openrouter");

    let app = build_app_with_workspace_root(data_dir, workspace_dir);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/config/doctor")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    match previous_api_key {
        Some(value) => std::env::set_var("OPENROUTER_API_KEY", value),
        None => std::env::remove_var("OPENROUTER_API_KEY"),
    }

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("provider_id"))
            .and_then(|value| value.as_str()),
        Some("openrouter")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("endpoint"))
            .and_then(|value| value.as_str()),
        Some("http://127.0.0.1:4010/v1/responses")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("compat"))
            .and_then(|value| value.get("provider"))
            .and_then(|value| value.get("provider_id"))
            .and_then(|value| value.as_str()),
        Some("openrouter")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("compat"))
            .and_then(|value| value.get("active_conversation_strategy"))
            .and_then(|value| value.as_str()),
        Some("stateless_history")
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("compat"))
            .and_then(|value| value.get("effective_validation"))
            .and_then(|value| value.get("missing_response_user"))
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        payload
            .get("openresponses")
            .and_then(|value| value.get("compat"))
            .and_then(|value| value.get("reasoning"))
            .and_then(|value| value.get("support"))
            .and_then(|value| value.get("supported_efforts"))
            .and_then(|value| value.as_array())
            .map(|values| values.len()),
        Some(4)
    );
}

#[tokio::test]
async fn openapi_spec_served() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    assert!(content_type.starts_with("application/json"));
    let body = response.into_body().collect().await.expect("body");
    let bytes = body.to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert!(value
        .get("paths")
        .and_then(|paths| paths.get("/sessions"))
        .is_some());
}

#[test]
fn openapi_snapshot_matches() {
    let json = build_openapi_router().1;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let path = root.join("schemas/ripd/openapi.json");
    if std::env::var("RIPD_UPDATE_OPENAPI").is_ok() {
        std::fs::create_dir_all(path.parent().expect("dir")).expect("mkdir");
        std::fs::write(&path, json).expect("write");
        return;
    }
    let existing = std::fs::read_to_string(&path).expect("snapshot missing");
    assert_eq!(existing, json);
}
