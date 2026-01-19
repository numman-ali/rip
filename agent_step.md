# Agent Step

Current focus
- Phase 1: shared session runner across server + CLI (frames are canonical).
- Default local execution: `rip run` in-process; `--server <url>` for remote; `rip serve` stays the remote control plane for SDKs.
- Next up: TypeScript SDK baseline over the server API.
- Keep CI/bench gates green; ratchet budgets only with replay coverage.

Reorientation (read in order after compaction)
- `AGENTS.md`
- `agent_step.md`
- `docs/00_index.md`
- `docs/00_doc_map.md`
- `docs/01_north_star.md`
- `docs/07_tasks/roadmap.md`
- `docs/03_contracts/openresponses_coverage.md`
- `docs/03_contracts/openresponses_capability_map.md`
- `docs/07_tasks/openresponses_coverage.md`

Active priorities
- Keep roadmap Now/Next aligned with the implementation work.
- Keep OpenResponses boundary full-fidelity while wiring new surfaces/adapters.
- Keep OpenResponses follow-ups spec-canonical; any compatibility user message is opt-in.
- Prefer deterministic, replayable fixtures over ad-hoc behavior changes.

Next checkpoints
- CI runs `scripts/check-fast` on push/PR.
- Bench harness includes TTFT + end-to-end loop and is CI-gated (`scripts/bench`).
- `rip run <prompt>` works without a separate `ripd` process (in-process session engine).
- `rip run <prompt> --server <url>` targets a remote server and streams identical event frames.
- `rip serve` exposes the session API for remote clients (SDK targets server only).
- Manual smoke: `cargo test -p ripd live_openresponses_smoke -- --ignored` observes real provider SSE + at least one tool call.
