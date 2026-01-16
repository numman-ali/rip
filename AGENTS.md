# AGENTS.md

Purpose
- This repo is operated by autonomous coding agents.
- The human operator is C-suite level and will not read code or docs.
- All coordination happens through chat; be concise, decision‑focused, and ask for validation before moving on.

Operator intent (non‑negotiables)
- Build the fastest coding‑agent harness possible.
- Modular, pluggable, and testable by default.
- Designed for autonomous agent development, not human collaboration.
- Server exposes the coding agent itself (session API), not an Open Responses API.
- Open Responses is used only at the provider boundary.

Success metrics
- TTFT overhead, parse overhead per event, tool dispatch latency, patch throughput, end‑to‑end loop latency.
- CI gates must fail on regressions.

Scope boundaries
- Phase 1: core runtime, provider adapters, tool runtime, workspace engine, event log, CLI (interactive + headless), agent server, benchmarks/fixtures.
- Phase 2: search/index, memory, context compiler, policy/steering, background workers, sync, DSPy sidecar.

Architecture posture
- Rust core runtime for hot path.
- Internal compact frames; JSON only at edges.
- Plugins default to WASM; hot path may be native in‑process.
- Heavy modules may run out‑of‑process.

Working style
- Build one concrete piece at a time.
- Always provide: goal, proposed change, acceptance criteria, and tests.
- Keep outputs deterministic; use replayable event logs + snapshots.
- Prefer simple, verifiable steps over broad refactors.
- Use `scripts/check` before reporting a task as complete.
- Use `scripts/install-hooks` once to enable repo hooks.
- Sync the canonical Open Responses OpenAPI spec via `scripts/update-openresponses-types` when schemas change.
- Pre-commit runs `scripts/check` (fmt/check/clippy/tests/doctests/udeps/coverage); ensure required cargo subcommands are installed.

Communication expectations
- Confirm understanding when requirements shift.
- Surface risks and tradeoffs early.
- Lead with a clear recommendation and ask for approval to proceed.
- Avoid offering multiple options unless explicitly requested.
- Avoid long explanations unless explicitly requested.

Quality gates
- Every module must have contract tests.
- Benchmarks are CI gates; regressions fail builds.
- Deterministic fixtures required for replay tests.

Engineering practices (Rust)
- Unit tests may live with the module; integration tests go in `tests/` when cross-crate behavior is exercised.
- Clippy warnings are treated as errors.

Definitions (avoid confusion)
- Server: exposes agent sessions over HTTP/SSE.
- Provider adapters: Open Responses boundary only.
- CLI: interactive and headless front‑ends to the same runtime.

Decision log
- Material changes require an ADR in docs/06_decisions/.

Maintenance
- AGENTS.md must be kept current as the system evolves; update it whenever intent, scope, or architecture changes.
- Research artifacts live in `temp/docs`, with an index at `temp/docs/references.md`.
