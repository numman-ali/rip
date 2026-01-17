# OpenResponses Compliance Map

Summary
- Source repo: `temp/openresponses` (local vendor).
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
| `apply_patch_call` | `ApplyPatchToolCallItemParam.json` | provider_event |
| `apply_patch_call_output` | `ApplyPatchToolCallOutputItemParam.json` | provider_event |
| `code_interpreter_call` | `CodeInterpreterCallItemParam.json` | provider_event |
| `compaction` | `CompactionSummaryItemParam.json` | provider_event |
| `computer_call` | `ComputerCallItemParam.json` | provider_event |
| `computer_call_output` | `ComputerCallOutputItemParam.json` | provider_event |
| `custom_tool_call` | `CustomToolCallItemParam.json` | provider_event |
| `custom_tool_call_output` | `CustomToolCallOutputItemParam.json` | provider_event |
| `file_search_call` | `FileSearchCallItemParam.json` | provider_event |
| `function_call` | `FunctionCallItemParam.json` | provider_event |
| `function_call_output` | `FunctionCallOutputItemParam.json` | provider_event |
| `image_generation_call` | `ImageGenCallItemParam.json` | provider_event |
| `local_shell_call` | `LocalShellCallItemParam.json` | provider_event |
| `local_shell_call_output` | `LocalShellCallOutputItemParam.json` | provider_event |
| `mcp_approval_request` | `MCPApprovalRequestItemParam.json` | provider_event |
| `mcp_approval_response` | `MCPApprovalResponseItemParam.json` | provider_event |
| `message` | `AssistantMessageItemParam.json` | provider_event |
| `message` | `DeveloperMessageItemParam.json` | provider_event |
| `message` | `SystemMessageItemParam.json` | provider_event |
| `message` | `UserMessageItemParam.json` | provider_event |
| `reasoning` | `ReasoningItemParam.json` | provider_event |
| `shell_call` | `FunctionShellCallItemParam.json` | provider_event |
| `shell_call_output` | `FunctionShellCallOutputItemParam.json` | provider_event |
| `unknown` | `ItemReferenceParam.json` | provider_event |
| `web_search_call` | `WebSearchCallItemParam.json` | provider_event |

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

### Error schemas
- `Error`
- `ErrorPayload`
- `HTTPError`
- `MCPProtocolError`
- `MCPToolExecutionError`

### Input item params
- `ApplyPatchToolCallItemParam`
- `ApplyPatchToolCallOutputItemParam`
- `AssistantMessageItemParam`
- `CodeInterpreterCallItemParam`
- `CompactionSummaryItemParam`
- `ComputerCallItemParam`
- `ComputerCallOutputItemParam`
- `CustomToolCallItemParam`
- `CustomToolCallOutputItemParam`
- `DeveloperMessageItemParam`
- `FileSearchCallItemParam`
- `FunctionCallItemParam`
- `FunctionCallOutputItemParam`
- `FunctionShellCallItemParam`
- `FunctionShellCallOutputItemParam`
- `ImageGenCallItemParam`
- `ItemParam`
- `LocalShellCallItemParam`
- `LocalShellCallOutputItemParam`
- `MCPApprovalRequestItemParam`
- `MCPApprovalResponseItemParam`
- `ReasoningItemParam`
- `SystemMessageItemParam`
- `UserMessageItemParam`
- `WebSearchCallItemParam`

### Other schemas
- `AllowedToolsParam`
- `Annotation`
- `ApiSourceParam`
- `ApplyPatchCallOutputStatus`
- `ApplyPatchCallOutputStatusParam`
- `ApplyPatchCallStatus`
- `ApplyPatchCallStatusParam`
- `ApplyPatchCreateFileOperation`
- `ApplyPatchCreateFileOperationParam`
- `ApplyPatchDeleteFileOperation`
- `ApplyPatchDeleteFileOperationParam`
- `ApplyPatchOperationParam`
- `ApplyPatchToolCall`
- `ApplyPatchToolCallOutput`
- `ApplyPatchUpdateFileOperation`
- `ApplyPatchUpdateFileOperationParam`
- `ApproximateLocation`
- `ApproximateLocationParam`
- `Billing`
- `ClickAction`
- `ClickButtonType`
- `ClickParam`
- `CodeInterpreterCall`
- `CodeInterpreterCallStatus`
- `CodeInterpreterOutputImage`
- `CodeInterpreterOutputLogs`
- `CodeInterpreterToolCallOutputImageParam`
- `CodeInterpreterToolCallOutputLogsParam`
- `CompactResource`
- `CompactResponseMethodPublicBody`
- `CompactionBody`
- `ComparisonFilterFieldCONTAINS`
- `ComparisonFilterFieldCONTAINSANY`
- `ComparisonFilterFieldEQ`
- `ComparisonFilterFieldGT`
- `ComparisonFilterFieldGTE`
- `ComparisonFilterFieldIN`
- `ComparisonFilterFieldLT`
- `ComparisonFilterFieldLTE`
- `ComparisonFilterFieldNCONTAINS`
- `ComparisonFilterFieldNCONTAINSANY`
- `ComparisonFilterFieldNE`
- `ComparisonFilterFieldNIN`
- `ComparisonFilterParamContainsAnyParam`
- `ComparisonFilterParamContainsParam`
- `ComparisonFilterParamEQParam`
- `ComparisonFilterParamGTEParam`
- `ComparisonFilterParamGTParam`
- `ComparisonFilterParamINParam`
- `ComparisonFilterParamLTEParam`
- `ComparisonFilterParamLTParam`
- `ComparisonFilterParamNContainsAnyParam`
- `ComparisonFilterParamNContainsParam`
- `ComparisonFilterParamNEParam`
- `ComparisonFilterParamNINParam`
- `CompoundFilterFieldAND`
- `CompoundFilterFieldOR`
- `CompoundFilterParamAndParam`
- `CompoundFilterParamOrParam`
- `ComputerCall`
- `ComputerCallOutput`
- `ComputerCallOutputStatus`
- `ComputerCallSafetyCheckParam`
- `ComputerEnvironment`
- `ComputerEnvironment1`
- `ComputerScreenshotContent`
- `ComputerScreenshotParam`
- `ContainerFileCitationBody`
- `ContainerFileCitationParam`
- `ContainerMemoryLimit`
- `ContextEdit`
- `ContextEditDetails`
- `Conversation`
- `ConversationParam`
- `CoordParam`
- `CreateImageBody15Param`
- `CreateImageBody1MiniParam`
- `CreateImageBody1Param`
- `CreateImageBodyChatGPTImageLatestParam`
- `CreateVideoBody`
- `CreateVideoRemixBody`
- `CustomGrammarFormatField`
- `CustomGrammarFormatParam`
- `CustomTextFormatField`
- `CustomTextFormatParam`
- `CustomToolCall`
- `CustomToolCallOutput`
- `CustomToolFormat`
- `DeletedResponseResource`
- `DeletedVideoResource`
- `DetailEnum`
- `DoubleClickAction`
- `DoubleClickParam`
- `DragAction`
- `DragParam`
- `DragPoint`
- `EditImageBody15Param`
- `EditImageBody1MiniParam`
- `EditImageBody1Param`
- `EditImageBodyChatGPTImageLatestParam`
- `EditsBodyDallE2Param`
- `EmptyAction`
- `EmptyModelParam`
- `ExcludeEnum`
- `FileCitationBody`
- `FileCitationParam`
- `FileSearchCall`
- `FileSearchRankingOptionsParam`
- `FileSearchResult`
- `FileSearchRetrievedChunksParam`
- `FileSearchToolCallStatusEnum`
- `Filters`
- `FunctionCall`
- `FunctionCallItemStatus`
- `FunctionCallOutput`
- `FunctionCallOutputStatusEnum`
- `FunctionCallStatus`
- `FunctionShellAction`
- `FunctionShellActionParam`
- `FunctionShellCall`
- `FunctionShellCallItemStatus`
- `FunctionShellCallOutput`
- `FunctionShellCallOutputContent`
- `FunctionShellCallOutputContentParam`
- `FunctionShellCallOutputExitOutcome`
- `FunctionShellCallOutputExitOutcomeParam`
- `FunctionShellCallOutputOutcomeParam`
- `FunctionShellCallOutputTimeoutOutcome`
- `FunctionShellCallOutputTimeoutOutcomeParam`
- `GenerationsBodyDallE2Param`
- `GenerationsBodyDallE3Param`
- `GrammarSyntax`
- `GrammarSyntax1`
- `HybridSearchOptions`
- `HybridSearchOptionsParam`
- `Image`
- `ImageBackground`
- `ImageDetail`
- `ImageGenAction`
- `ImageGenActionEnum`
- `ImageGenCall`
- `ImageGenCallStatus`
- `ImageGenInputUsageDetails`
- `ImageGenOutputTokensDetails`
- `ImageGenToolModel`
- `ImageGenUsage`
- `ImageModeration`
- `ImageOutputFormat`
- `ImageQuality`
- `ImageQualityDallE`
- `ImageResource`
- `ImageSize`
- `ImageSizeDallE2`
- `ImageSizeDallE3`
- `ImageStyleDallE`
- `ImageUsage`
- `ImageUsageInputTokensDetails`
- `ImageUsageOutputTokensDetails`
- `IncludeEnum`
- `IncompleteDetails`
- `InputFidelity`
- `InputFileContent`
- `InputFileContentParam`
- `InputImageContent`
- `InputImageContentParamAutoParam`
- `InputImageMaskContentParam`
- `InputTextContent`
- `InputTextContentParam`
- `InputTokensDetails`
- `ItemListResource`
- `ItemReferenceParam`
- `JsonObjectResponseFormat`
- `JsonSchemaResponseFormat`
- `KeyPressAction`
- `KeyPressParam`
- `LocalFileEnvironmentParam`
- `LocalShellCall`
- `LocalShellCallItemStatus`
- `LocalShellCallOutput`
- `LocalShellCallOutputStatusEnum`
- `LocalShellCallStatus`
- `LocalShellExecAction`
- `LocalShellExecActionParam`
- `LogProb`
- `MCPApprovalRequest`
- `MCPApprovalResponse`
- `MCPListTools`
- `MCPRequireApprovalApiEnum`
- `MCPRequireApprovalFieldEnum`
- `MCPRequireApprovalFilterField`
- `MCPRequireApprovalFilterParam`
- `MCPToolCall`
- `MCPToolCallStatus`
- `MCPToolFilterField`
- `MCPToolFilterParam`
- `Message`
- `MessageRole`
- `MessageRole1`
- `MessageStatus`
- `MetadataParam`
- `MoveAction`
- `MoveParam`
- `OrderEnum`
- `OutputTextContent`
- `OutputTextContentParam`
- `OutputTokensDetails`
- `Payer`
- `PromptCacheRetentionEnum`
- `PromptInstructionMessage`
- `RankerVersionType`
- `RankingOptions`
- `Reasoning`
- `ReasoningBody`
- `ReasoningEffortEnum`
- `ReasoningParam`
- `ReasoningSummaryContentParam`
- `ReasoningSummaryEnum`
- `ReasoningTextContent`
- `RefusalContent`
- `RefusalContentParam`
- `SafetyCheck`
- `ScreenshotAction`
- `ScreenshotParam`
- `ScrollAction`
- `ScrollParam`
- `SearchContextSize`
- `ServiceTierEnum`
- `SpecificApplyPatchParam`
- `SpecificCodeInterpreterParam`
- `SpecificComputerParam`
- `SpecificComputerPreviewParam`
- `SpecificFileSearchParam`
- `SpecificFunctionParam`
- `SpecificFunctionShellParam`
- `SpecificImageGenParam`
- `SpecificLocalShellParam`
- `SpecificMCPFunctionParam`
- `SpecificToolChoiceParam`
- `SpecificWebSearchParam`
- `SpecificWebSearchPreviewParam`
- `StreamOptionsParam`
- `SummaryTextContent`
- `TextContent`
- `TextField`
- `TextParam`
- `TextResponseFormat`
- `TokenCountsBody`
- `TokenCountsResource`
- `ToolChoiceParam`
- `ToolChoiceValueEnum`
- `TopLogProb`
- `TruncationEnum`
- `TypeAction`
- `TypeParam`
- `UrlCitationBody`
- `UrlCitationParam`
- `UrlSourceParam`
- `Usage`
- `VerbosityEnum`
- `VideoContentVariant`
- `VideoListResource`
- `VideoModel`
- `VideoResource`
- `VideoSeconds`
- `VideoSize`
- `VideoStatus`
- `WaitAction`
- `WaitParam`
- `WebSearchCall`
- `WebSearchCallActionFindInPage`
- `WebSearchCallActionFindInPageParam`
- `WebSearchCallActionOpenPage`
- `WebSearchCallActionOpenPageParam`
- `WebSearchCallActionSearch`
- `WebSearchCallActionSearchParam`
- `WebSearchCallStatus`
- `WebSearchPreviewToolParam_2025_03_11Param`
- `WebSearchToolParam_2025_08_14Param`

### Output item fields
- `ItemField`

### Request-related schemas
- `CreateResponseBody`

### Response-related schemas
- `ResponseFormatDallE`
- `ResponseResource`
- `ResponsesConversationParam`

### Streaming events
- `ErrorStreamingEvent`
- `ImageEditCompletedStreamingEvent`
- `ImageEditPartialImageStreamingEvent`
- `ImageGenerationCompletedStreamingEvent`
- `ImageGenerationPartialImageStreamingEvent`
- `ResponseApplyPatchCallOperationDiffDeltaStreamingEvent`
- `ResponseApplyPatchCallOperationDiffDoneStreamingEvent`
- `ResponseCodeInterpreterCallCodeDeltaStreamingEvent`
- `ResponseCodeInterpreterCallCodeDoneStreamingEvent`
- `ResponseCodeInterpreterCallCompletedStreamingEvent`
- `ResponseCodeInterpreterCallInProgressStreamingEvent`
- `ResponseCodeInterpreterCallInterpretingStreamingEvent`
- `ResponseCompletedStreamingEvent`
- `ResponseContentPartAddedStreamingEvent`
- `ResponseContentPartDoneStreamingEvent`
- `ResponseCreatedStreamingEvent`
- `ResponseCustomToolCallInputDeltaStreamingEvent`
- `ResponseCustomToolCallInputDoneStreamingEvent`
- `ResponseFailedStreamingEvent`
- `ResponseFileSearchCallCompletedStreamingEvent`
- `ResponseFileSearchCallInProgressStreamingEvent`
- `ResponseFileSearchCallSearchingStreamingEvent`
- `ResponseFunctionCallArgumentsDeltaStreamingEvent`
- `ResponseFunctionCallArgumentsDoneStreamingEvent`
- `ResponseImageGenCallCompletedStreamingEvent`
- `ResponseImageGenCallGeneratingStreamingEvent`
- `ResponseImageGenCallInProgressStreamingEvent`
- `ResponseImageGenCallPartialImageStreamingEvent`
- `ResponseInProgressStreamingEvent`
- `ResponseIncompleteStreamingEvent`
- `ResponseMCPCallArgumentsDeltaStreamingEvent`
- `ResponseMCPCallArgumentsDoneStreamingEvent`
- `ResponseMCPCallCompletedStreamingEvent`
- `ResponseMCPCallFailedStreamingEvent`
- `ResponseMCPCallInProgressStreamingEvent`
- `ResponseMCPListToolsCompletedStreamingEvent`
- `ResponseMCPListToolsFailedStreamingEvent`
- `ResponseMCPListToolsInProgressStreamingEvent`
- `ResponseOutputItemAddedStreamingEvent`
- `ResponseOutputItemDoneStreamingEvent`
- `ResponseOutputTextAnnotationAddedStreamingEvent`
- `ResponseOutputTextDeltaStreamingEvent`
- `ResponseOutputTextDoneStreamingEvent`
- `ResponseQueuedStreamingEvent`
- `ResponseReasoningDeltaStreamingEvent`
- `ResponseReasoningDoneStreamingEvent`
- `ResponseReasoningSummaryDeltaStreamingEvent`
- `ResponseReasoningSummaryDoneStreamingEvent`
- `ResponseReasoningSummaryPartAddedStreamingEvent`
- `ResponseReasoningSummaryPartDoneStreamingEvent`
- `ResponseRefusalDeltaStreamingEvent`
- `ResponseRefusalDoneStreamingEvent`
- `ResponseShellCallCommandAddedStreamingEvent`
- `ResponseShellCallCommandDeltaStreamingEvent`
- `ResponseShellCallCommandDoneStreamingEvent`
- `ResponseWebSearchCallCompletedStreamingEvent`
- `ResponseWebSearchCallInProgressStreamingEvent`
- `ResponseWebSearchCallSearchingStreamingEvent`

### Tool schemas
- `AllowedToolChoice`
- `ApplyPatchTool`
- `ApplyPatchToolChoice`
- `ApplyPatchToolParam`
- `AutoCodeInterpreterToolParam`
- `CodeInterpreterToolChoice`
- `CodeInterpreterToolParam`
- `ComputerToolChoice`
- `ComputerToolParam`
- `ComputerUsePreviewTool`
- `ComputerUsePreviewToolParam`
- `CustomTool`
- `CustomToolChoice`
- `CustomToolParam`
- `FileSearchTool`
- `FileSearchToolChoice`
- `FileSearchToolParam`
- `FunctionShellTool`
- `FunctionShellToolChoice`
- `FunctionShellToolParam`
- `FunctionTool`
- `FunctionToolChoice`
- `FunctionToolParam`
- `ImageGenTool`
- `ImageGenToolChoice`
- `ImageGenToolParam`
- `LocalShellToolChoice`
- `LocalShellToolParam`
- `MCPListToolsTool`
- `MCPTool`
- `MCPToolChoice`
- `MCPToolParam`
- `MemoryToolParam`
- `ResponsesToolParam`
- `SpecificCustomToolParam`
- `Tool`
- `WebSearchGADeprecatedToolParam`
- `WebSearchPreviewTool`
- `WebSearchPreviewToolParam`
- `WebSearchToolChoice`
- `WebSearchToolParam`
