# Contract: Command Registry (Phase 1)

Summary
- Provides a registry for slash commands and programmatic commands.
- Command execution is deterministic and synchronous.
- Commands can be built-in or registered by extensions/skills.

Scope
- Command registry with register/list/execute.
- Command context: session id (optional), args, raw input.
- Command outcomes: success string or error string.

Interfaces
- Register command: name + description + handler.
- Execute command by name with context.
- List commands for UI help.

Behavior
- Names are unique; duplicate registration fails.
- Execution errors are surfaced to caller.
- No dynamic user configuration beyond command registration.

Non-goals (Phase 1)
- Command auto-discovery from disk.
- Async command execution.
- Command permission prompts.

Acceptance Tests
- Register and execute command.
- Duplicate registration returns error.
- Command listing returns registered set.

Performance
- Registry lookup p50 under 0.05ms.
