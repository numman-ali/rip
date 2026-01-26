# OpenResponses Coverage Map

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
- Product intent: treat OpenResponses as a full capability surface at the provider boundary; schema validation is evidence, but capability ownership + parity is the work product.

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

## Capability ownership (canonical)

This section is the product-level mapping: every OpenResponses concept must be owned by one or more internal capability ids (and surfaced or explicitly gap-tracked).

Primary OpenResponses boundary capabilities
- `openresponses.request_fidelity`: preserve/pass-through all `CreateResponseBody` fields.
- `openresponses.response_fidelity`: preserve/pass-through full `ResponseResource` + output items.
- `openresponses.streaming_fidelity`: preserve/pass-through full SSE event stream (`event` == payload `type`, `[DONE]`, ordering, unknown events safe).
- `openresponses.item_lifecycle`: item required fields + status semantics (`in_progress`/`incomplete`/`completed`) and stream sequencing rules.
- `openresponses.content_union`: user/model content unions (text/image/file/video/refusal/summary).
- `openresponses.reasoning_items`: reasoning item payloads (`content`/`encrypted_content`/`summary`) and related stream events.
- `openresponses.tools_union`: full tool/tool_choice unions and tool-call item variants.
- `openresponses.errors`: error payloads + `response.failed`/`error` events.
- `openresponses.extensions`: vendor-prefixed items/events + schema extensions (unknown-safe).

Supporting (non-OpenResponses) capabilities referenced by the spec
- `session.previous_response`: `previous_response_id` semantics and continuity.
- `execution.json_stream`: streaming mode on/off + JSON streaming output.
- `execution.stream_options`: stream options that control extra emissions.
- `execution.response_include`: include/extras (logprobs, sources, tool outputs, etc.).
- `execution.structured_output`: JSON Schema response formats.
- `tool.choice`: `tool_choice` policy (`auto`/`required`/`none`/forced tool).
- `tool.allowed_tools`: `allowed_tools` enforcement.
- `tool.call_limits`: `max_tool_calls`/`parallel_tool_calls` limits.
- `model.select`: provider model selection (`model` string).
- `model.sampling_params`: `temperature`/`top_p`/penalties/seed controls.
- `model.max_output_tokens`: `max_output_tokens`.
- `model.logprobs`: `top_logprobs` + token logprobs.
- `model.prompt_cache`: prompt cache key + retention.
- `model.service_tier`: `service_tier` hint.
- `usage.token_counts`: `usage` and token-count shapes.
- `compaction.truncation_policy`: `truncation` policy.
- `policy.request_identifiers`: `user` + `safety_identifier`.

## Coverage evidence (current)
- Provider adapter validates streaming events against the split `paths/responses.json` schema and embedded `response` objects against split component schemas.
- Split schemas validate all 58 streaming event variants and 23 output item variants; bundled OpenAPI remains partial (24/58 streaming events, 4/23 output items).
- Provider adapter emits `output_text_delta` frames for `response.output_text.delta` events alongside `provider_event` frames.
- ResponseResource validation tests cover tool_choice variants (including allowed_tools + value enum), Tool union variants, shell output items, code interpreter calls, search/computer/image/apply_patch tool call items, MCPListTools output items, MCP approval items, and MCP tool calls (including error variants); MCP Memory/MCP filter/approval/error schemas plus search/tool call enums and code interpreter output params are validated directly.
- Content block schemas (input/output text, image, file, summary/refusal, reasoning text) and response format schemas (text/json + JSON schema formats) are validated; additive patch schemas (`InputVideoContent`, `JsonSchemaResponseFormatParam`, `TextFormatParam`) are validated via the bundled OpenAPI.
- Provider request builder validates CreateResponseBody payloads (errors captured; payload preserved); tool fields use per-variant validation to avoid jsonschema oneOf failures; request sending is wired for streaming + provider-driven tool loop in `ripd` (Phase 1: `function_call` -> ToolRunner -> `previous_response_id` follow-ups).
- Tool execution is constrained by `tool_choice`/`allowed_tools`: `ripd` rejects disallowed `function_call` items deterministically (no tool execution; `function_call_output.ok=false` + tool error events).
- Provider supports stateless history compatibility (opt-in via `RIP_OPENRESPONSES_STATELESS_HISTORY`) for providers that do not honor `previous_response_id`; in this mode, follow-ups resend accumulated input items with deterministic ids.
- Provider validation supports compatibility normalization for missing item `id` fields (opt-in via `RIP_OPENRESPONSES_STATELESS_HISTORY`; raw events preserved) to keep schema validation strict while accommodating non-compliant streams.
- Tool schemas are emitted with `strict: false` for broad provider compatibility; runtime still validates tool args locally.
- Tool schema validation uses split component schemas for `ResponsesToolParam` and `ToolChoiceParam`, validating optional fields and nested structures; bundled OpenAPI still only includes function tool variants.
- Split component schemas are vendored in `schemas/openresponses/split_components.json`; split paths schema is vendored in `schemas/openresponses/paths_responses.json`.
- Input item variants are mapped via `ItemParam` constructors in the provider request builder; runtime request-frame integration remains pending.
- ItemParam validation covers all input variants using required-field checks (message role/item reference handling included); runtime mapping remains pending.
- Split schema inventory and SSE event type map captured in `schemas/openresponses/` and reflected in the tables below.
- Requests are JSON-only per spec; form-encoded bodies are not supported (ADR-0002).
- Bundled OpenAPI schema currently includes 102 component schemas; the split OpenResponses schema defines 412 component schemas. Missing schemas are tracked in the checklist.
- Split schemas + additive patches are authoritative; the filter manifest represents a reduced allowlist and is not a coverage target.

Spec requirements (normative)

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
- Schema: `input` may be a string or an array of `ItemParam`; array inputs can include tool outputs and do not require a user message item.
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

Upstream acceptance tests (`temp/openresponses/src/pages/compliance.mdx`)
- The upstream project provides acceptance tests that validate API responses against the OpenAPI schema.

README/index/changelog/governance
- High-level positioning and project governance; no additional protocol requirements.

Doc discrepancies (resolved)
- Spec says request bodies MUST be `application/json`, while reference allows `application/x-www-form-urlencoded`. Decision: enforce JSON-only (ADR-0002).

## Additive patch schemas
| schema | purpose | capability owner | coverage evidence |
| --- | --- | --- | --- |
| `InputVideoContent` | Adds `input_video` content blocks to message content unions. | `openresponses.content_union` | covered (bundled OpenAPI) |
| `JsonSchemaResponseFormatParam` | Adds JSON Schema response format support. | `execution.structured_output` | covered (bundled OpenAPI) |
| `TextFormatParam` | Adds `json_schema` format option to text formats. | `execution.output_format` | covered (bundled OpenAPI) |

## CreateResponseBody fields
| field | required | capability owner(s) |
| --- | --- | --- |
| `background` | no | `openresponses.request_fidelity` |
| `conversation` | no | `thread.reference`, `openresponses.request_fidelity` |
| `frequency_penalty` | no | `model.sampling_params` |
| `include` | no | `execution.response_include` |
| `input` | no | `openresponses.content_union`, `openresponses.request_fidelity` |
| `instructions` | no | `context.compile`, `openresponses.request_fidelity` |
| `max_output_tokens` | no | `model.max_output_tokens` |
| `max_tool_calls` | no | `tool.call_limits` |
| `metadata` | no | `openresponses.request_fidelity` |
| `model` | no | `model.select` |
| `parallel_tool_calls` | no | `tool.call_limits` |
| `presence_penalty` | no | `model.sampling_params` |
| `previous_response_id` | no | `session.previous_response` |
| `prompt_cache_key` | no | `model.prompt_cache` |
| `prompt_cache_retention` | no | `model.prompt_cache` |
| `reasoning` | no | `model.thinking_levels`, `openresponses.reasoning_items` |
| `safety_identifier` | no | `policy.request_identifiers` |
| `service_tier` | no | `model.service_tier` |
| `store` | no | `openresponses.request_fidelity` |
| `stream` | no | `execution.json_stream` |
| `stream_options` | no | `execution.stream_options` |
| `temperature` | no | `model.sampling_params` |
| `text` | no | `execution.output_format`, `execution.structured_output` |
| `tool_choice` | no | `tool.choice`, `tool.allowed_tools` |
| `tools` | no | `tool.registry`, `tool.schema`, `openresponses.tools_union` |
| `top_logprobs` | no | `model.logprobs` |
| `top_p` | no | `model.sampling_params` |
| `truncation` | no | `compaction.truncation_policy` |
| `user` | no | `policy.request_identifiers` |

## ResponseResource fields
| field | required | capability owner(s) |
| --- | --- | --- |
| `background` | yes | `openresponses.response_fidelity` |
| `billing` | no | `openresponses.response_fidelity` |
| `completed_at` | yes | `openresponses.response_fidelity` |
| `context_edits` | no | `context.compile`, `openresponses.response_fidelity` |
| `conversation` | no | `thread.reference`, `openresponses.response_fidelity` |
| `cost_token` | no | `openresponses.response_fidelity` |
| `created_at` | yes | `openresponses.response_fidelity` |
| `error` | yes | `openresponses.errors` |
| `frequency_penalty` | yes | `model.sampling_params` |
| `id` | yes | `openresponses.response_fidelity` |
| `incomplete_details` | yes | `openresponses.item_lifecycle`, `openresponses.response_fidelity` |
| `input` | no | `openresponses.response_fidelity` |
| `instructions` | yes | `openresponses.response_fidelity` |
| `max_output_tokens` | yes | `model.max_output_tokens` |
| `max_tool_calls` | yes | `tool.call_limits` |
| `metadata` | yes | `openresponses.response_fidelity` |
| `model` | yes | `model.select` |
| `next_response_ids` | no | `session.previous_response`, `openresponses.response_fidelity` |
| `object` | yes | `openresponses.response_fidelity` |
| `output` | yes | `openresponses.response_fidelity` |
| `parallel_tool_calls` | yes | `tool.call_limits` |
| `presence_penalty` | yes | `model.sampling_params` |
| `previous_response_id` | yes | `session.previous_response` |
| `prompt_cache_key` | yes | `model.prompt_cache` |
| `prompt_cache_retention` | no | `model.prompt_cache` |
| `reasoning` | yes | `model.thinking_levels`, `openresponses.reasoning_items` |
| `safety_identifier` | yes | `policy.request_identifiers` |
| `service_tier` | yes | `model.service_tier` |
| `status` | yes | `openresponses.item_lifecycle`, `openresponses.response_fidelity` |
| `store` | yes | `openresponses.response_fidelity` |
| `temperature` | yes | `model.sampling_params` |
| `text` | yes | `execution.output_format`, `execution.structured_output` |
| `tool_choice` | yes | `tool.choice`, `tool.allowed_tools` |
| `tools` | yes | `tool.registry`, `tool.schema`, `openresponses.tools_union` |
| `top_logprobs` | yes | `model.logprobs` |
| `top_p` | yes | `model.sampling_params` |
| `truncation` | yes | `compaction.truncation_policy` |
| `usage` | yes | `usage.token_counts` |
| `user` | yes | `policy.request_identifiers` |

## Tool param variants (ResponsesToolParam)
| tool type | schema | capability owner(s) | coverage evidence |
| --- | --- | --- | --- |
| `function` | `FunctionToolParam.json` | `openresponses.tools_union`, `tool.schema` | covered (request validation) |
| `code_interpreter` | `CodeInterpreterToolParam.json` | `openresponses.tools_union`, `tool.schema` | covered (request validation) |
| `custom` | `CustomToolParam.json` | `openresponses.tools_union`, `tool.schema` | covered (request validation) |
| `web_search` | `WebSearchToolParam.json` | `openresponses.tools_union`, `tool.schema` | covered (request validation) |
| `web_search_2025_08_26` | `WebSearchToolParam_2025_08_14Param.json` | `openresponses.tools_union`, `tool.schema` | covered (request validation) |
| `web_search_ga` | `WebSearchGADeprecatedToolParam.json` | `openresponses.tools_union`, `tool.schema` | covered (request validation) |
| `web_search_preview` | `WebSearchPreviewToolParam.json` | `openresponses.tools_union`, `tool.schema` | covered (request validation) |
| `web_search_preview_2025_03_11` | `WebSearchPreviewToolParam_2025_03_11Param.json` | `openresponses.tools_union`, `tool.schema` | covered (request validation) |
| `image_generation` | `ImageGenToolParam.json` | `openresponses.tools_union`, `tool.schema` | covered (request validation) |
| `mcp` | `MCPToolParam.json` | `openresponses.tools_union`, `tool.schema` | covered (request validation) |
| `file_search` | `FileSearchToolParam.json` | `openresponses.tools_union`, `tool.schema` | covered (request validation) |
| `computer-preview` | `ComputerToolParam.json` | `openresponses.tools_union`, `tool.schema` | covered (request validation) |
| `computer_use_preview` | `ComputerUsePreviewToolParam.json` | `openresponses.tools_union`, `tool.schema` | covered (request validation) |
| `local_shell` | `LocalShellToolParam.json` | `openresponses.tools_union`, `tool.schema` | covered (request validation) |
| `shell` | `FunctionShellToolParam.json` | `openresponses.tools_union`, `tool.schema` | covered (request validation) |
| `apply_patch` | `ApplyPatchToolParam.json` | `openresponses.tools_union`, `tool.schema` | covered (request validation) |

### Tool param required fields
| schema | required fields | capability owner(s) | notes |
| --- | --- | --- | --- |
| `CodeInterpreterToolParam.json` | `type`, `container` | `openresponses.tools_union` | `container` is string or `AutoCodeInterpreterToolParam` |
| `FunctionToolParam.json` | `type`, `name` | `openresponses.tools_union` |  |
| `CustomToolParam.json` | `type`, `name` | `openresponses.tools_union` |  |
| `WebSearchToolParam.json` | `type` | `openresponses.tools_union` |  |
| `WebSearchToolParam_2025_08_14Param.json` | `type` | `openresponses.tools_union` |  |
| `WebSearchGADeprecatedToolParam.json` | `type` | `openresponses.tools_union` |  |
| `WebSearchPreviewToolParam.json` | `type` | `openresponses.tools_union` |  |
| `WebSearchPreviewToolParam_2025_03_11Param.json` | `type` | `openresponses.tools_union` |  |
| `ImageGenToolParam.json` | `type` | `openresponses.tools_union` |  |
| `MCPToolParam.json` | `type`, `server_label` | `openresponses.tools_union` |  |
| `FileSearchToolParam.json` | `type`, `vector_store_ids` | `openresponses.tools_union` |  |
| `ComputerToolParam.json` | `type`, `display_width`, `display_height`, `environment` | `openresponses.tools_union` | `environment` is `ComputerEnvironment` |
| `ComputerUsePreviewToolParam.json` | `type`, `display_width`, `display_height`, `environment` | `openresponses.tools_union` | `environment` is `ComputerEnvironment` |
| `LocalShellToolParam.json` | `type` | `openresponses.tools_union` |  |
| `FunctionShellToolParam.json` | `type` | `openresponses.tools_union` |  |
| `ApplyPatchToolParam.json` | `type` | `openresponses.tools_union` |  |

## Tool choice variants
| ToolChoiceParam variant | schema | capability owner(s) | coverage evidence |
| --- | --- | --- | --- |
| value enum | `ToolChoiceValueEnum.json` | `tool.choice` | covered (request validation) |
| allowed tools | `AllowedToolsParam.json` | `tool.allowed_tools` | covered (request validation) |
| specific tool | `SpecificToolChoiceParam.json` | `tool.choice` | covered (request validation) |

### Specific tool choices (SpecificToolChoiceParam)
| tool type | schema | required fields | capability owner(s) | coverage evidence |
| --- | --- | --- | --- | --- |
| `file_search` | `SpecificFileSearchParam.json` | `type` | `tool.choice` | covered (request validation) |
| `web_search` | `SpecificWebSearchParam.json` | `type` | `tool.choice` | covered (request validation) |
| `web_search_preview` | `SpecificWebSearchPreviewParam.json` | `type` | `tool.choice` | covered (request validation) |
| `image_generation` | `SpecificImageGenParam.json` | `type` | `tool.choice` | covered (request validation) |
| `computer-preview` | `SpecificComputerParam.json` | `type` | `tool.choice` | covered (request validation) |
| `computer_use_preview` | `SpecificComputerPreviewParam.json` | `type` | `tool.choice` | covered (request validation) |
| `code_interpreter` | `SpecificCodeInterpreterParam.json` | `type` | `tool.choice` | covered (request validation) |
| `function` | `SpecificFunctionParam.json` | `type`, `name` | `tool.choice` | covered (request validation) |
| `mcp` | `SpecificMCPFunctionParam.json` | `type`, `server_label` | `tool.choice` | covered (request validation) |
| `local_shell` | `SpecificLocalShellParam.json` | `type` | `tool.choice` | covered (request validation) |
| `shell` | `SpecificFunctionShellParam.json` | `type` | `tool.choice` | covered (request validation) |
| `custom` | `SpecificCustomToolParam.json` | `type`, `name` | `tool.choice` | covered (request validation) |
| `apply_patch` | `SpecificApplyPatchParam.json` | `type` | `tool.choice` | covered (request validation) |

## Error schemas
| schema | required fields | notes |
| --- | --- | --- |
| `Error.json` | `code`, `message` | base error payload (no `type` field) |
| `ErrorPayload.json` | `type`, `code`, `message`, `param` | `type` is freeform string |
| `HTTPError.json` | `type`, `code`, `message` | `type` enum: `http_error` |
| `MCPProtocolError.json` | `type`, `code`, `message` | `type` enum: `mcp_protocol_error` |
| `MCPToolExecutionError.json` | `type`, `content` | `type` enum: `mcp_tool_execution_error` |

## Streaming events (SSE)
Capability owner: `openresponses.streaming_fidelity`.

Internal frames
- Every SSE event is emitted as a `provider_event` frame (payload preserved, unknown-safe).
- `response.output_text.delta` also emits a derived `output_text_delta` frame (no payload loss; full event remains in `provider_event`).

| event type | schema | internal frames |
| --- | --- | --- |
| `error` | `ErrorStreamingEvent.json` | provider_event |
| `image_edit.completed` | `ImageEditCompletedStreamingEvent.json` | provider_event |
| `image_edit.partial_image` | `ImageEditPartialImageStreamingEvent.json` | provider_event |
| `image_generation.completed` | `ImageGenerationCompletedStreamingEvent.json` | provider_event |
| `image_generation.partial_image` | `ImageGenerationPartialImageStreamingEvent.json` | provider_event |
| `response.apply_patch_call_operation_diff.delta` | `ResponseApplyPatchCallOperationDiffDeltaStreamingEvent.json` | provider_event |
| `response.apply_patch_call_operation_diff.done` | `ResponseApplyPatchCallOperationDiffDoneStreamingEvent.json` | provider_event |
| `response.code_interpreter_call.completed` | `ResponseCodeInterpreterCallCompletedStreamingEvent.json` | provider_event |
| `response.code_interpreter_call.in_progress` | `ResponseCodeInterpreterCallInProgressStreamingEvent.json` | provider_event |
| `response.code_interpreter_call.interpreting` | `ResponseCodeInterpreterCallInterpretingStreamingEvent.json` | provider_event |
| `response.code_interpreter_call_code.delta` | `ResponseCodeInterpreterCallCodeDeltaStreamingEvent.json` | provider_event |
| `response.code_interpreter_call_code.done` | `ResponseCodeInterpreterCallCodeDoneStreamingEvent.json` | provider_event |
| `response.completed` | `ResponseCompletedStreamingEvent.json` | provider_event |
| `response.content_part.added` | `ResponseContentPartAddedStreamingEvent.json` | provider_event |
| `response.content_part.done` | `ResponseContentPartDoneStreamingEvent.json` | provider_event |
| `response.created` | `ResponseCreatedStreamingEvent.json` | provider_event |
| `response.custom_tool_call_input.delta` | `ResponseCustomToolCallInputDeltaStreamingEvent.json` | provider_event |
| `response.custom_tool_call_input.done` | `ResponseCustomToolCallInputDoneStreamingEvent.json` | provider_event |
| `response.failed` | `ResponseFailedStreamingEvent.json` | provider_event |
| `response.file_search_call.completed` | `ResponseFileSearchCallCompletedStreamingEvent.json` | provider_event |
| `response.file_search_call.in_progress` | `ResponseFileSearchCallInProgressStreamingEvent.json` | provider_event |
| `response.file_search_call.searching` | `ResponseFileSearchCallSearchingStreamingEvent.json` | provider_event |
| `response.function_call_arguments.delta` | `ResponseFunctionCallArgumentsDeltaStreamingEvent.json` | provider_event |
| `response.function_call_arguments.done` | `ResponseFunctionCallArgumentsDoneStreamingEvent.json` | provider_event |
| `response.image_generation_call.completed` | `ResponseImageGenCallCompletedStreamingEvent.json` | provider_event |
| `response.image_generation_call.generating` | `ResponseImageGenCallGeneratingStreamingEvent.json` | provider_event |
| `response.image_generation_call.in_progress` | `ResponseImageGenCallInProgressStreamingEvent.json` | provider_event |
| `response.image_generation_call.partial_image` | `ResponseImageGenCallPartialImageStreamingEvent.json` | provider_event |
| `response.in_progress` | `ResponseInProgressStreamingEvent.json` | provider_event |
| `response.incomplete` | `ResponseIncompleteStreamingEvent.json` | provider_event |
| `response.mcp_call.completed` | `ResponseMCPCallCompletedStreamingEvent.json` | provider_event |
| `response.mcp_call.failed` | `ResponseMCPCallFailedStreamingEvent.json` | provider_event |
| `response.mcp_call.in_progress` | `ResponseMCPCallInProgressStreamingEvent.json` | provider_event |
| `response.mcp_call_arguments.delta` | `ResponseMCPCallArgumentsDeltaStreamingEvent.json` | provider_event |
| `response.mcp_call_arguments.done` | `ResponseMCPCallArgumentsDoneStreamingEvent.json` | provider_event |
| `response.mcp_list_tools.completed` | `ResponseMCPListToolsCompletedStreamingEvent.json` | provider_event |
| `response.mcp_list_tools.failed` | `ResponseMCPListToolsFailedStreamingEvent.json` | provider_event |
| `response.mcp_list_tools.in_progress` | `ResponseMCPListToolsInProgressStreamingEvent.json` | provider_event |
| `response.output_item.added` | `ResponseOutputItemAddedStreamingEvent.json` | provider_event |
| `response.output_item.done` | `ResponseOutputItemDoneStreamingEvent.json` | provider_event |
| `response.output_text.annotation.added` | `ResponseOutputTextAnnotationAddedStreamingEvent.json` | provider_event |
| `response.output_text.delta` | `ResponseOutputTextDeltaStreamingEvent.json` | provider_event + output_text_delta |
| `response.output_text.done` | `ResponseOutputTextDoneStreamingEvent.json` | provider_event |
| `response.queued` | `ResponseQueuedStreamingEvent.json` | provider_event |
| `response.reasoning.delta` | `ResponseReasoningDeltaStreamingEvent.json` | provider_event |
| `response.reasoning.done` | `ResponseReasoningDoneStreamingEvent.json` | provider_event |
| `response.reasoning_summary_part.added` | `ResponseReasoningSummaryPartAddedStreamingEvent.json` | provider_event |
| `response.reasoning_summary_part.done` | `ResponseReasoningSummaryPartDoneStreamingEvent.json` | provider_event |
| `response.reasoning_summary_text.delta` | `ResponseReasoningSummaryDeltaStreamingEvent.json` | provider_event |
| `response.reasoning_summary_text.done` | `ResponseReasoningSummaryDoneStreamingEvent.json` | provider_event |
| `response.refusal.delta` | `ResponseRefusalDeltaStreamingEvent.json` | provider_event |
| `response.refusal.done` | `ResponseRefusalDoneStreamingEvent.json` | provider_event |
| `response.shell_call_command.added` | `ResponseShellCallCommandAddedStreamingEvent.json` | provider_event |
| `response.shell_call_command.delta` | `ResponseShellCallCommandDeltaStreamingEvent.json` | provider_event |
| `response.shell_call_command.done` | `ResponseShellCallCommandDoneStreamingEvent.json` | provider_event |
| `response.web_search_call.completed` | `ResponseWebSearchCallCompletedStreamingEvent.json` | provider_event |
| `response.web_search_call.in_progress` | `ResponseWebSearchCallInProgressStreamingEvent.json` | provider_event |
| `response.web_search_call.searching` | `ResponseWebSearchCallSearchingStreamingEvent.json` | provider_event |

## Input item variants
Capability owner: `openresponses.request_fidelity` + `openresponses.content_union` + `openresponses.tools_union`.

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
Capability owner: `openresponses.response_fidelity` + `openresponses.item_lifecycle` + `openresponses.tools_union`.

| item type | schema | mapping |
| --- | --- | --- |
| `apply_patch_call` | `ApplyPatchToolCall.json` | provider_event |
| `apply_patch_call_output` | `ApplyPatchToolCallOutput.json` | provider_event |
| `code_interpreter_call` | `CodeInterpreterCall.json` | provider_event |
| `compaction` | `CompactionBody.json` | provider_event |
| `computer_call` | `ComputerCall.json` | provider_event |
| `computer_call_output` | `ComputerCallOutput.json` | provider_event |
| `custom_tool_call` | `CustomToolCall.json` | provider_event |
| `custom_tool_call_output` | `CustomToolCallOutput.json` | provider_event |
| `file_search_call` | `FileSearchCall.json` | provider_event |
| `function_call` | `FunctionCall.json` | provider_event |
| `function_call_output` | `FunctionCallOutput.json` | provider_event |
| `image_generation_call` | `ImageGenCall.json` | provider_event |
| `local_shell_call` | `LocalShellCall.json` | provider_event |
| `local_shell_call_output` | `LocalShellCallOutput.json` | provider_event |
| `mcp_approval_request` | `MCPApprovalRequest.json` | provider_event |
| `mcp_approval_response` | `MCPApprovalResponse.json` | provider_event |
| `mcp_call` | `MCPToolCall.json` | provider_event |
| `mcp_list_tools` | `MCPListTools.json` | provider_event |
| `message` | `Message.json` | provider_event |
| `reasoning` | `ReasoningBody.json` | provider_event |
| `shell_call` | `FunctionShellCall.json` | provider_event |
| `shell_call_output` | `FunctionShellCallOutput.json` | provider_event |
| `web_search_call` | `WebSearchCall.json` | provider_event |

## Schema index (quality-gate coverage)

This list is exhaustive and drives the task tracker in `docs/07_tasks/openresponses_coverage.md`.

Legend
- `bundled`: schema is present in `schemas/openresponses/openapi.json`.
- `covered`: schema is reachable from split streaming-event or ResponseResource validation (request-side coverage is noted in `status`).
- `status`: mapping status in current codebase.

### Error schemas
| schema | bundled | covered | status |
| --- | --- | --- | --- |
| `Error` | yes | yes | provider_event |
| `ErrorPayload` | yes | yes | provider_event |
| `HTTPError` | no | yes | provider_event (validated) |
| `MCPProtocolError` | no | yes | provider_event (validated) |
| `MCPToolExecutionError` | no | yes | provider_event (validated) |

### Input item params
| schema | bundled | covered | status |
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
| schema | bundled | covered | status |
| --- | --- | --- | --- |
| `AllowedToolsParam` | yes | no | provider_request (validated) |
| `Annotation` | yes | yes | provider_event (validated) |
| `ApiSourceParam` | no | yes | provider_request (validated) |
| `ApplyPatchCallOutputStatus` | no | yes | provider_event (validated) |
| `ApplyPatchCallOutputStatusParam` | no | no | provider_request (validated) |
| `ApplyPatchCallStatus` | no | yes | provider_event (validated) |
| `ApplyPatchCallStatusParam` | no | no | provider_request (validated) |
| `ApplyPatchCreateFileOperation` | no | yes | provider_event (validated) |
| `ApplyPatchCreateFileOperationParam` | no | no | provider_request (validated) |
| `ApplyPatchDeleteFileOperation` | no | yes | provider_event (validated) |
| `ApplyPatchDeleteFileOperationParam` | no | no | provider_request (validated) |
| `ApplyPatchOperationParam` | no | no | provider_request (validated) |
| `ApplyPatchToolCall` | no | yes | provider_event (validated) |
| `ApplyPatchToolCallOutput` | no | yes | provider_event (validated) |
| `ApplyPatchUpdateFileOperation` | no | yes | provider_event (validated) |
| `ApplyPatchUpdateFileOperationParam` | no | no | provider_request (validated) |
| `ApproximateLocation` | no | yes | provider_event (validated) |
| `ApproximateLocationParam` | no | yes | provider_request (validated) |
| `Billing` | no | yes | provider_event (validated) |
| `ClickAction` | no | yes | provider_event (validated) |
| `ClickButtonType` | no | yes | provider_request (validated) |
| `ClickParam` | no | yes | provider_request (validated) |
| `CodeInterpreterCall` | no | yes | provider_event (validated) |
| `CodeInterpreterCallStatus` | no | yes | provider_event (validated) |
| `CodeInterpreterOutputImage` | no | yes | provider_event (validated) |
| `CodeInterpreterOutputLogs` | no | yes | provider_event (validated) |
| `CodeInterpreterToolCallOutputImageParam` | no | no | provider_request (validated) |
| `CodeInterpreterToolCallOutputLogsParam` | no | no | provider_request (validated) |
| `CompactResource` | no | no | provider_event (validated) |
| `CompactResponseMethodPublicBody` | no | no | provider_request (validated) |
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
| `ComputerCallOutputStatus` | no | yes | provider_event (validated) |
| `ComputerCallSafetyCheckParam` | no | yes | provider_request (validated) |
| `ComputerEnvironment` | no | yes | provider_request (validated) |
| `ComputerEnvironment1` | no | yes | provider_event (validated) |
| `ComputerScreenshotContent` | no | yes | provider_event (validated) |
| `ComputerScreenshotParam` | no | yes | provider_request (validated) |
| `ContainerFileCitationBody` | no | yes | provider_event (validated) |
| `ContainerFileCitationParam` | no | yes | provider_request (validated) |
| `ContainerMemoryLimit` | no | no | provider_request (validated) |
| `ContextEdit` | no | yes | provider_event (validated) |
| `ContextEditDetails` | no | yes | provider_event (validated) |
| `Conversation` | no | yes | provider_event (validated) |
| `ConversationParam` | no | yes | provider_request (validated) |
| `CoordParam` | no | yes | provider_request (validated) |
| `CreateImageBody15Param` | no | no | provider_request (validated) |
| `CreateImageBody1MiniParam` | no | no | provider_request (validated) |
| `CreateImageBody1Param` | no | no | provider_request (validated) |
| `CreateImageBodyChatGPTImageLatestParam` | no | no | provider_request (validated) |
| `CreateVideoBody` | no | no | provider_request (validated) |
| `CreateVideoRemixBody` | no | no | provider_request (validated) |
| `CustomGrammarFormatField` | no | yes | provider_request (validated) |
| `CustomGrammarFormatParam` | no | no | provider_request (validated) |
| `CustomTextFormatField` | no | yes | provider_request (validated) |
| `CustomTextFormatParam` | no | no | provider_request (validated) |
| `CustomToolCall` | no | yes | provider_event (validated) |
| `CustomToolCallOutput` | no | yes | provider_event (validated) |
| `CustomToolFormat` | no | yes | provider_request (validated) |
| `DeletedResponseResource` | no | no | provider_event (validated) |
| `DeletedVideoResource` | no | no | provider_event (validated) |
| `DetailEnum` | yes | yes | provider_request (validated) |
| `DoubleClickAction` | no | yes | provider_event (validated) |
| `DoubleClickParam` | no | yes | provider_request (validated) |
| `DragAction` | no | yes | provider_event (validated) |
| `DragParam` | no | yes | provider_request (validated) |
| `DragPoint` | no | yes | provider_event (validated) |
| `EditImageBody15Param` | no | no | provider_request (validated) |
| `EditImageBody1MiniParam` | no | no | provider_request (validated) |
| `EditImageBody1Param` | no | no | provider_request (validated) |
| `EditImageBodyChatGPTImageLatestParam` | no | no | provider_request (validated) |
| `EditsBodyDallE2Param` | no | no | provider_request (validated) |
| `EmptyAction` | no | yes | provider_event (validated) |
| `EmptyModelParam` | yes | no | provider_request (validated) |
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
| `FunctionShellAction` | no | yes | provider_event (validated) |
| `FunctionShellActionParam` | no | no | provider_request (validated) |
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
| `GenerationsBodyDallE2Param` | no | no | provider_request (validated) |
| `GenerationsBodyDallE3Param` | no | no | provider_request (validated) |
| `GrammarSyntax` | no | no | provider_request (validated) |
| `GrammarSyntax1` | no | yes | provider_request (validated) |
| `HybridSearchOptions` | no | yes | provider_event (validated) |
| `HybridSearchOptionsParam` | no | yes | provider_request (validated) |
| `Image` | no | no | provider_event (validated) |
| `ImageBackground` | no | yes | provider_request (validated) |
| `ImageDetail` | yes | yes | provider_request (validated) |
| `ImageGenAction` | no | yes | provider_request (validated) |
| `ImageGenActionEnum` | no | no | provider_request (validated) |
| `ImageGenCall` | no | yes | provider_event (validated) |
| `ImageGenCallStatus` | no | yes | provider_event (validated) |
| `ImageGenInputUsageDetails` | no | no | provider_event (validated) |
| `ImageGenOutputTokensDetails` | no | no | provider_event (validated) |
| `ImageGenToolModel` | no | yes | provider_request (validated) |
| `ImageGenUsage` | no | no | provider_event (validated) |
| `ImageModeration` | no | yes | provider_request (validated) |
| `ImageOutputFormat` | no | yes | provider_request (validated) |
| `ImageQuality` | no | yes | provider_request (validated) |
| `ImageQualityDallE` | no | no | provider_request (validated) |
| `ImageResource` | no | no | provider_event (validated) |
| `ImageSize` | no | yes | provider_request (validated) |
| `ImageSizeDallE2` | no | no | provider_request (validated) |
| `ImageSizeDallE3` | no | no | provider_request (validated) |
| `ImageStyleDallE` | no | no | provider_request (validated) |
| `ImageUsage` | no | yes | provider_event (validated) |
| `ImageUsageInputTokensDetails` | no | yes | provider_event (validated) |
| `ImageUsageOutputTokensDetails` | no | yes | provider_event (validated) |
| `IncludeEnum` | yes | no | provider_request (validated) |
| `IncompleteDetails` | yes | yes | provider_event (validated) |
| `InputFidelity` | no | no | provider_request (validated) |
| `InputFileContent` | yes | yes | provider_event (validated) |
| `InputFileContentParam` | yes | no | provider_request (validated) |
| `InputImageContent` | yes | yes | provider_event (validated) |
| `InputImageContentParamAutoParam` | yes | no | provider_request (validated) |
| `InputImageMaskContentParam` | no | no | provider_request (validated) |
| `InputTextContent` | yes | yes | provider_event (validated) |
| `InputTextContentParam` | yes | no | provider_request (validated) |
| `InputTokensDetails` | yes | yes | provider_event (validated) |
| `ItemListResource` | no | no | provider_event (validated) |
| `ItemReferenceParam` | yes | no | provider_request (validated) |
| `JsonObjectResponseFormat` | yes | yes | provider_event (validated) |
| `JsonSchemaResponseFormat` | yes | yes | provider_event (validated) |
| `KeyPressAction` | no | yes | provider_event (validated) |
| `KeyPressParam` | no | yes | provider_request (validated) |
| `LocalFileEnvironmentParam` | no | no | provider_request (validated) |
| `LocalShellCall` | no | yes | provider_event (validated) |
| `LocalShellCallItemStatus` | no | no | provider_event (validated) |
| `LocalShellCallOutput` | no | yes | provider_event (validated) |
| `LocalShellCallOutputStatusEnum` | no | yes | provider_event (validated) |
| `LocalShellCallStatus` | no | yes | provider_event (validated) |
| `LocalShellExecAction` | no | yes | provider_event (validated) |
| `LocalShellExecActionParam` | no | no | provider_request (validated) |
| `LogProb` | yes | yes | provider_event (validated) |
| `MCPApprovalRequest` | no | yes | provider_event (validated) |
| `MCPApprovalResponse` | no | yes | provider_event (validated) |
| `MCPListTools` | no | yes | provider_event (validated) |
| `MCPRequireApprovalApiEnum` | no | no | provider_request (validated) |
| `MCPRequireApprovalFieldEnum` | no | yes | provider_event (validated) |
| `MCPRequireApprovalFilterField` | no | yes | provider_event (validated) |
| `MCPRequireApprovalFilterParam` | no | no | provider_request (validated) |
| `MCPToolCall` | no | yes | provider_event (validated) |
| `MCPToolCallStatus` | no | yes | provider_event (validated) |
| `MCPToolFilterField` | no | yes | provider_event (validated) |
| `MCPToolFilterParam` | no | no | provider_request (validated) |
| `Message` | yes | yes | provider_event (validated) |
| `MessageRole` | yes | yes | provider_event (validated) |
| `MessageRole1` | no | no | provider_event (validated) |
| `MessageStatus` | yes | yes | provider_event (validated) |
| `MetadataParam` | yes | no | provider_request (validated) |
| `MoveAction` | no | yes | provider_event (validated) |
| `MoveParam` | no | yes | provider_request (validated) |
| `OrderEnum` | no | no | provider_request (validated) |
| `OutputTextContent` | yes | yes | provider_event (validated) |
| `OutputTextContentParam` | yes | no | provider_request (validated) |
| `OutputTokensDetails` | yes | yes | provider_event (validated) |
| `Payer` | no | yes | provider_event (validated) |
| `PromptCacheRetentionEnum` | no | yes | provider_request (validated) |
| `PromptInstructionMessage` | no | yes | provider_request (validated) |
| `RankerVersionType` | no | yes | provider_request (validated) |
| `RankingOptions` | no | yes | provider_request (validated) |
| `Reasoning` | yes | yes | provider_event (validated) |
| `ReasoningBody` | yes | yes | provider_event (validated) |
| `ReasoningEffortEnum` | yes | yes | provider_request (validated) |
| `ReasoningParam` | yes | no | provider_request (validated) |
| `ReasoningSummaryContentParam` | yes | no | provider_request (validated) |
| `ReasoningSummaryEnum` | yes | yes | provider_request (validated) |
| `ReasoningTextContent` | yes | yes | provider_event (validated) |
| `RefusalContent` | yes | yes | provider_event (validated) |
| `RefusalContentParam` | yes | no | provider_request (validated) |
| `SafetyCheck` | no | yes | provider_event (validated) |
| `ScreenshotAction` | no | yes | provider_event (validated) |
| `ScreenshotParam` | no | yes | provider_request (validated) |
| `ScrollAction` | no | yes | provider_event (validated) |
| `ScrollParam` | no | yes | provider_request (validated) |
| `SearchContextSize` | no | yes | provider_request (validated) |
| `ServiceTierEnum` | yes | no | provider_request (validated) |
| `SpecificApplyPatchParam` | no | no | provider_request (validated) |
| `SpecificCodeInterpreterParam` | no | no | provider_request (validated) |
| `SpecificComputerParam` | no | no | provider_request (validated) |
| `SpecificComputerPreviewParam` | no | no | provider_request (validated) |
| `SpecificFileSearchParam` | no | no | provider_request (validated) |
| `SpecificFunctionParam` | yes | no | provider_request (validated) |
| `SpecificFunctionShellParam` | no | no | provider_request (validated) |
| `SpecificImageGenParam` | no | no | provider_request (validated) |
| `SpecificLocalShellParam` | no | no | provider_request (validated) |
| `SpecificMCPFunctionParam` | no | no | provider_request (validated) |
| `SpecificToolChoiceParam` | yes | no | provider_request (validated) |
| `SpecificWebSearchParam` | no | no | provider_request (validated) |
| `SpecificWebSearchPreviewParam` | no | no | provider_request (validated) |
| `StreamOptionsParam` | yes | no | provider_request (validated) |
| `SummaryTextContent` | yes | yes | provider_event (validated) |
| `TextContent` | yes | yes | provider_event (validated) |
| `TextField` | yes | yes | provider_event (validated) |
| `TextParam` | yes | no | provider_request (validated) |
| `TextResponseFormat` | yes | yes | provider_event (validated) |
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
| `UrlSourceParam` | no | no | provider_request (validated) |
| `Usage` | yes | yes | provider_event (validated) |
| `VerbosityEnum` | yes | yes | provider_request (validated) |
| `VideoContentVariant` | no | no | provider_event (validated) |
| `VideoListResource` | no | no | provider_event (validated) |
| `VideoModel` | no | no | provider_request (validated) |
| `VideoResource` | no | no | provider_event (validated) |
| `VideoSeconds` | no | no | provider_request (validated) |
| `VideoSize` | no | no | provider_request (validated) |
| `VideoStatus` | no | no | provider_event (validated) |
| `WaitAction` | no | yes | provider_event (validated) |
| `WaitParam` | no | yes | provider_request (validated) |
| `WebSearchCall` | no | yes | provider_event (validated) |
| `WebSearchCallActionFindInPage` | no | yes | provider_event (validated) |
| `WebSearchCallActionFindInPageParam` | no | no | provider_request (validated) |
| `WebSearchCallActionOpenPage` | no | yes | provider_event (validated) |
| `WebSearchCallActionOpenPageParam` | no | no | provider_request (validated) |
| `WebSearchCallActionSearch` | no | yes | provider_event (validated) |
| `WebSearchCallActionSearchParam` | no | no | provider_request (validated) |
| `WebSearchCallStatus` | no | yes | provider_event (validated) |
| `WebSearchPreviewToolParam_2025_03_11Param` | no | no | provider_request (validated) |
| `WebSearchToolParam_2025_08_14Param` | no | no | provider_request (validated) |

### Output item fields
| schema | bundled | covered | status |
| --- | --- | --- | --- |
| `ItemField` | yes | yes | provider_event |

### Request-related schemas
| schema | bundled | covered | status |
| --- | --- | --- | --- |
| `CreateResponseBody` | yes | no | provider_request (validated) |

### Response-related schemas
| schema | bundled | covered | status |
| --- | --- | --- | --- |
| `ResponseFormatDallE` | no | no | provider_request (validated) |
| `ResponseResource` | yes | yes | provider_event |
| `ResponsesConversationParam` | no | no | provider_request (validated) |

### Streaming events
| schema | bundled | covered | status |
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
| schema | bundled | covered | status |
| --- | --- | --- | --- |
| `AllowedToolChoice` | yes | yes | provider_request (validated) |
| `ApplyPatchTool` | no | yes | provider_request (validated) |
| `ApplyPatchToolChoice` | no | yes | provider_request (validated) |
| `ApplyPatchToolParam` | no | no | provider_request (validated) |
| `AutoCodeInterpreterToolParam` | no | no | provider_request (validated) |
| `CodeInterpreterToolChoice` | no | yes | provider_request (validated) |
| `CodeInterpreterToolParam` | no | no | provider_request (validated) |
| `ComputerToolChoice` | no | yes | provider_request (validated) |
| `ComputerToolParam` | no | no | provider_request (validated) |
| `ComputerUsePreviewTool` | no | yes | provider_request (validated) |
| `ComputerUsePreviewToolParam` | no | no | provider_request (validated) |
| `CustomTool` | no | yes | provider_request (validated) |
| `CustomToolChoice` | no | yes | provider_request (validated) |
| `CustomToolParam` | no | no | provider_request (validated) |
| `FileSearchTool` | no | yes | provider_request (validated) |
| `FileSearchToolChoice` | no | yes | provider_request (validated) |
| `FileSearchToolParam` | no | no | provider_request (validated) |
| `FunctionShellTool` | no | yes | provider_request (validated) |
| `FunctionShellToolChoice` | no | yes | provider_request (validated) |
| `FunctionShellToolParam` | no | no | provider_request (validated) |
| `FunctionTool` | yes | yes | provider_request (validated) |
| `FunctionToolChoice` | yes | yes | provider_request (validated) |
| `FunctionToolParam` | yes | no | provider_request (validated) |
| `ImageGenTool` | no | yes | provider_request (validated) |
| `ImageGenToolChoice` | no | yes | provider_request (validated) |
| `ImageGenToolParam` | no | no | provider_request (validated) |
| `LocalShellToolChoice` | no | yes | provider_request (validated) |
| `LocalShellToolParam` | no | no | provider_request (validated) |
| `MCPListToolsTool` | no | yes | provider_request (validated) |
| `MCPTool` | no | yes | provider_request (validated) |
| `MCPToolChoice` | no | yes | provider_request (validated) |
| `MCPToolParam` | no | no | provider_request (validated) |
| `MemoryToolParam` | no | no | provider_request (validated) |
| `ResponsesToolParam` | yes | no | provider_request (validated) |
| `SpecificCustomToolParam` | no | no | provider_request (validated) |
| `Tool` | yes | yes | provider_request (validated) |
| `WebSearchGADeprecatedToolParam` | no | no | provider_request (validated) |
| `WebSearchPreviewTool` | no | yes | provider_request (validated) |
| `WebSearchPreviewToolParam` | no | no | provider_request (validated) |
| `WebSearchToolChoice` | no | yes | provider_request (validated) |
| `WebSearchToolParam` | no | no | provider_request (validated) |
