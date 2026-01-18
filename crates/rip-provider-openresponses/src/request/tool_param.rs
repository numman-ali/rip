use serde_json::{Map, Value};

use rip_openresponses::validate_responses_tool_param;

use super::{computer_tool_value, tool_type_only};

#[derive(Debug, Clone)]
pub struct ToolParam {
    value: Value,
    errors: Vec<String>,
}

impl ToolParam {
    pub fn new(value: Value) -> Self {
        let errors = match validate_responses_tool_param(&value) {
            Ok(_) => Vec::new(),
            Err(errs) => errs,
        };
        Self { value, errors }
    }

    pub fn function(name: impl Into<String>) -> Self {
        let mut obj = Map::new();
        obj.insert("type".to_string(), Value::String("function".to_string()));
        obj.insert("name".to_string(), Value::String(name.into()));
        let value = Value::Object(obj);
        Self::new(value)
    }

    pub fn code_interpreter(container: impl Into<Value>) -> Self {
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("code_interpreter".to_string()),
        );
        obj.insert("container".to_string(), container.into());
        Self::new(Value::Object(obj))
    }

    pub fn code_interpreter_auto(
        file_ids: Option<Vec<String>>,
        memory_limit: Option<String>,
    ) -> Self {
        let mut container = Map::new();
        container.insert("type".to_string(), Value::String("auto".to_string()));
        if let Some(file_ids) = file_ids {
            container.insert(
                "file_ids".to_string(),
                Value::Array(file_ids.into_iter().map(Value::String).collect()),
            );
        }
        if let Some(memory_limit) = memory_limit {
            container.insert("memory_limit".to_string(), Value::String(memory_limit));
        }
        Self::code_interpreter(Value::Object(container))
    }

    pub fn custom(name: impl Into<String>) -> Self {
        let mut obj = Map::new();
        obj.insert("type".to_string(), Value::String("custom".to_string()));
        obj.insert("name".to_string(), Value::String(name.into()));
        Self::new(Value::Object(obj))
    }

    pub fn web_search() -> Self {
        Self::new(Value::Object(tool_type_only("web_search")))
    }

    pub fn web_search_2025_08_26() -> Self {
        Self::new(Value::Object(tool_type_only("web_search_2025_08_26")))
    }

    pub fn web_search_ga() -> Self {
        Self::new(Value::Object(tool_type_only("web_search_ga")))
    }

    pub fn web_search_preview() -> Self {
        Self::new(Value::Object(tool_type_only("web_search_preview")))
    }

    pub fn web_search_preview_2025_03_11() -> Self {
        Self::new(Value::Object(tool_type_only(
            "web_search_preview_2025_03_11",
        )))
    }

    pub fn image_generation() -> Self {
        Self::new(Value::Object(tool_type_only("image_generation")))
    }

    pub fn mcp(server_label: impl Into<String>) -> Self {
        let mut obj = Map::new();
        obj.insert("type".to_string(), Value::String("mcp".to_string()));
        obj.insert(
            "server_label".to_string(),
            Value::String(server_label.into()),
        );
        Self::new(Value::Object(obj))
    }

    pub fn file_search(vector_store_ids: Vec<String>) -> Self {
        let mut obj = Map::new();
        obj.insert("type".to_string(), Value::String("file_search".to_string()));
        obj.insert(
            "vector_store_ids".to_string(),
            Value::Array(vector_store_ids.into_iter().map(Value::String).collect()),
        );
        Self::new(Value::Object(obj))
    }

    pub fn computer_preview(
        display_width: u64,
        display_height: u64,
        environment: impl Into<String>,
    ) -> Self {
        Self::new(Value::Object(computer_tool_value(
            "computer-preview",
            display_width,
            display_height,
            environment,
        )))
    }

    pub fn computer_use_preview(
        display_width: u64,
        display_height: u64,
        environment: impl Into<String>,
    ) -> Self {
        Self::new(Value::Object(computer_tool_value(
            "computer_use_preview",
            display_width,
            display_height,
            environment,
        )))
    }

    pub fn local_shell() -> Self {
        Self::new(Value::Object(tool_type_only("local_shell")))
    }

    pub fn shell() -> Self {
        Self::new(Value::Object(tool_type_only("shell")))
    }

    pub fn apply_patch() -> Self {
        Self::new(Value::Object(tool_type_only("apply_patch")))
    }

    pub fn value(&self) -> &Value {
        &self.value
    }

    pub fn into_value(self) -> Value {
        self.value
    }

    pub fn errors(&self) -> &[String] {
        &self.errors
    }
}
