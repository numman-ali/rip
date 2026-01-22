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
    - JSON responses: `thread.ensure`, `thread.list`, `thread.get`, `thread.post_message`
    - JSONL frames: `thread.stream_events` (continuity stream; past + live)
  - Task APIs are accessed via the `rip` CLI adapter (`rip tasks --server ...`), keeping SDK transport consistent with ADR-0006.

Local dev
- `scripts/check-sdk-ts` (builds `rip` + runs SDK tests)
