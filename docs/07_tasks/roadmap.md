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

## Tools: background tool tasks + interactive PTY control [confirm spec]
- Refs:
  - `docs/06_decisions/ADR-0007-tool-tasks-pty.md`
  - `docs/03_contracts/modules/phase-2/03_tool_tasks.md`
  - `docs/03_contracts/event_frames.md`
  - `docs/03_contracts/capability_registry.md`
  - `docs/02_architecture/capability_matrix.md`
- Spec snapshot:
  - Background work is a **task entity** (`task_id`) with its own event stream; Phase 1 session invariant remains (one session == one run).
  - Execution modes: `pipes` (default) and `pty` (opt-in; stdin/resize/signal).
  - Artifact-backed logs prevent context/log explosion; frames carry previews + artifact refs.
  - “Wake the agent” is orchestration: watchers start a new session referencing `{task_id, artifact_refs}`.
- Ready:
  - Implement task registry + event stream plumbing (task spawn/status/cancel + stream).
  - Implement PTY mode (policy-gated) with stdin/resize/signal operations.
  - Implement artifact-backed logs for long-running tasks (range fetch).
  - Add deterministic replay fixtures for: pipes task, PTY task, cancel, and “attach late”.
- Done:
  - `tool.task_*` capabilities are exposed via server + SDK; CLI/TUI can list, stream, and control tasks.
  - Replay fixtures cover: spawn→output→exit, spawn→cancel, PTY stdin/resize/signal, and artifact refs.
  - Bench budgets cover task overhead (registry + per-delta emit) and do not regress TTFT/loop latency.

Next

## SDK: transport + distribution (direct HTTP, packaging) [needs work]
- Refs: `docs/06_decisions/ADR-0006-sdk-transport.md`, `docs/04_execution/sdk.md`
- Ready:
  - `sdk/typescript` is exercised by `scripts/check-sdk-ts`.
- Done:
  - Decide distribution (PATH vs bundled binaries).
  - Decide whether to add a direct-HTTP transport (keeping SDK as a thin adapter over canonical frames).

Later
- Sessions/Threads: resume + branch (multi-turn workspaces) [needs work]
  - Refs: `docs/03_contracts/capability_registry.md` (`session.resume`, `thread.branch`, `thread.handoff`), `docs/03_contracts/modules/phase-1/06_server.md`
  - Decision packet:
    - Decision: how “continue later” is represented and exposed across surfaces without breaking Phase 1 replay invariants.
    - Options:
      1) Multi-turn session: allow repeated `session.send_input` on the same session id; redefine `session_ended` as end-of-turn.
         - Pros: simplest mental model for users.
         - Cons: breaks Phase 1 invariants (`session_ended` terminal); harder replay/compaction; complicates background tasks insertion.
      2) Thread entity: keep Phase 1 sessions as single-run “turns”; introduce `thread_id` and attach new session runs to a thread.
         - Pros: preserves Phase 1 session invariants; clean replay; enables branch/handoff/compaction naturally.
         - Cons: requires new server endpoints + SDK/CLI/TUI wiring.
    - Recommendation: Option 2 (thread entity). Keep “session == run” stable; implement “continue later” as threads built from session runs.
    - Reversibility: multi-turn sessions can later be added as a thin compatibility layer that creates/targets a thread behind the scenes.
  - Ready:
    - Define server API endpoints for thread create/list/resume/branch and how they map to session runs.
    - Define event-log entries for thread metadata + turn links (replayable).
  - Done:
    - “Continue later” works the same in CLI/TUI/SDK (parity enforced), with deterministic replay of a resumed thread.
- OpenResponses: parallel tool calls + background responses [needs work]
  - Refs: `docs/06_decisions/ADR-0005-openresponses-tool-loop.md`, `crates/ripd/src/provider_openresponses.rs`
  - Decision packet:
    - Decision: whether to support `parallel_tool_calls(true)` and `background(true)` in Phase 2 without breaking replay determinism.
    - Recommendation: keep Phase 1 strict (`parallel_tool_calls(false)`, `background(false)`), then add an explicit capability + policy-gated opt-in for parallel/background execution once task entities + threads exist.
  - Done:
    - Parallel tool calls are supported behind an explicit capability flag and are replayable (ordering + concurrency recorded).
    - Background responses are supported via task entities (spawn/poll/stream) with deterministic event framing.
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
- Skills: Agent Skills standard loader + progressive disclosure [needs work]
  - Refs: `docs/03_contracts/modules/phase-2/04_skills.md`, `docs/03_contracts/capability_registry.md`, `docs/02_architecture/capability_matrix.md`, `temp/docs/agentskills/notes_2026-01-19.md`
  - Decision packet:
    - Decision: default skill discovery roots, collision precedence, and how skills become invokable commands.
    - Options:
      1) RIP-native roots only (`~/.rip/skills`, `<workspace>/.rip/skills`).
         - Pros: least surprising; clean separation from other tools.
         - Cons: less immediate reuse of existing skill libraries.
      2) Include compatibility roots (Codex/Claude/Pi) by default.
         - Pros: instant ecosystem reuse.
         - Cons: surprising defaults; more collisions; harder policy story.
    - Recommendation: Option 1 by default; Option 2 available via explicit config flags (policy-gated).
    - Reversibility: roots + precedence are config-driven; collisions are logged; can expand safely later.
  - Ready:
    - Decide default skill discovery roots + collision precedence (explicit and deterministic).
    - Define skill events and how skills map to commands (`/skill:<name>`).
    - Define how `allowed-tools` interacts with policy profiles (safe vs full-auto).
  - Done:
    - Skill catalog is injected by the context compiler (frontmatter-only; fast scan).
    - Skill activation loads full `SKILL.md` and can run scripts/tools under policy.
    - Server + SDK expose skill lifecycle events; CLI/TUI render them; replay fixtures cover end-to-end behavior.
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
- TUI surface has design docs + an MVP-0 renderer crate (`rip-tui`) and can attach to a live server stream; richer interactions (threads/palette/editor/resume) remain Phase 2.
- MCP surface is documented but deferred to Phase 2 (`rip-mcp`).
- Bench harness includes TTFT + end-to-end loop benchmarks; budgets are intentionally conservative (ratchet over time).
- Fixture repos exist (`fixtures/repo_small`, `fixtures/repo_medium`); OpenResponses tool-loop fixtures cover tool execution + follow-up + snapshot/log replay equivalence.

Decisions
- Event frames live in `rip-kernel`; schema documented at `docs/03_contracts/event_frames.md`.
- Phase 1 frame types: `session_started`, `output_text_delta`, `session_ended`, `provider_event`, tool events.

Open questions
- (empty)

Done (recent)
- 2026-01-20: TUI remote attach: `rip --server <url> --session <id>` streams server SSE frames into the fullscreen UI; fixture + snapshot parity test.
- 2026-01-20: Default UX: `rip` launches a fullscreen terminal UI (TUI) and runs the session engine in-process; `rip run` remains the headless/automation entrypoint.
- 2026-01-20: TypeScript SDK scaffold added (`sdk/typescript`): spawns `rip` and parses JSONL event frames; `scripts/check-sdk-ts` wired into `scripts/check`; CI check-fast now pins stable toolchain for fmt/check/clippy/test.
- 2026-01-19: OpenResponses validation compatibility: normalize missing item ids for schema validation (raw events preserved); output view now streams only output_text (tool stdout/stderr fallback when no model output).
- 2026-01-19: CLI: added provider/model/stateless/followup flags for local OpenResponses testing; OpenAI/OpenRouter one-liners documented.
- 2026-01-19: OpenResponses follow-up compatibility is opt-in (`RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE`); spec-canonical tool-output-only follow-ups remain default.
- 2026-01-19: OpenResponses stateless history mode added (`RIP_OPENRESPONSES_STATELESS_HISTORY`) for provider compatibility; tools emitted with `strict: false`.
- 2026-01-19: Live OpenResponses API smoke test: passed against configured endpoint (`live_openresponses_smoke`).
- 2026-01-19: Bench budgets: ratcheted to tight CI gates (sse_parse=200us/event, ttft=200us, tool_runner_noop=100us, workspace_apply_patch=2000us, e2e_loop=10000us).
- 2026-01-19: Tools: `bash` stores oversized stdout/stderr as workspace-local artifacts + added `artifact_fetch` builtin (range reads).
- 2026-01-19: CLI: added `rip serve` (embedded server) to reduce `ripd` UX friction.
- 2026-01-19: CLI: `rip run` defaults to in-process execution; `--server <url>` targets remote; server+CLI share the same session runner.
- 2026-01-19: TUI: added `rip-tui` MVP-0 skeleton (frame-driven state + ratatui golden render snapshots).
- 2026-01-19: Fixtures: added deterministic OpenResponses tool-loop SSE fixtures + replay equivalence tests; updated `e2e_loop_us` to exercise tool-loop + follow-up parsing.
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
