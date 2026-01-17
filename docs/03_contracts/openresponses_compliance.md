# OpenResponses Compliance Map

Summary
- Source repo: `temp/openresponses` (local vendor).
- Reviewed: 2026-01-17.
- Schema files: 412 split components in `temp/openresponses/schema/components/schemas` (+3 additive patch schemas: `InputVideoContent`, `JsonSchemaResponseFormatParam`, `TextFormatParam`).
- Streaming event schemas: 58 (from `temp/openresponses/schema/paths/responses.json`).
- Input item variants: 25 (from `ItemParam.json`).
- Output item variants: 23 (from `ItemField.json`).
- Inventory artifacts: `temp/docs/openresponses/schema_inventory.json` (full schema + variant counts) and `temp/docs/openresponses/event_types.json` (SSE event type map).

Sources reviewed
- `temp/openresponses/README.md`
- `temp/openresponses/src/pages/index.mdx`
- `temp/openresponses/src/pages/specification.mdx`
- `temp/openresponses/src/pages/reference.mdx`
- `temp/openresponses/src/pages/compliance.mdx`
- `temp/openresponses/src/pages/changelog.mdx`
- `temp/openresponses/src/pages/governance.mdx`
- `temp/openresponses/schema/openapi.json`
- `temp/openresponses/schema/openapi_additive_patches.yaml`
- `temp/openresponses/schema/openapi_filter_manifest.yaml`
- `temp/openresponses/schema/paths/responses.json`
- `temp/openresponses/schema/components/schemas/*.json`
- `temp/docs/openresponses/schema_inventory.json` (generated)
- `temp/docs/openresponses/event_types.json` (generated)

Mapping rules (Phase 1)
- All OpenResponses SSE events map to `provider_event` frames with full payload fidelity.
- Internal frames are emitted for a subset (session + text/tool deltas); all other events remain provider-only until explicitly promoted.
- No OpenResponses fields/events are dropped at the provider boundary.

Implementation status (current)
- Provider adapter validates streaming events and embedded `response` objects against the bundled OpenAPI schema (`schemas/openresponses/openapi.json`).
- Bundled OpenAPI validates 24/58 streaming event schemas and 4/23 output item variants; remaining streaming events and output items are pending validation until the split schemas are integrated.
- Provider adapter emits `output_text_delta` frames for `response.output_text.delta` events alongside `provider_event` frames.
- Provider request builder validates CreateResponseBody payloads (errors captured; payload preserved); tool fields use per-variant validation to avoid jsonschema oneOf failures; request sending is not wired yet.
- Tool schema validation uses split component schemas for `ResponsesToolParam` and `ToolChoiceParam`, validating optional fields and nested structures; bundled OpenAPI still only includes function tool variants.
- Split component schemas are vendored in `schemas/openresponses/split_components.json` for tool validation.
- Input item variants are not yet mapped to internal request frames (pending).
- ItemParam validation covers all input variants using manual required-field checks (message role/item reference handling included); internal request-frame mapping is still pending.
- Split schema inventory and SSE event type map captured in `temp/docs/openresponses/` and reflected in the tables below.
- Requests are JSON-only per spec; form-encoded bodies are not supported (ADR-0002).
- Bundled OpenAPI schema currently includes 102 component schemas; the split OpenResponses schema defines 412 component schemas. Missing schemas are tracked in the checklist.
- Split schemas + additive patches are authoritative; the filter manifest represents a reduced allowlist and is not a compliance target.

Doc review notes (normative requirements)

Specification (`temp/openresponses/src/pages/specification.mdx`)
- Requests MUST use HTTP with `Authorization` + `Content-Type` headers; request bodies MUST be `application/json`.
- Non-stream responses MUST be `application/json`; streams MUST be `text/event-stream` with terminal `[DONE]`.
- SSE `event` field MUST match payload `type`; servers SHOULD NOT use `id`.
- Items are state machines with `in_progress`, `incomplete`, `completed`; `incomplete` is terminal and MUST be the last item, and response MUST be `incomplete`.
- Every item MUST include `id`, `type`, `status`; extension types MUST be prefixed; clients SHOULD tolerate unknown item/status values.
- First item event MUST be `response.output_item.added` with all non-nullable fields present (use zero values).
- Streamable content MUST emit `response.content_part.added` -> repeated `response.<content>.delta` -> `response.<content>.done` -> `response.content_part.done`.
- Items MAY emit multiple content parts; items close with `response.output_item.done`.
- Extended item types MUST be prefixed with implementor slug; clients SHOULD tolerate unknown items.
- Streaming events MUST be either delta or state-machine events.
- `previous_response_id`: server MUST load prior input + output and preserve order `previous_response.input` -> `previous_response.output` -> new `input` (truncation allowed per policy).
- `tool_choice`: `auto`/`required`/`none` controls tool use; structured choice can force a tool.
- `allowed_tools`: server MUST enforce; tool calls outside allowed list MUST be rejected/suppressed.
- `truncation`: `disabled` MUST NOT truncate and MUST error on overflow; `auto` MAY truncate and SHOULD preserve system + recent context.
- Error types include `server_error`, `invalid_request`, `not_found`, `model_error`, `too_many_requests`.
- Extended streaming events MUST be prefixed with implementor slug and include `type` + `sequence_number`; clients MUST ignore unknown events safely.
- Schema extensions MUST NOT change core semantics; extensions SHOULD be optional and documented.
- Service tiers MAY exist; implementations SHOULD document supported tiers, behaviors, and quotas.
- Reasoning items MAY expose raw `content`, `encrypted_content`, and/or `summary`; clients SHOULD treat `encrypted_content` as opaque.

Reference (`temp/openresponses/src/pages/reference.mdx`)
- `/v1/responses` request bodies are documented as JSON or form-encoded; response bodies as JSON or SSE.

Compliance (`temp/openresponses/src/pages/compliance.mdx`)
- Acceptance tests validate API responses against the OpenAPI schema.

README/index/changelog/governance
- High-level positioning and project governance; no additional protocol requirements.

Doc discrepancies (resolved)
- Spec says request bodies MUST be `application/json`, while reference allows `application/x-www-form-urlencoded`. Decision: enforce JSON-only (ADR-0002).

## Additive patch schemas
| schema | purpose | mapping |
| --- | --- | --- |
| `InputVideoContent` | Adds `input_video` content blocks to message content unions. | pending |
| `JsonSchemaResponseFormatParam` | Adds JSON Schema response format support. | pending |
| `TextFormatParam` | Adds `json_schema` format option to text formats. | pending |

## CreateResponseBody fields
| field | required |
| --- | --- |
| `background` | no |
| `conversation` | no |
| `frequency_penalty` | no |
| `include` | no |
| `input` | no |
| `instructions` | no |
| `max_output_tokens` | no |
| `max_tool_calls` | no |
| `metadata` | no |
| `model` | no |
| `parallel_tool_calls` | no |
| `presence_penalty` | no |
| `previous_response_id` | no |
| `prompt_cache_key` | no |
| `prompt_cache_retention` | no |
| `reasoning` | no |
| `safety_identifier` | no |
| `service_tier` | no |
| `store` | no |
| `stream` | no |
| `stream_options` | no |
| `temperature` | no |
| `text` | no |
| `tool_choice` | no |
| `tools` | no |
| `top_logprobs` | no |
| `top_p` | no |
| `truncation` | no |
| `user` | no |

## ResponseResource fields
| field | required |
| --- | --- |
| `background` | yes |
| `billing` | no |
| `completed_at` | yes |
| `context_edits` | no |
| `conversation` | no |
| `cost_token` | no |
| `created_at` | yes |
| `error` | yes |
| `frequency_penalty` | yes |
| `id` | yes |
| `incomplete_details` | yes |
| `input` | no |
| `instructions` | yes |
| `max_output_tokens` | yes |
| `max_tool_calls` | yes |
| `metadata` | yes |
| `model` | yes |
| `next_response_ids` | no |
| `object` | yes |
| `output` | yes |
| `parallel_tool_calls` | yes |
| `presence_penalty` | yes |
| `previous_response_id` | yes |
| `prompt_cache_key` | yes |
| `prompt_cache_retention` | no |
| `reasoning` | yes |
| `safety_identifier` | yes |
| `service_tier` | yes |
| `status` | yes |
| `store` | yes |
| `temperature` | yes |
| `text` | yes |
| `tool_choice` | yes |
| `tools` | yes |
| `top_logprobs` | yes |
| `top_p` | yes |
| `truncation` | yes |
| `usage` | yes |
| `user` | yes |

## Tool param variants (ResponsesToolParam)
| tool type | schema | request validation |
| --- | --- | --- |
| `function` | `FunctionToolParam.json` | implemented |
| `code_interpreter` | `CodeInterpreterToolParam.json` | implemented |
| `custom` | `CustomToolParam.json` | implemented |
| `web_search` | `WebSearchToolParam.json` | implemented |
| `web_search_2025_08_26` | `WebSearchToolParam_2025_08_14Param.json` | implemented |
| `web_search_ga` | `WebSearchGADeprecatedToolParam.json` | implemented |
| `web_search_preview` | `WebSearchPreviewToolParam.json` | implemented |
| `web_search_preview_2025_03_11` | `WebSearchPreviewToolParam_2025_03_11Param.json` | implemented |
| `image_generation` | `ImageGenToolParam.json` | implemented |
| `mcp` | `MCPToolParam.json` | implemented |
| `file_search` | `FileSearchToolParam.json` | implemented |
| `computer-preview` | `ComputerToolParam.json` | implemented |
| `computer_use_preview` | `ComputerUsePreviewToolParam.json` | implemented |
| `local_shell` | `LocalShellToolParam.json` | implemented |
| `shell` | `FunctionShellToolParam.json` | implemented |
| `apply_patch` | `ApplyPatchToolParam.json` | implemented |

### Tool param required fields
| schema | required fields | notes |
| --- | --- | --- |
| `CodeInterpreterToolParam.json` | `type`, `container` | `container` is string or `AutoCodeInterpreterToolParam` |
| `FunctionToolParam.json` | `type`, `name` |  |
| `CustomToolParam.json` | `type`, `name` |  |
| `WebSearchToolParam.json` | `type` |  |
| `WebSearchToolParam_2025_08_14Param.json` | `type` |  |
| `WebSearchGADeprecatedToolParam.json` | `type` |  |
| `WebSearchPreviewToolParam.json` | `type` |  |
| `WebSearchPreviewToolParam_2025_03_11Param.json` | `type` |  |
| `ImageGenToolParam.json` | `type` |  |
| `MCPToolParam.json` | `type`, `server_label` |  |
| `FileSearchToolParam.json` | `type`, `vector_store_ids` |  |
| `ComputerToolParam.json` | `type`, `display_width`, `display_height`, `environment` | `environment` is `ComputerEnvironment` |
| `ComputerUsePreviewToolParam.json` | `type`, `display_width`, `display_height`, `environment` | `environment` is `ComputerEnvironment` |
| `LocalShellToolParam.json` | `type` |  |
| `FunctionShellToolParam.json` | `type` |  |
| `ApplyPatchToolParam.json` | `type` |  |

## Tool choice variants
| ToolChoiceParam variant | schema | status |
| --- | --- | --- |
| value enum | `ToolChoiceValueEnum.json` | implemented |
| allowed tools | `AllowedToolsParam.json` | implemented |
| specific tool | `SpecificToolChoiceParam.json` | implemented |

### Specific tool choices (SpecificToolChoiceParam)
| tool type | schema | required fields | status |
| --- | --- | --- | --- |
| `file_search` | `SpecificFileSearchParam.json` | `type` | implemented |
| `web_search` | `SpecificWebSearchParam.json` | `type` | implemented |
| `web_search_preview` | `SpecificWebSearchPreviewParam.json` | `type` | implemented |
| `image_generation` | `SpecificImageGenParam.json` | `type` | implemented |
| `computer-preview` | `SpecificComputerParam.json` | `type` | implemented |
| `computer_use_preview` | `SpecificComputerPreviewParam.json` | `type` | implemented |
| `code_interpreter` | `SpecificCodeInterpreterParam.json` | `type` | implemented |
| `function` | `SpecificFunctionParam.json` | `type`, `name` | implemented |
| `mcp` | `SpecificMCPFunctionParam.json` | `type`, `server_label` | implemented |
| `local_shell` | `SpecificLocalShellParam.json` | `type` | implemented |
| `shell` | `SpecificFunctionShellParam.json` | `type` | implemented |
| `custom` | `SpecificCustomToolParam.json` | `type`, `name` | implemented |
| `apply_patch` | `SpecificApplyPatchParam.json` | `type` | implemented |

## Error schemas
| schema | required fields | notes |
| --- | --- | --- |
| `Error.json` | `code`, `message` | base error payload (no `type` field) |
| `ErrorPayload.json` | `type`, `code`, `message`, `param` | `type` is freeform string |
| `HTTPError.json` | `type`, `code`, `message` | `type` enum: `http_error` |
| `MCPProtocolError.json` | `type`, `code`, `message` | `type` enum: `mcp_protocol_error` |
| `MCPToolExecutionError.json` | `type`, `content` | `type` enum: `mcp_tool_execution_error` |

## Streaming events (SSE)
| event type | schema | mapping |
| --- | --- | --- |
| `error` | `ErrorStreamingEvent.json` | provider_event (validated) |
| `image_edit.completed` | `ImageEditCompletedStreamingEvent.json` | provider_event (pending validation) |
| `image_edit.partial_image` | `ImageEditPartialImageStreamingEvent.json` | provider_event (pending validation) |
| `image_generation.completed` | `ImageGenerationCompletedStreamingEvent.json` | provider_event (pending validation) |
| `image_generation.partial_image` | `ImageGenerationPartialImageStreamingEvent.json` | provider_event (pending validation) |
| `response.apply_patch_call_operation_diff.delta` | `ResponseApplyPatchCallOperationDiffDeltaStreamingEvent.json` | provider_event (pending validation) |
| `response.apply_patch_call_operation_diff.done` | `ResponseApplyPatchCallOperationDiffDoneStreamingEvent.json` | provider_event (pending validation) |
| `response.code_interpreter_call.completed` | `ResponseCodeInterpreterCallCompletedStreamingEvent.json` | provider_event (pending validation) |
| `response.code_interpreter_call.in_progress` | `ResponseCodeInterpreterCallInProgressStreamingEvent.json` | provider_event (pending validation) |
| `response.code_interpreter_call.interpreting` | `ResponseCodeInterpreterCallInterpretingStreamingEvent.json` | provider_event (pending validation) |
| `response.code_interpreter_call_code.delta` | `ResponseCodeInterpreterCallCodeDeltaStreamingEvent.json` | provider_event (pending validation) |
| `response.code_interpreter_call_code.done` | `ResponseCodeInterpreterCallCodeDoneStreamingEvent.json` | provider_event (pending validation) |
| `response.completed` | `ResponseCompletedStreamingEvent.json` | provider_event (validated) |
| `response.content_part.added` | `ResponseContentPartAddedStreamingEvent.json` | provider_event (validated) |
| `response.content_part.done` | `ResponseContentPartDoneStreamingEvent.json` | provider_event (validated) |
| `response.created` | `ResponseCreatedStreamingEvent.json` | provider_event (validated) |
| `response.custom_tool_call_input.delta` | `ResponseCustomToolCallInputDeltaStreamingEvent.json` | provider_event (pending validation) |
| `response.custom_tool_call_input.done` | `ResponseCustomToolCallInputDoneStreamingEvent.json` | provider_event (pending validation) |
| `response.failed` | `ResponseFailedStreamingEvent.json` | provider_event (validated) |
| `response.file_search_call.completed` | `ResponseFileSearchCallCompletedStreamingEvent.json` | provider_event (pending validation) |
| `response.file_search_call.in_progress` | `ResponseFileSearchCallInProgressStreamingEvent.json` | provider_event (pending validation) |
| `response.file_search_call.searching` | `ResponseFileSearchCallSearchingStreamingEvent.json` | provider_event (pending validation) |
| `response.function_call_arguments.delta` | `ResponseFunctionCallArgumentsDeltaStreamingEvent.json` | provider_event (validated) |
| `response.function_call_arguments.done` | `ResponseFunctionCallArgumentsDoneStreamingEvent.json` | provider_event (validated) |
| `response.image_generation_call.completed` | `ResponseImageGenCallCompletedStreamingEvent.json` | provider_event (pending validation) |
| `response.image_generation_call.generating` | `ResponseImageGenCallGeneratingStreamingEvent.json` | provider_event (pending validation) |
| `response.image_generation_call.in_progress` | `ResponseImageGenCallInProgressStreamingEvent.json` | provider_event (pending validation) |
| `response.image_generation_call.partial_image` | `ResponseImageGenCallPartialImageStreamingEvent.json` | provider_event (pending validation) |
| `response.in_progress` | `ResponseInProgressStreamingEvent.json` | provider_event (validated) |
| `response.incomplete` | `ResponseIncompleteStreamingEvent.json` | provider_event (validated) |
| `response.mcp_call.completed` | `ResponseMCPCallCompletedStreamingEvent.json` | provider_event (pending validation) |
| `response.mcp_call.failed` | `ResponseMCPCallFailedStreamingEvent.json` | provider_event (pending validation) |
| `response.mcp_call.in_progress` | `ResponseMCPCallInProgressStreamingEvent.json` | provider_event (pending validation) |
| `response.mcp_call_arguments.delta` | `ResponseMCPCallArgumentsDeltaStreamingEvent.json` | provider_event (pending validation) |
| `response.mcp_call_arguments.done` | `ResponseMCPCallArgumentsDoneStreamingEvent.json` | provider_event (pending validation) |
| `response.mcp_list_tools.completed` | `ResponseMCPListToolsCompletedStreamingEvent.json` | provider_event (pending validation) |
| `response.mcp_list_tools.failed` | `ResponseMCPListToolsFailedStreamingEvent.json` | provider_event (pending validation) |
| `response.mcp_list_tools.in_progress` | `ResponseMCPListToolsInProgressStreamingEvent.json` | provider_event (pending validation) |
| `response.output_item.added` | `ResponseOutputItemAddedStreamingEvent.json` | provider_event (validated) |
| `response.output_item.done` | `ResponseOutputItemDoneStreamingEvent.json` | provider_event (validated) |
| `response.output_text.annotation.added` | `ResponseOutputTextAnnotationAddedStreamingEvent.json` | provider_event (validated) |
| `response.output_text.delta` | `ResponseOutputTextDeltaStreamingEvent.json` | provider_event (validated) |
| `response.output_text.done` | `ResponseOutputTextDoneStreamingEvent.json` | provider_event (validated) |
| `response.queued` | `ResponseQueuedStreamingEvent.json` | provider_event (validated) |
| `response.reasoning.delta` | `ResponseReasoningDeltaStreamingEvent.json` | provider_event (validated) |
| `response.reasoning.done` | `ResponseReasoningDoneStreamingEvent.json` | provider_event (validated) |
| `response.reasoning_summary_part.added` | `ResponseReasoningSummaryPartAddedStreamingEvent.json` | provider_event (validated) |
| `response.reasoning_summary_part.done` | `ResponseReasoningSummaryPartDoneStreamingEvent.json` | provider_event (validated) |
| `response.reasoning_summary_text.delta` | `ResponseReasoningSummaryDeltaStreamingEvent.json` | provider_event (validated) |
| `response.reasoning_summary_text.done` | `ResponseReasoningSummaryDoneStreamingEvent.json` | provider_event (validated) |
| `response.refusal.delta` | `ResponseRefusalDeltaStreamingEvent.json` | provider_event (validated) |
| `response.refusal.done` | `ResponseRefusalDoneStreamingEvent.json` | provider_event (validated) |
| `response.shell_call_command.added` | `ResponseShellCallCommandAddedStreamingEvent.json` | provider_event (pending validation) |
| `response.shell_call_command.delta` | `ResponseShellCallCommandDeltaStreamingEvent.json` | provider_event (pending validation) |
| `response.shell_call_command.done` | `ResponseShellCallCommandDoneStreamingEvent.json` | provider_event (pending validation) |
| `response.web_search_call.completed` | `ResponseWebSearchCallCompletedStreamingEvent.json` | provider_event (pending validation) |
| `response.web_search_call.in_progress` | `ResponseWebSearchCallInProgressStreamingEvent.json` | provider_event (pending validation) |
| `response.web_search_call.searching` | `ResponseWebSearchCallSearchingStreamingEvent.json` | provider_event (pending validation) |

## Input item variants
| item type | schema | mapping |
| --- | --- | --- |
| `apply_patch_call` | `ApplyPatchToolCallItemParam.json` | provider_request (validated; internal mapping pending) |
| `apply_patch_call_output` | `ApplyPatchToolCallOutputItemParam.json` | provider_request (validated; internal mapping pending) |
| `code_interpreter_call` | `CodeInterpreterCallItemParam.json` | provider_request (validated; internal mapping pending) |
| `compaction` | `CompactionSummaryItemParam.json` | provider_request (validated; internal mapping pending) |
| `computer_call` | `ComputerCallItemParam.json` | provider_request (validated; internal mapping pending) |
| `computer_call_output` | `ComputerCallOutputItemParam.json` | provider_request (validated; internal mapping pending) |
| `custom_tool_call` | `CustomToolCallItemParam.json` | provider_request (validated; internal mapping pending) |
| `custom_tool_call_output` | `CustomToolCallOutputItemParam.json` | provider_request (validated; internal mapping pending) |
| `file_search_call` | `FileSearchCallItemParam.json` | provider_request (validated; internal mapping pending) |
| `function_call` | `FunctionCallItemParam.json` | provider_request (validated; internal mapping pending) |
| `function_call_output` | `FunctionCallOutputItemParam.json` | provider_request (validated; internal mapping pending) |
| `image_generation_call` | `ImageGenCallItemParam.json` | provider_request (validated; internal mapping pending) |
| `local_shell_call` | `LocalShellCallItemParam.json` | provider_request (validated; internal mapping pending) |
| `local_shell_call_output` | `LocalShellCallOutputItemParam.json` | provider_request (validated; internal mapping pending) |
| `mcp_approval_request` | `MCPApprovalRequestItemParam.json` | provider_request (validated; internal mapping pending) |
| `mcp_approval_response` | `MCPApprovalResponseItemParam.json` | provider_request (validated; internal mapping pending) |
| `message` | `AssistantMessageItemParam.json` | provider_request (validated; internal mapping pending) |
| `message` | `DeveloperMessageItemParam.json` | provider_request (validated; internal mapping pending) |
| `message` | `SystemMessageItemParam.json` | provider_request (validated; internal mapping pending) |
| `message` | `UserMessageItemParam.json` | provider_request (validated; internal mapping pending) |
| `reasoning` | `ReasoningItemParam.json` | provider_request (validated; internal mapping pending) |
| `shell_call` | `FunctionShellCallItemParam.json` | provider_request (validated; internal mapping pending) |
| `shell_call_output` | `FunctionShellCallOutputItemParam.json` | provider_request (validated; internal mapping pending) |
| `item_reference` | `ItemReferenceParam.json` | provider_request (validated; internal mapping pending) |
| `web_search_call` | `WebSearchCallItemParam.json` | provider_request (validated; internal mapping pending) |

## Output item variants
| item type | schema | mapping |
| --- | --- | --- |
| `apply_patch_call` | `ApplyPatchToolCall.json` | provider_event (pending validation) |
| `apply_patch_call_output` | `ApplyPatchToolCallOutput.json` | provider_event (pending validation) |
| `code_interpreter_call` | `CodeInterpreterCall.json` | provider_event (pending validation) |
| `compaction` | `CompactionBody.json` | provider_event (pending validation) |
| `computer_call` | `ComputerCall.json` | provider_event (pending validation) |
| `computer_call_output` | `ComputerCallOutput.json` | provider_event (pending validation) |
| `custom_tool_call` | `CustomToolCall.json` | provider_event (pending validation) |
| `custom_tool_call_output` | `CustomToolCallOutput.json` | provider_event (pending validation) |
| `file_search_call` | `FileSearchCall.json` | provider_event (pending validation) |
| `function_call` | `FunctionCall.json` | provider_event (validated) |
| `function_call_output` | `FunctionCallOutput.json` | provider_event (validated) |
| `image_generation_call` | `ImageGenCall.json` | provider_event (pending validation) |
| `local_shell_call` | `LocalShellCall.json` | provider_event (pending validation) |
| `local_shell_call_output` | `LocalShellCallOutput.json` | provider_event (pending validation) |
| `mcp_approval_request` | `MCPApprovalRequest.json` | provider_event (pending validation) |
| `mcp_approval_response` | `MCPApprovalResponse.json` | provider_event (pending validation) |
| `mcp_call` | `MCPToolCall.json` | provider_event (pending validation) |
| `mcp_list_tools` | `MCPListTools.json` | provider_event (pending validation) |
| `message` | `Message.json` | provider_event (validated) |
| `reasoning` | `ReasoningBody.json` | provider_event (validated) |
| `shell_call` | `FunctionShellCall.json` | provider_event (pending validation) |
| `shell_call_output` | `FunctionShellCallOutput.json` | provider_event (pending validation) |
| `web_search_call` | `WebSearchCall.json` | provider_event (pending validation) |

## Schema index (all components)

This list is exhaustive and drives the task tracker in `docs/07_tasks/openresponses_compliance.md`.

Legend
- `bundled`: schema is present in `schemas/openresponses/openapi.json` (used for current validation).
- `validated`: schema is reachable from streaming-event or ResponseResource validation in the bundled OpenAPI (request validation is noted in `status`).
- `status`: mapping status in current codebase.

### Error schemas
| schema | bundled | validated | status |
| --- | --- | --- | --- |
| `Error` | yes | yes | provider_event |
| `ErrorPayload` | yes | yes | provider_event |
| `HTTPError` | no | no | pending |
| `MCPProtocolError` | no | no | pending |
| `MCPToolExecutionError` | no | no | pending |

### Input item params
| schema | bundled | validated | status |
| --- | --- | --- | --- |
| `ApplyPatchToolCallItemParam` | no | no | provider_request (validated) | 
| `ApplyPatchToolCallOutputItemParam` | no | no | provider_request (validated) | 
| `AssistantMessageItemParam` | yes | no | provider_request (validated) | 
| `CodeInterpreterCallItemParam` | no | no | provider_request (validated) | 
| `CompactionSummaryItemParam` | no | no | provider_request (validated) | 
| `ComputerCallItemParam` | no | no | provider_request (validated) | 
| `ComputerCallOutputItemParam` | no | no | provider_request (validated) | 
| `CustomToolCallItemParam` | no | no | provider_request (validated) | 
| `CustomToolCallOutputItemParam` | no | no | provider_request (validated) | 
| `DeveloperMessageItemParam` | yes | no | provider_request (validated) | 
| `FileSearchCallItemParam` | no | no | provider_request (validated) | 
| `FunctionCallItemParam` | yes | no | provider_request (validated) | 
| `FunctionCallOutputItemParam` | yes | no | provider_request (validated) | 
| `FunctionShellCallItemParam` | no | no | provider_request (validated) | 
| `FunctionShellCallOutputItemParam` | no | no | provider_request (validated) | 
| `ImageGenCallItemParam` | no | no | provider_request (validated) | 
| `ItemParam` | yes | no | provider_request (validated) | 
| `LocalShellCallItemParam` | no | no | provider_request (validated) | 
| `LocalShellCallOutputItemParam` | no | no | provider_request (validated) | 
| `MCPApprovalRequestItemParam` | no | no | provider_request (validated) | 
| `MCPApprovalResponseItemParam` | no | no | provider_request (validated) | 
| `ReasoningItemParam` | yes | no | provider_request (validated) | 
| `SystemMessageItemParam` | yes | no | provider_request (validated) | 
| `UserMessageItemParam` | yes | no | provider_request (validated) | 
| `WebSearchCallItemParam` | no | no | provider_request (validated) | 

### Other schemas
| schema | bundled | validated | status |
| --- | --- | --- | --- |
| `AllowedToolsParam` | yes | no | provider_request (validated) |
| `Annotation` | yes | yes | pending |
| `ApiSourceParam` | no | no | pending |
| `ApplyPatchCallOutputStatus` | no | no | pending |
| `ApplyPatchCallOutputStatusParam` | no | no | pending |
| `ApplyPatchCallStatus` | no | no | pending |
| `ApplyPatchCallStatusParam` | no | no | pending |
| `ApplyPatchCreateFileOperation` | no | no | pending |
| `ApplyPatchCreateFileOperationParam` | no | no | pending |
| `ApplyPatchDeleteFileOperation` | no | no | pending |
| `ApplyPatchDeleteFileOperationParam` | no | no | pending |
| `ApplyPatchOperationParam` | no | no | pending |
| `ApplyPatchToolCall` | no | no | provider_event (pending validation) |
| `ApplyPatchToolCallOutput` | no | no | provider_event (pending validation) |
| `ApplyPatchUpdateFileOperation` | no | no | pending |
| `ApplyPatchUpdateFileOperationParam` | no | no | pending |
| `ApproximateLocation` | no | no | pending |
| `ApproximateLocationParam` | no | no | pending |
| `Billing` | no | no | pending |
| `ClickAction` | no | no | pending |
| `ClickButtonType` | no | no | pending |
| `ClickParam` | no | no | pending |
| `CodeInterpreterCall` | no | no | provider_event (pending validation) |
| `CodeInterpreterCallStatus` | no | no | pending |
| `CodeInterpreterOutputImage` | no | no | pending |
| `CodeInterpreterOutputLogs` | no | no | pending |
| `CodeInterpreterToolCallOutputImageParam` | no | no | pending |
| `CodeInterpreterToolCallOutputLogsParam` | no | no | pending |
| `CompactResource` | no | no | pending |
| `CompactResponseMethodPublicBody` | no | no | pending |
| `CompactionBody` | no | no | provider_event (pending validation) |
| `ComparisonFilterFieldCONTAINS` | no | no | pending |
| `ComparisonFilterFieldCONTAINSANY` | no | no | pending |
| `ComparisonFilterFieldEQ` | no | no | pending |
| `ComparisonFilterFieldGT` | no | no | pending |
| `ComparisonFilterFieldGTE` | no | no | pending |
| `ComparisonFilterFieldIN` | no | no | pending |
| `ComparisonFilterFieldLT` | no | no | pending |
| `ComparisonFilterFieldLTE` | no | no | pending |
| `ComparisonFilterFieldNCONTAINS` | no | no | pending |
| `ComparisonFilterFieldNCONTAINSANY` | no | no | pending |
| `ComparisonFilterFieldNE` | no | no | pending |
| `ComparisonFilterFieldNIN` | no | no | pending |
| `ComparisonFilterParamContainsAnyParam` | no | no | pending |
| `ComparisonFilterParamContainsParam` | no | no | pending |
| `ComparisonFilterParamEQParam` | no | no | pending |
| `ComparisonFilterParamGTEParam` | no | no | pending |
| `ComparisonFilterParamGTParam` | no | no | pending |
| `ComparisonFilterParamINParam` | no | no | pending |
| `ComparisonFilterParamLTEParam` | no | no | pending |
| `ComparisonFilterParamLTParam` | no | no | pending |
| `ComparisonFilterParamNContainsAnyParam` | no | no | pending |
| `ComparisonFilterParamNContainsParam` | no | no | pending |
| `ComparisonFilterParamNEParam` | no | no | pending |
| `ComparisonFilterParamNINParam` | no | no | pending |
| `CompoundFilterFieldAND` | no | no | pending |
| `CompoundFilterFieldOR` | no | no | pending |
| `CompoundFilterParamAndParam` | no | no | pending |
| `CompoundFilterParamOrParam` | no | no | pending |
| `ComputerCall` | no | no | provider_event (pending validation) |
| `ComputerCallOutput` | no | no | provider_event (pending validation) |
| `ComputerCallOutputStatus` | no | no | pending |
| `ComputerCallSafetyCheckParam` | no | no | pending |
| `ComputerEnvironment` | no | no | pending |
| `ComputerEnvironment1` | no | no | pending |
| `ComputerScreenshotContent` | no | no | pending |
| `ComputerScreenshotParam` | no | no | pending |
| `ContainerFileCitationBody` | no | no | pending |
| `ContainerFileCitationParam` | no | no | pending |
| `ContainerMemoryLimit` | no | no | pending |
| `ContextEdit` | no | no | pending |
| `ContextEditDetails` | no | no | pending |
| `Conversation` | no | no | pending |
| `ConversationParam` | no | no | pending |
| `CoordParam` | no | no | pending |
| `CreateImageBody15Param` | no | no | pending |
| `CreateImageBody1MiniParam` | no | no | pending |
| `CreateImageBody1Param` | no | no | pending |
| `CreateImageBodyChatGPTImageLatestParam` | no | no | pending |
| `CreateVideoBody` | no | no | pending |
| `CreateVideoRemixBody` | no | no | pending |
| `CustomGrammarFormatField` | no | no | pending |
| `CustomGrammarFormatParam` | no | no | pending |
| `CustomTextFormatField` | no | no | pending |
| `CustomTextFormatParam` | no | no | pending |
| `CustomToolCall` | no | no | provider_event (pending validation) |
| `CustomToolCallOutput` | no | no | provider_event (pending validation) |
| `CustomToolFormat` | no | no | pending |
| `DeletedResponseResource` | no | no | pending |
| `DeletedVideoResource` | no | no | pending |
| `DetailEnum` | yes | no | pending |
| `DoubleClickAction` | no | no | pending |
| `DoubleClickParam` | no | no | pending |
| `DragAction` | no | no | pending |
| `DragParam` | no | no | pending |
| `DragPoint` | no | no | pending |
| `EditImageBody15Param` | no | no | pending |
| `EditImageBody1MiniParam` | no | no | pending |
| `EditImageBody1Param` | no | no | pending |
| `EditImageBodyChatGPTImageLatestParam` | no | no | pending |
| `EditsBodyDallE2Param` | no | no | pending |
| `EmptyAction` | no | no | pending |
| `EmptyModelParam` | yes | no | pending |
| `ExcludeEnum` | no | no | pending |
| `FileCitationBody` | no | no | pending |
| `FileCitationParam` | no | no | pending |
| `FileSearchCall` | no | no | provider_event (pending validation) |
| `FileSearchRankingOptionsParam` | no | no | pending |
| `FileSearchResult` | no | no | pending |
| `FileSearchRetrievedChunksParam` | no | no | pending |
| `FileSearchToolCallStatusEnum` | no | no | pending |
| `Filters` | no | no | pending |
| `FunctionCall` | yes | yes | provider_event (validated) |
| `FunctionCallItemStatus` | yes | no | pending |
| `FunctionCallOutput` | yes | yes | provider_event (validated) |
| `FunctionCallOutputStatusEnum` | yes | yes | pending |
| `FunctionCallStatus` | yes | yes | pending |
| `FunctionShellAction` | no | no | pending |
| `FunctionShellActionParam` | no | no | pending |
| `FunctionShellCall` | no | no | provider_event (pending validation) |
| `FunctionShellCallItemStatus` | no | no | pending |
| `FunctionShellCallOutput` | no | no | provider_event (pending validation) |
| `FunctionShellCallOutputContent` | no | no | pending |
| `FunctionShellCallOutputContentParam` | no | no | pending |
| `FunctionShellCallOutputExitOutcome` | no | no | pending |
| `FunctionShellCallOutputExitOutcomeParam` | no | no | pending |
| `FunctionShellCallOutputOutcomeParam` | no | no | pending |
| `FunctionShellCallOutputTimeoutOutcome` | no | no | pending |
| `FunctionShellCallOutputTimeoutOutcomeParam` | no | no | pending |
| `GenerationsBodyDallE2Param` | no | no | pending |
| `GenerationsBodyDallE3Param` | no | no | pending |
| `GrammarSyntax` | no | no | pending |
| `GrammarSyntax1` | no | no | pending |
| `HybridSearchOptions` | no | no | pending |
| `HybridSearchOptionsParam` | no | no | pending |
| `Image` | no | no | pending |
| `ImageBackground` | no | no | pending |
| `ImageDetail` | yes | yes | pending |
| `ImageGenAction` | no | no | pending |
| `ImageGenActionEnum` | no | no | pending |
| `ImageGenCall` | no | no | provider_event (pending validation) |
| `ImageGenCallStatus` | no | no | pending |
| `ImageGenInputUsageDetails` | no | no | pending |
| `ImageGenOutputTokensDetails` | no | no | pending |
| `ImageGenToolModel` | no | no | pending |
| `ImageGenUsage` | no | no | pending |
| `ImageModeration` | no | no | pending |
| `ImageOutputFormat` | no | no | pending |
| `ImageQuality` | no | no | pending |
| `ImageQualityDallE` | no | no | pending |
| `ImageResource` | no | no | pending |
| `ImageSize` | no | no | pending |
| `ImageSizeDallE2` | no | no | pending |
| `ImageSizeDallE3` | no | no | pending |
| `ImageStyleDallE` | no | no | pending |
| `ImageUsage` | no | no | pending |
| `ImageUsageInputTokensDetails` | no | no | pending |
| `ImageUsageOutputTokensDetails` | no | no | pending |
| `IncludeEnum` | yes | no | pending |
| `IncompleteDetails` | yes | yes | pending |
| `InputFidelity` | no | no | pending |
| `InputFileContent` | yes | yes | pending |
| `InputFileContentParam` | yes | no | pending |
| `InputImageContent` | yes | yes | pending |
| `InputImageContentParamAutoParam` | yes | no | pending |
| `InputImageMaskContentParam` | no | no | pending |
| `InputTextContent` | yes | yes | pending |
| `InputTextContentParam` | yes | no | pending |
| `InputTokensDetails` | yes | yes | pending |
| `ItemListResource` | no | no | pending |
| `ItemReferenceParam` | yes | no | provider_request (validated) | 
| `JsonObjectResponseFormat` | yes | yes | pending |
| `JsonSchemaResponseFormat` | yes | yes | pending |
| `KeyPressAction` | no | no | pending |
| `KeyPressParam` | no | no | pending |
| `LocalFileEnvironmentParam` | no | no | pending |
| `LocalShellCall` | no | no | provider_event (pending validation) |
| `LocalShellCallItemStatus` | no | no | pending |
| `LocalShellCallOutput` | no | no | provider_event (pending validation) |
| `LocalShellCallOutputStatusEnum` | no | no | pending |
| `LocalShellCallStatus` | no | no | pending |
| `LocalShellExecAction` | no | no | pending |
| `LocalShellExecActionParam` | no | no | pending |
| `LogProb` | yes | yes | pending |
| `MCPApprovalRequest` | no | no | provider_event (pending validation) |
| `MCPApprovalResponse` | no | no | provider_event (pending validation) |
| `MCPListTools` | no | no | provider_event (pending validation) |
| `MCPRequireApprovalApiEnum` | no | no | pending |
| `MCPRequireApprovalFieldEnum` | no | no | pending |
| `MCPRequireApprovalFilterField` | no | no | pending |
| `MCPRequireApprovalFilterParam` | no | no | pending |
| `MCPToolCall` | no | no | provider_event (pending validation) |
| `MCPToolCallStatus` | no | no | pending |
| `MCPToolFilterField` | no | no | pending |
| `MCPToolFilterParam` | no | no | pending |
| `Message` | yes | yes | provider_event (validated) |
| `MessageRole` | yes | yes | pending |
| `MessageRole1` | no | no | pending |
| `MessageStatus` | yes | yes | pending |
| `MetadataParam` | yes | no | pending |
| `MoveAction` | no | no | pending |
| `MoveParam` | no | no | pending |
| `OrderEnum` | no | no | pending |
| `OutputTextContent` | yes | yes | pending |
| `OutputTextContentParam` | yes | no | pending |
| `OutputTokensDetails` | yes | yes | pending |
| `Payer` | no | no | pending |
| `PromptCacheRetentionEnum` | no | no | pending |
| `PromptInstructionMessage` | no | no | pending |
| `RankerVersionType` | no | no | pending |
| `RankingOptions` | no | no | pending |
| `Reasoning` | yes | yes | pending |
| `ReasoningBody` | yes | yes | provider_event (validated) |
| `ReasoningEffortEnum` | yes | yes | pending |
| `ReasoningParam` | yes | no | pending |
| `ReasoningSummaryContentParam` | yes | no | pending |
| `ReasoningSummaryEnum` | yes | yes | pending |
| `ReasoningTextContent` | yes | yes | pending |
| `RefusalContent` | yes | yes | pending |
| `RefusalContentParam` | yes | no | pending |
| `SafetyCheck` | no | no | pending |
| `ScreenshotAction` | no | no | pending |
| `ScreenshotParam` | no | no | pending |
| `ScrollAction` | no | no | pending |
| `ScrollParam` | no | no | pending |
| `SearchContextSize` | no | no | pending |
| `ServiceTierEnum` | yes | no | pending |
| `SpecificApplyPatchParam` | no | no | provider_request (validated) | 
| `SpecificCodeInterpreterParam` | no | no | provider_request (validated) | 
| `SpecificComputerParam` | no | no | provider_request (validated) | 
| `SpecificComputerPreviewParam` | no | no | provider_request (validated) | 
| `SpecificFileSearchParam` | no | no | provider_request (validated) | 
| `SpecificFunctionParam` | yes | no | pending |
| `SpecificFunctionShellParam` | no | no | provider_request (validated) | 
| `SpecificImageGenParam` | no | no | provider_request (validated) | 
| `SpecificLocalShellParam` | no | no | provider_request (validated) | 
| `SpecificMCPFunctionParam` | no | no | provider_request (validated) | 
| `SpecificToolChoiceParam` | yes | no | provider_request (validated) |
| `SpecificWebSearchParam` | no | no | provider_request (validated) | 
| `SpecificWebSearchPreviewParam` | no | no | provider_request (validated) | 
| `StreamOptionsParam` | yes | no | pending |
| `SummaryTextContent` | yes | yes | pending |
| `TextContent` | yes | yes | pending |
| `TextField` | yes | yes | pending |
| `TextParam` | yes | no | pending |
| `TextResponseFormat` | yes | yes | pending |
| `TokenCountsBody` | no | no | pending |
| `TokenCountsResource` | no | no | pending |
| `ToolChoiceParam` | yes | no | provider_request (validated) |
| `ToolChoiceValueEnum` | yes | yes | provider_request (validated) |
| `TopLogProb` | yes | yes | pending |
| `TruncationEnum` | yes | yes | pending |
| `TypeAction` | no | no | pending |
| `TypeParam` | no | no | pending |
| `UrlCitationBody` | yes | yes | pending |
| `UrlCitationParam` | yes | no | pending |
| `UrlSourceParam` | no | no | pending |
| `Usage` | yes | yes | pending |
| `VerbosityEnum` | yes | yes | pending |
| `VideoContentVariant` | no | no | pending |
| `VideoListResource` | no | no | pending |
| `VideoModel` | no | no | pending |
| `VideoResource` | no | no | pending |
| `VideoSeconds` | no | no | pending |
| `VideoSize` | no | no | pending |
| `VideoStatus` | no | no | pending |
| `WaitAction` | no | no | pending |
| `WaitParam` | no | no | pending |
| `WebSearchCall` | no | no | provider_event (pending validation) |
| `WebSearchCallActionFindInPage` | no | no | pending |
| `WebSearchCallActionFindInPageParam` | no | no | pending |
| `WebSearchCallActionOpenPage` | no | no | pending |
| `WebSearchCallActionOpenPageParam` | no | no | pending |
| `WebSearchCallActionSearch` | no | no | pending |
| `WebSearchCallActionSearchParam` | no | no | pending |
| `WebSearchCallStatus` | no | no | pending |
| `WebSearchPreviewToolParam_2025_03_11Param` | no | no | provider_request (validated) | 
| `WebSearchToolParam_2025_08_14Param` | no | no | provider_request (validated) | 

### Output item fields
| schema | bundled | validated | status |
| --- | --- | --- | --- |
| `ItemField` | yes | yes | provider_event |

### Request-related schemas
| schema | bundled | validated | status |
| --- | --- | --- | --- |
| `CreateResponseBody` | yes | no | provider_request (validated) |

### Response-related schemas
| schema | bundled | validated | status |
| --- | --- | --- | --- |
| `ResponseFormatDallE` | no | no | pending |
| `ResponseResource` | yes | yes | provider_event |
| `ResponsesConversationParam` | no | no | pending |

### Streaming events
| schema | bundled | validated | status |
| --- | --- | --- | --- |
| `ErrorStreamingEvent` | yes | yes | provider_event (validated) |
| `ImageEditCompletedStreamingEvent` | no | no | provider_event (pending validation) |
| `ImageEditPartialImageStreamingEvent` | no | no | provider_event (pending validation) |
| `ImageGenerationCompletedStreamingEvent` | no | no | provider_event (pending validation) |
| `ImageGenerationPartialImageStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseApplyPatchCallOperationDiffDeltaStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseApplyPatchCallOperationDiffDoneStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseCodeInterpreterCallCodeDeltaStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseCodeInterpreterCallCodeDoneStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseCodeInterpreterCallCompletedStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseCodeInterpreterCallInProgressStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseCodeInterpreterCallInterpretingStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseCompletedStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseContentPartAddedStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseContentPartDoneStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseCreatedStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseCustomToolCallInputDeltaStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseCustomToolCallInputDoneStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseFailedStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseFileSearchCallCompletedStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseFileSearchCallInProgressStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseFileSearchCallSearchingStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseFunctionCallArgumentsDeltaStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseFunctionCallArgumentsDoneStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseImageGenCallCompletedStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseImageGenCallGeneratingStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseImageGenCallInProgressStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseImageGenCallPartialImageStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseInProgressStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseIncompleteStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseMCPCallArgumentsDeltaStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseMCPCallArgumentsDoneStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseMCPCallCompletedStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseMCPCallFailedStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseMCPCallInProgressStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseMCPListToolsCompletedStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseMCPListToolsFailedStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseMCPListToolsInProgressStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseOutputItemAddedStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseOutputItemDoneStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseOutputTextAnnotationAddedStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseOutputTextDeltaStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseOutputTextDoneStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseQueuedStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseReasoningDeltaStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseReasoningDoneStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseReasoningSummaryDeltaStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseReasoningSummaryDoneStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseReasoningSummaryPartAddedStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseReasoningSummaryPartDoneStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseRefusalDeltaStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseRefusalDoneStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseShellCallCommandAddedStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseShellCallCommandDeltaStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseShellCallCommandDoneStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseWebSearchCallCompletedStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseWebSearchCallInProgressStreamingEvent` | no | no | provider_event (pending validation) |
| `ResponseWebSearchCallSearchingStreamingEvent` | no | no | provider_event (pending validation) |

### Tool schemas
| schema | bundled | validated | status |
| --- | --- | --- | --- |
| `AllowedToolChoice` | yes | yes | pending |
| `ApplyPatchTool` | no | no | pending |
| `ApplyPatchToolChoice` | no | no | pending |
| `ApplyPatchToolParam` | no | no | provider_request (validated) | 
| `AutoCodeInterpreterToolParam` | no | no | pending |
| `CodeInterpreterToolChoice` | no | no | pending |
| `CodeInterpreterToolParam` | no | no | provider_request (validated) | 
| `ComputerToolChoice` | no | no | pending |
| `ComputerToolParam` | no | no | provider_request (validated) | 
| `ComputerUsePreviewTool` | no | no | pending |
| `ComputerUsePreviewToolParam` | no | no | provider_request (validated) | 
| `CustomTool` | no | no | pending |
| `CustomToolChoice` | no | no | pending |
| `CustomToolParam` | no | no | provider_request (validated) | 
| `FileSearchTool` | no | no | pending |
| `FileSearchToolChoice` | no | no | pending |
| `FileSearchToolParam` | no | no | provider_request (validated) | 
| `FunctionShellTool` | no | no | pending |
| `FunctionShellToolChoice` | no | no | pending |
| `FunctionShellToolParam` | no | no | provider_request (validated) | 
| `FunctionTool` | yes | yes | pending |
| `FunctionToolChoice` | yes | yes | pending |
| `FunctionToolParam` | yes | no | provider_request (validated) |
| `ImageGenTool` | no | no | pending |
| `ImageGenToolChoice` | no | no | pending |
| `ImageGenToolParam` | no | no | provider_request (validated) | 
| `LocalShellToolChoice` | no | no | pending |
| `LocalShellToolParam` | no | no | provider_request (validated) | 
| `MCPListToolsTool` | no | no | pending |
| `MCPTool` | no | no | pending |
| `MCPToolChoice` | no | no | pending |
| `MCPToolParam` | no | no | provider_request (validated) | 
| `MemoryToolParam` | no | no | pending |
| `ResponsesToolParam` | yes | no | provider_request (validated) |
| `SpecificCustomToolParam` | no | no | provider_request (validated) | 
| `Tool` | yes | yes | pending |
| `WebSearchGADeprecatedToolParam` | no | no | provider_request (validated) | 
| `WebSearchPreviewTool` | no | no | pending |
| `WebSearchPreviewToolParam` | no | no | provider_request (validated) | 
| `WebSearchToolChoice` | no | no | pending |
| `WebSearchToolParam` | no | no | provider_request (validated) | 
