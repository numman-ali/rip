use super::*;

fn fixture_response_resource() -> Value {
    let raw =
        include_str!("../../../rip-provider-openresponses/fixtures/openresponses/stream_all.jsonl");
    for line in raw.lines() {
        let value: Value = serde_json::from_str(line).expect("fixture line must be valid json");
        if let Some(response) = value.get("response") {
            return response.clone();
        }
    }
    panic!("stream fixture missing response resource");
}

fn response_with_tool_choice(choice: Value) -> Value {
    let mut response = fixture_response_resource();
    if let Value::Object(map) = &mut response {
        map.insert("tool_choice".to_string(), choice);
    }
    response
}

fn response_with_tools(tools: Vec<Value>) -> Value {
    let mut response = fixture_response_resource();
    if let Value::Object(map) = &mut response {
        map.insert("tools".to_string(), Value::Array(tools));
    }
    response
}

fn response_with_output(items: Vec<Value>) -> Value {
    let mut response = fixture_response_resource();
    if let Value::Object(map) = &mut response {
        map.insert("output".to_string(), Value::Array(items));
    }
    response
}

fn schema_errors(name: &str, value: Value) -> Vec<String> {
    let schema = compile_split_schema(name);
    let errors = match schema.validate(&value) {
        Ok(_) => Vec::new(),
        Err(errors) => errors.map(|err| err.to_string()).collect(),
    };
    errors
}

fn openapi_schema_errors(name: &str, value: Value) -> Vec<String> {
    let root_ref = serde_json::json!({
        "$ref": format!("urn:openresponses:openapi#/components/schemas/{name}")
    });
    let validator = JSONSchema::options()
        .with_document("urn:openresponses:openapi".to_string(), OPENAPI.clone())
        .compile(&root_ref)
        .unwrap_or_else(|_| panic!("compile openapi schema {name}"));
    let errors = match validator.validate(&value) {
        Ok(_) => Vec::new(),
        Err(errors) => errors.map(|err| err.to_string()).collect(),
    };
    errors
}

mod helper_validation;
mod item_param;
mod request_validation;
mod response_resource;
mod schema_validation_content;
mod schema_validation_filters;
mod schema_validation_misc;
mod schema_validation_tools;
mod smoke;
mod streaming;
