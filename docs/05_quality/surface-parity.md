# Surface Parity Gates

Summary
- Parity is enforced by tests that validate every active surface exposes the same capabilities.
- No feature is considered done unless parity is satisfied or an approved gap is tracked.

Active surfaces (important)
- A surface is considered **active** when it has at least one capability marked `supported` in `docs/03_contracts/capability_registry.md`.
- Parity checks apply only across active surfaces; inactive surfaces are allowed during bring-up, but should not silently ship untracked behavior.
- To activate a surface intentionally:
  - mark the relevant capability ids `supported` for that surface in the registry,
  - update `docs/05_quality/surface-parity-gaps.json` for any remaining planned gaps on active surfaces (owner + reason + expiry).

Checks
- Capability registry vs surface adapters: all ids present.
- Capability versions are aligned across surfaces.
- Server OpenAPI schema includes all active capability ids.
- Gaps must be explicitly listed with approval and expiry date.

Artifacts
- Parity matrix (generated): lists surfaces x capabilities.
- Gap list (manual): approved exceptions with owner + reason.

Fail conditions
- Missing capability in any active surface.
- Mismatched capability versions.
- Unapproved gap entry.
