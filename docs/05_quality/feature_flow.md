# Feature Workflow (Capability-first)

Summary
- New product ideas must land as capability ids + contracts before implementation.
- Keep the system flexible by treating **capabilities** and **event frames** as the stable interface; implementations and surfaces are adapters.
- Any external spec we integrate must be tracked (traceability) and kept up to date.

Canonical process (do this in order)

1) Intake + classification
- Decide which layer(s) the feature touches:
  - core runtime (ripd hot path)
  - provider boundary (OpenResponses)
  - tool runtime / workspace engine
  - surfaces (CLI/server/SDK/TUI/MCP)
- If there’s ambiguity, write a decision packet (AGENTS.md) and proceed with the recommendation unless an approval gate applies.

2) Upstream spec sync (if applicable)
- Check `temp/docs/references.md` first.
- If the relevant upstream spec/docs aren’t already vendored, retrieve evidence into `temp/docs/<topic>/` and update the index.
- Record authoritative internal conclusions in `docs/` (contracts/ADRs/roadmap), not in `temp/docs`.

Spec-specific rules
- OpenResponses provider logic:
  - Review split schemas in `temp/openresponses/schema` and spec MDX in `temp/openresponses/src/pages` before changing adapters.
  - Never drop fields/events; preserve into `provider_event` frames.
- Skills (Agent Skills / “OpenSkills”):
  - Review the Agent Skills standard evidence in `temp/docs/agentskills/notes_2026-01-19.md` (and ecosystem notes under `temp/docs/*/skills*`) before implementing the skill loader.
  - Treat spec-defined frontmatter fields as canonical; preserve unknown frontmatter fields as extensions.
  - Validate with `skills-ref` (or an equivalent validator) and record any deviations as an explicit compatibility decision.

3) Capability contract + registry
- Add/adjust capability ids in `docs/03_contracts/capability_registry.md`.
- If the change is material or breaking, bump versions and add an ADR.

4) Phase placement + hook points
- Update `docs/02_architecture/capability_matrix.md` to make phase/hook intent explicit.

5) Roadmap tracking
- Add/adjust an item in `docs/07_tasks/roadmap.md` with:
  - confidence tag
  - refs to contracts/specs
  - ready checklist
  - done criteria (tests + replay + benches + parity)

6) Module contracts
- Add/update a module contract under `docs/03_contracts/modules/phase-*` describing:
  - interfaces, invariants, determinism/replay rules
  - required tests + benchmarks

7) Implementation
- Implement in the core runtime/module, keeping hot path fast and JSON at edges.

8) Tests + replay fixtures (required)
- Add contract tests and replay tests.
- Fixtures must be deterministic and offline where possible.

9) Benchmarks + budgets (CI gates)
- Add/adjust benchmarks in `crates/rip-bench` and budgets in `docs/05_quality/benchmarks_budgets.json`.
- Ratchet budgets over time; regressions fail CI.

10) Surface parity gates
- Ensure new capabilities are exposed consistently or gap-tracked.
- Regenerate parity matrix when registry changes (see `docs/05_quality/surface-parity.md`).
