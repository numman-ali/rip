# Contract: Extension Host (Phase 2)

Summary
- Provides a first-class extension system that can add tools, intercept decisions, customize rendering, and integrate external systems.
- Default plugin boundary is WASM; heavy modules may run out-of-process.
- Extensions must preserve determinism and replay: all extension decisions are expressed as structured event frames.
- Surfaces remain thin adapters: extensions do not inject business logic into UI/transport layers.

Related capabilities
- `extension.api`, `extension.state`, `extension.ui`, `extension.event_bus`, `extension.commands`, `extension.tool_renderers`, `extension.errors`, `extension.session_control`, `extension.agent_state`
- `hooks.*` (permission/compaction/matching/types/timeouts)
- `tool.*` (registry/override/remote/permissions)
- `execution.rpc`, `execution.json_input` (for request/response style UI and automation)

Goals
- Allow extensions to:
  - register tools and tool renderers (call/result)
  - intercept tool calls/results (allow/deny/modify)
  - inject context/messages and participate in compaction
  - participate in model routing decisions (advisory/authoritative)
  - request UI interactions via structured UI frames (select/confirm/input/custom)
  - persist extension state as session entries
- Keep hot path fast: extension overhead must be bounded and benchmarked.
- Preserve replay fidelity: given the same recorded inputs/events, the same outputs must be produced.

Non-goals (initial)
- Arbitrary, surface-specific UI code injection (no “run custom TUI code in-process” as the default).
- Extension-managed network access without explicit policy profiles.
- Unlogged side effects.

Architecture (hybrid)

1) WASM extensions (default)
- Packaging: `.wasm` module + manifest metadata (id, version, requested capabilities, policy hints).
- Interface: versioned host ABI (WIT/Wasm component model or equivalent stable ABI).
- Execution: sandboxed, budgeted (time/memory), deterministic by default.

2) Out-of-process extension services (optional)
- Used for heavy workloads: indexing, retrieval, long-running integrations.
- Transport: structured RPC (local loopback or unix domain socket) with strict schemas.
- Replay: every RPC request/response is logged as event frames; replays may “mock” the service by playing back recorded responses.

3) Trusted native plugins (exception)
- Only in an explicit “trusted” profile; not a default distribution mechanism.
- Must still emit the same structured frames and preserve replay.

Hook points (Phase 2 target)
- Session lifecycle: start/end, cancel.
- Turn lifecycle: turn start/end, input normalization.
- Tool pipeline:
  - before tool dispatch (permission, allow/deny/modify args, select tool impl)
  - after tool result (modify result, attach artifacts, render hints)
- Compaction:
  - before compaction (provide instructions, cut-point rules)
  - compaction provider (optional override of summary generation)
- Model routing:
  - advisory routing decision event
  - authoritative routing decision event (if enabled by policy profile)
- UI:
  - emit `ui_request` frames; consume `ui_response` inputs (mode-dependent).

Extension outputs (canonical)
- Extensions do not mutate internal state invisibly.
- Extensions emit structured frames representing:
  - decisions (allow/deny/modify) and their reasons
  - additional messages/context injections
  - tool render hints and UI requests
  - persistent state entries

Determinism & replay rules
- Every extension invocation must be reproducible from the event log:
  - inputs to extension hooks are captured as frames (or derivable from prior frames)
  - outputs from hooks are captured as frames
- External IO is only allowed through:
  - explicit tools (whose results are logged), or
  - out-of-process services with fully logged RPC.

Performance gates (initial)
- p50/p99 overhead budgets for:
  - “no extensions loaded” baseline
  - “N extensions loaded, no-op hooks”
  - “tool_call interception” path
- Benchmarks must be CI-gated and ratcheted over time.

Tests (required)
- Contract tests:
  - hook ordering is stable and deterministic
  - allow/deny/modify semantics are correct
  - UI request/response protocol is consistent across surfaces
- Replay tests:
  - recorded extension decisions reproduce identical final snapshot and frame stream
- Negative tests:
  - extension timeouts and faults are contained and logged
  - policy profile enforcement (safe vs full-auto) is respected
