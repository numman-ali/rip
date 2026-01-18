use serde_json::{Map, Value};

use rip_openresponses::{validate_specific_tool_choice_param, validate_tool_choice_param};

use super::tool_type_only;

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
        Self::specific(SpecificToolChoiceParam::function(name))
    }

    pub fn specific(tool: SpecificToolChoiceParam) -> Self {
        Self::new(tool.into_value())
    }

    pub fn specific_file_search() -> Self {
        Self::specific(SpecificToolChoiceParam::file_search())
    }

    pub fn specific_web_search() -> Self {
        Self::specific(SpecificToolChoiceParam::web_search())
    }

    pub fn specific_web_search_preview() -> Self {
        Self::specific(SpecificToolChoiceParam::web_search_preview())
    }

    pub fn specific_image_generation() -> Self {
        Self::specific(SpecificToolChoiceParam::image_generation())
    }

    pub fn specific_computer_preview() -> Self {
        Self::specific(SpecificToolChoiceParam::computer_preview())
    }

    pub fn specific_computer_use_preview() -> Self {
        Self::specific(SpecificToolChoiceParam::computer_use_preview())
    }

    pub fn specific_code_interpreter() -> Self {
        Self::specific(SpecificToolChoiceParam::code_interpreter())
    }

    pub fn specific_local_shell() -> Self {
        Self::specific(SpecificToolChoiceParam::local_shell())
    }

    pub fn specific_shell() -> Self {
        Self::specific(SpecificToolChoiceParam::shell())
    }

    pub fn specific_apply_patch() -> Self {
        Self::specific(SpecificToolChoiceParam::apply_patch())
    }

    pub fn specific_custom(name: impl Into<String>) -> Self {
        Self::specific(SpecificToolChoiceParam::custom(name))
    }

    pub fn specific_mcp(server_label: impl Into<String>) -> Self {
        Self::specific(SpecificToolChoiceParam::mcp(server_label))
    }

    pub fn allowed_tools(tools: Vec<SpecificToolChoiceParam>) -> Self {
        Self::allowed_tools_with_mode(tools, None)
    }

    pub fn allowed_tools_with_mode(
        tools: Vec<SpecificToolChoiceParam>,
        mode: Option<ToolChoiceValue>,
    ) -> Self {
        let array = tools
            .into_iter()
            .map(SpecificToolChoiceParam::into_value)
            .collect::<Vec<_>>();
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("allowed_tools".to_string()),
        );
        obj.insert("tools".to_string(), Value::Array(array));
        if let Some(mode) = mode {
            obj.insert("mode".to_string(), Value::String(mode.as_str().to_string()));
        }
        Self::new(Value::Object(obj))
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

#[derive(Debug, Clone, Copy)]
pub enum ToolChoiceValue {
    Auto,
    Required,
    None,
}

impl ToolChoiceValue {
    fn as_str(&self) -> &'static str {
        match self {
            ToolChoiceValue::Auto => "auto",
            ToolChoiceValue::Required => "required",
            ToolChoiceValue::None => "none",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SpecificToolChoiceParam {
    value: Value,
    errors: Vec<String>,
}

impl SpecificToolChoiceParam {
    pub fn new(value: Value) -> Self {
        let errors = match validate_specific_tool_choice_param(&value) {
            Ok(_) => Vec::new(),
            Err(errs) => errs,
        };
        Self { value, errors }
    }

    pub fn function(name: impl Into<String>) -> Self {
        let mut obj = Map::new();
        obj.insert("type".to_string(), Value::String("function".to_string()));
        obj.insert("name".to_string(), Value::String(name.into()));
        Self::new(Value::Object(obj))
    }

    pub fn custom(name: impl Into<String>) -> Self {
        let mut obj = Map::new();
        obj.insert("type".to_string(), Value::String("custom".to_string()));
        obj.insert("name".to_string(), Value::String(name.into()));
        Self::new(Value::Object(obj))
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

    pub fn file_search() -> Self {
        Self::new(Value::Object(tool_type_only("file_search")))
    }

    pub fn web_search() -> Self {
        Self::new(Value::Object(tool_type_only("web_search")))
    }

    pub fn web_search_preview() -> Self {
        Self::new(Value::Object(tool_type_only("web_search_preview")))
    }

    pub fn image_generation() -> Self {
        Self::new(Value::Object(tool_type_only("image_generation")))
    }

    pub fn computer_preview() -> Self {
        Self::new(Value::Object(tool_type_only("computer-preview")))
    }

    pub fn computer_use_preview() -> Self {
        Self::new(Value::Object(tool_type_only("computer_use_preview")))
    }

    pub fn code_interpreter() -> Self {
        Self::new(Value::Object(tool_type_only("code_interpreter")))
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
