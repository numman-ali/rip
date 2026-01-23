# Contract: provider adapters (Open Responses)

Summary
- Translate between internal frames and provider protocol.
- Open Responses used only at the boundary.
- Canonical schema source is the bundled OpenAPI JSON synced into this repo; split component/paths schemas are vendored for full tool and streaming validation.
- SSE parsing uses a deterministic decoder; `[DONE]` is treated as terminal.

Inputs
- Internal request frames (model, instructions, tools, context).
- Compiled context bundles (`rip.context_bundle.v1`) produced by `context.compile` (provider-agnostic input to the adapter).

Outputs
- Internal event frames mapped from provider SSE events (`docs/03_contracts/event_frames.md`).
- Streaming event `type` validation against Open Responses schema-derived list.
- Full streaming event and response validation against OpenAPI JSON schemas.

Spec sync
- Run `scripts/update-openresponses-types` to sync `schemas/openresponses/openapi.json` and derived event types.

Config
- Provider selection and routing rules.
- Retry policy and timeouts.

Invariants
- Preserve event order and timestamps.
- No transformation that loses semantic meaning.
- Emit one `provider_event` frame per SSE event (including `[DONE]` and invalid JSON).
- Provider conversation state (cursors) is cache only (ADR-0010); adapters must never be required to reconstruct continuity truth.

OpenResponses request shape (for `context.compile`)
- Spec/schema reviewed: `temp/openresponses/src/pages/specification.mdx` and `temp/openresponses/schema/components/schemas/CreateResponseBody.json`.
- `CreateResponseBody.input` is a union: string (user message) or `ItemParam[]` (structured items).
- `previous_response_id` semantics are explicitly: `previous_response.input` -> `previous_response.output` -> new `input`.
- RIP posture:
  - Cross-run “memory” is provided by `rip.context_bundle.v1` (continuity truth), rendered into `CreateResponseBody.input` as `ItemParam[]`.
  - Fresh runs omit `previous_response_id` (provider state is not required for continuity truth).
  - In-run tool follow-ups remain OpenResponses-native: either `previous_response_id`-based (default) or stateless-history (opt-in) per config.

Phase 1 mapping
- All SSE events map to `provider_event` frames with full payload fidelity.
- No events are dropped; `[DONE]` is captured as a `provider_event` with `status=done`.
- `response.output_text.delta` also emits `output_text_delta` frames (derived, no payload loss).

Tests
- Acceptance fixtures against Open Responses schema.
- Golden stream replay vs expected internal frames.

Benchmarks
- Parse overhead per SSE event.
- TTFT overhead (first byte -> first internal event).
