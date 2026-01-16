# RIP Docs Index

Summary
- This folder is the source of truth for scope, architecture, contracts, and tasks.
- Docs are written for agents, not humans. Short summaries first, details below.
- The server exposes the coding agent (not an Open Responses API).

Navigation
- North star and success metrics: docs/01_north_star.md
- Architecture map and data flow: docs/02_architecture/component_map.md
- Capability baseline (vendor-neutral): docs/02_architecture/capability_baseline.md
- Capability matrix (phases + hook points): docs/02_architecture/capability_matrix.md
- Module contracts (Phase 1): docs/03_contracts/modules/phase-1/
- CLI and server usage model: docs/04_execution/
- Quality gates (tests, benchmarks): docs/05_quality/
- Decision log (ADRs): docs/06_decisions/
- Task cards by phase: docs/07_tasks/

Status
- Phase 1: foundation (kernel, adapters, tools, workspace, CLI, server, benchmarks)
- Phase 2: expansion (search, memory, context compiler, policy, background workers)

Rules
- Every module must have a contract doc before implementation.
- Every task card must define acceptance tests and performance gates.
- If a decision changes, add an ADR instead of editing history.
