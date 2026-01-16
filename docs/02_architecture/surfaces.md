# Surface Layers

Summary
- All surfaces are thin adapters over the same core session + capability API.
- No surface owns business logic; parity is enforced by contract and tests.

Surfaces (Phase 1 + planned)
- Headless CLI: `rip-cli` (default entrypoint for automation)
- Terminal UI (TUI): `rip-tui` (rich rendering only)
- Server: `ripd` (session HTTP/SSE)
- MCP server: `rip-mcp` (capability exposure via MCP)
- SDKs: `rip-sdk-*` (generated or thin clients)

Adapter rule
- Surfaces may translate transports, render output, and handle IO.
- Surfaces must not implement core behaviors, policies, or capability semantics.
- All behaviors originate in the core runtime + capability registry.

Parity rule
- A feature is "done" only if it is:
  - Defined in the core capability contract, and
  - Exposed by every active surface, or
  - Explicitly deferred with a tracked gap and approval.

Implications
- New feature work starts by extending the capability contract.
- Surfaces then wire to the same capability id/version and inherit behavior.
- If a surface cannot support a capability, it must be documented as a gap.
