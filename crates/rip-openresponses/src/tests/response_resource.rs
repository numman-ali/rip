use super::*;

#[test]
fn validate_response_resource_rejects_empty() {
    let value = serde_json::json!({});
    assert!(validate_response_resource(&value).is_err());
}

#[test]
fn validate_response_resource_accepts_fixture() {
    let value = fixture_response_resource();
    let errors = validate_response_resource(&value).err().unwrap_or_default();
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn validate_response_resource_accepts_tool_choice_variants() {
    let variants = vec![
        serde_json::json!({ "type": "code_interpreter" }),
        serde_json::json!({ "type": "function" }),
        serde_json::json!({ "type": "mcp", "server_label": "srv", "name": null }),
        serde_json::json!({ "type": "file_search" }),
        serde_json::json!({ "type": "web_search_preview" }),
        serde_json::json!({ "type": "image_generation" }),
        serde_json::json!({ "type": "computer_use_preview" }),
        serde_json::json!({ "type": "local_shell" }),
        serde_json::json!({ "type": "shell" }),
        serde_json::json!({ "type": "apply_patch" }),
        serde_json::json!({ "type": "custom" }),
        serde_json::json!({
            "type": "allowed_tools",
            "tools": [
                { "type": "function" }
            ],
            "mode": "auto"
        }),
        serde_json::json!("auto"),
        serde_json::json!("required"),
        serde_json::json!("none"),
    ];

    for choice in variants {
        let value = response_with_tool_choice(choice.clone());
        let errors = validate_response_resource(&value).err().unwrap_or_default();
        assert!(errors.is_empty(), "errors: {errors:?} for {choice}");
    }
}

#[test]
fn validate_response_resource_accepts_tool_variants() {
    let tools = vec![
        serde_json::json!({
            "type": "file_search",
            "vector_store_ids": ["vs_1"],
            "max_num_results": 1,
            "ranking_options": {
                "ranker": "auto",
                "score_threshold": 0.0
            },
            "filters": null
        }),
        serde_json::json!({
            "type": "function",
            "name": "echo",
            "description": null,
            "parameters": null,
            "strict": null
        }),
        serde_json::json!({
            "type": "web_search_preview",
            "user_location": null,
            "search_context_size": "medium"
        }),
        serde_json::json!({
            "type": "mcp",
            "server_label": "srv",
            "server_description": null,
            "server_url": null,
            "headers": null,
            "allowed_tools": null,
            "require_approval": "always"
        }),
        serde_json::json!({
            "type": "computer_use_preview",
            "environment": "browser",
            "display_width": 800,
            "display_height": 600
        }),
        serde_json::json!({
            "type": "image_generation",
            "model": null,
            "n": 1,
            "quality": null,
            "size": null,
            "output_format": null,
            "output_compression": 100,
            "moderation": null,
            "background": null
        }),
        serde_json::json!({ "type": "shell" }),
        serde_json::json!({
            "type": "custom",
            "name": "custom_tool",
            "description": null,
            "format": null
        }),
        serde_json::json!({ "type": "apply_patch" }),
    ];

    for tool in tools {
        let value = response_with_tools(vec![tool.clone()]);
        let errors = validate_response_resource(&value).err().unwrap_or_default();
        assert!(errors.is_empty(), "errors: {errors:?} for {tool}");
    }
}

#[test]
fn validate_response_resource_accepts_mcp_list_tools_output() {
    let item = serde_json::json!({
        "type": "mcp_list_tools",
        "id": "list_1",
        "server_label": "srv",
        "tools": [
            {
                "name": "tool_a",
                "description": null,
                "input_schema": {},
                "annotations": null
            }
        ]
    });
    let value = response_with_output(vec![item]);
    let errors = validate_response_resource(&value).err().unwrap_or_default();
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn validate_response_resource_accepts_mcp_approval_items() {
    let request = serde_json::json!({
        "type": "mcp_approval_request",
        "id": "req_1",
        "server_label": "srv",
        "name": "tool",
        "arguments": "{}"
    });
    let response = serde_json::json!({
        "type": "mcp_approval_response",
        "id": "resp_1",
        "approval_request_id": "req_1",
        "approve": true,
        "reason": null
    });
    for item in [request, response] {
        let value = response_with_output(vec![item.clone()]);
        let errors = validate_response_resource(&value).err().unwrap_or_default();
        assert!(errors.is_empty(), "errors: {errors:?} for {item}");
    }
}

#[test]
fn validate_response_resource_accepts_mcp_tool_calls() {
    let base = serde_json::json!({
        "type": "mcp_call",
        "id": "call_1",
        "status": "completed",
        "approval_request_id": null,
        "server_label": "srv",
        "name": "tool",
        "arguments": "{}",
        "output": null,
        "error": null
    });
    let value = response_with_output(vec![base]);
    let errors = validate_response_resource(&value).err().unwrap_or_default();
    assert!(errors.is_empty(), "errors: {errors:?}");

    let error_variants = vec![
        serde_json::json!({
            "type": "mcp_protocol_error",
            "code": 1,
            "message": "oops"
        }),
        serde_json::json!({
            "type": "mcp_tool_execution_error",
            "content": { "detail": "failed" }
        }),
        serde_json::json!({
            "type": "http_error",
            "code": 500,
            "message": "server"
        }),
    ];

    for error in error_variants {
        let item = serde_json::json!({
            "type": "mcp_call",
            "id": "call_2",
            "status": "failed",
            "approval_request_id": null,
            "server_label": "srv",
            "name": "tool",
            "arguments": "{}",
            "output": null,
            "error": error
        });
        let value = response_with_output(vec![item.clone()]);
        let errors = validate_response_resource(&value).err().unwrap_or_default();
        assert!(errors.is_empty(), "errors: {errors:?} for {item}");
    }
}

#[test]
fn validate_response_resource_accepts_shell_items() {
    let local_call = serde_json::json!({
        "type": "local_shell_call",
        "id": "ls_1",
        "call_id": "call_1",
        "action": {
            "type": "exec",
            "command": ["echo", "hi"],
            "env": {}
        },
        "status": "in_progress"
    });
    let local_output = serde_json::json!({
        "type": "local_shell_call_output",
        "id": "ls_out_1",
        "call_id": "call_1",
        "output": "{\"stdout\":\"hi\"}",
        "status": "completed"
    });
    let shell_call = serde_json::json!({
        "type": "shell_call",
        "id": "sh_1",
        "call_id": "call_2",
        "action": {
            "commands": ["ls"],
            "timeout_ms": null,
            "max_output_length": null
        },
        "status": "completed"
    });
    let shell_output = serde_json::json!({
        "type": "shell_call_output",
        "id": "sh_out_1",
        "call_id": "call_2",
        "output": [
            {
                "stdout": "",
                "stderr": "",
                "outcome": {
                    "type": "exit",
                    "exit_code": 0
                }
            }
        ],
        "max_output_length": null
    });

    for item in [local_call, local_output, shell_call, shell_output] {
        let value = response_with_output(vec![item.clone()]);
        let errors = validate_response_resource(&value).err().unwrap_or_default();
        assert!(errors.is_empty(), "errors: {errors:?} for {item}");
    }
}

#[test]
fn validate_response_resource_accepts_search_and_tool_calls() {
    let file_search = serde_json::json!({
        "type": "file_search_call",
        "id": "fs_1",
        "status": "completed",
        "queries": ["query"],
        "results": [
            {
                "file_id": "file_1",
                "filename": "notes.txt",
                "text": "hello",
                "attributes": {},
                "score": 0.1,
                "vector_store_id": null
            }
        ]
    });
    let web_search = serde_json::json!({
        "type": "web_search_call",
        "id": "ws_1",
        "status": "completed",
        "action": {
            "type": "search",
            "query": null,
            "queries": ["query"]
        }
    });
    let image_gen = serde_json::json!({
        "type": "image_generation_call",
        "id": "ig_1",
        "status": "completed"
    });
    let computer_call = serde_json::json!({
        "type": "computer_call",
        "id": "cc_1",
        "call_id": "call_1",
        "pending_safety_checks": []
    });
    let computer_output = serde_json::json!({
        "type": "computer_call_output",
        "id": "cc_out_1",
        "call_id": "call_1",
        "output": { "type": "input_text", "text": "ok" },
        "status": "completed",
        "current_url": null
    });
    let apply_patch_call = serde_json::json!({
        "type": "apply_patch_call",
        "id": "ap_1",
        "call_id": "call_2",
        "status": "completed",
        "operation": {
            "type": "create_file",
            "path": "notes.txt",
            "diff": "@@ -0,0 +1 @@\\n+hello\\n"
        }
    });
    let apply_patch_output = serde_json::json!({
        "type": "apply_patch_call_output",
        "id": "ap_out_1",
        "call_id": "call_2",
        "status": "completed",
        "output": null
    });

    for item in [
        file_search,
        web_search,
        image_gen,
        computer_call,
        computer_output,
        apply_patch_call,
        apply_patch_output,
    ] {
        let value = response_with_output(vec![item.clone()]);
        let errors = validate_response_resource(&value).err().unwrap_or_default();
        assert!(errors.is_empty(), "errors: {errors:?} for {item}");
    }
}

#[test]
fn validate_response_resource_accepts_code_interpreter_call() {
    let item = serde_json::json!({
        "type": "code_interpreter_call",
        "id": "ci_1",
        "status": "completed",
        "container_id": "cntr_1",
        "code": null,
        "outputs": [
            {
                "type": "logs",
                "logs": "ok"
            }
        ]
    });
    let value = response_with_output(vec![item.clone()]);
    let errors = validate_response_resource(&value).err().unwrap_or_default();
    assert!(errors.is_empty(), "errors: {errors:?} for {item}");
}
