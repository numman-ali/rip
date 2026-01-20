# Surface Layers

Summary
- All surfaces are thin adapters over the same core session + capability API.
- No surface owns business logic; parity is enforced by contract and tests.
- Server API is the canonical control plane for remote capability access; SDKs may run locally via `rip` exec (JSONL frames) or attach to a server.

Surfaces (Phase 1)
- Interactive CLI: `rip-cli` (lightweight streaming renderer; in-process by default, optional remote server)
- Headless CLI: `rip-cli --headless` (automation JSON; in-process by default, optional remote server)
- Server: `ripd` (remote session HTTP/SSE + OpenAPI spec)

Surfaces (Phase 2 / planned)
- SDKs: `rip-sdk-*` (TypeScript first; wraps `rip` + JSONL frames; optional direct-HTTP transport later)
- Terminal UI (TUI): `rip-tui` (rich rendering only)
- MCP server: `rip-mcp` (capability exposure via MCP)

Adapter rule
- Surfaces may translate transports, render output, and handle IO.
- Surfaces must not implement core behaviors, policies, or capability semantics.
- All behaviors originate in the core runtime + capability registry.

Parity rule
- A feature is "done" only if it is:
  - Defined in the core capability contract, and
  - Exposed by every active surface, or
  - Explicitly deferred with a tracked gap and approval.

Surface-specific capabilities
- Some capabilities are UI-only or transport-specific.
- They still must be declared in the capability registry, and other surfaces must explicitly mark support or unsupported status.

Implications
- New feature work starts by extending the capability contract.
- Surfaces then wire to the same capability id/version and inherit behavior.
- If a surface cannot support a capability, it must be documented as a gap.
