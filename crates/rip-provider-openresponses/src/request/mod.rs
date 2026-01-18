use serde_json::{Map, Value};

mod create_response;
mod item_param;
mod tool_choice;
mod tool_param;

pub use create_response::{CreateResponseBuilder, CreateResponsePayload};
pub use item_param::ItemParam;
pub use tool_choice::{SpecificToolChoiceParam, ToolChoiceParam, ToolChoiceValue};
pub use tool_param::ToolParam;

pub(super) fn tool_type_only(tool_type: &str) -> Map<String, Value> {
    let mut obj = Map::new();
    obj.insert("type".to_string(), Value::String(tool_type.to_string()));
    obj
}

pub(super) fn item_type_only(item_type: &str) -> Map<String, Value> {
    let mut obj = Map::new();
    obj.insert("type".to_string(), Value::String(item_type.to_string()));
    obj
}

pub(super) fn computer_tool_value(
    tool_type: &str,
    display_width: u64,
    display_height: u64,
    environment: impl Into<String>,
) -> Map<String, Value> {
    let mut obj = Map::new();
    obj.insert("type".to_string(), Value::String(tool_type.to_string()));
    obj.insert(
        "display_width".to_string(),
        Value::Number(display_width.into()),
    );
    obj.insert(
        "display_height".to_string(),
        Value::Number(display_height.into()),
    );
    obj.insert("environment".to_string(), Value::String(environment.into()));
    obj
}
