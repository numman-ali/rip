# OpenResponses Compliance Map

Summary
- Source repo: `temp/openresponses` (local vendor).
- Reviewed: 2026-01-17.
- Schema files: 412 total components in `temp/openresponses/schema/components/schemas`.
- Streaming event schemas: 58 (from `temp/openresponses/schema/paths/responses.json`).
- Input item variants: 25 (from `ItemParam.json`).
- Output item variants: 23 (from `ItemField.json`).

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

Mapping rules (Phase 1)
- All OpenResponses SSE events map to `provider_event` frames with full payload fidelity.
- Internal frames are emitted for a subset (session + text/tool deltas); all other events remain provider-only until explicitly promoted.
- No OpenResponses fields/events are dropped at the provider boundary.

Implementation status (current)
- Provider adapter validates streaming events and embedded `response` objects against the bundled OpenAPI schema (`schemas/openresponses/openapi.json`).
- Request bodies are not yet validated; CreateResponseBody fields are mapped only when provider request building is implemented.
- Input item variants are not yet mapped to internal request frames (pending).
- Bundled OpenAPI schema currently includes 102 component schemas; the split OpenResponses schema defines 412 component schemas. Missing schemas are tracked in the checklist.

Doc review notes (normative requirements)

Specification (`temp/openresponses/src/pages/specification.mdx`)
- Requests MUST use HTTP with `Authorization` + `Content-Type` headers; request bodies MUST be `application/json`.
- Non-stream responses MUST be `application/json`; streams MUST be `text/event-stream` with terminal `[DONE]`.
- SSE `event` field MUST match payload `type`; servers SHOULD NOT use `id`.
- Items are state machines with `in_progress`, `incomplete`, `completed`; `incomplete` is terminal and MUST be the last item, and response MUST be `incomplete`.
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

Reference (`temp/openresponses/src/pages/reference.mdx`)
- `/v1/responses` request bodies are documented as JSON or form-encoded; response bodies as JSON or SSE.

Compliance (`temp/openresponses/src/pages/compliance.mdx`)
- Acceptance tests validate API responses against the OpenAPI schema.

README/index/changelog/governance
- High-level positioning and project governance; no additional protocol requirements.

Doc discrepancies to resolve
- Spec says request bodies MUST be `application/json`, while reference allows `application/x-www-form-urlencoded`. Decide canonical behavior and document.

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

## Streaming events (SSE)
| event type | schema | mapping |
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
| `response.output_text.delta` | `ResponseOutputTextDeltaStreamingEvent.json` | provider_event |
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
| item type | schema | mapping |
| --- | --- | --- |
| `apply_patch_call` | `ApplyPatchToolCallItemParam.json` | pending (request mapping) |
| `apply_patch_call_output` | `ApplyPatchToolCallOutputItemParam.json` | pending (request mapping) |
| `code_interpreter_call` | `CodeInterpreterCallItemParam.json` | pending (request mapping) |
| `compaction` | `CompactionSummaryItemParam.json` | pending (request mapping) |
| `computer_call` | `ComputerCallItemParam.json` | pending (request mapping) |
| `computer_call_output` | `ComputerCallOutputItemParam.json` | pending (request mapping) |
| `custom_tool_call` | `CustomToolCallItemParam.json` | pending (request mapping) |
| `custom_tool_call_output` | `CustomToolCallOutputItemParam.json` | pending (request mapping) |
| `file_search_call` | `FileSearchCallItemParam.json` | pending (request mapping) |
| `function_call` | `FunctionCallItemParam.json` | pending (request mapping) |
| `function_call_output` | `FunctionCallOutputItemParam.json` | pending (request mapping) |
| `image_generation_call` | `ImageGenCallItemParam.json` | pending (request mapping) |
| `local_shell_call` | `LocalShellCallItemParam.json` | pending (request mapping) |
| `local_shell_call_output` | `LocalShellCallOutputItemParam.json` | pending (request mapping) |
| `mcp_approval_request` | `MCPApprovalRequestItemParam.json` | pending (request mapping) |
| `mcp_approval_response` | `MCPApprovalResponseItemParam.json` | pending (request mapping) |
| `message` | `AssistantMessageItemParam.json` | pending (request mapping) |
| `message` | `DeveloperMessageItemParam.json` | pending (request mapping) |
| `message` | `SystemMessageItemParam.json` | pending (request mapping) |
| `message` | `UserMessageItemParam.json` | pending (request mapping) |
| `reasoning` | `ReasoningItemParam.json` | pending (request mapping) |
| `shell_call` | `FunctionShellCallItemParam.json` | pending (request mapping) |
| `shell_call_output` | `FunctionShellCallOutputItemParam.json` | pending (request mapping) |
| `unknown` | `ItemReferenceParam.json` | pending (request mapping) |
| `web_search_call` | `WebSearchCallItemParam.json` | pending (request mapping) |

## Output item variants
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

## Schema index (all components)

This list is exhaustive and drives the task tracker in `docs/07_tasks/openresponses_compliance.md`.

Legend
- `bundled`: schema is present in `schemas/openresponses/openapi.json` (used for current validation).
- `validated`: schema is reachable from streaming-event or ResponseResource validation in the bundled OpenAPI.
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
| `ApplyPatchToolCallItemParam` | no | no | pending |
| `ApplyPatchToolCallOutputItemParam` | no | no | pending |
| `AssistantMessageItemParam` | yes | no | pending |
| `CodeInterpreterCallItemParam` | no | no | pending |
| `CompactionSummaryItemParam` | no | no | pending |
| `ComputerCallItemParam` | no | no | pending |
| `ComputerCallOutputItemParam` | no | no | pending |
| `CustomToolCallItemParam` | no | no | pending |
| `CustomToolCallOutputItemParam` | no | no | pending |
| `DeveloperMessageItemParam` | yes | no | pending |
| `FileSearchCallItemParam` | no | no | pending |
| `FunctionCallItemParam` | yes | no | pending |
| `FunctionCallOutputItemParam` | yes | no | pending |
| `FunctionShellCallItemParam` | no | no | pending |
| `FunctionShellCallOutputItemParam` | no | no | pending |
| `ImageGenCallItemParam` | no | no | pending |
| `ItemParam` | yes | no | pending |
| `LocalShellCallItemParam` | no | no | pending |
| `LocalShellCallOutputItemParam` | no | no | pending |
| `MCPApprovalRequestItemParam` | no | no | pending |
| `MCPApprovalResponseItemParam` | no | no | pending |
| `ReasoningItemParam` | yes | no | pending |
| `SystemMessageItemParam` | yes | no | pending |
| `UserMessageItemParam` | yes | no | pending |
| `WebSearchCallItemParam` | no | no | pending |

### Other schemas
| schema | bundled | validated | status |
| --- | --- | --- | --- |
| `AllowedToolsParam` | yes | no | pending |
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
| `ApplyPatchToolCall` | no | no | pending |
| `ApplyPatchToolCallOutput` | no | no | pending |
| `ApplyPatchUpdateFileOperation` | no | no | pending |
| `ApplyPatchUpdateFileOperationParam` | no | no | pending |
| `ApproximateLocation` | no | no | pending |
| `ApproximateLocationParam` | no | no | pending |
| `Billing` | no | no | pending |
| `ClickAction` | no | no | pending |
| `ClickButtonType` | no | no | pending |
| `ClickParam` | no | no | pending |
| `CodeInterpreterCall` | no | no | pending |
| `CodeInterpreterCallStatus` | no | no | pending |
| `CodeInterpreterOutputImage` | no | no | pending |
| `CodeInterpreterOutputLogs` | no | no | pending |
| `CodeInterpreterToolCallOutputImageParam` | no | no | pending |
| `CodeInterpreterToolCallOutputLogsParam` | no | no | pending |
| `CompactResource` | no | no | pending |
| `CompactResponseMethodPublicBody` | no | no | pending |
| `CompactionBody` | no | no | pending |
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
| `ComputerCall` | no | no | pending |
| `ComputerCallOutput` | no | no | pending |
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
| `CustomToolCall` | no | no | pending |
| `CustomToolCallOutput` | no | no | pending |
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
| `FileSearchCall` | no | no | pending |
| `FileSearchRankingOptionsParam` | no | no | pending |
| `FileSearchResult` | no | no | pending |
| `FileSearchRetrievedChunksParam` | no | no | pending |
| `FileSearchToolCallStatusEnum` | no | no | pending |
| `Filters` | no | no | pending |
| `FunctionCall` | yes | yes | pending |
| `FunctionCallItemStatus` | yes | no | pending |
| `FunctionCallOutput` | yes | yes | pending |
| `FunctionCallOutputStatusEnum` | yes | yes | pending |
| `FunctionCallStatus` | yes | yes | pending |
| `FunctionShellAction` | no | no | pending |
| `FunctionShellActionParam` | no | no | pending |
| `FunctionShellCall` | no | no | pending |
| `FunctionShellCallItemStatus` | no | no | pending |
| `FunctionShellCallOutput` | no | no | pending |
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
| `ImageGenCall` | no | no | pending |
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
| `ItemReferenceParam` | yes | no | pending |
| `JsonObjectResponseFormat` | yes | yes | pending |
| `JsonSchemaResponseFormat` | yes | yes | pending |
| `KeyPressAction` | no | no | pending |
| `KeyPressParam` | no | no | pending |
| `LocalFileEnvironmentParam` | no | no | pending |
| `LocalShellCall` | no | no | pending |
| `LocalShellCallItemStatus` | no | no | pending |
| `LocalShellCallOutput` | no | no | pending |
| `LocalShellCallOutputStatusEnum` | no | no | pending |
| `LocalShellCallStatus` | no | no | pending |
| `LocalShellExecAction` | no | no | pending |
| `LocalShellExecActionParam` | no | no | pending |
| `LogProb` | yes | yes | pending |
| `MCPApprovalRequest` | no | no | pending |
| `MCPApprovalResponse` | no | no | pending |
| `MCPListTools` | no | no | pending |
| `MCPRequireApprovalApiEnum` | no | no | pending |
| `MCPRequireApprovalFieldEnum` | no | no | pending |
| `MCPRequireApprovalFilterField` | no | no | pending |
| `MCPRequireApprovalFilterParam` | no | no | pending |
| `MCPToolCall` | no | no | pending |
| `MCPToolCallStatus` | no | no | pending |
| `MCPToolFilterField` | no | no | pending |
| `MCPToolFilterParam` | no | no | pending |
| `Message` | yes | yes | pending |
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
| `ReasoningBody` | yes | yes | pending |
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
| `SpecificApplyPatchParam` | no | no | pending |
| `SpecificCodeInterpreterParam` | no | no | pending |
| `SpecificComputerParam` | no | no | pending |
| `SpecificComputerPreviewParam` | no | no | pending |
| `SpecificFileSearchParam` | no | no | pending |
| `SpecificFunctionParam` | yes | no | pending |
| `SpecificFunctionShellParam` | no | no | pending |
| `SpecificImageGenParam` | no | no | pending |
| `SpecificLocalShellParam` | no | no | pending |
| `SpecificMCPFunctionParam` | no | no | pending |
| `SpecificToolChoiceParam` | yes | no | pending |
| `SpecificWebSearchParam` | no | no | pending |
| `SpecificWebSearchPreviewParam` | no | no | pending |
| `StreamOptionsParam` | yes | no | pending |
| `SummaryTextContent` | yes | yes | pending |
| `TextContent` | yes | yes | pending |
| `TextField` | yes | yes | pending |
| `TextParam` | yes | no | pending |
| `TextResponseFormat` | yes | yes | pending |
| `TokenCountsBody` | no | no | pending |
| `TokenCountsResource` | no | no | pending |
| `ToolChoiceParam` | yes | no | pending |
| `ToolChoiceValueEnum` | yes | yes | pending |
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
| `WebSearchCall` | no | no | pending |
| `WebSearchCallActionFindInPage` | no | no | pending |
| `WebSearchCallActionFindInPageParam` | no | no | pending |
| `WebSearchCallActionOpenPage` | no | no | pending |
| `WebSearchCallActionOpenPageParam` | no | no | pending |
| `WebSearchCallActionSearch` | no | no | pending |
| `WebSearchCallActionSearchParam` | no | no | pending |
| `WebSearchCallStatus` | no | no | pending |
| `WebSearchPreviewToolParam_2025_03_11Param` | no | no | pending |
| `WebSearchToolParam_2025_08_14Param` | no | no | pending |

### Output item fields
| schema | bundled | validated | status |
| --- | --- | --- | --- |
| `ItemField` | yes | yes | provider_event |

### Request-related schemas
| schema | bundled | validated | status |
| --- | --- | --- | --- |
| `CreateResponseBody` | yes | no | pending |

### Response-related schemas
| schema | bundled | validated | status |
| --- | --- | --- | --- |
| `ResponseFormatDallE` | no | no | pending |
| `ResponseResource` | yes | yes | provider_event |
| `ResponsesConversationParam` | no | no | pending |

### Streaming events
| schema | bundled | validated | status |
| --- | --- | --- | --- |
| `ErrorStreamingEvent` | yes | yes | provider_event |
| `ImageEditCompletedStreamingEvent` | no | no | pending |
| `ImageEditPartialImageStreamingEvent` | no | no | pending |
| `ImageGenerationCompletedStreamingEvent` | no | no | pending |
| `ImageGenerationPartialImageStreamingEvent` | no | no | pending |
| `ResponseApplyPatchCallOperationDiffDeltaStreamingEvent` | no | no | pending |
| `ResponseApplyPatchCallOperationDiffDoneStreamingEvent` | no | no | pending |
| `ResponseCodeInterpreterCallCodeDeltaStreamingEvent` | no | no | pending |
| `ResponseCodeInterpreterCallCodeDoneStreamingEvent` | no | no | pending |
| `ResponseCodeInterpreterCallCompletedStreamingEvent` | no | no | pending |
| `ResponseCodeInterpreterCallInProgressStreamingEvent` | no | no | pending |
| `ResponseCodeInterpreterCallInterpretingStreamingEvent` | no | no | pending |
| `ResponseCompletedStreamingEvent` | yes | yes | provider_event |
| `ResponseContentPartAddedStreamingEvent` | yes | yes | provider_event |
| `ResponseContentPartDoneStreamingEvent` | yes | yes | provider_event |
| `ResponseCreatedStreamingEvent` | yes | yes | provider_event |
| `ResponseCustomToolCallInputDeltaStreamingEvent` | no | no | pending |
| `ResponseCustomToolCallInputDoneStreamingEvent` | no | no | pending |
| `ResponseFailedStreamingEvent` | yes | yes | provider_event |
| `ResponseFileSearchCallCompletedStreamingEvent` | no | no | pending |
| `ResponseFileSearchCallInProgressStreamingEvent` | no | no | pending |
| `ResponseFileSearchCallSearchingStreamingEvent` | no | no | pending |
| `ResponseFunctionCallArgumentsDeltaStreamingEvent` | yes | yes | provider_event |
| `ResponseFunctionCallArgumentsDoneStreamingEvent` | yes | yes | provider_event |
| `ResponseImageGenCallCompletedStreamingEvent` | no | no | pending |
| `ResponseImageGenCallGeneratingStreamingEvent` | no | no | pending |
| `ResponseImageGenCallInProgressStreamingEvent` | no | no | pending |
| `ResponseImageGenCallPartialImageStreamingEvent` | no | no | pending |
| `ResponseInProgressStreamingEvent` | yes | yes | provider_event |
| `ResponseIncompleteStreamingEvent` | yes | yes | provider_event |
| `ResponseMCPCallArgumentsDeltaStreamingEvent` | no | no | pending |
| `ResponseMCPCallArgumentsDoneStreamingEvent` | no | no | pending |
| `ResponseMCPCallCompletedStreamingEvent` | no | no | pending |
| `ResponseMCPCallFailedStreamingEvent` | no | no | pending |
| `ResponseMCPCallInProgressStreamingEvent` | no | no | pending |
| `ResponseMCPListToolsCompletedStreamingEvent` | no | no | pending |
| `ResponseMCPListToolsFailedStreamingEvent` | no | no | pending |
| `ResponseMCPListToolsInProgressStreamingEvent` | no | no | pending |
| `ResponseOutputItemAddedStreamingEvent` | yes | yes | provider_event |
| `ResponseOutputItemDoneStreamingEvent` | yes | yes | provider_event |
| `ResponseOutputTextAnnotationAddedStreamingEvent` | yes | yes | provider_event |
| `ResponseOutputTextDeltaStreamingEvent` | yes | yes | provider_event |
| `ResponseOutputTextDoneStreamingEvent` | yes | yes | provider_event |
| `ResponseQueuedStreamingEvent` | yes | yes | provider_event |
| `ResponseReasoningDeltaStreamingEvent` | yes | yes | provider_event |
| `ResponseReasoningDoneStreamingEvent` | yes | yes | provider_event |
| `ResponseReasoningSummaryDeltaStreamingEvent` | yes | yes | provider_event |
| `ResponseReasoningSummaryDoneStreamingEvent` | yes | yes | provider_event |
| `ResponseReasoningSummaryPartAddedStreamingEvent` | yes | yes | provider_event |
| `ResponseReasoningSummaryPartDoneStreamingEvent` | yes | yes | provider_event |
| `ResponseRefusalDeltaStreamingEvent` | yes | yes | provider_event |
| `ResponseRefusalDoneStreamingEvent` | yes | yes | provider_event |
| `ResponseShellCallCommandAddedStreamingEvent` | no | no | pending |
| `ResponseShellCallCommandDeltaStreamingEvent` | no | no | pending |
| `ResponseShellCallCommandDoneStreamingEvent` | no | no | pending |
| `ResponseWebSearchCallCompletedStreamingEvent` | no | no | pending |
| `ResponseWebSearchCallInProgressStreamingEvent` | no | no | pending |
| `ResponseWebSearchCallSearchingStreamingEvent` | no | no | pending |

### Tool schemas
| schema | bundled | validated | status |
| --- | --- | --- | --- |
| `AllowedToolChoice` | yes | yes | pending |
| `ApplyPatchTool` | no | no | pending |
| `ApplyPatchToolChoice` | no | no | pending |
| `ApplyPatchToolParam` | no | no | pending |
| `AutoCodeInterpreterToolParam` | no | no | pending |
| `CodeInterpreterToolChoice` | no | no | pending |
| `CodeInterpreterToolParam` | no | no | pending |
| `ComputerToolChoice` | no | no | pending |
| `ComputerToolParam` | no | no | pending |
| `ComputerUsePreviewTool` | no | no | pending |
| `ComputerUsePreviewToolParam` | no | no | pending |
| `CustomTool` | no | no | pending |
| `CustomToolChoice` | no | no | pending |
| `CustomToolParam` | no | no | pending |
| `FileSearchTool` | no | no | pending |
| `FileSearchToolChoice` | no | no | pending |
| `FileSearchToolParam` | no | no | pending |
| `FunctionShellTool` | no | no | pending |
| `FunctionShellToolChoice` | no | no | pending |
| `FunctionShellToolParam` | no | no | pending |
| `FunctionTool` | yes | yes | pending |
| `FunctionToolChoice` | yes | yes | pending |
| `FunctionToolParam` | yes | no | pending |
| `ImageGenTool` | no | no | pending |
| `ImageGenToolChoice` | no | no | pending |
| `ImageGenToolParam` | no | no | pending |
| `LocalShellToolChoice` | no | no | pending |
| `LocalShellToolParam` | no | no | pending |
| `MCPListToolsTool` | no | no | pending |
| `MCPTool` | no | no | pending |
| `MCPToolChoice` | no | no | pending |
| `MCPToolParam` | no | no | pending |
| `MemoryToolParam` | no | no | pending |
| `ResponsesToolParam` | yes | no | pending |
| `SpecificCustomToolParam` | no | no | pending |
| `Tool` | yes | yes | pending |
| `WebSearchGADeprecatedToolParam` | no | no | pending |
| `WebSearchPreviewTool` | no | no | pending |
| `WebSearchPreviewToolParam` | no | no | pending |
| `WebSearchToolChoice` | no | no | pending |
| `WebSearchToolParam` | no | no | pending |
