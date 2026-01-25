# SDK Execution Model (TypeScript)

Summary
- SDK is a thin adapter over the canonical event frames (`docs/03_contracts/event_frames.md`).
- Default transport is local exec: spawn `rip run ... --headless --view raw` and parse JSONL frames from stdout.
- Optional remote: target a running server via `rip run --server <url> ...` and consume the same frames.

TypeScript SDK
- Source: `sdk/typescript`
- Current posture:
  - No business logic in the SDK (only transport + frame parsing + light aggregation helpers).
  - The authoritative log is the frame stream (SDK can expose convenience fields derived from frames).
  - Thread APIs are accessed via the `rip` CLI adapter (`rip threads ...`), keeping SDK transport consistent with ADR-0006:
    - JSON responses: `thread.ensure`, `thread.list`, `thread.get`, `thread.branch`, `thread.handoff`, `thread.post_message`
    - JSONL frames: `thread.stream_events` (continuity stream; past + live)
  - Task APIs are accessed via the `rip` CLI adapter (`rip tasks --server ...`), keeping SDK transport consistent with ADR-0006.

Portability rules (Phase 1)
- If the host can spawn a native binary and has a real workspace filesystem: use **exec transport** (default) and run locally (no daemon required).
- If the host cannot spawn processes (edge/serverless sandboxes, mobile apps) but can do HTTP: use the **opt-in direct HTTP/SSE transport** to a remote control plane (`rip serve` / `ripd`) (ADR-0017).
- Bundling a binary inside an npm package improves install UX on desktop/server but does not make RIP runnable in environments that prohibit native execution.
- Mobile apps (iOS/Android) are treated as remote surfaces: they attach to a control plane and render/drive frames; they do not run the full RIP tool runtime locally in Phase 1.

Continuity authority (multi-device / multi-plane)
- Continuity truth lives with the runtime that owns the event log + artifacts.
- To use the *same* continuity across machines (laptop/VPS/serverless clients), point all clients at the same control plane (`--server <url>` or HTTP transport).
- Multi-writer sync/merge across independent logs is a deferred capability and must be explicit and replay-safe (future phase).

Local dev
- `scripts/check-sdk-ts` (builds `rip` + runs SDK tests)
