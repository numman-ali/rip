# OpenResponses Capability Map

Summary
- Maps OpenResponses specification + schemas to internal capability ids and surface coverage.
- Complements the schema-level compliance map in `docs/03_contracts/openresponses_compliance.md`.
- Drives updates to `docs/03_contracts/capability_registry.md`, `docs/02_architecture/capability_matrix.md`, and `docs/07_tasks/roadmap.md`.

Sources (authoritative)
- `temp/openresponses/src/pages/specification.mdx`
- `temp/openresponses/src/pages/reference.mdx`
- `temp/openresponses/src/pages/compliance.mdx`
- `temp/openresponses/public/openapi/openapi.json` (canonical OpenAPI)
- `temp/openresponses/schema/openapi.json` (fallback)
- `temp/openresponses/schema/components/schemas/*.json`
- `temp/openresponses/schema/paths/responses.json`
- `docs/03_contracts/openresponses_traceability.md`

Mapping rules
- OpenResponses features map to internal capability ids; missing ids are flagged as “needs registry”.
- Provider-boundary validation (schema tests) is not sufficient; runtime behavior must be mapped to frames + surfaces.
- Surface parity must be explicit in `docs/02_architecture/capability_matrix.md`.

Capability alignment (expanded)
| feature group | spec refs | schema refs | internal capability ids | surface impact | status |
| --- | --- | --- | --- | --- | --- |
| Transport + SSE invariants | spec: HTTP Requests/Responses/Streaming | `paths/responses.json` (streaming events) | `execution.json_stream`, `session.stream_events`, `openresponses.streaming_fidelity` | server + cli_h + cli_i + sdk | needs registry/matrix entry |
| Response lifecycle + statuses | spec: State machines, Streaming | `ResponseResource`, `ResponseStatusEnum`, response state events | `openresponses.response_fidelity`, `openresponses.streaming_fidelity` | server + cli_h + cli_i + sdk | needs registry/matrix entry |
| Items + required fields | spec: Items, Items are state machines | `ItemParam`, `ItemField`, `ItemStatusEnum`, `MessageRoleEnum` | `openresponses.item_lifecycle`, `openresponses.response_fidelity` | server + cli_h + cli_i + sdk | needs registry/matrix entry |
| Item streaming sequence | spec: Items are streamable | `ResponseOutputItemAdded/Done`, `ResponseContentPart*`, `ResponseOutputText*`, `ResponseReasoning*` | `openresponses.streaming_fidelity`, `session.stream_events` | server + cli_h + cli_i + sdk | needs registry/matrix entry |
| Content unions (user vs model) | spec: Content | `InputTextContent*`, `InputImageContent*`, `InputFileContent*`, `InputVideoContent`, `OutputTextContent*`, `RefusalContent*`, `SummaryTextContent*` | `openresponses.content_union` | server + cli_h + cli_i + sdk | needs registry/matrix entry |
| Reasoning items | spec: Reasoning | `ReasoningItemParam`, `Reasoning`, `ReasoningTextContent`, `ReasoningSummaryContentParam` | `openresponses.reasoning_items`, `model.thinking_levels` | server + cli_h + cli_i + sdk | needs registry/matrix entry |
| Errors + streaming failures | spec: Errors | `Error`, `ErrorPayload`, `ResponseFailedStreamingEvent` | `openresponses.errors`, `session.stream_events` | server + cli_h + cli_i + sdk | needs registry/matrix entry |
| Tools (external + internal) | spec: Tools | `ResponsesToolParam`, `Tool`, tool call items (function/web/file/computer/mcp/image/etc.) | `tool.registry`, `tool.schema`, `openresponses.tools_union` | server + cli_h + cli_i + sdk | partially mapped; runtime semantics pending |
| `tool_choice` + `allowed_tools` | spec: tool_choice + allowed_tools | `ToolChoiceParam`, `AllowedToolsParam`, `ToolChoiceValueEnum` | `tool.choice`, `tool.allowed_tools`, `tool.permissions` | server + cli_h + cli_i + sdk | needs registry/matrix entry |
| Tool call limits | spec: (schema-only) | `max_tool_calls`, `parallel_tool_calls` | `tool.call_limits` | server + cli_h + cli_i + sdk | needs registry/matrix entry |
| Conversation continuity | spec: previous_response_id | `previous_response_id`, `ResponsesConversationParam`, `Conversation` | `session.previous_response`, `thread.reference`, `context.compile` | server + cli_h + cli_i + sdk | needs registry/matrix entry |
| Truncation policy | spec: truncation | `TruncationEnum` | `compaction.truncation_policy` | server + cli_h + cli_i + sdk | needs registry/matrix entry |
| Service tier routing | spec: service_tier | `ServiceTierEnum` | `model.service_tier` | server + sdk | needs registry/matrix entry |
| Sampling controls | spec: (schema-only) | `temperature`, `top_p`, `presence_penalty`, `frequency_penalty`, `max_output_tokens`, `seed` | `model.sampling_params`, `model.max_output_tokens` | server + cli_h + cli_i + sdk | needs registry/matrix entry |
| Response format + verbosity | spec: (schema-only) | `TextParam`, `TextFormatParam`, `JsonSchemaResponseFormatParam`, `ResponseFormat*` | `execution.output_format`, `execution.structured_output` | server + cli_h + cli_i + sdk | mapped to existing ids; parity update pending |
| Response include/extras | spec: (schema-only) | `IncludeEnum`, `top_logprobs` | `execution.response_include`, `model.logprobs` | server + cli_h + cli_i + sdk | needs registry/matrix entry |
| Usage + token counts | spec: (implicit via schema) | `Usage`, `TokenCountsBody/Resource` | `usage.token_counts` | server + sdk | needs registry/matrix entry |
| Request metadata + identity | spec: (schema-only) | `metadata`, `user`, `safety_identifier`, `prompt_cache_*`, `store`, `background` | `policy.request_identifiers`, `model.prompt_cache`, `openresponses.request_fidelity` | server + cli_h + cli_i + sdk | needs registry/matrix entry |
| Extensions (items/events/schemas) | spec: Extending Open Responses | extension-prefixed items/events, schema extensions | `openresponses.extensions`, `extensions.*` | server + tui + mcp | needs registry/matrix entry |

Next steps
- Add missing capability ids to `capability_registry.md` and update surface parity.
- Update `docs/02_architecture/capability_matrix.md` to mention OpenResponses boundary fidelity.
- Add roadmap items for any feature not covered by Phase 1 scope.
