use super::*;

#[tokio::test]
async fn prompt_openresponses_http_error_emits_provider_error() {
    use axum::http::header::CONTENT_TYPE;
    use axum::http::StatusCode;
    use axum::routing::post;
    use axum::{response::IntoResponse, Router as AxumRouter};
    use tokio::net::TcpListener;

    let provider_app = AxumRouter::new().route(
        "/v1/responses",
        post(|| async move {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(CONTENT_TYPE, "text/plain")],
                "fail",
            )
                .into_response()
        }),
    );

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, provider_app).await.expect("serve");
    });
    let endpoint = format!("http://{addr}/v1/responses");

    let dir = tempdir().expect("tmp");
    let app = build_test_app_with_openresponses_provider(&dir, endpoint, false);
    let session_id = create_session_id(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut reader = TestSseReader::new(response.into_body());

    let send_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sessions/{session_id}/input"))
                .header("content-type", "application/json")
                .body(Body::from("{\"input\":\"hi\"}"))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(send_response.status(), axum::http::StatusCode::ACCEPTED);

    let mut saw_provider_error = false;
    let mut saw_session_ended = false;

    timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                match value.get("type").and_then(|value| value.as_str()) {
                    Some("provider_event") => {
                        let has_errors = value
                            .get("errors")
                            .and_then(|value| value.as_array())
                            .map(|errors| !errors.is_empty())
                            .unwrap_or(false);
                        let has_response_errors = value
                            .get("response_errors")
                            .and_then(|value| value.as_array())
                            .map(|errors| !errors.is_empty())
                            .unwrap_or(false);
                        if has_errors || has_response_errors {
                            saw_provider_error = true;
                        }
                    }
                    Some("session_ended") => {
                        saw_session_ended = true;
                        assert_eq!(
                            value.get("reason").and_then(|value| value.as_str()),
                            Some("provider_error")
                        );
                        break;
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("timeout");

    assert!(saw_provider_error, "expected provider_event with errors");
    assert!(saw_session_ended, "expected session_ended");
}

#[tokio::test]
async fn prompt_openresponses_connection_error_emits_provider_error() {
    let endpoint = "http://127.0.0.1:1/v1/responses".to_string();

    let dir = tempdir().expect("tmp");
    let app = build_test_app_with_openresponses_provider(&dir, endpoint, false);
    let session_id = create_session_id(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut reader = TestSseReader::new(response.into_body());

    let send_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sessions/{session_id}/input"))
                .header("content-type", "application/json")
                .body(Body::from("{\"input\":\"hi\"}"))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(send_response.status(), axum::http::StatusCode::ACCEPTED);

    let mut saw_provider_error = false;
    let mut saw_session_ended = false;

    timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                match value.get("type").and_then(|value| value.as_str()) {
                    Some("provider_event") => {
                        let has_errors = value
                            .get("errors")
                            .and_then(|value| value.as_array())
                            .map(|errors| !errors.is_empty())
                            .unwrap_or(false);
                        let has_response_errors = value
                            .get("response_errors")
                            .and_then(|value| value.as_array())
                            .map(|errors| !errors.is_empty())
                            .unwrap_or(false);
                        if has_errors || has_response_errors {
                            saw_provider_error = true;
                        }
                    }
                    Some("session_ended") => {
                        saw_session_ended = true;
                        assert_eq!(
                            value.get("reason").and_then(|value| value.as_str()),
                            Some("provider_error")
                        );
                        break;
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("timeout");

    assert!(saw_provider_error, "expected provider_event with errors");
    assert!(saw_session_ended, "expected session_ended");
}
