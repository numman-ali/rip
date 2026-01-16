# Task 06: CLI (interactive + headless)

Goal
- Build interactive and headless CLI modes.
- TUI is a separate surface (`rip-tui`, Phase 2) and is tracked in the roadmap.

Inputs
- Module contract: docs/03_contracts/modules/phase-1/05_cli.md

Outputs
- CLI binary that connects to ripd.

Acceptance criteria
- Interactive UI renders event stream (no diffs/approvals in Phase 1).
- Headless mode emits JSON events.

Tests
- Golden stream render tests.
- Headless JSON schema validation.
