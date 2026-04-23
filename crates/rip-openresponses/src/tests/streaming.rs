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

#[test]
fn validate_stream_event_accepts_prefixed_extension_output_item() {
    let value = serde_json::json!({
        "type": "response.output_item.added",
        "sequence_number": 2,
        "output_index": 0,
        "item": {
            "id": "st_tmp_1",
            "type": "openrouter:web_search",
            "status": "in_progress"
        }
    });

    let errors = validate_stream_event(&value).err().unwrap_or_default();
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn validate_stream_event_rejects_prefixed_extension_item_without_required_fields() {
    let value = serde_json::json!({
        "type": "response.output_item.added",
        "sequence_number": 2,
        "output_index": 0,
        "item": {
            "type": "openrouter:web_search"
        }
    });

    assert!(validate_stream_event(&value).is_err());
}
