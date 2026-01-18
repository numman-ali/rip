use super::*;

#[test]
fn validate_filter_schemas() {
    let eq_field = serde_json::json!({
        "type": "eq",
        "key": "tag",
        "value": "alpha"
    });
    let errors = schema_errors("ComparisonFilterFieldEQ.json", eq_field.clone());
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterFieldNE.json",
        serde_json::json!({ "type": "ne", "key": "tag", "value": "beta" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterFieldLT.json",
        serde_json::json!({ "type": "lt", "key": "score", "value": "10" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterFieldLTE.json",
        serde_json::json!({ "type": "lte", "key": "score", "value": "10" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterFieldGT.json",
        serde_json::json!({ "type": "gt", "key": "score", "value": "5" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterFieldGTE.json",
        serde_json::json!({ "type": "gte", "key": "score", "value": "5" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterFieldIN.json",
        serde_json::json!({ "type": "in", "key": "tag", "value": ["a", "b"] }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterFieldNIN.json",
        serde_json::json!({ "type": "nin", "key": "tag", "value": ["x", "y"] }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterFieldCONTAINS.json",
        serde_json::json!({ "type": "contains", "key": "tag", "value": "foo" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterFieldNCONTAINS.json",
        serde_json::json!({ "type": "ncontains", "key": "tag", "value": "bar" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterFieldCONTAINSANY.json",
        serde_json::json!({ "type": "containsany", "key": "tag", "value": ["foo", "bar"] }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterFieldNCONTAINSANY.json",
        serde_json::json!({ "type": "ncontainsany", "key": "tag", "value": ["baz"] }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "CompoundFilterFieldAND.json",
        serde_json::json!({ "type": "and", "filters": [eq_field.clone()] }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "CompoundFilterFieldOR.json",
        serde_json::json!({ "type": "or", "filters": [eq_field.clone()] }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("Filters.json", eq_field);
    assert!(errors.is_empty(), "errors: {errors:?}");

    let eq_param = serde_json::json!({
        "type": "eq",
        "key": "tag",
        "value": "alpha"
    });
    let errors = schema_errors("ComparisonFilterParamEQParam.json", eq_param.clone());
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterParamNEParam.json",
        serde_json::json!({ "type": "ne", "key": "tag", "value": "beta" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterParamLTParam.json",
        serde_json::json!({ "type": "lt", "key": "score", "value": "10" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterParamLTEParam.json",
        serde_json::json!({ "type": "lte", "key": "score", "value": "10" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterParamGTParam.json",
        serde_json::json!({ "type": "gt", "key": "score", "value": "5" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterParamGTEParam.json",
        serde_json::json!({ "type": "gte", "key": "score", "value": "5" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterParamINParam.json",
        serde_json::json!({ "type": "in", "key": "tag", "value": ["a", "b"] }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterParamNINParam.json",
        serde_json::json!({ "type": "nin", "key": "tag", "value": ["x", "y"] }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterParamContainsParam.json",
        serde_json::json!({ "type": "contains", "key": "tag", "value": "foo" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterParamNContainsParam.json",
        serde_json::json!({ "type": "ncontains", "key": "tag", "value": "bar" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterParamContainsAnyParam.json",
        serde_json::json!({ "type": "containsany", "key": "tag", "value": ["foo", "bar"] }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComparisonFilterParamNContainsAnyParam.json",
        serde_json::json!({ "type": "ncontainsany", "key": "tag", "value": ["baz"] }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "CompoundFilterParamAndParam.json",
        serde_json::json!({ "type": "and", "filters": [eq_param.clone()] }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "CompoundFilterParamOrParam.json",
        serde_json::json!({ "type": "or", "filters": [eq_param] }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");
}
