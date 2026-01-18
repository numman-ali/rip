use super::*;

#[test]
fn validate_content_block_schemas() {
    let errors = schema_errors(
        "InputTextContent.json",
        serde_json::json!({ "type": "input_text", "text": "hi" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "InputTextContentParam.json",
        serde_json::json!({ "type": "input_text", "text": "hi" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "InputImageContent.json",
        serde_json::json!({
            "type": "input_image",
            "image_url": "https://example.com/image.png",
            "file_id": null,
            "detail": "auto"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "InputImageContentParamAutoParam.json",
        serde_json::json!({
            "type": "input_image",
            "image_url": "https://example.com/image.png",
            "file_id": null,
            "detail": "auto"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("InputImageMaskContentParam.json", serde_json::json!({}));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "InputFileContent.json",
        serde_json::json!({
            "type": "input_file",
            "file_id": "file_1",
            "filename": "notes.txt"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "InputFileContentParam.json",
        serde_json::json!({
            "type": "input_file",
            "file_id": "file_1"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = openapi_schema_errors(
        "InputVideoContent",
        serde_json::json!({
            "type": "input_video",
            "video_url": "https://example.com/video.mp4"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "OutputTextContent.json",
        serde_json::json!({
            "type": "output_text",
            "text": "hi",
            "annotations": [],
            "logprobs": []
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "OutputTextContentParam.json",
        serde_json::json!({ "type": "output_text", "text": "hi" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "TextContent.json",
        serde_json::json!({ "type": "text", "text": "hi" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "SummaryTextContent.json",
        serde_json::json!({ "type": "summary_text", "text": "ok" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ReasoningTextContent.json",
        serde_json::json!({ "type": "reasoning_text", "text": "ok" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "RefusalContent.json",
        serde_json::json!({ "type": "refusal", "refusal": "nope" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "RefusalContentParam.json",
        serde_json::json!({ "type": "refusal", "refusal": "nope" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ReasoningSummaryContentParam.json",
        serde_json::json!({ "type": "summary_text", "text": "ok" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn validate_annotation_schemas() {
    let file_citation = serde_json::json!({
        "type": "file_citation",
        "file_id": "file_1",
        "index": 0,
        "filename": "notes.txt"
    });
    let errors = schema_errors("FileCitationBody.json", file_citation.clone());
    assert!(errors.is_empty(), "errors: {errors:?}");

    let url_citation = serde_json::json!({
        "type": "url_citation",
        "url": "https://example.com",
        "start_index": 0,
        "end_index": 10,
        "title": "Example"
    });
    let errors = schema_errors("UrlCitationBody.json", url_citation.clone());
    assert!(errors.is_empty(), "errors: {errors:?}");

    let container_citation = serde_json::json!({
        "type": "container_file_citation",
        "container_id": "cntr_1",
        "file_id": "file_1",
        "start_index": 0,
        "end_index": 4,
        "filename": "doc.txt"
    });
    let errors = schema_errors("ContainerFileCitationBody.json", container_citation.clone());
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("Annotation.json", file_citation);
    assert!(errors.is_empty(), "errors: {errors:?}");

    let file_citation_param = serde_json::json!({
        "type": "file_citation",
        "index": 0,
        "file_id": "file_1",
        "filename": "notes.txt"
    });
    let errors = schema_errors("FileCitationParam.json", file_citation_param.clone());
    assert!(errors.is_empty(), "errors: {errors:?}");

    let url_citation_param = serde_json::json!({
        "type": "url_citation",
        "start_index": 0,
        "end_index": 10,
        "url": "https://example.com",
        "title": "Example"
    });
    let errors = schema_errors("UrlCitationParam.json", url_citation_param.clone());
    assert!(errors.is_empty(), "errors: {errors:?}");

    let container_citation_param = serde_json::json!({
        "type": "container_file_citation",
        "start_index": 0,
        "end_index": 4,
        "container_id": "cntr_1",
        "file_id": "file_1",
        "filename": "doc.txt"
    });
    let errors = schema_errors("ContainerFileCitationParam.json", container_citation_param);
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "OutputTextContentParam.json",
        serde_json::json!({
            "type": "output_text",
            "text": "hi",
            "annotations": [file_citation_param]
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "OutputTextContent.json",
        serde_json::json!({
            "type": "output_text",
            "text": "hi",
            "annotations": [url_citation],
            "logprobs": []
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn validate_response_format_schemas() {
    let errors = schema_errors(
        "TextResponseFormat.json",
        serde_json::json!({ "type": "text" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "JsonObjectResponseFormat.json",
        serde_json::json!({ "type": "json_object" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "JsonSchemaResponseFormat.json",
        serde_json::json!({
            "type": "json_schema",
            "name": "schema",
            "description": null,
            "schema": {},
            "strict": false
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = openapi_schema_errors(
        "JsonSchemaResponseFormatParam",
        serde_json::json!({
            "type": "json_schema",
            "name": "schema",
            "description": "desc",
            "schema": {},
            "strict": false
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = openapi_schema_errors("TextFormatParam", serde_json::json!({ "type": "text" }));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "TextField.json",
        serde_json::json!({
            "format": { "type": "text" },
            "verbosity": "medium"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "TextParam.json",
        serde_json::json!({
            "format": { "type": "text" },
            "verbosity": "medium"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");
}
