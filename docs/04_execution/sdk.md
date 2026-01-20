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

Local dev
- `scripts/check-sdk-ts` (builds `rip` + runs SDK tests)
