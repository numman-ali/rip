# AGENTS.md

Purpose
- This repo is operated by autonomous coding agents.
- The human operator is C-suite level and will not read code or docs.
- All coordination happens through chat; be concise, decision‑focused, announce the plan, and proceed unless blocked (ask for approval only when required by gates).
- `docs/` is the source of truth for scope, architecture, contracts, and tasks.

Core directive: Open Responses coverage
- Implement full, first-class support for the entire Open Responses spec (no minimal/partial mappings).
- Before changing provider logic, review the split schemas in `temp/openresponses/schema` AND the specification markdown/MDX in `temp/openresponses/src/pages` (specification/reference/compliance/etc.), then capture findings in committed docs (use `docs/` for authoritative notes; use `temp/docs` only for external/online evidence, not internal references or working notes).
- Never drop fields/events; preserve full fidelity into internal frames.

Operator intent (non‑negotiables)
- Build the fastest coding‑agent harness possible.
- Modular, pluggable, and testable by default.
- Designed for autonomous agent development, not human collaboration.
- Server exposes the coding agent itself (session API), not an Open Responses API.
- Open Responses is used only at the provider boundary.
- Programmatic SDK (TypeScript first) is a required surface, not an optional add-on.

Continuity OS posture (non-negotiable)
- RIP is a **continuity OS**, not a chat app: default UX is “one chat forever”.
- The continuity event log is the source of truth (append-only + replayable). Provider conversation state (`previous_response_id`, vendor thread ids) is a cache and may be rotated/rebuilt at any time.
- “Sessions” are compute runs/turns; they are not user-facing by default (surfaces “continue” by targeting a continuity and spawning runs behind the scenes).
- Background/subconscious agents are just jobs over event streams (summarizers, indexers, auditors, subagents). They must emit structured events + artifacts; no hidden mutable state.
- Multi-actor/shared continuities are first-class: every input/action must carry provenance (`actor_id`, `origin`) so team workflows remain replayable and auditable.

Agent mindset (avoid monotony)
- Optimize for the end-state architecture (complexity is allowed); implement in slices but **never** introduce “temporary” concepts that fight the Continuity OS model.
- For any feature, explicitly sanity-check: 1M+ events, provider cursor rotation, parallel jobs, multi-actor/shared continuities, remote control plane, replay determinism, and surface parity.
- When you notice a mismatch between docs/vision and implementation, treat it as a defect: fix docs and/or add a roadmap item in the same change.

Success metrics
- TTFT overhead, parse overhead per event, tool dispatch latency, patch throughput, end‑to‑end loop latency.
- CI gates must fail on regressions.

Scope boundaries
- Phase 1: core runtime, provider adapters, tool runtime, workspace engine, event log + snapshots, CLI (interactive + headless), agent server (HTTP/SSE + OpenAPI), benchmarks/fixtures.
- Phase 2: config/policy foundations, extensions/hooks, skills, subagents, context compiler + compaction, UI/interaction, SDKs, TUI/MCP surfaces, integrations, expanded execution modes + model/provider routing.
- Phase 3: search/index + memory, background workers/sync, policy/steering (adaptive budgets), enterprise config, extended sandboxing.

Architecture posture
- Rust core runtime for hot path.
- Internal compact frames; JSON only at edges.
- Plugins default to WASM; hot path may be native in‑process.
- Heavy modules may run out‑of‑process.
- All inter-module traffic is structured events, not raw text.

Surface parity principle
- No feature is considered done unless it exists in the core capability contract and is exposed in all active surfaces, or the gap is explicitly tracked and approved (owner + reason + expiry).
- Surface packages are adapters only; no business logic or core decisions live in UI or transport layers.
- Surface-specific capabilities still require explicit support/unsupported declarations on every surface.
- Parity matrix + gap list are required artifacts; CI enforces them.

Capability delivery order (operator directive)
- For any new capability/feature work, implement and validate in this order (treat earlier stages as gates):
  1) Headless CLI **local runtime** (in-process; no server required)
  2) TUI (frame-driven UX over the same capability)
  3) Server (HTTP/SSE exposure)
  4) Remote mode (CLI/TUI targeting `--server <url>`)
  5) SDK (programmatic surface; local + remote as applicable)
- If a capability is implemented server-first for expedience, it is **not** “done” until (1) is complete; record the gap in `docs/07_tasks/roadmap.md` and `agent_state.md`.

Capability contract
- Capabilities are the canonical, versioned API; the registry is the source of truth (`docs/03_contracts/capability_registry.md`).
- New feature work starts by updating the capability contract + registry, then wiring surface adapters and tests.
- Breaking changes require a version bump and an ADR.

Approval gates
- External operational changes (new third-party deps, scripts/hooks, repo config, commits, pushes) require explicit operator approval.
- Internal wiring changes are pre-approved when scoped to Phase 1 quality work (tests/fixtures/benches) and do not widen default permissions:
  - adding/removing *path* dependencies between existing workspace crates
  - updating fixtures, replay tests, benchmark harnesses/budgets
- If a non-obvious choice impacts runtime behavior/public API, security posture, or widens permissions by default, request explicit operator approval before changes.

Standing approvals (chat-level)
- If the operator says “proceed / do whatever you want / approved”, treat it as standing approval for the current thread for items in the “Internal wiring” bucket above.
- Still escalate for external deps, permission widening, or hard-to-reverse architecture choices (ADR-worthy).

Up-to-date sources rule
- For any non-obvious implementation choice, consult current official docs before acting.
- If multiple viable approaches exist or guidance is unclear, escalate to the operator before implementation.
- Capture external/online research evidence in `temp/docs` (indexed in `temp/docs/references.md`); keep internal references and authoritative decisions in `docs/`.
- Check `temp/docs/references.md` before any web search to see what documentation is already available.

Working style
- Build one concrete piece at a time.
- Always provide: goal, proposed change, acceptance criteria, and tests.
- Keep outputs deterministic; use replayable event logs + snapshots.
- Prefer simple, verifiable steps over broad refactors.
- Keep Rust source files small and single-purpose; split modules when files exceed ~600–700 lines unless there's a clear reason not to.
- Batch related checklist items when they share schemas/logic; avoid unnecessary micro-changes.
- Reorient by reading `AGENTS.md`, `agent_state.md`, `docs/00_index.md`, `docs/00_doc_map.md`, `docs/01_north_star.md`, `docs/02_architecture/continuity_os.md`, and `docs/07_tasks/roadmap.md` before resuming after compaction.
- Also read `docs/00_resonance_pack.md` first if you need the full "why" and current posture in one place.
- Use `scripts/check` before reporting a task as complete.
- Use `scripts/install-hooks` once to enable repo hooks.
- Sync the canonical Open Responses OpenAPI spec via `scripts/update-openresponses-types` when schemas change.
- Pre-commit runs `scripts/check-fast` (core checks + udeps); ensure required cargo subcommands are installed.

Roadmap discipline
- Maintain `docs/07_tasks/roadmap.md` as the single source for Now/Next/Later.
- Every roadmap item must include a confidence tag: `[confirm spec]` or `[needs work]`.
- If `[needs work]`, write a decision packet and record the choice (roadmap note or ADR). Default: proceed with the recommendation unless an approval gate applies or the choice is hard to reverse.
- Record deferrals and doc/impl gaps in the roadmap so "now vs later" is explicit.

Communication expectations
- Confirm understanding when requirements shift.
- Surface risks and tradeoffs early.
- Lead with a clear recommendation; minimize questions; ask for approval only when required by gates.
- Avoid offering multiple options unless explicitly requested.
- Avoid long explanations unless explicitly requested.

Decision packets (required)
- Before asking the operator to “confirm” a non-obvious choice, provide a short decision packet:
  - Decision: one sentence describing what will be decided.
  - Options: 2–3 viable approaches (max), each with 1–2 key tradeoffs.
  - Recommendation: the default choice, with why (performance/safety/determinism).
  - Reversibility: how we can iterate later without breaking replay/surface parity (versioning/ADR/tests).
- If the operator says “I don’t know”, treat it as delegation: proceed with the recommendation and record the choice (ADR if material; otherwise roadmap note).

Auto mode posture
- Phase 1 defaults should be safe + deterministic (explicit allowlists, strong sandbox defaults).
- “Full auto mode” (everything allowed) is a planned policy profile and must be explicit, versioned, and logged; do not silently widen permissions.

Quality gates
- Every module must have contract tests and replay tests.
- Benchmarks are CI gates; regressions fail builds.
- Deterministic fixtures required for replay tests.

Engineering practices (Rust)
- Unit tests may live with the module; integration tests go in `tests/` when cross-crate behavior is exercised.
- Clippy warnings are treated as errors.

Definitions (avoid confusion)
- Server: exposes agent sessions over HTTP/SSE; canonical control plane for active capabilities; serves generated OpenAPI.
- Provider adapters: Open Responses boundary only.
- CLI: interactive and headless front‑ends to the same runtime.
- SDK: programmatic surface over the server API (TypeScript first).
- Continuity (“thread”): user-facing, durable identity/state (“one chat forever”); internal truth is the continuity event log.
- Session (“run”): a single execution unit/turn attached to a continuity; sessions emit frames and end (`session_ended` remains terminal).
- Provider cursor: provider-side conversation handle used as an optimization only; can be reset without changing continuity truth.

Decision log
- Material changes require an ADR in docs/06_decisions/.

Maintenance
- AGENTS.md must be kept current as the system evolves; update it whenever intent, scope, or architecture changes.
- External research artifacts (web-sourced evidence only) live in `temp/docs`, with an index at `temp/docs/references.md`. Committed docs live in `docs/`.
- Doc drift is a defect: if you notice docs diverging from each other or the implementation, fix it in the same change. If the fix is non-trivial, flag it and get approval before proceeding.
- Maintain `agent_state.md` as the mutable working log and reorientation checklist; update it whenever the focus shifts, when blocked, and before ending a work session.
