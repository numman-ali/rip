# ADR-0017: SDK transport + distribution (exec-by-default; optional direct HTTP; PATH-first)

Status
- Accepted (2026-01-25)

Decision packet
- Decision: how the TypeScript SDK locates/ships the `rip` binary and whether it should have a direct-HTTP transport to the RIP server, while keeping the SDK thin over canonical event frames.
- Options:
  1) PATH-installed `rip` only; exec transport only (current).
     - Pros: minimal SDK; deterministic; local-first with no server requirement.
     - Cons: cannot run where subprocess spawn is disallowed; remote users still need a binary; per-turn process startup overhead.
  2) Bundle `rip` binaries inside the npm SDK; exec transport only.
     - Pros: `npm install` UX; no PATH fiddling; reproducible versions.
     - Cons: release pipeline complexity; larger packages; supply-chain/security review burden; cross-platform artifact management.
  3) Keep PATH-first exec transport as default; add opt-in direct-HTTP transport for server mode; defer bundling.
     - Pros: preserves ADR-0006 defaults; enables spawn-less environments for remote; avoids new deps (Node 18 `fetch`); keeps protocol canonical (frames).
     - Cons: add HTTP/SSE parsing code + tests; transport matrix to maintain.
- Recommendation: Option 3.
- Reversibility: transport selection is additive and capability ids/frames are unchanged; bundling (if desired) can be added later as an optional package without changing replay semantics.

Context
- ADR-0006 sets the SDK posture: thin adapter over the `rip` binary + JSONL event frames; remote currently works by still spawning `rip` with `--server`.
- Phase 1 constraints: no new third-party deps; continuity truth is the event stream; no hidden mutable state.
- Some environments (locked-down runtimes, serverless, embedded) cannot spawn subprocesses but can make HTTP requests.
- Server already exposes canonical capabilities over HTTP/SSE (`schemas/ripd/openapi.json`), streaming event frames as SSE `data:` messages (`crates/ripd/src/server_tests.rs`).

Decision

## 1) Distribution: PATH-first (no bundling by default)
- `rip-sdk-ts` continues to assume `rip` is available via PATH, or via explicit `executablePath`.
- The SDK does not download or bundle binaries by default in Phase 1.
- Future bundling (if/when desired) is tracked separately and must be an explicit, reviewed distribution mechanism (no silent downloads).

## 2) Transport: add opt-in direct HTTP (server-only)
- Add a second SDK transport that talks directly to the server API:
  - Uses Node 18+ `fetch` and a small SSE parser (no deps).
  - Streams canonical event frames unchanged.
  - Supports explicit `headers`/auth injection and a `fetch` override for future proxy/auth needs.
- Default remains exec transport (ADR-0006). No behavior changes without opt-in.

## 3) Parity + determinism rules
- SDK stays a surface adapter; no agent/runtime business logic is added.
- Both transports map to the same capability ids and return the same frame shapes.
- The authoritative log remains the frame stream; any convenience aggregation stays derived.

Implementation slices (recommended)
1) Introduce a small `RipTransport` interface in `sdk/typescript` with `exec` (existing) and `http` implementations.
2) Implement HTTP client coverage for the current SDK surface:
   - Sessions: create/input/cancel/events (SSE -> frames)
   - Threads: ensure/list/get/post_message/branch/handoff/events
   - Tasks: spawn/list/status/cancel/output/events/PTY controls
3) Add contract tests that run both transports against a local `rip serve` instance and assert frame-level equivalence for a small fixture run.
4) Keep CI green: `scripts/check-fast` + `scripts/check-sdk-ts`.

Consequences
- Pros: remote integration without a local binary; preserves local-first exec default; avoids new deps.
- Cons: transport matrix to test; HTTP transport only works when a server exists.

References
- `docs/06_decisions/ADR-0006-sdk-transport.md`
- `docs/04_execution/sdk.md`
- `docs/03_contracts/event_frames.md`
- `schemas/ripd/openapi.json`
