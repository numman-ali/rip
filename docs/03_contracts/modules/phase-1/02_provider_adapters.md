# Contract: provider adapters (Open Responses)

Summary
- Translate between internal frames and provider protocol.
- Open Responses used only at the boundary.

Inputs
- Internal request frames (model, instructions, tools, context).

Outputs
- Internal event frames mapped from provider SSE events.
- Streaming event `type` validation against Open Responses schema-derived list.

Config
- Provider selection and routing rules.
- Retry policy and timeouts.

Invariants
- Preserve event order and timestamps.
- No transformation that loses semantic meaning.

Tests
- Acceptance fixtures against Open Responses schema.
- Golden stream replay vs expected internal frames.

Benchmarks
- Parse overhead per SSE event.
- TTFT overhead (first byte -> first internal event).
