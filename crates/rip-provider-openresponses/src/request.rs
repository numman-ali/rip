use serde_json::{Map, Value};

use rip_openresponses::{
    validate_create_response_body, validate_item_param, validate_responses_tool_param,
    validate_tool_choice_param,
};

#[derive(Debug, Clone)]
pub struct ItemParam {
    value: Value,
    errors: Vec<String>,
}

impl ItemParam {
    pub fn new(value: Value) -> Self {
        let errors = match validate_item_param(&value) {
            Ok(_) => Vec::new(),
            Err(errs) => errs,
        };
        Self { value, errors }
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

#[derive(Debug, Clone)]
pub struct ToolChoiceParam {
    value: Value,
    errors: Vec<String>,
}

impl ToolChoiceParam {
    pub fn new(value: Value) -> Self {
        let errors = match validate_tool_choice_param(&value) {
            Ok(_) => Vec::new(),
            Err(errs) => errs,
        };
        Self { value, errors }
    }

    pub fn auto() -> Self {
        Self::new(Value::String("auto".to_string()))
    }

    pub fn none() -> Self {
        Self::new(Value::String("none".to_string()))
    }

    pub fn required() -> Self {
        Self::new(Value::String("required".to_string()))
    }

    pub fn specific_function(name: impl Into<String>) -> Self {
        let mut obj = Map::new();
        obj.insert("type".to_string(), Value::String("function".to_string()));
        obj.insert("name".to_string(), Value::String(name.into()));
        let value = Value::Object(obj);
        Self::new(value)
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
