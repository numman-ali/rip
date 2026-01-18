# ADR-0004: Plugin architecture is WASM-first with optional out-of-process services

Status
- Accepted

Context
- We want a first-class plugin/extension system that can add tools, intercept decisions, customize rendering, and integrate external systems.
- Non-negotiables: deterministic replay, strict surface parity, and a fast Rust hot path.
- In-process dynamic linking (native plugins) complicates sandboxing, versioning/ABI stability, and reproducible execution.
- Some plugin workloads (indexing, retrieval, heavy analysis) should not run in the hot path.

Decision
- Default plugin boundary: **WASM modules** with a versioned host interface.
- Heavy/optional plugins: **out-of-process services** connected via a structured RPC protocol that is fully logged as event frames for replay.
- Native in-process plugins may exist only as an explicit “trusted” deployment/profile (not the default).
- UI customization is expressed as **structured UI/render-hint frames**, not surface-owned business logic.

Consequences
- Rust remains a first-class plugin language (compiled to WASM for default distribution).
- The runtime can enforce budgets, permissions, and determinism properties at the plugin boundary.
- Surface parity is preserved: all surfaces consume the same event frames; UI surfaces render additional UI frames, headless surfaces stream them.
- We will define and version:
  - plugin manifest metadata
  - host ABI (WASM) / RPC schema (out-of-proc)
  - replay requirements (log every plugin input/output)
