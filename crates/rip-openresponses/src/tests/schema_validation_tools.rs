use super::*;

#[test]
fn validate_memory_tool_param_schema() {
    let value = serde_json::json!({
        "type": "memory",
        "memory": "remember this",
        "environment": {
            "type": "local_file",
            "root": "/tmp"
        }
    });
    let errors = schema_errors("MemoryToolParam.json", value);
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn validate_mcp_filter_and_require_approval_schemas() {
    let errors = schema_errors(
        "MCPToolFilterField.json",
        serde_json::json!({
            "tool_names": ["tool_a"],
            "read_only": null
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "MCPToolFilterParam.json",
        serde_json::json!({
            "tool_names": ["tool_a"],
            "read_only": true
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "MCPRequireApprovalApiEnum.json",
        serde_json::json!("always"),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "MCPRequireApprovalFieldEnum.json",
        serde_json::json!("never"),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "MCPRequireApprovalFilterField.json",
        serde_json::json!({
            "always": null,
            "never": null
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "MCPRequireApprovalFilterParam.json",
        serde_json::json!({
            "always": { "tool_names": ["tool_b"], "read_only": false }
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("MCPToolCallStatus.json", serde_json::json!("completed"));
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn validate_mcp_error_schemas() {
    let errors = schema_errors(
        "MCPProtocolError.json",
        serde_json::json!({
            "type": "mcp_protocol_error",
            "code": 400,
            "message": "bad request"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "MCPToolExecutionError.json",
        serde_json::json!({
            "type": "mcp_tool_execution_error",
            "content": { "detail": "fail" }
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "HTTPError.json",
        serde_json::json!({
            "type": "http_error",
            "code": 500,
            "message": "server error"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn validate_shell_param_schemas() {
    let errors = schema_errors(
        "LocalShellExecActionParam.json",
        serde_json::json!({
            "type": "exec",
            "command": ["echo", "hi"],
            "env": {}
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "LocalShellCallItemStatus.json",
        serde_json::json!("completed"),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("LocalShellCallStatus.json", serde_json::json!("completed"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "LocalShellCallOutputStatusEnum.json",
        serde_json::json!("completed"),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "FunctionShellActionParam.json",
        serde_json::json!({
            "commands": ["ls"],
            "timeout_ms": null,
            "max_output_length": null
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "FunctionShellCallItemStatus.json",
        serde_json::json!("completed"),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "FunctionShellCallOutputContentParam.json",
        serde_json::json!({
            "stdout": "",
            "stderr": "",
            "outcome": {
                "type": "exit",
                "exit_code": 0
            }
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "FunctionShellCallOutputOutcomeParam.json",
        serde_json::json!({ "type": "timeout" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "FunctionShellCallOutputExitOutcomeParam.json",
        serde_json::json!({ "type": "exit", "exit_code": 0 }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "FunctionShellCallOutputTimeoutOutcomeParam.json",
        serde_json::json!({ "type": "timeout" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn validate_file_search_and_status_schemas() {
    let errors = schema_errors("RankerVersionType.json", serde_json::json!("auto"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "HybridSearchOptionsParam.json",
        serde_json::json!({ "embedding_weight": 0.4, "text_weight": 0.6 }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "HybridSearchOptions.json",
        serde_json::json!({ "embedding_weight": 0.4, "text_weight": 0.6 }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "FileSearchRankingOptionsParam.json",
        serde_json::json!({
            "ranker": "auto",
            "score_threshold": 0.2,
            "hybrid_search": { "embedding_weight": 0.4, "text_weight": 0.6 }
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "FileSearchRetrievedChunksParam.json",
        serde_json::json!({
            "file_id": "file_1",
            "filename": "notes.txt",
            "text": "chunk",
            "attributes": {},
            "score": 0.1,
            "vector_store_id": null
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "FileSearchResult.json",
        serde_json::json!({
            "file_id": "file_1",
            "filename": "notes.txt",
            "text": "chunk",
            "attributes": {},
            "score": 0.1,
            "vector_store_id": null
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "FileSearchToolCallStatusEnum.json",
        serde_json::json!("completed"),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("FunctionCallStatus.json", serde_json::json!("completed"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "FunctionCallOutputStatusEnum.json",
        serde_json::json!("completed"),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "FunctionCallItemStatus.json",
        serde_json::json!("completed"),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "FunctionShellCallItemStatus.json",
        serde_json::json!("completed"),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "FunctionShellCallOutputExitOutcome.json",
        serde_json::json!({ "type": "exit", "exit_code": 0 }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "FunctionShellCallOutputTimeoutOutcome.json",
        serde_json::json!({ "type": "timeout" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "FunctionShellCallOutputContent.json",
        serde_json::json!({
            "stdout": "",
            "stderr": "",
            "outcome": { "type": "exit", "exit_code": 0 }
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn validate_search_and_tool_param_schemas() {
    let errors = schema_errors(
        "FileSearchToolCallStatusEnum.json",
        serde_json::json!("completed"),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("WebSearchCallStatus.json", serde_json::json!("completed"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "WebSearchCallActionSearchParam.json",
        serde_json::json!({
            "type": "search",
            "query": null,
            "queries": ["q"],
            "sources": [
                { "type": "url", "url": "https://example.com" },
                { "type": "api", "name": "internal" }
            ]
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "WebSearchCallActionOpenPageParam.json",
        serde_json::json!({
            "type": "open_page",
            "url": null
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "WebSearchCallActionFindInPageParam.json",
        serde_json::json!({
            "type": "find_in_page",
            "url": null,
            "pattern": null
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComputerCallOutputStatus.json",
        serde_json::json!("completed"),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ComputerCallSafetyCheckParam.json",
        serde_json::json!({ "id": "sc_1" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("ImageGenCallStatus.json", serde_json::json!("completed"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("ImageGenAction.json", serde_json::json!("generate"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("ApplyPatchCallStatus.json", serde_json::json!("completed"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ApplyPatchCallStatusParam.json",
        serde_json::json!("completed"),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ApplyPatchCallOutputStatus.json",
        serde_json::json!("completed"),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ApplyPatchCallOutputStatusParam.json",
        serde_json::json!("completed"),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ApplyPatchOperationParam.json",
        serde_json::json!({
            "type": "update_file",
            "path": "notes.txt",
            "diff": "@@ -1 +1 @@\\n-hello\\n+hi\\n"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ApplyPatchCreateFileOperationParam.json",
        serde_json::json!({
            "type": "create_file",
            "path": "notes.txt",
            "diff": "@@ -0,0 +1 @@\\n+hello\\n"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ApplyPatchDeleteFileOperationParam.json",
        serde_json::json!({
            "type": "delete_file",
            "path": "notes.txt"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ApplyPatchUpdateFileOperationParam.json",
        serde_json::json!({
            "type": "update_file",
            "path": "notes.txt",
            "diff": "@@ -1 +1 @@\\n-hello\\n+hi\\n"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn validate_code_interpreter_param_schemas() {
    let errors = schema_errors(
        "CodeInterpreterCallStatus.json",
        serde_json::json!("completed"),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "CodeInterpreterOutputLogs.json",
        serde_json::json!({
            "type": "logs",
            "logs": "ok"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "CodeInterpreterOutputImage.json",
        serde_json::json!({
            "type": "image",
            "url": "https://example.com/img.png"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "CodeInterpreterToolCallOutputLogsParam.json",
        serde_json::json!({
            "type": "logs",
            "logs": "ok"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "CodeInterpreterToolCallOutputImageParam.json",
        serde_json::json!({
            "type": "image",
            "url": "https://example.com/img.png"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn validate_custom_tool_and_format_schemas() {
    let errors = schema_errors(
        "CustomToolCall.json",
        serde_json::json!({
            "type": "custom_tool_call",
            "id": "ctc_1",
            "call_id": "call_1",
            "name": "custom_tool",
            "input": "{}",
            "status": "completed"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "CustomToolCallOutput.json",
        serde_json::json!({
            "type": "custom_tool_call_output",
            "id": "ctc_out_1",
            "call_id": "call_1",
            "output": "ok",
            "status": "completed"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "CustomTextFormatField.json",
        serde_json::json!({ "type": "text" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "CustomTextFormatParam.json",
        serde_json::json!({ "type": "text" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "CustomGrammarFormatField.json",
        serde_json::json!({ "type": "grammar", "syntax": "lark", "definition": "root: /./" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "CustomGrammarFormatParam.json",
        serde_json::json!({ "type": "grammar", "syntax": "lark", "definition": "root: /./" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "CustomToolFormat.json",
        serde_json::json!({ "type": "text" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "RankingOptions.json",
        serde_json::json!({ "ranker": "auto", "score_threshold": 0.0 }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "UrlSourceParam.json",
        serde_json::json!({ "type": "url", "url": "https://example.com" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "LocalFileEnvironmentParam.json",
        serde_json::json!({ "type": "local_file", "root": "/tmp" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");
}
