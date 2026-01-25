# Doc Map

Purpose
- Fast navigation for agents (treat as a code map for the docs).
- Every “product requirement” must land as a capability id + roadmap item; this map makes the path obvious.

Entry points (read order for reorientation)
- `docs/01_north_star.md` (why + constraints)
- `docs/00_resonance_pack.md` (agent rehydration pack; Continuity OS posture + current priorities)
- `docs/02_architecture/continuity_os.md` (the “one chat forever” operating model)
- `docs/07_tasks/roadmap.md` (now/next/later, with confidence tags)
- `docs/06_decisions/ADR-0010-context-compiler-truth.md` (decision: context compiler is truth-derived; provider cursors are caches)
- `docs/06_decisions/ADR-0019-continuity-authority-and-index-stores.md` (decision: single authority per store; indexes are rebuildable caches; retrieval is truth-logged by reference)
- `docs/03_contracts/capability_registry.md` (canonical capability ids + surface support)
- `docs/02_architecture/capability_matrix.md` (phase placement + hook points)
- `docs/02_architecture/runtime_and_control_plane.md` (runtime vs control plane vs remote runtime)
- `docs/03_contracts/event_frames.md` (internal event schema)
- `docs/03_contracts/handoff_context_bundle.md` (artifact schema for `thread.handoff` curated context bundles)
- `docs/03_contracts/context_bundle.md` (artifact schema for `context.compile` compiled context bundles)
- `docs/03_contracts/compaction_summary.md` (artifact schema for compaction summary checkpoints)
- `docs/05_quality/feature_flow.md` (how we turn ideas into shipped capabilities)
- OpenResponses boundary:
  - `docs/03_contracts/openresponses_coverage.md` (exhaustive spec/schema coverage + capability ownership)
  - `docs/03_contracts/openresponses_capability_map.md` (feature-group → capability ids)
  - `docs/03_contracts/openresponses_traceability.md` (upstream snapshot + diff procedure)
- Extension host (Phase 2):
  - `docs/03_contracts/modules/phase-2/02_extension_host.md` (WASM-first plugins, out-of-proc services, UI render-hints)
- Tool tasks (Phase 2):
  - `docs/03_contracts/modules/phase-2/03_tool_tasks.md` (background tools, artifact-backed outputs, determinism/replay)
- Skills (Phase 2):
  - `docs/03_contracts/modules/phase-2/04_skills.md` (Agent Skills/OpenSkills format + discovery/invoke + policy)
- Context compiler (Phase 2):
  - `docs/03_contracts/modules/phase-2/05_context_compiler.md` (deterministic compilation, bundles, compaction/cursor-rotation posture)

How we keep product/UX aligned
- Capability ids are the canonical interface contract (not provider schemas).
- Roadmap items must reference capability ids and the docs that define them.
- Provider/OpenResponses schema validation is evidence; capability ownership + parity is the deliverable.

Doc taxonomy (where things go)
- `docs/01_*`: north star / product posture
- `docs/02_architecture/*`: component map, surfaces, capability matrix
- `docs/03_contracts/*`: capability contract/registry, event frames, module contracts
  - `docs/03_contracts/modules/phase-1/*`: Phase 1 module contracts
  - `docs/03_contracts/modules/phase-2/*`: Phase 2 module contracts
- `docs/04_execution/*`: how to run/operate (CLI/server)
- `docs/05_quality/*`: CI gates, parity, benchmarks, tests
- `docs/06_decisions/*`: ADRs (material decisions only)
- `docs/07_tasks/*`: roadmap + task cards (per phase)

Doc index (authoritative docs only)
| area | doc | what it answers |
| --- | --- | --- |
| north star | `docs/01_north_star.md` | What we’re building and why. |
| continuity OS | `docs/02_architecture/continuity_os.md` | What “one chat forever” means and how we keep it replayable + provider-agnostic. |
| roadmap | `docs/07_tasks/roadmap.md` | What’s next and what “done” means. |
| capabilities | `docs/03_contracts/capabilities.md` | What a capability contract contains. |
| capability registry | `docs/03_contracts/capability_registry.md` | Full capability id list + surface statuses. |
| capability matrix | `docs/02_architecture/capability_matrix.md` | Phases + hook points by capability group. |
| surfaces | `docs/02_architecture/surfaces.md` | Surface roles and parity expectations. |
| event frames | `docs/03_contracts/event_frames.md` | Internal event schema; what all surfaces consume. |
| handoff bundle | `docs/03_contracts/handoff_context_bundle.md` | What artifact `thread.handoff` writes (summary + refs) and how it maps to future `context.compile`. |
| context bundle | `docs/03_contracts/context_bundle.md` | What artifact `context.compile` writes (compiled context bundle) and how provider adapters consume it. |
| OpenResponses coverage | `docs/03_contracts/openresponses_coverage.md` | Exhaustive OpenResponses spec/schema coverage + ownership. |
| OpenResponses capability map | `docs/03_contracts/openresponses_capability_map.md` | Feature groups → internal capability ids. |
| OpenResponses traceability | `docs/03_contracts/openresponses_traceability.md` | Upstream snapshot + sync/diff procedure. |
| model routing (Phase 2) | `docs/03_contracts/modules/phase-2/01_model_routing.md` | Model switching, routing policies, catalogs, determinism. |
| extension host (Phase 2) | `docs/03_contracts/modules/phase-2/02_extension_host.md` | Plugin/extension system contract and determinism rules. |
| tool tasks (Phase 2) | `docs/03_contracts/modules/phase-2/03_tool_tasks.md` | Background tools + artifact-backed outputs + replay requirements. |
| skills (Phase 2) | `docs/03_contracts/modules/phase-2/04_skills.md` | Skills format, discovery, progressive disclosure, and invocation. |
| context compiler (Phase 2) | `docs/03_contracts/modules/phase-2/05_context_compiler.md` | Deterministic context compilation and context bundle artifacts. |
| provider adapters | `docs/03_contracts/modules/phase-1/02_provider_adapters.md` | OpenResponses boundary invariants and tests. |
| tool runtime | `docs/03_contracts/modules/phase-1/03_tool_runtime.md` | Built-in tools and tool dispatch contract. |
| server | `docs/03_contracts/modules/phase-1/06_server.md` | Session API surface contract. |
| CLI | `docs/03_contracts/modules/phase-1/05_cli.md` | CLI surface contract (interactive + headless). |
| CLI execution | `docs/04_execution/cli.md` | How to run `rip` locally/remotely and interpret output modes. |
| server execution | `docs/04_execution/server.md` | How sessions/events work over HTTP/SSE. |
| SDK execution | `docs/04_execution/sdk.md` | How the TypeScript SDK runs (local exec + optional remote). |
