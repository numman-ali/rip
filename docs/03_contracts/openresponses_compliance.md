# OpenResponses Compliance Map

Summary
- Source repo: `temp/openresponses` (local vendor).
- Reviewed: 2026-01-17.
- Traceability: see `docs/03_contracts/openresponses_traceability.md` for the current upstream snapshot.
- OpenAPI source: `temp/openresponses/public/openapi/openapi.json` (canonical; `temp/openresponses/schema/openapi.json` is fallback only).
- Schema files: 412 split components in `temp/openresponses/schema/components/schemas` (+3 additive patch schemas: `InputVideoContent`, `JsonSchemaResponseFormatParam`, `TextFormatParam`).
- Streaming event schemas: 58 (from `temp/openresponses/schema/paths/responses.json`).
- Input item variants: 25 (from `ItemParam.json`).
- Output item variants: 23 (from `ItemField.json`).
- Inventory artifacts: `schemas/openresponses/schema_inventory.json` (full schema + variant counts) and `schemas/openresponses/streaming_event_type_map.json` (SSE event type map).

Sources reviewed
- `temp/openresponses/README.md`
- `temp/openresponses/src/pages/index.mdx`
- `temp/openresponses/src/pages/specification.mdx`
- `temp/openresponses/src/pages/reference.mdx`
- `temp/openresponses/src/pages/compliance.mdx`
- `temp/openresponses/src/pages/changelog.mdx`
- `temp/openresponses/src/pages/governance.mdx`
- `temp/openresponses/public/openapi/openapi.json`
- `temp/openresponses/schema/openapi.json`
- `temp/openresponses/schema/openapi_additive_patches.yaml`
- `temp/openresponses/schema/openapi_filter_manifest.yaml`
- `temp/openresponses/schema/paths/responses.json`
- `temp/openresponses/schema/components/schemas/*.json`
- `schemas/openresponses/schema_inventory.json` (generated)
- `schemas/openresponses/streaming_event_type_map.json` (generated)

Mapping rules (Phase 1)
- All OpenResponses SSE events map to `provider_event` frames with full payload fidelity.
- Internal frames are emitted for a subset (session + text/tool deltas); all other events remain provider-only until explicitly promoted.
- No OpenResponses fields/events are dropped at the provider boundary.

Implementation status (current)
- Provider adapter validates streaming events against the split `paths/responses.json` schema and embedded `response` objects against split component schemas.
- Split schemas validate all 58 streaming event variants and 23 output item variants; bundled OpenAPI remains partial (24/58 streaming events, 4/23 output items).
- Provider adapter emits `output_text_delta` frames for `response.output_text.delta` events alongside `provider_event` frames.
- ResponseResource validation tests cover tool_choice variants (including allowed_tools + value enum), Tool union variants, shell output items, code interpreter calls, search/computer/image/apply_patch tool call items, MCPListTools output items, MCP approval items, and MCP tool calls (including error variants); MCP Memory/MCP filter/approval/error schemas plus search/tool call enums and code interpreter output params are validated directly.
- Content block schemas (input/output text, image, file, summary/refusal, reasoning text) and response format schemas (text/json + JSON schema formats) are validated; additive patch schemas (`InputVideoContent`, `JsonSchemaResponseFormatParam`, `TextFormatParam`) are validated via the bundled OpenAPI.
- Provider request builder validates CreateResponseBody payloads (errors captured; payload preserved); tool fields use per-variant validation to avoid jsonschema oneOf failures; request sending is not wired yet.
- Tool schema validation uses split component schemas for `ResponsesToolParam` and `ToolChoiceParam`, validating optional fields and nested structures; bundled OpenAPI still only includes function tool variants.
- Split component schemas are vendored in `schemas/openresponses/split_components.json`; split paths schema is vendored in `schemas/openresponses/paths_responses.json`.
- Input item variants are mapped via `ItemParam` constructors in the provider request builder; runtime request-frame integration remains pending.
- ItemParam validation covers all input variants using required-field checks (message role/item reference handling included); runtime mapping remains pending.
- Split schema inventory and SSE event type map captured in `schemas/openresponses/` and reflected in the tables below.
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
| `InputVideoContent` | Adds `input_video` content blocks to message content unions. | validated (bundled OpenAPI) |
| `JsonSchemaResponseFormatParam` | Adds JSON Schema response format support. | validated (bundled OpenAPI) |
| `TextFormatParam` | Adds `json_schema` format option to text formats. | validated (bundled OpenAPI) |

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
| `image_edit.completed` | `ImageEditCompletedStreamingEvent.json` | provider_event (validated) |
| `image_edit.partial_image` | `ImageEditPartialImageStreamingEvent.json` | provider_event (validated) |
| `image_generation.completed` | `ImageGenerationCompletedStreamingEvent.json` | provider_event (validated) |
| `image_generation.partial_image` | `ImageGenerationPartialImageStreamingEvent.json` | provider_event (validated) |
| `response.apply_patch_call_operation_diff.delta` | `ResponseApplyPatchCallOperationDiffDeltaStreamingEvent.json` | provider_event (validated) |
| `response.apply_patch_call_operation_diff.done` | `ResponseApplyPatchCallOperationDiffDoneStreamingEvent.json` | provider_event (validated) |
| `response.code_interpreter_call.completed` | `ResponseCodeInterpreterCallCompletedStreamingEvent.json` | provider_event (validated) |
| `response.code_interpreter_call.in_progress` | `ResponseCodeInterpreterCallInProgressStreamingEvent.json` | provider_event (validated) |
| `response.code_interpreter_call.interpreting` | `ResponseCodeInterpreterCallInterpretingStreamingEvent.json` | provider_event (validated) |
| `response.code_interpreter_call_code.delta` | `ResponseCodeInterpreterCallCodeDeltaStreamingEvent.json` | provider_event (validated) |
| `response.code_interpreter_call_code.done` | `ResponseCodeInterpreterCallCodeDoneStreamingEvent.json` | provider_event (validated) |
| `response.completed` | `ResponseCompletedStreamingEvent.json` | provider_event (validated) |
| `response.content_part.added` | `ResponseContentPartAddedStreamingEvent.json` | provider_event (validated) |
| `response.content_part.done` | `ResponseContentPartDoneStreamingEvent.json` | provider_event (validated) |
| `response.created` | `ResponseCreatedStreamingEvent.json` | provider_event (validated) |
| `response.custom_tool_call_input.delta` | `ResponseCustomToolCallInputDeltaStreamingEvent.json` | provider_event (validated) |
| `response.custom_tool_call_input.done` | `ResponseCustomToolCallInputDoneStreamingEvent.json` | provider_event (validated) |
| `response.failed` | `ResponseFailedStreamingEvent.json` | provider_event (validated) |
| `response.file_search_call.completed` | `ResponseFileSearchCallCompletedStreamingEvent.json` | provider_event (validated) |
| `response.file_search_call.in_progress` | `ResponseFileSearchCallInProgressStreamingEvent.json` | provider_event (validated) |
| `response.file_search_call.searching` | `ResponseFileSearchCallSearchingStreamingEvent.json` | provider_event (validated) |
| `response.function_call_arguments.delta` | `ResponseFunctionCallArgumentsDeltaStreamingEvent.json` | provider_event (validated) |
| `response.function_call_arguments.done` | `ResponseFunctionCallArgumentsDoneStreamingEvent.json` | provider_event (validated) |
| `response.image_generation_call.completed` | `ResponseImageGenCallCompletedStreamingEvent.json` | provider_event (validated) |
| `response.image_generation_call.generating` | `ResponseImageGenCallGeneratingStreamingEvent.json` | provider_event (validated) |
| `response.image_generation_call.in_progress` | `ResponseImageGenCallInProgressStreamingEvent.json` | provider_event (validated) |
| `response.image_generation_call.partial_image` | `ResponseImageGenCallPartialImageStreamingEvent.json` | provider_event (validated) |
| `response.in_progress` | `ResponseInProgressStreamingEvent.json` | provider_event (validated) |
| `response.incomplete` | `ResponseIncompleteStreamingEvent.json` | provider_event (validated) |
| `response.mcp_call.completed` | `ResponseMCPCallCompletedStreamingEvent.json` | provider_event (validated) |
| `response.mcp_call.failed` | `ResponseMCPCallFailedStreamingEvent.json` | provider_event (validated) |
| `response.mcp_call.in_progress` | `ResponseMCPCallInProgressStreamingEvent.json` | provider_event (validated) |
| `response.mcp_call_arguments.delta` | `ResponseMCPCallArgumentsDeltaStreamingEvent.json` | provider_event (validated) |
| `response.mcp_call_arguments.done` | `ResponseMCPCallArgumentsDoneStreamingEvent.json` | provider_event (validated) |
| `response.mcp_list_tools.completed` | `ResponseMCPListToolsCompletedStreamingEvent.json` | provider_event (validated) |
| `response.mcp_list_tools.failed` | `ResponseMCPListToolsFailedStreamingEvent.json` | provider_event (validated) |
| `response.mcp_list_tools.in_progress` | `ResponseMCPListToolsInProgressStreamingEvent.json` | provider_event (validated) |
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
| `response.shell_call_command.added` | `ResponseShellCallCommandAddedStreamingEvent.json` | provider_event (validated) |
| `response.shell_call_command.delta` | `ResponseShellCallCommandDeltaStreamingEvent.json` | provider_event (validated) |
| `response.shell_call_command.done` | `ResponseShellCallCommandDoneStreamingEvent.json` | provider_event (validated) |
| `response.web_search_call.completed` | `ResponseWebSearchCallCompletedStreamingEvent.json` | provider_event (validated) |
| `response.web_search_call.in_progress` | `ResponseWebSearchCallInProgressStreamingEvent.json` | provider_event (validated) |
| `response.web_search_call.searching` | `ResponseWebSearchCallSearchingStreamingEvent.json` | provider_event (validated) |

## Input item variants
| item type | schema | mapping |
| --- | --- | --- |
| `apply_patch_call` | `ApplyPatchToolCallItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `apply_patch_call_output` | `ApplyPatchToolCallOutputItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `code_interpreter_call` | `CodeInterpreterCallItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `compaction` | `CompactionSummaryItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `computer_call` | `ComputerCallItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `computer_call_output` | `ComputerCallOutputItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `custom_tool_call` | `CustomToolCallItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `custom_tool_call_output` | `CustomToolCallOutputItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `file_search_call` | `FileSearchCallItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `function_call` | `FunctionCallItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `function_call_output` | `FunctionCallOutputItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `image_generation_call` | `ImageGenCallItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `local_shell_call` | `LocalShellCallItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `local_shell_call_output` | `LocalShellCallOutputItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `mcp_approval_request` | `MCPApprovalRequestItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `mcp_approval_response` | `MCPApprovalResponseItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `message` | `AssistantMessageItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `message` | `DeveloperMessageItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `message` | `SystemMessageItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `message` | `UserMessageItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `reasoning` | `ReasoningItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `shell_call` | `FunctionShellCallItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `shell_call_output` | `FunctionShellCallOutputItemParam.json` | provider_request (mapped; runtime mapping pending) |
| `item_reference` | `ItemReferenceParam.json` | provider_request (mapped; runtime mapping pending) |
| `web_search_call` | `WebSearchCallItemParam.json` | provider_request (mapped; runtime mapping pending) |

## Output item variants
| item type | schema | mapping |
| --- | --- | --- |
| `apply_patch_call` | `ApplyPatchToolCall.json` | provider_event (validated) |
| `apply_patch_call_output` | `ApplyPatchToolCallOutput.json` | provider_event (validated) |
| `code_interpreter_call` | `CodeInterpreterCall.json` | provider_event (validated) |
| `compaction` | `CompactionBody.json` | provider_event (validated) |
| `computer_call` | `ComputerCall.json` | provider_event (validated) |
| `computer_call_output` | `ComputerCallOutput.json` | provider_event (validated) |
| `custom_tool_call` | `CustomToolCall.json` | provider_event (validated) |
| `custom_tool_call_output` | `CustomToolCallOutput.json` | provider_event (validated) |
| `file_search_call` | `FileSearchCall.json` | provider_event (validated) |
| `function_call` | `FunctionCall.json` | provider_event (validated) |
| `function_call_output` | `FunctionCallOutput.json` | provider_event (validated) |
| `image_generation_call` | `ImageGenCall.json` | provider_event (validated) |
| `local_shell_call` | `LocalShellCall.json` | provider_event (validated) |
| `local_shell_call_output` | `LocalShellCallOutput.json` | provider_event (validated) |
| `mcp_approval_request` | `MCPApprovalRequest.json` | provider_event (validated) |
| `mcp_approval_response` | `MCPApprovalResponse.json` | provider_event (validated) |
| `mcp_call` | `MCPToolCall.json` | provider_event (validated) |
| `mcp_list_tools` | `MCPListTools.json` | provider_event (validated) |
| `message` | `Message.json` | provider_event (validated) |
| `reasoning` | `ReasoningBody.json` | provider_event (validated) |
| `shell_call` | `FunctionShellCall.json` | provider_event (validated) |
| `shell_call_output` | `FunctionShellCallOutput.json` | provider_event (validated) |
| `web_search_call` | `WebSearchCall.json` | provider_event (validated) |

## Schema index (all components)

This list is exhaustive and drives the task tracker in `docs/07_tasks/openresponses_compliance.md`.

Legend
- `bundled`: schema is present in `schemas/openresponses/openapi.json`.
- `validated`: schema is reachable from split streaming-event or ResponseResource validation (request validation is noted in `status`).
- `status`: mapping status in current codebase.

### Error schemas
| schema | bundled | validated | status |
| --- | --- | --- | --- |
| `Error` | yes | yes | provider_event |
| `ErrorPayload` | yes | yes | provider_event |
| `HTTPError` | no | yes | pending |
| `MCPProtocolError` | no | yes | pending |
| `MCPToolExecutionError` | no | yes | pending |

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
| `Annotation` | yes | yes | provider_event (validated) |
| `ApiSourceParam` | no | yes | provider_request (validated) |
| `ApplyPatchCallOutputStatus` | no | yes | pending |
| `ApplyPatchCallOutputStatusParam` | no | no | pending |
| `ApplyPatchCallStatus` | no | yes | pending |
| `ApplyPatchCallStatusParam` | no | no | pending |
| `ApplyPatchCreateFileOperation` | no | yes | pending |
| `ApplyPatchCreateFileOperationParam` | no | no | pending |
| `ApplyPatchDeleteFileOperation` | no | yes | pending |
| `ApplyPatchDeleteFileOperationParam` | no | no | pending |
| `ApplyPatchOperationParam` | no | no | pending |
| `ApplyPatchToolCall` | no | yes | provider_event (validated) |
| `ApplyPatchToolCallOutput` | no | yes | provider_event (validated) |
| `ApplyPatchUpdateFileOperation` | no | yes | pending |
| `ApplyPatchUpdateFileOperationParam` | no | no | pending |
| `ApproximateLocation` | no | yes | provider_event (validated) |
| `ApproximateLocationParam` | no | yes | provider_request (validated) |
| `Billing` | no | yes | provider_event (validated) |
| `ClickAction` | no | yes | provider_event (validated) |
| `ClickButtonType` | no | yes | provider_request (validated) |
| `ClickParam` | no | yes | provider_request (validated) |
| `CodeInterpreterCall` | no | yes | provider_event (validated) |
| `CodeInterpreterCallStatus` | no | yes | pending |
| `CodeInterpreterOutputImage` | no | yes | pending |
| `CodeInterpreterOutputLogs` | no | yes | pending |
| `CodeInterpreterToolCallOutputImageParam` | no | no | pending |
| `CodeInterpreterToolCallOutputLogsParam` | no | no | pending |
| `CompactResource` | no | no | pending |
| `CompactResponseMethodPublicBody` | no | no | pending |
| `CompactionBody` | no | yes | provider_event (validated) |
| `ComparisonFilterFieldCONTAINS` | no | yes | provider_event (validated) |
| `ComparisonFilterFieldCONTAINSANY` | no | yes | provider_event (validated) |
| `ComparisonFilterFieldEQ` | no | yes | provider_event (validated) |
| `ComparisonFilterFieldGT` | no | yes | provider_event (validated) |
| `ComparisonFilterFieldGTE` | no | yes | provider_event (validated) |
| `ComparisonFilterFieldIN` | no | yes | provider_event (validated) |
| `ComparisonFilterFieldLT` | no | yes | provider_event (validated) |
| `ComparisonFilterFieldLTE` | no | yes | provider_event (validated) |
| `ComparisonFilterFieldNCONTAINS` | no | yes | provider_event (validated) |
| `ComparisonFilterFieldNCONTAINSANY` | no | yes | provider_event (validated) |
| `ComparisonFilterFieldNE` | no | yes | provider_event (validated) |
| `ComparisonFilterFieldNIN` | no | yes | provider_event (validated) |
| `ComparisonFilterParamContainsAnyParam` | no | yes | provider_request (validated) |
| `ComparisonFilterParamContainsParam` | no | yes | provider_request (validated) |
| `ComparisonFilterParamEQParam` | no | yes | provider_request (validated) |
| `ComparisonFilterParamGTEParam` | no | yes | provider_request (validated) |
| `ComparisonFilterParamGTParam` | no | yes | provider_request (validated) |
| `ComparisonFilterParamINParam` | no | yes | provider_request (validated) |
| `ComparisonFilterParamLTEParam` | no | yes | provider_request (validated) |
| `ComparisonFilterParamLTParam` | no | yes | provider_request (validated) |
| `ComparisonFilterParamNContainsAnyParam` | no | yes | provider_request (validated) |
| `ComparisonFilterParamNContainsParam` | no | yes | provider_request (validated) |
| `ComparisonFilterParamNEParam` | no | yes | provider_request (validated) |
| `ComparisonFilterParamNINParam` | no | yes | provider_request (validated) |
| `CompoundFilterFieldAND` | no | yes | provider_event (validated) |
| `CompoundFilterFieldOR` | no | yes | provider_event (validated) |
| `CompoundFilterParamAndParam` | no | yes | provider_request (validated) |
| `CompoundFilterParamOrParam` | no | yes | provider_request (validated) |
| `ComputerCall` | no | yes | provider_event (validated) |
| `ComputerCallOutput` | no | yes | provider_event (validated) |
| `ComputerCallOutputStatus` | no | yes | pending |
| `ComputerCallSafetyCheckParam` | no | yes | provider_request (validated) |
| `ComputerEnvironment` | no | yes | provider_request (validated) |
| `ComputerEnvironment1` | no | yes | provider_event (validated) |
| `ComputerScreenshotContent` | no | yes | provider_event (validated) |
| `ComputerScreenshotParam` | no | yes | provider_request (validated) |
| `ContainerFileCitationBody` | no | yes | provider_event (validated) |
| `ContainerFileCitationParam` | no | yes | provider_request (validated) |
| `ContainerMemoryLimit` | no | no | pending |
| `ContextEdit` | no | yes | provider_event (validated) |
| `ContextEditDetails` | no | yes | provider_event (validated) |
| `Conversation` | no | yes | provider_event (validated) |
| `ConversationParam` | no | yes | provider_request (validated) |
| `CoordParam` | no | yes | provider_request (validated) |
| `CreateImageBody15Param` | no | no | pending |
| `CreateImageBody1MiniParam` | no | no | pending |
| `CreateImageBody1Param` | no | no | pending |
| `CreateImageBodyChatGPTImageLatestParam` | no | no | pending |
| `CreateVideoBody` | no | no | pending |
| `CreateVideoRemixBody` | no | no | pending |
| `CustomGrammarFormatField` | no | yes | pending |
| `CustomGrammarFormatParam` | no | no | pending |
| `CustomTextFormatField` | no | yes | pending |
| `CustomTextFormatParam` | no | no | pending |
| `CustomToolCall` | no | yes | provider_event (validated) |
| `CustomToolCallOutput` | no | yes | provider_event (validated) |
| `CustomToolFormat` | no | yes | pending |
| `DeletedResponseResource` | no | no | pending |
| `DeletedVideoResource` | no | no | pending |
| `DetailEnum` | yes | yes | provider_request (validated) |
| `DoubleClickAction` | no | yes | provider_event (validated) |
| `DoubleClickParam` | no | yes | provider_request (validated) |
| `DragAction` | no | yes | provider_event (validated) |
| `DragParam` | no | yes | provider_request (validated) |
| `DragPoint` | no | yes | provider_event (validated) |
| `EditImageBody15Param` | no | no | pending |
| `EditImageBody1MiniParam` | no | no | pending |
| `EditImageBody1Param` | no | no | pending |
| `EditImageBodyChatGPTImageLatestParam` | no | no | pending |
| `EditsBodyDallE2Param` | no | no | pending |
| `EmptyAction` | no | yes | provider_event (validated) |
| `EmptyModelParam` | yes | no | pending |
| `ExcludeEnum` | no | no | provider_request (validated) |
| `FileCitationBody` | no | yes | provider_event (validated) |
| `FileCitationParam` | no | yes | provider_request (validated) |
| `FileSearchCall` | no | yes | provider_event (validated) |
| `FileSearchRankingOptionsParam` | no | yes | provider_request (validated) |
| `FileSearchResult` | no | yes | provider_event (validated) |
| `FileSearchRetrievedChunksParam` | no | yes | provider_request (validated) |
| `FileSearchToolCallStatusEnum` | no | yes | provider_event (validated) |
| `Filters` | no | yes | provider_event (validated) |
| `FunctionCall` | yes | yes | provider_event (validated) |
| `FunctionCallItemStatus` | yes | yes | provider_request (validated) |
| `FunctionCallOutput` | yes | yes | provider_event (validated) |
| `FunctionCallOutputStatusEnum` | yes | yes | provider_event (validated) |
| `FunctionCallStatus` | yes | yes | provider_event (validated) |
| `FunctionShellAction` | no | yes | pending |
| `FunctionShellActionParam` | no | no | pending |
| `FunctionShellCall` | no | yes | provider_event (validated) |
| `FunctionShellCallItemStatus` | no | yes | provider_request (validated) |
| `FunctionShellCallOutput` | no | yes | provider_event (validated) |
| `FunctionShellCallOutputContent` | no | yes | provider_event (validated) |
| `FunctionShellCallOutputContentParam` | no | yes | provider_request (validated) |
| `FunctionShellCallOutputExitOutcome` | no | yes | provider_event (validated) |
| `FunctionShellCallOutputExitOutcomeParam` | no | yes | provider_request (validated) |
| `FunctionShellCallOutputOutcomeParam` | no | yes | provider_request (validated) |
| `FunctionShellCallOutputTimeoutOutcome` | no | yes | provider_event (validated) |
| `FunctionShellCallOutputTimeoutOutcomeParam` | no | yes | provider_request (validated) |
| `GenerationsBodyDallE2Param` | no | no | pending |
| `GenerationsBodyDallE3Param` | no | no | pending |
| `GrammarSyntax` | no | no | pending |
| `GrammarSyntax1` | no | yes | pending |
| `HybridSearchOptions` | no | yes | provider_event (validated) |
| `HybridSearchOptionsParam` | no | yes | provider_request (validated) |
| `Image` | no | no | pending |
| `ImageBackground` | no | yes | pending |
| `ImageDetail` | yes | yes | pending |
| `ImageGenAction` | no | yes | pending |
| `ImageGenActionEnum` | no | no | pending |
| `ImageGenCall` | no | yes | provider_event (validated) |
| `ImageGenCallStatus` | no | yes | pending |
| `ImageGenInputUsageDetails` | no | no | pending |
| `ImageGenOutputTokensDetails` | no | no | pending |
| `ImageGenToolModel` | no | yes | pending |
| `ImageGenUsage` | no | no | pending |
| `ImageModeration` | no | yes | pending |
| `ImageOutputFormat` | no | yes | pending |
| `ImageQuality` | no | yes | pending |
| `ImageQualityDallE` | no | no | pending |
| `ImageResource` | no | no | pending |
| `ImageSize` | no | yes | pending |
| `ImageSizeDallE2` | no | no | pending |
| `ImageSizeDallE3` | no | no | pending |
| `ImageStyleDallE` | no | no | pending |
| `ImageUsage` | no | yes | pending |
| `ImageUsageInputTokensDetails` | no | yes | pending |
| `ImageUsageOutputTokensDetails` | no | yes | pending |
| `IncludeEnum` | yes | no | provider_request (validated) |
| `IncompleteDetails` | yes | yes | pending |
| `InputFidelity` | no | no | pending |
| `InputFileContent` | yes | yes | pending |
| `InputFileContentParam` | yes | no | pending |
| `InputImageContent` | yes | yes | pending |
| `InputImageContentParamAutoParam` | yes | no | pending |
| `InputImageMaskContentParam` | no | no | pending |
| `InputTextContent` | yes | yes | pending |
| `InputTextContentParam` | yes | no | pending |
| `InputTokensDetails` | yes | yes | provider_event (validated) |
| `ItemListResource` | no | no | pending |
| `ItemReferenceParam` | yes | no | provider_request (validated) |
| `JsonObjectResponseFormat` | yes | yes | pending |
| `JsonSchemaResponseFormat` | yes | yes | pending |
| `KeyPressAction` | no | yes | provider_event (validated) |
| `KeyPressParam` | no | yes | provider_request (validated) |
| `LocalFileEnvironmentParam` | no | no | pending |
| `LocalShellCall` | no | yes | provider_event (validated) |
| `LocalShellCallItemStatus` | no | no | pending |
| `LocalShellCallOutput` | no | yes | provider_event (validated) |
| `LocalShellCallOutputStatusEnum` | no | yes | pending |
| `LocalShellCallStatus` | no | yes | pending |
| `LocalShellExecAction` | no | yes | pending |
| `LocalShellExecActionParam` | no | no | pending |
| `LogProb` | yes | yes | provider_event (validated) |
| `MCPApprovalRequest` | no | yes | provider_event (validated) |
| `MCPApprovalResponse` | no | yes | provider_event (validated) |
| `MCPListTools` | no | yes | provider_event (validated) |
| `MCPRequireApprovalApiEnum` | no | no | pending |
| `MCPRequireApprovalFieldEnum` | no | yes | pending |
| `MCPRequireApprovalFilterField` | no | yes | pending |
| `MCPRequireApprovalFilterParam` | no | no | pending |
| `MCPToolCall` | no | yes | provider_event (validated) |
| `MCPToolCallStatus` | no | yes | pending |
| `MCPToolFilterField` | no | yes | pending |
| `MCPToolFilterParam` | no | no | pending |
| `Message` | yes | yes | provider_event (validated) |
| `MessageRole` | yes | yes | provider_event (validated) |
| `MessageRole1` | no | no | provider_event (validated) |
| `MessageStatus` | yes | yes | provider_event (validated) |
| `MetadataParam` | yes | no | provider_request (validated) |
| `MoveAction` | no | yes | provider_event (validated) |
| `MoveParam` | no | yes | provider_request (validated) |
| `OrderEnum` | no | no | provider_request (validated) |
| `OutputTextContent` | yes | yes | pending |
| `OutputTextContentParam` | yes | no | pending |
| `OutputTokensDetails` | yes | yes | provider_event (validated) |
| `Payer` | no | yes | provider_event (validated) |
| `PromptCacheRetentionEnum` | no | yes | provider_request (validated) |
| `PromptInstructionMessage` | no | yes | pending |
| `RankerVersionType` | no | yes | provider_request (validated) |
| `RankingOptions` | no | yes | pending |
| `Reasoning` | yes | yes | pending |
| `ReasoningBody` | yes | yes | provider_event (validated) |
| `ReasoningEffortEnum` | yes | yes | pending |
| `ReasoningParam` | yes | no | pending |
| `ReasoningSummaryContentParam` | yes | no | pending |
| `ReasoningSummaryEnum` | yes | yes | pending |
| `ReasoningTextContent` | yes | yes | pending |
| `RefusalContent` | yes | yes | pending |
| `RefusalContentParam` | yes | no | pending |
| `SafetyCheck` | no | yes | provider_event (validated) |
| `ScreenshotAction` | no | yes | provider_event (validated) |
| `ScreenshotParam` | no | yes | provider_request (validated) |
| `ScrollAction` | no | yes | provider_event (validated) |
| `ScrollParam` | no | yes | provider_request (validated) |
| `SearchContextSize` | no | yes | pending |
| `ServiceTierEnum` | yes | no | provider_request (validated) |
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
| `TokenCountsBody` | no | no | provider_request (validated) |
| `TokenCountsResource` | no | no | provider_event (validated) |
| `ToolChoiceParam` | yes | no | provider_request (validated) |
| `ToolChoiceValueEnum` | yes | yes | provider_request (validated) |
| `TopLogProb` | yes | yes | provider_event (validated) |
| `TruncationEnum` | yes | yes | provider_request (validated) |
| `TypeAction` | no | yes | provider_event (validated) |
| `TypeParam` | no | yes | provider_request (validated) |
| `UrlCitationBody` | yes | yes | provider_event (validated) |
| `UrlCitationParam` | yes | yes | provider_request (validated) |
| `UrlSourceParam` | no | no | pending |
| `Usage` | yes | yes | provider_event (validated) |
| `VerbosityEnum` | yes | yes | provider_request (validated) |
| `VideoContentVariant` | no | no | pending |
| `VideoListResource` | no | no | pending |
| `VideoModel` | no | no | pending |
| `VideoResource` | no | no | pending |
| `VideoSeconds` | no | no | pending |
| `VideoSize` | no | no | pending |
| `VideoStatus` | no | no | pending |
| `WaitAction` | no | yes | provider_event (validated) |
| `WaitParam` | no | yes | provider_request (validated) |
| `WebSearchCall` | no | yes | provider_event (validated) |
| `WebSearchCallActionFindInPage` | no | yes | pending |
| `WebSearchCallActionFindInPageParam` | no | no | pending |
| `WebSearchCallActionOpenPage` | no | yes | pending |
| `WebSearchCallActionOpenPageParam` | no | no | pending |
| `WebSearchCallActionSearch` | no | yes | pending |
| `WebSearchCallActionSearchParam` | no | no | pending |
| `WebSearchCallStatus` | no | yes | pending |
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
| `ImageEditCompletedStreamingEvent` | no | yes | provider_event (validated) |
| `ImageEditPartialImageStreamingEvent` | no | yes | provider_event (validated) |
| `ImageGenerationCompletedStreamingEvent` | no | yes | provider_event (validated) |
| `ImageGenerationPartialImageStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseApplyPatchCallOperationDiffDeltaStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseApplyPatchCallOperationDiffDoneStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseCodeInterpreterCallCodeDeltaStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseCodeInterpreterCallCodeDoneStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseCodeInterpreterCallCompletedStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseCodeInterpreterCallInProgressStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseCodeInterpreterCallInterpretingStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseCompletedStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseContentPartAddedStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseContentPartDoneStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseCreatedStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseCustomToolCallInputDeltaStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseCustomToolCallInputDoneStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseFailedStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseFileSearchCallCompletedStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseFileSearchCallInProgressStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseFileSearchCallSearchingStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseFunctionCallArgumentsDeltaStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseFunctionCallArgumentsDoneStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseImageGenCallCompletedStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseImageGenCallGeneratingStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseImageGenCallInProgressStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseImageGenCallPartialImageStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseInProgressStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseIncompleteStreamingEvent` | yes | yes | provider_event (validated) |
| `ResponseMCPCallArgumentsDeltaStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseMCPCallArgumentsDoneStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseMCPCallCompletedStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseMCPCallFailedStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseMCPCallInProgressStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseMCPListToolsCompletedStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseMCPListToolsFailedStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseMCPListToolsInProgressStreamingEvent` | no | yes | provider_event (validated) |
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
| `ResponseShellCallCommandAddedStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseShellCallCommandDeltaStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseShellCallCommandDoneStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseWebSearchCallCompletedStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseWebSearchCallInProgressStreamingEvent` | no | yes | provider_event (validated) |
| `ResponseWebSearchCallSearchingStreamingEvent` | no | yes | provider_event (validated) |

### Tool schemas
| schema | bundled | validated | status |
| --- | --- | --- | --- |
| `AllowedToolChoice` | yes | yes | pending |
| `ApplyPatchTool` | no | yes | pending |
| `ApplyPatchToolChoice` | no | yes | pending |
| `ApplyPatchToolParam` | no | no | provider_request (validated) |
| `AutoCodeInterpreterToolParam` | no | no | pending |
| `CodeInterpreterToolChoice` | no | yes | pending |
| `CodeInterpreterToolParam` | no | no | provider_request (validated) |
| `ComputerToolChoice` | no | yes | pending |
| `ComputerToolParam` | no | no | provider_request (validated) |
| `ComputerUsePreviewTool` | no | yes | pending |
| `ComputerUsePreviewToolParam` | no | no | provider_request (validated) |
| `CustomTool` | no | yes | pending |
| `CustomToolChoice` | no | yes | pending |
| `CustomToolParam` | no | no | provider_request (validated) |
| `FileSearchTool` | no | yes | pending |
| `FileSearchToolChoice` | no | yes | pending |
| `FileSearchToolParam` | no | no | provider_request (validated) |
| `FunctionShellTool` | no | yes | pending |
| `FunctionShellToolChoice` | no | yes | pending |
| `FunctionShellToolParam` | no | no | provider_request (validated) |
| `FunctionTool` | yes | yes | pending |
| `FunctionToolChoice` | yes | yes | pending |
| `FunctionToolParam` | yes | no | provider_request (validated) |
| `ImageGenTool` | no | yes | pending |
| `ImageGenToolChoice` | no | yes | pending |
| `ImageGenToolParam` | no | no | provider_request (validated) |
| `LocalShellToolChoice` | no | yes | pending |
| `LocalShellToolParam` | no | no | provider_request (validated) |
| `MCPListToolsTool` | no | yes | pending |
| `MCPTool` | no | yes | pending |
| `MCPToolChoice` | no | yes | pending |
| `MCPToolParam` | no | no | provider_request (validated) |
| `MemoryToolParam` | no | no | pending |
| `ResponsesToolParam` | yes | no | provider_request (validated) |
| `SpecificCustomToolParam` | no | no | provider_request (validated) |
| `Tool` | yes | yes | pending |
| `WebSearchGADeprecatedToolParam` | no | no | provider_request (validated) |
| `WebSearchPreviewTool` | no | yes | pending |
| `WebSearchPreviewToolParam` | no | no | provider_request (validated) |
| `WebSearchToolChoice` | no | yes | pending |
| `WebSearchToolParam` | no | no | provider_request (validated) |
