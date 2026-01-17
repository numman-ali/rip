# Contract: provider adapters (Open Responses)

Summary
- Translate between internal frames and provider protocol.
- Open Responses used only at the boundary.
- Canonical schema source is the bundled OpenAPI JSON synced into this repo; split component schemas are vendored for full tool validation.
- SSE parsing uses a deterministic decoder; `[DONE]` is treated as terminal.

Inputs
- Internal request frames (model, instructions, tools, context).

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
