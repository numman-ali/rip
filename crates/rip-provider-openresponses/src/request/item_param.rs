use serde_json::{Map, Value};

use rip_openresponses::validate_item_param;

use super::item_type_only;

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

    pub fn item_reference(id: impl Into<String>) -> Self {
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("item_reference".to_string()),
        );
        obj.insert("id".to_string(), Value::String(id.into()));
        Self::new(Value::Object(obj))
    }

    pub fn message(role: impl Into<String>, content: Value) -> Self {
        let mut obj = Map::new();
        obj.insert("type".to_string(), Value::String("message".to_string()));
        obj.insert("role".to_string(), Value::String(role.into()));
        obj.insert("content".to_string(), content);
        Self::new(Value::Object(obj))
    }

    pub fn message_text(role: impl Into<String>, text: impl Into<String>) -> Self {
        Self::message(role, Value::String(text.into()))
    }

    pub fn user_message(content: Value) -> Self {
        Self::message("user", content)
    }

    pub fn user_message_text(text: impl Into<String>) -> Self {
        Self::message_text("user", text)
    }

    pub fn assistant_message(content: Value) -> Self {
        Self::message("assistant", content)
    }

    pub fn assistant_message_text(text: impl Into<String>) -> Self {
        Self::message_text("assistant", text)
    }

    pub fn developer_message(content: Value) -> Self {
        Self::message("developer", content)
    }

    pub fn developer_message_text(text: impl Into<String>) -> Self {
        Self::message_text("developer", text)
    }

    pub fn system_message(content: Value) -> Self {
        Self::message("system", content)
    }

    pub fn system_message_text(text: impl Into<String>) -> Self {
        Self::message_text("system", text)
    }

    pub fn function_call(
        call_id: impl Into<String>,
        name: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Self {
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("function_call".to_string()),
        );
        obj.insert("call_id".to_string(), Value::String(call_id.into()));
        obj.insert("name".to_string(), Value::String(name.into()));
        obj.insert("arguments".to_string(), Value::String(arguments.into()));
        Self::new(Value::Object(obj))
    }

    pub fn function_call_output(call_id: impl Into<String>, output: Value) -> Self {
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("function_call_output".to_string()),
        );
        obj.insert("call_id".to_string(), Value::String(call_id.into()));
        obj.insert("output".to_string(), output);
        Self::new(Value::Object(obj))
    }

    pub fn reasoning(summary: Vec<Value>) -> Self {
        let mut obj = Map::new();
        obj.insert("type".to_string(), Value::String("reasoning".to_string()));
        obj.insert("summary".to_string(), Value::Array(summary));
        Self::new(Value::Object(obj))
    }

    pub fn compaction(encrypted_content: impl Into<String>) -> Self {
        let mut obj = Map::new();
        obj.insert("type".to_string(), Value::String("compaction".to_string()));
        obj.insert(
            "encrypted_content".to_string(),
            Value::String(encrypted_content.into()),
        );
        Self::new(Value::Object(obj))
    }

    pub fn code_interpreter_call(
        id: impl Into<String>,
        container_id: impl Into<String>,
        code: impl Into<String>,
    ) -> Self {
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("code_interpreter_call".to_string()),
        );
        obj.insert("id".to_string(), Value::String(id.into()));
        obj.insert(
            "container_id".to_string(),
            Value::String(container_id.into()),
        );
        obj.insert("code".to_string(), Value::String(code.into()));
        Self::new(Value::Object(obj))
    }

    pub fn computer_call(call_id: impl Into<String>, action: Value) -> Self {
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("computer_call".to_string()),
        );
        obj.insert("call_id".to_string(), Value::String(call_id.into()));
        obj.insert("action".to_string(), action);
        Self::new(Value::Object(obj))
    }

    pub fn computer_call_output(call_id: impl Into<String>, output: Value) -> Self {
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("computer_call_output".to_string()),
        );
        obj.insert("call_id".to_string(), Value::String(call_id.into()));
        obj.insert("output".to_string(), output);
        Self::new(Value::Object(obj))
    }

    pub fn custom_tool_call(
        call_id: impl Into<String>,
        name: impl Into<String>,
        input: impl Into<String>,
    ) -> Self {
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("custom_tool_call".to_string()),
        );
        obj.insert("call_id".to_string(), Value::String(call_id.into()));
        obj.insert("name".to_string(), Value::String(name.into()));
        obj.insert("input".to_string(), Value::String(input.into()));
        Self::new(Value::Object(obj))
    }

    pub fn custom_tool_call_output(call_id: impl Into<String>, output: impl Into<String>) -> Self {
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("custom_tool_call_output".to_string()),
        );
        obj.insert("call_id".to_string(), Value::String(call_id.into()));
        obj.insert("output".to_string(), Value::String(output.into()));
        Self::new(Value::Object(obj))
    }

    pub fn file_search_call(id: impl Into<String>, queries: Vec<String>) -> Self {
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("file_search_call".to_string()),
        );
        obj.insert("id".to_string(), Value::String(id.into()));
        obj.insert(
            "queries".to_string(),
            Value::Array(queries.into_iter().map(Value::String).collect()),
        );
        Self::new(Value::Object(obj))
    }

    pub fn web_search_call() -> Self {
        Self::new(Value::Object(item_type_only("web_search_call")))
    }

    pub fn image_generation_call(id: impl Into<String>) -> Self {
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("image_generation_call".to_string()),
        );
        obj.insert("id".to_string(), Value::String(id.into()));
        Self::new(Value::Object(obj))
    }

    pub fn local_shell_call(call_id: impl Into<String>, action: Value) -> Self {
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("local_shell_call".to_string()),
        );
        obj.insert("call_id".to_string(), Value::String(call_id.into()));
        obj.insert("action".to_string(), action);
        Self::new(Value::Object(obj))
    }

    pub fn local_shell_call_output(call_id: impl Into<String>, output: impl Into<String>) -> Self {
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("local_shell_call_output".to_string()),
        );
        obj.insert("call_id".to_string(), Value::String(call_id.into()));
        obj.insert("output".to_string(), Value::String(output.into()));
        Self::new(Value::Object(obj))
    }

    pub fn shell_call(call_id: impl Into<String>, action: Value) -> Self {
        let mut obj = Map::new();
        obj.insert("type".to_string(), Value::String("shell_call".to_string()));
        obj.insert("call_id".to_string(), Value::String(call_id.into()));
        obj.insert("action".to_string(), action);
        Self::new(Value::Object(obj))
    }

    pub fn shell_call_output(call_id: impl Into<String>, output: Vec<Value>) -> Self {
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("shell_call_output".to_string()),
        );
        obj.insert("call_id".to_string(), Value::String(call_id.into()));
        obj.insert("output".to_string(), Value::Array(output));
        Self::new(Value::Object(obj))
    }

    pub fn apply_patch_call(
        call_id: impl Into<String>,
        status: impl Into<String>,
        operation: Value,
    ) -> Self {
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("apply_patch_call".to_string()),
        );
        obj.insert("call_id".to_string(), Value::String(call_id.into()));
        obj.insert("status".to_string(), Value::String(status.into()));
        obj.insert("operation".to_string(), operation);
        Self::new(Value::Object(obj))
    }

    pub fn apply_patch_call_output(call_id: impl Into<String>, status: impl Into<String>) -> Self {
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("apply_patch_call_output".to_string()),
        );
        obj.insert("call_id".to_string(), Value::String(call_id.into()));
        obj.insert("status".to_string(), Value::String(status.into()));
        Self::new(Value::Object(obj))
    }

    pub fn mcp_approval_request(
        server_label: impl Into<String>,
        name: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Self {
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("mcp_approval_request".to_string()),
        );
        obj.insert(
            "server_label".to_string(),
            Value::String(server_label.into()),
        );
        obj.insert("name".to_string(), Value::String(name.into()));
        obj.insert("arguments".to_string(), Value::String(arguments.into()));
        Self::new(Value::Object(obj))
    }

    pub fn mcp_approval_response(approval_request_id: impl Into<String>, approve: bool) -> Self {
        let mut obj = Map::new();
        obj.insert(
            "type".to_string(),
            Value::String("mcp_approval_response".to_string()),
        );
        obj.insert(
            "approval_request_id".to_string(),
            Value::String(approval_request_id.into()),
        );
        obj.insert("approve".to_string(), Value::Bool(approve));
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
