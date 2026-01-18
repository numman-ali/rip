# Agent Step

Current focus
- Phase 1: keep CI/bench gates green and ratchet budgets.
- Ratchet TTFT + end-to-end loop benchmark budgets.
- Design/track the agent tool-call loop (provider tool calls -> ToolRunner -> follow-up requests).

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
- Ensure benchmarks/fixtures become CI gates (no regressions).
- Keep OpenResponses boundary full-fidelity while wiring live provider streaming.

Next checkpoints
- CI runs `scripts/check-fast` on push/PR.
- Bench harness includes TTFT + end-to-end loop and is CI-gated (`scripts/bench`).
- Provider streaming emits `provider_event` frames from an OpenResponses endpoint (`RIP_OPENRESPONSES_ENDPOINT`).
