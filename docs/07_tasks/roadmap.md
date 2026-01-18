# Rolling Roadmap

Summary
- Single source for now/next/later decisions and capability coverage across surfaces.
- Lightweight on purpose but exhaustive; detailed task specs remain in `docs/07_tasks/phase-1/`.
- Each actionable item includes references + start/finish criteria so a fresh context can resume fast.

How to use
- Every actionable item includes a confidence tag: `[confirm spec]` or `[needs work]`.
- `[needs work]` means confirm spec or design choice before implementation.
- Now/Next/Later items include refs, ready checklist, and done criteria.
- Coverage map is an index only (no checklists), used to ensure every capability group is tracked.
- Date-stamp moves between Now/Next/Later to preserve intent over time.

Now

## Bench budgets: ratchet TTFT + end-to-end loop [confirm spec]
- Refs: `docs/05_quality/benchmarks.md`, `docs/05_quality/benchmarks_budgets.json`, `scripts/bench`
- Ready:
  - Capture CI baselines for `ttft_overhead_us` and `e2e_loop_us` (multiple runs).
- Done:
  - Budgets tightened with explicit headroom; regressions fail CI.

Next

## Fixtures: deterministic tool outputs + replayable logs [needs work]
- Refs: `docs/07_tasks/phase-1/09_fixtures.md`
- Ready: tool runtime emits deterministic frames
- Done: fixture repos + replay tests in CI

Later
- Models & providers: multi-provider + routing + catalogs (Phase 2) [needs work]
  - Refs: `docs/03_contracts/modules/phase-2/01_model_routing.md`, `docs/03_contracts/capability_registry.md`, `docs/02_architecture/capability_matrix.md`, `docs/03_contracts/event_frames.md`
  - Ready: OpenResponses boundary stable; routing decisions can be logged as event frames
  - Done: multi-provider configs + versioned model catalogs; per-turn `{provider_id, model_id}` switching; routing policy (advisory/authoritative) with replayable recorded decisions
- Extensions: plugin host (WASM-first + out-of-process services) (Phase 2) [confirm spec]
  - Refs: `docs/03_contracts/modules/phase-2/02_extension_host.md`, `docs/06_decisions/ADR-0004-plugin-architecture.md`, `docs/03_contracts/capability_registry.md`, `docs/03_contracts/event_frames.md`
  - Ready:
    - Define plugin manifest + host interface versioning.
    - Define canonical UI request/response frames (surface-agnostic; UI renders, headless streams).
    - Define determinism + replay requirements for WASM and out-of-process plugins.
  - Done:
    - Load a WASM plugin that can intercept tool calls and emit deterministic frames.
    - Provide at least one plugin-defined tool renderer (as render-hint frames).
    - Replay fixture covers plugin decisions and reproduces identical snapshots.
- TUI surface (`rip-tui`) plan + MVP renderer [needs work]
  - Refs: `docs/02_architecture/surfaces.md`, `docs/02_architecture/capability_matrix.md`, `temp/docs/ratatui/notes.md`
  - Ready: confirm TUI stack + input model; define surface-specific capabilities
  - Done: `rip-tui` package skeleton + streaming renderer; golden render tests
- MCP surface (`rip-mcp`) parity adapter [needs work]
  - Refs: `docs/02_architecture/surfaces.md`, `docs/02_architecture/capability_matrix.md`
  - Ready: server capability registry expanded; MCP protocol mapping defined
  - Done: MCP server exposes core capabilities + session lifecycle
- SDK surface parity (TypeScript first) [needs work]
  - Refs: `docs/02_architecture/component_map.md`, `docs/02_architecture/capability_matrix.md`
  - Ready: session API + event frames stable
  - Done: TS SDK supports session lifecycle + streaming

Capability coverage map (index)
- Sessions & threads [confirm spec] - Phase 1 core + server + CLI; TUI/SDK parity later.
- Session storage & replay [confirm spec] - Phase 1 event log + snapshots; surfaces consume.
- Context & guidance [needs work] - Phase 2 context compiler + guidance loader.
- Configuration & policy [needs work] - Phase 2 layered config + permission engine.
- Commands & automation [needs work] - Phase 1 in-memory registry; Phase 2 disk-based commands.
- Execution modes [needs work] - Phase 1 interactive/headless + JSONL; Phase 2 RPC/SDK expansion.
- Tools & tooling [confirm spec] - Phase 1 tool runtime; policy integration pending.
- Compaction & summarization [needs work] - Phase 2 compaction engine.
- Policy & steering [needs work] - Phase 3 adaptive budgets + rule engine.
- Extensions & hooks [needs work] - Phase 2 extension registry + hook bus.
- Skills [needs work] - Phase 2 skill loader + commands.
- Subagents [needs work] - Phase 2 subagent manager + budgets.
- Models & providers [needs work] - Phase 1 adapter boundary; multi-provider routing.
- Output styles [needs work] - Phase 2 style registry.
- UI/interaction [needs work] - Phase 2 TUI + interaction affordances.
- Integrations [needs work] - Phase 2 MCP/IDE/LSP.
- Background workers [needs work] - Phase 3.
- Checkpointing & rewind [needs work] - Phase 1 workspace engine integration.
- Security & safety [needs work] - Phase 1 baseline + Phase 3 extended sandboxing.
- Search/index & memory [needs work] - Phase 3.

Doc/impl gaps
- TUI surface is documented but not implemented (`rip-tui`).
- MCP surface is documented but deferred to Phase 2 (`rip-mcp`).
- Bench harness includes TTFT + end-to-end loop benchmarks; budgets are intentionally conservative (ratchet over time).
- Fixture repos exist (`fixtures/repo_small`, `fixtures/repo_medium`), but replayable “full agent loop” fixtures are still pending.

Decisions
- Event frames live in `rip-kernel`; schema documented at `docs/03_contracts/event_frames.md`.
- Phase 1 frame types: `session_started`, `output_text_delta`, `session_ended`, `provider_event`, tool events.

Open questions
- (empty)

Done (recent)
- 2026-01-18: Phase 1 closeout: CI + fixtures + bench harness are CI-gated (plus baseline budgets).
- 2026-01-18: Decision: plugin architecture is WASM-first with optional out-of-process services (ADR-0004).
- 2026-01-18: Phase 1 hygiene: tests use temp workspace roots (no writes under `crates/*`).
- 2026-01-18: Benchmarks: added TTFT (`ttft_overhead_us`) + end-to-end loop (`e2e_loop_us`) CI gates.
- 2026-01-18: Provider integration wired (OpenResponses SSE -> `provider_event` + derived deltas + tool loop in `ripd`, env-configured, integration tests).
- 2026-01-18: Agent loop: provider `function_call` -> ToolRunner -> `previous_response_id` follow-ups (ADR-0005 + integration test).
- 2026-01-18: Workspace checkpoint hooks wired into ripd session execution (tool + checkpoint envelopes, tests).
- 2026-01-18: CLI interactive streaming renderer complete (minimal UI + golden render test).
- 2026-01-18: Capability validation pass complete (parity + headless schema + tool conformance + OpenResponses invariants + server smoke).
- 2026-01-18: OpenResponses capability alignment complete (capability map + matrix updates + roadmap tracking).
- 2026-01-18: OpenResponses coverage inventory + exhaustive checklist complete (schemas + spec MDX); coverage map reconciled.
- 2026-01-16: Capability parity matrix + gap list enforcement added.
- 2026-01-16: Headless CLI validates JSON event frames.
- 2026-01-16: Built-in tools crate + conformance tests added.
- 2026-01-16: Server OpenAPI spec generation + schema snapshot.
- 2026-01-16: Server SSE compliance tests + session lifecycle integration.
- 2026-01-16: Tool runtime emits structured tool events with limits + tests.
- 2026-01-16: Provider adapter emits full provider_event frames + fixtures/tests.
- 2026-01-16: Event log replay equivalence + corruption detection tests.
- 2026-01-16: Event frame schema defined + serialized across ripd/log/CLI.
- 2026-01-16: Roadmap expanded to include full surface/capability coverage.
- 2026-01-16: Capability registry expanded to cover full baseline + surface support fields.
- 2026-01-16: Command registry contract implemented + tests.
- 2026-01-16: Session hooks engine implemented + tests.
