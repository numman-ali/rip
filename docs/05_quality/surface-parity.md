# Surface Parity Gates

Summary
- Parity is enforced by tests that validate every active surface exposes the same capabilities.
- No feature is considered done unless parity is satisfied or an approved gap is tracked.

Checks
- Capability registry vs surface adapters: all ids present.
- Capability versions are aligned across surfaces.
- Gaps must be explicitly listed with approval and expiry date.

Artifacts
- Parity matrix (generated): lists surfaces x capabilities.
- Gap list (manual): approved exceptions with owner + reason.

Fail conditions
- Missing capability in any active surface.
- Mismatched capability versions.
- Unapproved gap entry.
