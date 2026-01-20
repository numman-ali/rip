# RIP TUI Design Index

Status: **Early Design** | Phase: 2 | Last updated: 2026-01-20

---

## Executive Summary

**What**: A terminal-based UI (Ratatui) for observing and interacting with RIP agent sessions.

**Current implementation posture**: A baseline fullscreen terminal UI ships as the default `rip` UX (observability-first, frame-driven). Basic remote attach is supported (`rip --server <url> --session <id>`). Phase 2 work is the richer interaction layer (threads/palette/editor/resume) on top of the same frames.

**Why**: Provide real-time observability into agent behavior, natural-language interaction, and efficient keyboard-driven workflows for power users.

**Core principles**:
1. **Observability-first** — Users should always know what the agent is doing
2. **Frame-driven** — UI renders event frames; no business logic in the presentation layer
3. **Surface parity** — Every TUI feature has CLI/headless/SDK equivalents
4. **Build for change** — Capabilities will evolve; UI components must be modular

**Primary screens**: Start → Live Session (main workspace) → Tool Detail / Artifacts / Tasks

**Key interactions**: `Ctrl+K` command palette, `j/k` navigation, `Tab` focus cycling, `?` help

**Design status**: Early/exploratory. Flexibility and modularity are prioritized over pixel-perfect fidelity. The capability matrix is the source of truth; these designs visualize intent, not requirements.

---

## Design Status

These designs are **exploratory and subject to change**. RIP is in a research and experimental phase where we are building fast but require flexibility to pivot as capabilities evolve.

**For implementers**: When working from these designs, prioritize:
- **Flexibility** — Components should be easy to add, remove, or reorganize
- **Modularity** — Each screen/widget should be independent and composable
- **Adaptability** — Assume capabilities will expand, contract, or change
- **Minimal coupling** — Avoid hard dependencies between UI elements

The capability matrix is the source of truth. These designs visualize how capabilities surface to users, but the capabilities themselves may change. Build for change.

---

## Glossary

| Term | Definition |
|------|------------|
| **Frame** | A single event from the session event stream (e.g., `tool_started`, `provider_event`) |
| **Screen** | A full-terminal view (Start, Live Session, Thread Browser, etc.) |
| **Panel** | A region within a screen (Timeline, Output, Inspector, etc.) |
| **Overlay** | A modal layer on top of a screen (Command Palette, Tool Detail, Permissions) |
| **Thread** | A conversation branch; threads can fork and reference each other |
| **Checkpoint** | A workspace snapshot that can be reverted to |
| **Artifact** | A stored output (file, log, etc.) referenced by ID rather than inlined |
| **Capability** | A discrete feature from the capability registry (`session.create`, `tool.task_spawn`, etc.) |
| **Surface** | A presentation layer (TUI, CLI, headless, SDK) — all consume the same capabilities |
| **Render hint** | A data-only annotation from extensions that suggests how to display content |

---

## Screen Map

| Screen | Purpose | Primary Capabilities |
|--------|---------|---------------------|
| [Start](screens/01_start.md) | Launch point, new/attach/resume | `session.create`, `session.resume` |
| [Thread Browser](screens/02_thread_browser.md) | Search and manage threads | `thread.search.*`, `thread.tags`, `thread.archive` |
| [Thread Map](screens/03_thread_map.md) | Visual branch graph | `thread.map`, `thread.branch` |
| [Live Session](screens/04_live_session.md) | Primary workspace | `session.stream_events`, `session.send_input` |
| [Tool Detail](screens/05_tool_detail.md) | Inspect tool execution | `tool.*` frame inspection |
| [Artifact Viewer](screens/06_artifact_viewer.md) | View large outputs | `tool.output_fetch`, `context.refs.artifact` |
| [Background Tasks](screens/07_background_tasks.md) | Monitor async work | `tool.task_spawn`, `tool.task_status`, `tool.task_cancel`, `tool.task_stream_events` |
| [Permissions](screens/08_permissions.md) | Approval prompts | `policy.permissions.*`, `security.permissions` |
| [Review Panel](screens/09_review_panel.md) | Batch change review | `ui.review`, `checkpoint.rewind` |
| [Branch/Handoff](screens/10_branch_handoff.md) | Fork conversations | `thread.branch`, `thread.handoff` |
| [Command Palette](screens/11_command_palette.md) | Quick actions | `command.*`, `ui.palette` |
| [Errors/Help](screens/12_errors_help.md) | Recovery and guidance | Connection states, `ui.shortcuts` |

---

## Capability Mapping (Full)

Complete mapping of capability IDs to TUI elements with phase and parity notes.

| Capability ID | Phase | TUI Location | Parity (CLI/Headless/SDK) |
|--------------|-------|--------------|---------------------------|
| **Sessions & Threads** |
| `session.create` | P1 | Start screen | `rip` / `rip run` / `createSession()` |
| `session.send_input` | P1 | Input panel | Prompt arg / stdin / `sendInput()` |
| `session.stream_events` | P1 | Timeline, Output | `rip --server <url> --session <id>` / JSON stream / event iterator |
| `session.cancel` | P1 | `Ctrl+C`, status bar | `Ctrl+C` / SIGINT / `cancel()` |
| `session.resume` | P2 | Start, Thread Browser | `rip resume <id>` / `--session-id` / `resume()` |
| `thread.branch` | P2 | Branch flow | `rip branch` / same / `branch()` |
| `thread.handoff` | P2 | Handoff flow | `rip handoff` / same / `handoff()` |
| `thread.map` | P2 | Thread Map screen | `rip threads --tree` / JSON / `getThreadMap()` |
| `thread.search.*` | P2 | Thread Browser | `rip threads --search` / same / `searchThreads()` |
| `thread.tags` | P2 | Thread Browser | `rip thread tag` / same / `tagThread()` |
| **Tools & Tooling** |
| `tool.*` frames | P1 | Timeline, Tool Detail | `--verbose` / JSON frames / frame access |
| `tool.task_spawn` | P2 | Tasks panel | `rip task spawn` / same / `spawnTask()` |
| `tool.task_status` | P2 | Tasks panel | `rip tasks` / `--tasks --json` / `listTasks()` |
| `tool.task_cancel` | P2 | Tasks panel `[c]` | `rip task cancel` / same / `cancelTask()` |
| `tool.task_stream_events` | P2 | Tasks panel (attach) | `rip task stream` / SSE stream / `streamTaskEvents()` |
| `tool.task_write_stdin` | P2 | Task attach view | `rip task send` / HTTP / `writeTaskStdin()` |
| `tool.task_resize` | P2 | Task attach view | Auto (on terminal resize) / HTTP / `resizeTask()` |
| `tool.task_signal` | P2 | Tasks panel / Task attach view | `rip task signal` / HTTP / `signalTask()` |
| `tool.output_fetch` | P2 | Artifact Viewer | `rip artifact <id>` / raw bytes / `getArtifact()` |
| **Checkpointing** |
| `checkpoint.auto` | P1 | Tool Detail (cp ref) | Implicit / frame data / frame access |
| `checkpoint.rewind` | P1 | Review Panel, Tool Detail | `rip revert` / same / `revert()` |
| **Policy & Permissions** |
| `policy.permissions.*` | P2 | Permissions overlay | Interactive prompt / `--allow` flags / callback |
| `security.permissions` | P2 | Permissions overlay | Same | Same |
| **Commands** |
| `command.slash` | P2 | Command Palette, Input | `/command` / `--command` / method call |
| `command.registry` | P1 | Command Palette | `rip help commands` / `--list-commands` / `getCommands()` |
| **UI-Specific** |
| `ui.palette` | P2 | Command Palette | Slash commands + `rip help` / flags / method calls |
| `ui.review` | P2 | Review Panel | `rip review` / `--changes --json` / `getChanges()` |
| `ui.background_tasks` | P2 | Tasks panel | `rip tasks` / same / `listTasks()` |
| `ui.shortcuts` | P2 | Help overlay | `rip help` / `--help` / docs |
| `ui.keybindings` | P2 | Help overlay, config | Config file / N/A / N/A |
| `ui.theme` | P2 | Palette settings (`set:theme`) | Config file / N/A / N/A |
| `ui.raw_events` | P2 | Output/Timeline raw mode | `--json` output / JSON frames / frame iterator |
| `ui.clipboard` | P2 | Copy actions (`y`) | Pipe/redirect / N/A / N/A |
| **Extensions** |
| `extension.tool_renderers` | P2 | Output, Tool Detail | Formatted text / JSON hints / structured |
| **Skills** |
| `skill.invoke` | P2 | Command Palette | `rip skill <name>` / same / `invokeSkill()` |

---

## Document Structure

**Core Design**
- [Design Principles](01_design_principles.md) — UX philosophy, flexibility stance, parity rules
- [Navigation Model](02_navigation_model.md) — Screen hierarchy, state machine, transitions
- [Interaction Patterns](03_interaction_patterns.md) — Keys, focus, palettes, input conventions
- [Graceful Degradation](04_graceful_degradation.md) — How TUI features map to simpler surfaces
- [Performance Considerations](05_performance_considerations.md) — What needs to feel fast, scale concerns

**Screens**
- [Screens](screens/) — Individual screen specifications (12 screens)

---

## Open Questions

Track design decisions that need resolution:

- [ ] How prominent should cost/token display be?
- [ ] Should thread map be a modal or full screen?
- [ ] Vim mode: full vim emulation or vim-like navigation only?
- [ ] Image/media handling in terminal constraints?

---

## Changelog

| Date | Change |
|------|--------|
| 2026-01-19 | Initial design document created |
