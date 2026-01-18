use super::*;

#[test]
fn validate_usage_and_logprob_schemas() {
    let errors = schema_errors(
        "TopLogProb.json",
        serde_json::json!({
            "token": "hello",
            "logprob": -0.12,
            "bytes": [104, 101, 108, 108, 111]
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "LogProb.json",
        serde_json::json!({
            "token": "hello",
            "logprob": -0.12,
            "bytes": [104, 101, 108, 108, 111],
            "top_logprobs": [
                { "token": "hi", "logprob": -0.2, "bytes": [104, 105] }
            ]
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "InputTokensDetails.json",
        serde_json::json!({ "cached_tokens": 2 }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "OutputTokensDetails.json",
        serde_json::json!({ "reasoning_tokens": 1 }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "Usage.json",
        serde_json::json!({
            "input_tokens": 10,
            "output_tokens": 5,
            "total_tokens": 15,
            "input_tokens_details": { "cached_tokens": 2 },
            "output_tokens_details": { "reasoning_tokens": 1 }
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "TokenCountsBody.json",
        serde_json::json!({ "model": "gpt-4.1" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "TokenCountsResource.json",
        serde_json::json!({
            "object": "response.input_tokens",
            "input_tokens": 123
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");
}
