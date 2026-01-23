# Agent State (Working Log)

Last updated: 2026-01-23

How to use
- Update this file whenever focus shifts, before ending a work session, and when blocked.
- Keep it short and decision-focused; link to docs/roadmap/ADRs instead of duplicating.

Current focus
- Continuity OS posture is locked: continuity log is truth; provider state is cache; sessions are runs/turns (not user-facing by default).
- Now: Continuities (threads) are the primary user-facing entity ("one chat forever") and must be implemented end-to-end across surfaces.
- Phase 1 baseline remains: shared session runner across server + CLI (frames are canonical).
- Default local execution: `rip` launches fullscreen TUI (in-process); `rip run` stays headless; `--server <url>` enables remote runs; `rip serve` stays the remote control plane.
- TUI UX is explicitly “conversational-first + drill-down”: ambient background signals (tools/tasks/agents), responsive layouts (phone/SSH/web terminals), and an experience review gate are tracked in `docs/02_architecture/tui/06_experience_review.md`.
- OpenResponses provider compatibility: stateless history mode + tool schema strict=false; fix provider_errors without dropping raw fidelity.
- Output view: human-friendly aggregation (no tool arg deltas), aligned with Codex exec expectations.
- Background tool tasks are implemented (`pipes` + policy-gated `pty`) with deterministic replay fixtures.
- Operator gate: capability delivery order is `cli_h(local)` -> `tui` -> `server` -> `remote` -> `sdk`.
- Terminology: see `docs/02_architecture/runtime_and_control_plane.md` (runtime vs control plane vs remote runtime).
- Implemented: stream-aware frame envelope on the wire (`stream_kind`, `stream_id`) + per-stream replay validation.
- Implemented: continuity store (`ensure_default`, `append_message`, `append_run_spawned`, `append_run_ended`, `branch`, `handoff`) + local `rip run` posts to the default continuity before spawning a run.
- Implemented: continuity run lifecycle frames carry provenance (`continuity_run_spawned.actor_id/origin`) and completion (`continuity_run_ended.reason`).
- Implemented: context compiler kernel v1 (`recent_messages_v1`) writes `rip.context_bundle.v1` artifacts + emits `continuity_context_compiled`; OpenResponses runs start from compiled bundles (fresh provider conversation per run).
- Implemented: branch/handoff posture is “link-only” in the continuity log (no history copying) (ADR-0009) + relationship frames (`continuity_branched`, `continuity_handoff_created`).
- Implemented: handoff writes an artifact-backed context bundle referenced by `continuity_handoff_created.summary_artifact_id` (`docs/03_contracts/handoff_context_bundle.md`).
- Implemented: server exposes `thread.*` (ensure/list/get/post_message/branch/handoff/stream_events) and OpenAPI is updated.
- Implemented: `rip threads ...` CLI adapter + TypeScript SDK `thread.*` wrappers (ensure/list/get/post_message/branch/handoff/stream_events) while keeping ADR-0006 transport (SDK spawns `rip`; no TS HTTP/SSE client).
- Implemented: workspace mutation serialization across sessions + background tasks (workspace lock) with contract + replay tests.
- Implemented: continuity stream logs workspace-mutating tool side-effects with full provenance (`continuity_tool_side_effects`) and replay coverage under parallel runs/tasks.
- Decision locked (ADR-0010): `context.compile` is the canonical way runs “remember” across time; provider cursors are optional caches only.
- Drafted contracts (docs-first):
  - New continuity frame: `continuity_context_compiled` (`docs/03_contracts/event_frames.md`)
  - New artifact schema: `rip.context_bundle.v1` (`docs/03_contracts/context_bundle.md`)
  - Context compiler module contract: `docs/03_contracts/modules/phase-2/05_context_compiler.md`
- Keep CI/bench gates green; ratchet budgets only with replay coverage.

Reorientation (read in order after compaction)
- `AGENTS.md`
- `agent_state.md`
- `docs/00_index.md`
- `docs/00_doc_map.md`
- `docs/01_north_star.md`
- `docs/02_architecture/continuity_os.md`
- `docs/07_tasks/roadmap.md`
- `docs/03_contracts/openresponses_coverage.md`
- `docs/03_contracts/openresponses_capability_map.md`
- `docs/07_tasks/openresponses_coverage.md`

Open risks / notes
- Tests no longer write `./data` under the repo (ripd export test uses temp dirs).
- Note: local runs still default to `./data` unless `RIP_DATA_DIR` is set.
- Perf: context compiler hot path now avoids global `events.jsonl` scans when caches exist (snapshot-first session aggregation + per-continuity sidecar replay); remaining work is to avoid full continuity stream loads per run (tail reads/indexing).

Active priorities
- Keep roadmap Now/Next aligned with the implementation work.
- Next slice (code): add compaction/summary artifacts + a compile strategy that mixes summaries with recent raw messages (versioned; replay-addressable).
- Keep OpenResponses boundary full-fidelity while wiring new surfaces/adapters.
- Keep OpenResponses follow-ups spec-canonical; any compatibility user message is opt-in.
- Keep stateless history compatibility opt-in; default remains `previous_response_id`.
- Prefer deterministic, replayable fixtures over ad-hoc behavior changes.
- Validate OpenAI + OpenRouter runs side-by-side; remove provider_errors in normal operation.

Next checkpoints
- CI runs `scripts/check-fast` on push/PR.
- SDK TS checks (local): `scripts/check-sdk-ts`.
- Bench harness includes TTFT + end-to-end loop and is CI-gated (`scripts/bench`).
- `rip run <prompt>` works without a separate `ripd` process (in-process session engine).
- `rip run <prompt> --server <url>` targets a remote server and streams identical event frames.
- `rip serve` exposes the session API for remote clients; SDK can target it via `--server` (but defaults to local `rip` exec).
- Manual smoke: `cargo test -p ripd live_openresponses_smoke -- --ignored` observes real provider SSE + at least one tool call.
- Manual provider smoke (Option 1 flags, local CLI).
- OpenAI: `RIP_DATA_DIR="$(mktemp -d)" RIP_WORKSPACE_ROOT="$PWD/fixtures/repo_small" OPENAI_API_KEY=... cargo run -p rip-cli -- run "List what's in this directory. Use the ls tool, then answer with just the filenames." --provider openai --model gpt-5-nano-2025-08-07 --view output`
- OpenRouter: `RIP_DATA_DIR="$(mktemp -d)" RIP_WORKSPACE_ROOT="$PWD/fixtures/repo_small" OPENROUTER_API_KEY=... cargo run -p rip-cli -- run "List what's in this directory. Use the ls tool, then answer with just the filenames." --provider openrouter --model mistralai/devstral-2512:free --stateless-history --view output`
- Parallel tool calls (request-only): append `--parallel-tool-calls` to the command above (execution remains sequential in Phase 1).
- Live API sweep script (real APIs): `scripts/live-openresponses-sweep` (supports `--provider` and `--skip-parallel-case`)
