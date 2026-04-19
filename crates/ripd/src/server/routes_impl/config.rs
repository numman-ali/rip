use super::super::*;
use axum::{extract::State, response::IntoResponse};

#[utoipa::path(
    get,
    path = "/config/doctor",
    responses(
        (status = 200, description = "Resolved configuration summary", body = ConfigDoctorResponse)
    )
)]
pub(crate) async fn config_doctor(State(state): State<AppState>) -> impl IntoResponse {
    let store = state.engine.continuities();
    let (resolved_openresponses, loaded) = crate::config::resolve_openresponses_config(
        store.workspace_root(),
        crate::config::OpenResponsesOverrideInput::default(),
    );

    let sources = loaded
        .sources
        .into_iter()
        .map(|source| ConfigDoctorSource {
            path: source.path,
            status: source.status,
            error: source.error,
        })
        .collect();

    let openresponses = resolved_openresponses.map(|cfg| ConfigDoctorOpenResponses {
        provider_id: cfg.provider_id,
        route: cfg.route,
        effective_route: cfg.effective_route,
        route_source: cfg.route_source,
        endpoint: cfg.endpoint,
        endpoint_source: cfg.endpoint_source,
        model: cfg.model,
        model_source: cfg.model_source,
        has_api_key: cfg
            .api_key
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false),
        api_key_source: cfg.api_key_source,
        headers: cfg.headers.into_iter().map(|(name, _)| name).collect(),
        stateless_history: cfg.stateless_history,
        stateless_history_source: cfg.stateless_history_source,
        parallel_tool_calls: cfg.parallel_tool_calls,
        parallel_tool_calls_source: cfg.parallel_tool_calls_source,
        followup_user_message: cfg.followup_user_message,
        followup_user_message_source: cfg.followup_user_message_source,
    });

    (
        StatusCode::OK,
        Json(ConfigDoctorResponse {
            sources,
            openresponses,
        }),
    )
}
