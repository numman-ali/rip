# Capability Baseline (Modern Coding Agents)

Summary
- Capability checklist for modern coding agents.
- Used to ensure architectural coverage across phases.

Sessions & Threads
- Start, continue, and resume sessions from prior state.
- Session tree with branching/forking from any prior point.
- Handoff: move work to a new thread with curated context.
- Reference other threads by ID/URL and extract relevant context.
- Archive threads without deleting history; searchable archive.
- Thread labels/tags and thread map/graph views.
- Thread sharing with visibility levels (private, group, workspace, public).
- Thread search by keyword and by files touched.
- Agents panel for viewing and managing multiple active threads.

Session Storage & Replay
- Append-only session storage with deterministic replay (JSONL or equivalent).
- Tree-structured entries with parent/child links (not just linear history).
- Branch summaries stored when switching branches.
- Compaction entries stored with summaries and cut points, including file tracking.
- Session metadata entries (name, labels, model changes, thinking-level changes).
- Custom entries for extension state; custom message entries that affect context.

Context & Guidance
- Project guidance files loaded automatically by path and subtree.
- Glob-scoped guidance that applies only to matching files.
- System prompt overrides from project or user scope.
- Deterministic context compilation and replayable snapshots.
- File- and thread-referencing in prompts (e.g., @file, @thread).

Configuration & Policy
- Layered settings scopes (managed, user, project, local) with clear precedence.
- Config merging across scopes (non-destructive merges).
- JSON/JSONC config support with schema validation.
- Environment variable overrides for critical settings.
- Permission rules with allow/ask/deny/delegate and argument matching.
- Sandbox profiles and execution policies stored in config.
- Output style and model defaults configurable per scope.
- Plugin, tool, and subagent configuration stored in config.
- Managed policies to restrict plugins, hooks, and marketplaces.

Commands & Automation
- Slash commands for common actions (session control, model selection, permissions).
- Custom commands loaded from directories with namespacing and arguments.
- Command aliases and prompt templates.
- Commands can run scripts or insert structured prompts.
- Command definitions can include allowed-tools and file references.
- Commands can define scoped hooks that run while the command is active.

Execution Modes
- Interactive TUI/CLI mode.
- Headless execute mode for automation/CI.
- Streaming JSON output for programmatic monitoring.
- Print mode (formatted output) and RPC mode (machine commands).
- RPC/SDK command surface for prompting, steering, follow-ups, and aborts.
- Streaming JSON input mode (stdin as event stream).
- Structured JSONL event stream with an explicit output schema.
- Resume prior sessions by ID in headless mode.
- Output format control for text, JSON, and streaming JSON.
- Structured output using a JSON Schema.

Tools & Tooling
- Built-in file tools (read/write/edit/grep/find/ls) and shell tool.
- Tool registry with dynamic enable/disable at runtime.
- Toolboxes: executable tools discovered from a directory.
- Tool schemas for structured inputs/outputs.
- Tool output truncation and safety limits.
- Plan/read-only mode that restricts tool set.
- Tool permissions: allow/ask/reject/delegate based on tool + args.
- Override or replace built-in tools with custom implementations.
- Remote tool execution as an optional backend.
- Tool-level permissions and per-tool argument rules.
- Allowed-tools lists for noninteractive runs to auto-approve safe tools.
- Read tools support line ranges for large files.
- LSP tool integration for code intelligence and navigation.

Compaction & Summarization
- Auto-compaction when context exceeds threshold (reserve tokens).
- Manual compaction with optional custom instructions.
- Split-turn handling when a single turn exceeds budget.
- Compaction cut-point rules (avoid splitting tool calls/results).
- Branch summarization when navigating session tree.
- File-operation tracking included in summaries.
- Hook-based custom compaction and custom branch summaries.

Extensions & Hooks
- Lifecycle hooks (session start, tool call, agent end, etc.).
- Hooks for permission requests, notification events, and pre-compaction.
- Hook matching rules (tool name, args, regex/path filters).
- Hook outputs can allow/deny/modify actions or inject messages.
- Hook handlers can be commands or prompt-based evaluators.
- Extension API to inject messages, append system prompts, and run commands.
- Persistent extension state stored in session entries.
- UI extension points (status, widgets, dialogs, custom editor).
- Event bus for cross-extension communication.
- Register commands, shortcuts, and flags from extensions.
- Custom tool renderers for tool calls/results.
- Extension error reporting and recovery hooks.
- Extension access to session control (new session, branch, navigate tree).
- Extension access to agent state (idle detection, abort, queue visibility).
- Hook events include permission, user prompt, stop, subagent start/stop, tool failure, and pre-compaction.
- Hook types include command, prompt, and agentic verification.
- Hook timeouts and once-only hooks for scoped components.

Skills
- Skills discovered by `SKILL.md` files in workspace/user scopes.
- On-demand skill loading to avoid context bloat.
- User-invokable skill loading (manual activation over auto-load).
- Skill install/list/remove workflows and filters/ignores.
- Skill commands (`/skill:name`) for explicit activation.
- Skill repositories with scripts, references, and assets.
- Skill metadata for compatibility and allowed tools.
- Skill-defined model overrides, tool limits, and hooks.
- Skill-defined forked context or subagent invocation.

Subagents
- Spawn subagents with independent context windows and tool access.
- Specialized subagents (search, review, analysis) as optional modules.
- Subagent profiles with constrained tools, budgets, or objectives.
- Background/foreground subagent execution with handoff controls.
- Auto-delegation rules from the primary agent to subagents.
- Subagent definitions loaded from config or markdown.
- Subagents definable at runtime via CLI flags or RPC config.
- Manual subagent invocation via mention syntax.

Models & Providers
- Multi-provider support with model switching at runtime.
- Thinking/reasoning levels with configurable budgets.
- Custom model registry and per-provider auth.
- Runtime model cycling and model availability queries.
- Agent mode profiles (fast vs deep) with different budgets.
- Second-opinion model/tool for review-grade reasoning.
- Per-run overrides for model, sandbox, and approval policy.

Output Styles
- Output styles that alter response format or tone via system prompts.
- Custom output styles loaded from files and selected by command.

UI/Interaction
- Command palette with prompt templates.
- External editor for composing prompts.
- Multi-line input and message queueing.
- File path autocompletion for @file mentions.
- Session export (HTML/JSON) and session stats.
- Image input and image generation/editing tools.
- Undo/revert last agent action with file rollback.
- Session tree UI with branch navigation and summaries.
- Message queue modes (steer vs follow-up) during streaming.
- Agentic review panel for reviewing agent-generated changes.
- PDF/image analysis without bloating core context.
- Thread map visualization and thread labels UI.
- Edit prior message to revert downstream changes.
- Custom commands (prompt templates or scripts) via command palette.
- Interactive mode keyboard shortcuts, reverse search, and editor integrations.
- Background task execution with status updates.
- Direct shell input mode for quick commands.
- Permission mode toggles (auto-accept, plan/read-only, normal).
- Vim-style editor mode for input.

Integrations
- MCP servers (local/remote) with OAuth support.
- CLI tool integrations preferred where possible.
- IDE/editor adapters via RPC.
- On-demand MCP tool loading to reduce context/tool bloat.
- Agent can run as a local MCP server for external orchestration.
- CI automation via a thin wrapper or action around headless mode.
- Programmatic SDK to start/resume sessions and stream events.
- TUI can attach to an existing server instance.

Checkpointing & Rewind
- Automatic checkpoints for tool-based file edits.
- Rewind conversation and workspace state to a checkpoint.
- Checkpoints persist across sessions.

Security & Safety
- Sandboxing profiles for untrusted execution.
- Secret redaction in logs/outputs where applicable.
- Permission policy engine (allow/ask/reject/delegate).
- Loop detection to stop repeated identical tool calls.
