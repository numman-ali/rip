# OpenResponses Capability Map

Summary
- Maps OpenResponses specification + schemas to internal capability ids and surface coverage.
- Complements the schema-level compliance map in `docs/03_contracts/openresponses_compliance.md`.
- Drives updates to `docs/03_contracts/capability_registry.md`, `docs/02_architecture/capability_matrix.md`, and `docs/07_tasks/roadmap.md`.

Sources (authoritative)
- `temp/openresponses/src/pages/specification.mdx`
- `temp/openresponses/src/pages/reference.mdx`
- `temp/openresponses/src/pages/compliance.mdx`
- `temp/openresponses/schema/openapi.json`
- `temp/openresponses/schema/components/schemas/*.json`
- `temp/openresponses/schema/paths/responses.json`

Mapping rules
- OpenResponses features map to internal capability ids; missing ids are flagged as “needs registry”.
- Provider-boundary validation (schema tests) is not sufficient; runtime behavior must be mapped to frames + surfaces.
- Surface parity must be explicit in `docs/02_architecture/capability_matrix.md`.

Capability alignment (draft; expand to full coverage)
| feature group | spec refs | schema refs | internal capability ids | surface impact | status |
| --- | --- | --- | --- | --- | --- |
| Response lifecycle (sync + streaming) | spec: HTTP Requests/Responses/Streaming | `CreateResponseBody`, `ResponseResource`, streaming events | `session.send_input`, `session.stream_events`, `execution.json_stream` | server + cli_h + cli_i + sdk | mapped to existing ids; confirm surface parity |
| Items + state machines | spec: Items, State machines | `ItemParam`, `ItemField`, item status enums | event frames (`provider_event`, tool events, deltas) | server + cli + sdk | mapped to frames; needs capability registry entry for item/state fidelity |
| Semantic events (SSE) | spec: Semantic events, Streaming | streaming events schema | `execution.json_stream`, provider events | server + cli + sdk | provider event mapping validated; parity map update pending |
| Tools (provider + developer) | spec: Tools | `ResponsesToolParam`, tool call items | `tool.registry`, `tool.schema`, `tool.output_limits` | server + cli + sdk | partially mapped; runtime semantics pending |
| `tool_choice` + `allowed_tools` | spec: tool_choice | `ToolChoiceParam` | `tool.permissions`, `command.allowed_tools` (or new) | server + cli + sdk | needs registry/matrix entry |
| Conversation state (`previous_response_id`, `conversation`) | spec: previous_response_id | `Conversation`, `ResponsesConversationParam` | `thread.reference`, `context.compile` | server + cli + sdk | needs registry/matrix entry |
| Truncation policy | spec: truncation | `TruncationEnum` | `compaction.*` | server + cli + sdk | needs registry/matrix entry |
| Service tier / routing | spec: service_tier | `ServiceTierEnum` | `models.routing` (new) | server + sdk | needs registry/matrix entry |
| Reasoning controls + summaries | spec: Reasoning | `Reasoning`, `ReasoningParam`, reasoning summary content | `execution.structured_output` (partial) + new reasoning capability | server + cli + sdk | needs registry/matrix entry |
| Content types (text/image/file/video) | spec: Content | content schemas + image/video params | `context.refs.file` + new multimodal capability | server + cli + sdk | needs registry/matrix entry |
| Errors | spec: Errors | `Error`, `ErrorPayload`, streaming error event | `session.stream_events`, server error model | server + cli + sdk | mapped to frames; need capability mapping |
| Usage + token counts | spec: (implicit via schema) | `Usage`, `TokenCountsBody/Resource` | new `usage.token_counts` capability | server + sdk | needs registry/matrix entry |
| Extensions | spec: Extending Open Responses | extension schema refs | `extensions.*`, `hooks.*` | server + tui + mcp | needs registry/matrix entry |

Next steps
- Expand each feature group into concrete capability ids in `capability_registry.md`.
- Update `capability_matrix.md` to reflect surface parity.
- Add roadmap items for any feature not covered by Phase 1 scope.
