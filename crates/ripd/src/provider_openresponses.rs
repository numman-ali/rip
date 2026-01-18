#[cfg(not(test))]
use std::env;

use rip_provider_openresponses::{CreateResponseBuilder, CreateResponsePayload};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct OpenResponsesConfig {
    pub endpoint: String,
    pub api_key: Option<String>,
    pub model: Option<String>,
}

impl OpenResponsesConfig {
    #[cfg(not(test))]
    pub fn from_env() -> Option<Self> {
        let endpoint = env::var("RIP_OPENRESPONSES_ENDPOINT").ok()?;
        let api_key = env::var("RIP_OPENRESPONSES_API_KEY").ok();
        let model = env::var("RIP_OPENRESPONSES_MODEL").ok();
        Some(Self {
            endpoint,
            api_key,
            model,
        })
    }
}

pub fn build_streaming_request(
    config: &OpenResponsesConfig,
    prompt: &str,
) -> CreateResponsePayload {
    let mut builder = CreateResponseBuilder::new()
        .input_text(prompt)
        .insert_raw("stream", Value::Bool(true));
    if let Some(model) = config.model.as_deref() {
        builder = builder.model(model.to_string());
    }
    builder.build()
}
