# Benchmarks

Summary
- Benchmarks are CI gates; regressions fail PRs.

Phase 1 benchmarks
- Event parse overhead per SSE event.
- TTFT overhead (first byte -> first internal event).
- Tool dispatch latency.
- Patch apply throughput.
- End-to-end loop latency on fixture repos.

Fixture requirements
- Small repo (fast CI).
- Medium repo (realistic).
- Deterministic prompts and tool outputs.

Harness (Phase 1)
- Budgets: `docs/05_quality/benchmarks_budgets.json`
- Runner: `scripts/bench` (release mode; fails if budgets are exceeded)
- Fixture root: `fixtures/`
