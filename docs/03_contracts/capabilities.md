# Capability Contract

Summary
- Capabilities are the canonical, versioned API of the harness.
- Every surface (CLI/TUI/server/MCP/SDK) must map to the same capability ids.

Contract structure (per capability)
- id: stable string id (ex: "session.run")
- version: semver for behavior changes
- intent: one-line description
- inputs: typed schema + defaults
- outputs: typed schema + streaming events if applicable
- side-effects: filesystem, network, subprocesses
- errors: enumerated error codes + meanings
- determinism: rules for replayable behavior

Registry rules
- Capabilities live in a single registry doc (source of truth): docs/03_contracts/capability_registry.md
- Adding a capability requires: contract + tests + surface adapters.
- Breaking changes require a version bump and an ADR.

Compliance
- Each surface declares support for each capability version.
- Parity gaps must be explicitly tracked and approved.
