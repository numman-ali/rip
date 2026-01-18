# RIP Docs Index

Summary
- This folder is the source of truth for scope, architecture, contracts, and tasks.
- Docs are written for agents, not humans. Short summaries first, details below.
- The server exposes the coding agent (not an Open Responses API).

Navigation
- North star and success metrics: docs/01_north_star.md
- Doc map (fast navigation): docs/00_doc_map.md
- Architecture map and data flow: docs/02_architecture/component_map.md
- Capability baseline: docs/02_architecture/capability_baseline.md
- Capability matrix (phases + hook points): docs/02_architecture/capability_matrix.md
- Surface layers + parity rules: docs/02_architecture/surfaces.md
- Module contracts (Phase 1): docs/03_contracts/modules/phase-1/
- Module contracts (Phase 2): docs/03_contracts/modules/phase-2/
- Event frame schema: docs/03_contracts/event_frames.md
- Capability contract + registry: docs/03_contracts/capabilities.md
- Capability registry (source of truth): docs/03_contracts/capability_registry.md
- OpenResponses coverage map: docs/03_contracts/openresponses_coverage.md
- OpenResponses capability map: docs/03_contracts/openresponses_capability_map.md
- CLI and server usage model: docs/04_execution/
- Quality gates (tests, benchmarks): docs/05_quality/
- Source-of-truth policy: docs/05_quality/source-policy.md
- Surface parity gates: docs/05_quality/surface-parity.md
- Decision log (ADRs): docs/06_decisions/
- Task cards by phase: docs/07_tasks/
- OpenResponses coverage checklist: docs/07_tasks/openresponses_coverage.md

Status
- Phase 1: foundation (kernel, adapters, tools, workspace, CLI, server, benchmarks)
- Phase 2: expansion (TUI, MCP, search, memory, context compiler, policy, background workers)

Rules
- Every module must have a contract doc before implementation.
- Every task card must define acceptance tests and performance gates.
- If a decision changes, add an ADR instead of editing history.
