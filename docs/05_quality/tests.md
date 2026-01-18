# Tests

Summary
- Contract tests and replay tests are mandatory for all modules.
- `scripts/check` is the canonical local + CI gate and enforces minimum coverage (ratcheted over time).

Phase 1 tests
- Provider adapter acceptance tests (Open Responses fixtures).
- Golden stream replay -> snapshot equivalence.
- Tool runtime timeout and cancellation.
- Workspace patch apply/rollback.
- CLI headless JSON schema validation.
- Server session lifecycle.
- Server OpenAPI schema generation/validation.

Coverage gate
- `scripts/check` enforces `cargo llvm-cov` coverage floors (Phase 1 baseline): **90%** lines/regions/functions.
