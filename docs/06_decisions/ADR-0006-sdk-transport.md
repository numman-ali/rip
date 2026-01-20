# ADR-0006: TypeScript SDK transport uses the `rip` binary + JSONL event frames (optional remote via `--server`)

Status
- Accepted

Context
- We need a first-class TypeScript SDK surface for programmatic use (Phase 2) without duplicating agent/runtime logic in TypeScript.
- Phase 1 already has:
  - A Rust core runtime (`ripd`) that emits canonical event frames (`docs/03_contracts/event_frames.md`).
  - A CLI (`rip`) that can run in-process by default and emit newline-delimited JSON frames (`rip run --headless --view raw`).
  - A server (`rip serve` / `ripd`) that exposes the same frames over HTTP/SSE.
- We want local-first ergonomics and performance (no required daemon), while still supporting remote control via the server.
- Comparable systems (e.g. Codex) ship a TypeScript SDK that wraps a bundled native binary and exchanges JSONL events over stdio (`temp/codex/sdk/typescript/README.md`) and explicitly treat the protocol as transport-agnostic (`temp/codex/codex-rs/docs/protocol_v1.md`).

Decision
- The TypeScript SDK is a **thin adapter** over the Rust runtime via the `rip` binary:
  - Default transport: spawn `rip run ... --headless --view raw` and parse JSONL event frames from stdout.
  - Remote transport: still spawn `rip`, but pass `--server <url>` so the same SDK can target a remote server without reimplementing HTTP/SSE client logic.
- The SDK must not implement agent/runtime business logic:
  - It may provide convenience aggregation (e.g. “final output text”, “tool results”, “files changed”) derived from frames, but the authoritative log is the frame stream.
- Cancellation uses process-level control (e.g. `AbortSignal` -> terminate the spawned `rip` process). Future server-backed cancellation remains available via `--server`.

Consequences
- Pros:
  - Local-first (no daemon required) and minimal surface-specific logic.
  - High parity: the SDK consumes the same canonical frames as CLI/headless/server.
  - Remote support remains available without blocking on a separate HTTP client implementation.
- Cons:
  - Requires a `rip` binary to be present (installed or bundled) and incurs process startup overhead per run.
  - Environments that cannot spawn subprocesses will need a future direct HTTP client transport.
- Reversibility:
  - We can add an explicit direct-HTTP transport later without breaking replay or parity, because the canonical interchange remains event frames and capability ids.
