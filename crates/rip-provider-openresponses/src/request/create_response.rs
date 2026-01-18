use serde_json::{Map, Value};

use rip_openresponses::validate_create_response_body;

use super::{ItemParam, ToolChoiceParam, ToolParam};

#[derive(Debug, Clone)]
pub struct CreateResponsePayload {
    body: Value,
    errors: Vec<String>,
}

impl CreateResponsePayload {
    pub fn new(body: Value) -> Self {
        let errors = match validate_create_response_body(&body) {
            Ok(_) => Vec::new(),
            Err(errs) => errs,
        };
        Self { body, errors }
    }

    pub fn body(&self) -> &Value {
        &self.body
    }

    pub fn into_body(self) -> Value {
        self.body
    }

    pub fn errors(&self) -> &[String] {
        &self.errors
    }
}

#[derive(Debug, Default)]
pub struct CreateResponseBuilder {
    body: Map<String, Value>,
}

impl CreateResponseBuilder {
    pub fn new() -> Self {
        Self { body: Map::new() }
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.body
            .insert("model".to_string(), Value::String(model.into()));
        self
    }

    pub fn input_text(mut self, text: impl Into<String>) -> Self {
        self.body
            .insert("input".to_string(), Value::String(text.into()));
        self
    }

    pub fn input_items(mut self, items: Vec<ItemParam>) -> Self {
        let array = items
            .into_iter()
            .map(ItemParam::into_value)
            .collect::<Vec<_>>();
        self.body.insert("input".to_string(), Value::Array(array));
        self
    }

    pub fn input_items_raw(mut self, items: Vec<Value>) -> Self {
        self.body.insert("input".to_string(), Value::Array(items));
        self
    }

    pub fn tools(mut self, tools: Vec<ToolParam>) -> Self {
        let array = tools
            .into_iter()
            .map(ToolParam::into_value)
            .collect::<Vec<_>>();
        self.body.insert("tools".to_string(), Value::Array(array));
        self
    }

    pub fn tools_raw(mut self, tools: Vec<Value>) -> Self {
        self.body.insert("tools".to_string(), Value::Array(tools));
        self
    }

    pub fn tool_choice(mut self, choice: ToolChoiceParam) -> Self {
        self.body
            .insert("tool_choice".to_string(), choice.into_value());
        self
    }

    pub fn tool_choice_raw(mut self, choice: Value) -> Self {
        self.body.insert("tool_choice".to_string(), choice);
        self
    }

    pub fn parallel_tool_calls(mut self, enabled: bool) -> Self {
        self.body
            .insert("parallel_tool_calls".to_string(), Value::Bool(enabled));
        self
    }

    pub fn max_tool_calls(mut self, max_calls: u64) -> Self {
        self.body.insert(
            "max_tool_calls".to_string(),
            Value::Number(max_calls.into()),
        );
        self
    }

    pub fn insert_raw(mut self, key: impl Into<String>, value: Value) -> Self {
        self.body.insert(key.into(), value);
        self
    }

    pub fn build(self) -> CreateResponsePayload {
        CreateResponsePayload::new(Value::Object(self.body))
    }
}
