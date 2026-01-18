use super::*;

#[test]
fn validate_stream_event_rejects_empty() {
    let value = serde_json::json!({});
    assert!(validate_stream_event(&value).is_err());
}

#[test]
fn validate_stream_event_accepts_output_text_delta() {
    let value = serde_json::json!({
        "type": "response.output_text.delta",
        "sequence_number": 1,
        "item_id": "item_1",
        "output_index": 0,
        "content_index": 0,
        "delta": "hi",
        "logprobs": []
    });
    let errors = validate_stream_event(&value).err().unwrap_or_default();
    assert!(errors.is_empty(), "errors: {errors:?}");
}
