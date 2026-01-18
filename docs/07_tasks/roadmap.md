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
- OpenResponses capability coverage inventory + exhaustive checklist [confirm spec]
  - Refs: `docs/03_contracts/openresponses_coverage.md`, `docs/07_tasks/openresponses_coverage.md`, `temp/openresponses/schema/`, `temp/openresponses/src/pages/specification.mdx`, `docs/03_contracts/modules/phase-1/02_provider_adapters.md`
  - Ready: OpenResponses schema repo present in `temp/openresponses`
  - Gate: no provider-logic changes until coverage inventory + checklist are complete
  - Done: coverage inventory enumerates every spec feature + schema/event/item; checklist covers all schema components; roadmap references coverage for future phases
- OpenResponses capability alignment (spec + schemas -> capability registry/matrix/roadmap) [confirm spec]
  - Refs: `docs/03_contracts/openresponses_capability_map.md`, `docs/03_contracts/capability_registry.md`, `docs/02_architecture/capability_matrix.md`, `temp/openresponses/src/pages/specification.mdx`, `temp/openresponses/src/pages/reference.mdx`
  - Ready: coverage inventory and schema inventory available; spec MDX reviewed
  - Done: every OpenResponses feature is mapped to capability ids; capability matrix updated for surface parity; roadmap reflects missing capability work
- Capability validation pass (parity + headless schema + tool conformance + server smoke) [needs work]
  - Refs: `docs/05_quality/tests.md`, `docs/05_quality/surface-parity.md`, `docs/03_contracts/event_frames.md`, `docs/03_contracts/capability_registry.md`, `docs/07_tasks/phase-1/03_tool_runtime.md`, `docs/04_execution/cli.md`, `docs/04_execution/server.md`
  - Ready: event frame schema stable; tool runtime emits structured tool events; server session API stable
  - Done: parity matrix + gap list enforced in CI; headless JSON schema validation; tool runtime conformance tests (read/write/grep/ls + limits + line ranges); server API smoke tests (sessions + SSE ordering)
- CLI interactive streaming renderer (minimal UI) [needs work]
  - Refs: `docs/07_tasks/phase-1/06_cli.md`, `docs/03_contracts/modules/phase-1/05_cli.md`, `docs/04_execution/cli.md`
  - Ready: event frame schema final; interactive input/controls scope confirmed
  - Done: interactive UI renders event stream (no diffs/approvals in Phase 1); golden render tests

Next
- Workspace engine: checkpoint + rewind hooks integration in runtime [needs work]
  - Refs: `docs/07_tasks/phase-1/04_workspace_engine.md`, `docs/03_contracts/modules/phase-1/04_workspace_engine.md`
  - Ready: hook points in runtime defined; checkpoint event frames defined
  - Done: runtime integration + rewind behavior tests

Later
- TUI surface (`rip-tui`) plan + MVP renderer [needs work]
  - Refs: `docs/02_architecture/surfaces.md`, `docs/02_architecture/capability_matrix.md`, `temp/docs/ratatui/notes.md`
  - Ready: confirm TUI stack + input model; define surface-specific capabilities
  - Done: `rip-tui` package skeleton + streaming renderer; golden render tests
- MCP surface (`rip-mcp`) parity adapter [needs work]
  - Refs: `docs/02_architecture/surfaces.md`, `docs/02_architecture/capability_matrix.md`
  - Ready: server capability registry expanded; MCP protocol mapping defined
  - Done: MCP server exposes core capabilities + session lifecycle
- Benchmarks: TTFT, parse overhead, tool dispatch, patch throughput, end-to-end loop [needs work]
  - Refs: `docs/07_tasks/phase-1/08_benchmarks.md`, `docs/05_quality/benchmarks.md`
  - Ready: event frames + tool runtime stable
  - Done: CI-gated benchmarks with baseline budgets
- Fixtures: deterministic tool outputs + replayable logs [needs work]
  - Refs: `docs/07_tasks/phase-1/09_fixtures.md`
  - Ready: tool runtime emits deterministic frames
  - Done: fixture repos + replay tests in CI
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
- Interactive CLI (minimal streaming renderer) is specified but not implemented (see `docs/07_tasks/phase-1/06_cli.md`).
- TUI surface is documented but not implemented (`rip-tui`).
- MCP surface is documented but deferred to Phase 2 (`rip-mcp`).
- Tool runtime not yet integrated into session execution.
- Benchmarks are required by docs but no harness exists.

Decisions
- Event frames live in `rip-kernel`; schema documented at `docs/03_contracts/event_frames.md`.
- Phase 1 frame types: `session_started`, `output_text_delta`, `session_ended`, `provider_event`, tool events.

Open questions
- (empty)

Done (recent)
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
