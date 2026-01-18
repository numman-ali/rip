# Agent Step

Current focus
- Phase 1 closeout: land CI + fixtures + benchmarks gates.
- Eliminate test artifact leakage (no writes under `crates/*`).
- Start provider integration MVP wiring (single OpenResponses endpoint).

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
- Bench harness + fixture repos exist and are CI-gated.
- Provider streaming MVP emits `provider_event` frames from a real endpoint.
