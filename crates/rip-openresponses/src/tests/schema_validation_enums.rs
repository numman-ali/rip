use super::*;

#[test]
fn validate_enum_and_metadata_schemas() {
    let errors = schema_errors("MessageRole.json", serde_json::json!("assistant"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("MessageRole1.json", serde_json::json!("user"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("MessageStatus.json", serde_json::json!("completed"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("MetadataParam.json", serde_json::json!({ "tag": "alpha" }));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("OrderEnum.json", serde_json::json!("asc"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "IncludeEnum.json",
        serde_json::json!("file_search_call.results"),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("ExcludeEnum.json", serde_json::json!("instructions"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("TruncationEnum.json", serde_json::json!("auto"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("ServiceTierEnum.json", serde_json::json!("auto"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("VerbosityEnum.json", serde_json::json!("medium"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("PromptCacheRetentionEnum.json", serde_json::json!("24h"));
    assert!(errors.is_empty(), "errors: {errors:?}");
}
