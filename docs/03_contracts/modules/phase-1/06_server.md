# Contract: agent server

Summary
- Exposes the coding agent via HTTP/SSE sessions.
- Not an Open Responses API.
- Canonical control plane for active capabilities; API surface must match the capability registry.

Inputs
- Session lifecycle requests (start, send input, cancel).
- Tool/checkpoint command envelopes via session input (for deterministic testing).

Outputs
- Structured event stream over SSE (event frames: `docs/03_contracts/event_frames.md`).
- Session status and artifacts.
- OpenAPI spec generated from server code and exposed at a canonical endpoint.

Config
- Bind address, auth, session limits.

Invariants
- One session maps to one agent run.
- Event stream is ordered and replayable.

Tests
- Session lifecycle integration tests.
- SSE stream compliance tests.
- OpenAPI schema generation/validation tests.
