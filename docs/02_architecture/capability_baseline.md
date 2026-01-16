# Capability Baseline (Modern Coding Agents)

Summary
- Vendor-neutral capability checklist for modern coding agents.
- No product names or vendor references.
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

Execution Modes
- Interactive TUI/CLI mode.
- Headless execute mode for automation/CI.
- Streaming JSON output for programmatic monitoring.
- Print mode (formatted output) and RPC mode (machine commands).
- RPC/SDK command surface for prompting, steering, follow-ups, and aborts.
- Streaming JSON input mode (stdin as event stream).

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
- Extension API to inject messages, append system prompts, and run commands.
- Persistent extension state stored in session entries.
- UI extension points (status, widgets, dialogs, custom editor).
- Event bus for cross-extension communication.
- Register commands, shortcuts, and flags from extensions.
- Custom tool renderers for tool calls/results.
- Extension error reporting and recovery hooks.
 - Extension access to session control (new session, branch, navigate tree).
 - Extension access to agent state (idle detection, abort, queue visibility).

Skills
- Skills discovered by `SKILL.md` files in workspace/user scopes.
- On-demand skill loading to avoid context bloat.
- User-invokable skill loading (manual activation over auto-load).
- Skill install/list/remove workflows and filters/ignores.
- Skill commands (`/skill:name`) for explicit activation.
- Skill repositories with scripts, references, and assets.
- Skill metadata for compatibility and allowed tools.

Subagents
- Spawn subagents with independent context windows and tool access.
- Specialized subagents (search, review, analysis) as optional modules.
- Subagent profiles with constrained tools, budgets, or objectives.

Models & Providers
- Multi-provider support with model switching at runtime.
- Thinking/reasoning levels with configurable budgets.
- Custom model registry and per-provider auth.
- Runtime model cycling and model availability queries.
- Agent mode profiles (fast vs deep) with different budgets.
 - Second-opinion model/tool for review-grade reasoning.

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

Integrations
- MCP servers (local/remote) with OAuth support.
- CLI tool integrations preferred where possible.
- IDE/editor adapters via RPC.
- On-demand MCP tool loading to reduce context/tool bloat.

Security & Safety
- Sandboxing profiles for untrusted execution.
- Secret redaction in logs/outputs where applicable.
- Permission policy engine (allow/ask/reject/delegate).
