# TUI — design

Authoritative, concise source of truth for the RIP TUI. Supersedes the 8-file
`tui/` tree that preceded Phase A. If this doc and the code disagree, the code
is truth; fix this doc.

## Identity

**Continuity surface, not a chat app.** The TUI is one *canvas* that narrates
one continuity over time. "Turns" are a canvas-level concept, not a session-
lifecycle reset. Ambient state — running tools, tasks, background jobs,
compiled context — persists across turns.

**Surface-only posture.** Every TUI action flows through a capability
(local runtime) or transport call (`--server <url>`). The TUI never reads
continuity state from disk, writes to the event log directly, or encodes
business rules. UI-local prefs (theme, activity-rail pin, input history) live
under `~/.rip/state/` and are explicitly **not** continuity truth.

**Multi-actor canvas.** `UserTurn.actor_id` + `origin` and `AgentTurn.role`
+ `AgentTurn.actor_id` are required fields on every canvas message. Rendering
branches on `role`; nothing assumes a single agent or a single human.

**Strategy lives in the kernel / extension host.** Memory, retrieval, RLM,
reviewer planning, subagent orchestration, compaction planning — all
capability-owned. The TUI *renders* their jobs, artifacts, decisions, and
controls; it never computes them. New cognition modules ship as capabilities
emitting structured frames (`ContinuityJob*`, future `Extension*`); the TUI
picks them up via `JobNotice` / `ExtensionPanel` with zero structural change.

## Interaction model

**One screen, four zones:**

1. *Hero* — borderless top row. `thread · agent · model` left, state +
   TTFT right. Responsive truncation cascade: thread shrinks first to 20
   chars, then agent becomes its glyph, then model tail truncates.
2. *Canvas* — vertical list of structured `CanvasMessage`s. 3-column gutter
   narrates the conversation; body at col 3. Tool cards, task cards, system
   notices, context notices, compaction checkpoints are first-class.
3. *Activity strip* — single row, borderless, `fg_muted`. Auto-hides when
   idle and at bottom. Colored by worst current state (danger / warn / tool
   / task / job / context).
4. *Input zone* — `▎`-prefixed editor that grows up to 6 rows for newlines;
   below it, a state-dispatched *keylight* row showing the 2–4 most useful
   shortcuts for the current state.

**Palette is the control plane.** Every action lives there. `⌃K` opens the
Command mode; `Tab` cycles modes inside an open palette. Direct hotkeys for
high-traffic modes: `⌃G` Go To, `⌃T` Threads, `M-m` Models, `M-o` Options.
`?` opens Help.

**Overlays are a stack.** `Vec<Overlay>` in `TuiState`. Esc pops an overlay
when one owns input; otherwise it stops the active run. `C-c` quits and, by
default, stops any active session before exit. Activity, Palette,
ToolDetail, TaskDetail, ErrorDetail, StallDetail, Debug, Help, ErrorRecovery
all live on the same stack and use the shared dim-behind layout.
ErrorRecovery auto-opens on the first provider-error frame per run. An
explicit detach/leave-running action is a separate behavior and should not be
confused with the default quit path.

**Summoned, not stacked.** No tabs. No permanent panels. Canvas is always
the only canvas. Everything else is summoned (palette, overlays) or ambient
(activity strip, keylight).

## Canvas model

`CanvasMessage` enum + `messages: Vec<CanvasMessage>` inside `CanvasModel`.
Variants:

- `UserTurn { message_id, actor_id, origin, blocks, submitted_at_ms }`
- `AgentTurn { message_id, run_session_id, agent_id, role, actor_id,
  model, blocks, streaming_tail, streaming, started_at_ms, ended_at_ms }`
- `ToolCard { message_id, tool_id, tool_name, args_block, status, body,
  expanded, artifact_ids, started_seq, started_at_ms }`
- `TaskCard { message_id, task_id, tool_name, title, execution_mode,
  status, body, expanded, artifact_ids, started_at_ms }`
- `JobNotice { message_id, job_id, job_kind, details, status, actor_id,
  origin, started_at_ms, ended_at_ms }`
- `SystemNotice { message_id, level, text, origin_event_kind, seq }`
- `ContextNotice { message_id, run_session_id, strategy, status,
  bundle_artifact_id, contributed_artifact_ids }`
- `CompactionCheckpoint { message_id, checkpoint_id, from_seq, to_seq,
  summary_artifact_id }`
- `ExtensionPanel { message_id, panel_id, extension_id, title, placement,
  lines, keys, artifact_ids }` — declared but not yet produced by
  ingestion; lights up when the `extension.ui` capability ships.

Everything derives from frames. The TUI never invents an event kind, job
kind, or actor id — it dispatches on whatever the kernel emits.

**StreamCollector** (`rip-tui::canvas::stream_collector`) owns the streaming
text plumbing: accumulates `OutputTextDelta` into a fence-aware parser,
promotes stable blocks (paragraph, heading, list, code fence, etc.) into
`AgentTurn.blocks`, and holds the current incomplete tail in
`streaming_tail` for live rendering.

**Block cache** (`CachedText`) holds pre-built ratatui `Text<'static>` that
is theme-invariant — toggling the theme does not invalidate the cache
because per-token styling is applied at render time, not at parse time.

## Palette engine

`PaletteSource` trait (in `rip_tui::palette`) — per-mode contract. Phase C
ships five modes: `Command`, `Models`, `GoTo`, `Threads`, `Options`.

`CommandAction` (in `rip_tui::palette::modes::command`) is the canonical
enum of every palette-driven action. Each entry carries:

- `id()` — stable string for dispatch (e.g. `"canvas.scroll-bottom"`).
- `title()` — user-facing label.
- `category()` — `CANVAS / THREADS / RUNS / OPTIONS / DEBUG / SYSTEM`.
- `is_available()` — `false` for `[deferred]` entries whose backing
  capability is not yet in the registry. Those entries ship *visible* with
  an `unavailable` chip.

The Help overlay (`?`) renders from the same `CommandAction::ALL` table,
so adding a palette entry automatically makes it discoverable in Help.

## Overlays

| Overlay        | Opens via                   | Key actions                    |
| -------------- | --------------------------- | ------------------------------ |
| Activity       | Command palette             | `⎋` close                      |
| Palette        | `⌃K / ⌃G / ⌃T / M-m / M-o`  | `↑↓`, `⏎`, `Tab`, `⎋`          |
| ToolDetail     | `x` on focused card         | `⎋` close                      |
| TaskDetail     | `x` on focused card         | `⎋` close                      |
| TaskList       | Command palette             | `↑↓`, `⏎`, `⎋`                 |
| ErrorDetail    | `x` on focused error notice | `⎋` close                      |
| StallDetail    | auto on stall               | `⎋` close                      |
| Debug          | Command → Show debug info   | `⎋` close                      |
| Help           | `?`                         | `⎋` close                      |
| ErrorRecovery  | auto on provider error      | `r/c/m/x/⎋`                    |

The overlay renderer dims the canvas area under the top overlay so the
operator always sees the hero, input, and keylight.

## Theme

`Theme` struct with semantic tokens (`bg_base`, `bg_raised`, `bg_sunken`,
`fg_primary`, `fg_body`, `fg_muted`, `fg_quiet`, `accent_user`,
`accent_agent`, `accent_tool`, `accent_task`, `accent_warn`,
`accent_danger`, `accent_success`, `rule`). Two themes ship: **Graphite**
(default dark) and **Ink** (warm off-white). Terminal capability detection
degrades to 256 / 16 / Mono.

Switch with `⇧T` or Options → Toggle theme. Theme changes are a one-frame
repaint — no cache invalidation, no flicker.

## Animation

**Policy: motion reflects real work or guides attention to a just-changed
element. No decoration.**

Shipped today:
- New message fade-in (2 frames, `fg_muted → fg_primary`).
- Theme swap (snap repaint).

Planned (Phase D extension work; infrastructure is in the frame loop but
not yet wired):
- Idle breath (`·` in input gutter, 2400 ms cycle).
- Thinking cycle (`◐◓◑◒` on agent gutter pre-first-token).
- Streaming pulse (agent-gutter color modulates with token arrival).

**Never shipped:** spinners, ASCII progress bars, shimmer, gradient
dithering, typewriter reveals, auto-scroll bounce.

## Responsive breakpoints

```rust
Xs  w < 80    phone SSH; no outer frame, no activity strip as first-class row
S   w < 100   activity strip, modal palette 70% wide
M   w < 140   optional palette preview pane ≥120
L   w ≥ 140   thin outer frame; optional pinned activity rail
```

Snapshots are gated at xs/s/m across graphite/ink/nocolor per journey.

## Keymap

Core defaults (everything else via palette or user config at
`~/.rip/keybindings.json`):

| Key         | Action                                          |
| ----------- | ----------------------------------------------- |
| `⌃C / ⌃D`   | Quit                                            |
| `⏎`         | Submit / Expand focused card / Apply palette    |
| `⇧⏎ / M-⏎`  | Newline                                         |
| `⎋`         | Pop top overlay                                 |
| `⌃K`        | Palette (Command)                               |
| `⌃G`        | Palette (Go To)                                 |
| `⌃T`        | Palette (Threads)                               |
| `M-m`       | Palette (Models)                                |
| `M-o`       | Palette (Options)                               |
| `?`         | Help                                            |
| `[ / ]`     | Focus prev / next canvas message                |
| `x`         | Open per-item detail (X-ray)                    |
| `⌃R`        | Open X-ray on focused item (ex-"toggle raw")    |
| `⌃F`        | Toggle follow-tail                              |
| `⌃Y`        | Copy selected                                   |
| `PageUp/Dn` | Scroll canvas                                   |
| `↑ / ↓`     | Select prev / next event                        |
| `Tab`       | Cycle palette mode (inside open palette)        |

Retired defaults (reachable via palette; rebind in config for muscle
memory): `⌃B`, `M-t`, legacy `Tab` → details-mode, `⌃T` → tasks, `⌃R`
→ global raw view.

## Capability backing

Every TUI action names a capability from `docs/03_contracts/capability_registry.md`
or is marked `[deferred]`. Full matrix in the revamp plan
(`docs/07_tasks/tui_revamp.md` Part 17). Highlights:

- **Submit / Retry** → `thread.post_message` (supported)
- **Stop streaming** → `session.cancel` (supported)
- **Stream events** → `session.stream_events` (supported)
- **Thread list / get** → `thread.list`, `thread.get` (supported)
- **Compaction run / schedule / status** → `compaction.*` (supported)
- **Provider cursor rotate / status** → `thread.provider_cursor.*` (supported)
- **Context status** → `thread.context_selection.status` (supported)
- **Config doctor** → `config.doctor` (supported)
- **Clipboard / theme / keybindings** → `ui.*` (supported)
- **X-ray** → in-memory `FrameStore` (supported; no capability needed)

Deferred (ship with disabled entries or absent overlays):
- `thread.create` / `thread.rename` — palette entries 11, 15 disabled.
- `tool.output_fetch` / `tool.output_store` — ArtifactViewer overlay
  declared but disabled.
- `ui.palette`, `ui.multiline`, `ui.editor` flip from planned →
  supported when this revamp lands; register update lives in a
  follow-up roadmap item.
- `extension.commands` / `extension.ui` / `extension.tool_renderers` —
  `CanvasMessage::ExtensionPanel` variant declared; panel slot is
  render-only until capabilities ship.

## Source map

- `crates/rip-tui/src/state.rs` — `TuiState`, `Overlay` enum, overlay
  helpers, `begin_pending_turn` (ambient-state-preserving).
- `crates/rip-tui/src/canvas/{mod,model,stream_collector,markdown}.rs` —
  `CanvasMessage`, `Block`, `CachedText`, `StreamCollector`.
- `crates/rip-tui/src/render/{mod,status_bar,canvas,activity,input}.rs` —
  zones. Hero is `status_bar.rs` (name kept for minimal churn).
- `crates/rip-tui/src/render/overlays/{mod,palette,debug,help,
  error_recovery,task_list,task_detail,tool_detail,error,stall,
  activity}.rs` — overlay renderers.
- `crates/rip-tui/src/palette/{mod,modes/*}.rs` — palette engine.
- `crates/rip-tui/src/theme.rs` — semantic tokens + Graphite/Ink +
  color-depth degradation.
- `crates/rip-tui/src/summary.rs` — event → one-line summary.
- `crates/rip-tui/src/frame_store.rs` — in-memory frame cache for X-ray.
- `crates/rip-cli/src/fullscreen.rs` — driver (event loop, keymap,
  transport, palette apply dispatcher, error-recovery key handlers).
- `crates/rip-cli/src/fullscreen/keymap.rs` — `Command` enum + default
  bindings + `~/.rip/keybindings.json` loader.

## Testing

- **Snapshot journeys.** `crates/rip-tui/tests/golden.rs` with
  `RIPTUI_UPDATE_SNAPSHOTS=1 cargo test -p rip-tui` regeneration flow.
  Current coverage: basic × xs/s/m, follow-a-run × xs/s/m,
  background-tasks × xs/s/m, recover-error × xs/s/m, multi-turn
  continuity × s, markdown × s/m, tool-card expand, debug overlay,
  help overlay, error-recovery overlay, palette command / go-to.
- **Unit tests.** Every palette mode, hero segment builder, keylight
  dispatcher, strip builder, theme constructor, and stream collector
  has dedicated unit tests.
- **Driver tests.** `crates/rip-cli/src/fullscreen.rs::tests` exercises
  keymap dispatch, palette hotkeys, and the Tab/M-t retirement.

## Not in scope for this revamp

- Canvas virtualization (only if 10k+ bench regresses).
- True spatial palette positioning (per-origin geometry).
- Motion primitives (breath, thinking cycle, streaming pulse) beyond
  infrastructure.
- Subagent color palette (data model supports it; theme tokens land with
  D.4 follow-up).
- Vim input mode (opt-in toggle stub; real implementation is
  follow-up work).
- Mouse polish (click hero segments, drag-select, click canvas items).
- ArtifactViewer full UX (shipped [deferred] until `tool.output_fetch`).
- ThreadPicker full UX (shipped [deferred]; Threads palette mode is the
  default path for now).
- Extension panels (`ExtensionPanel` variant + ExtensionOverlay slot
  declared; no ingestion path until `extension.ui` ships).
