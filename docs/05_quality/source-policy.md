# Source-of-Truth Policy

Summary
- Non-obvious technical choices must be validated against current official upstream docs.
- If multiple viable approaches exist or guidance is unclear, escalate to the operator before implementation.

Rules
- Prefer official upstream docs for packages, protocols, and tools.
- Capture evidence in `temp/docs`.
- If the choice impacts architecture, add an ADR.
- If the choice impacts implementation details or deps, request explicit approval before changes.

Escalation triggers
- Competing official recommendations.
- Missing/ambiguous upstream guidance.
- Tradeoffs that affect performance, modularity, or surface parity.
