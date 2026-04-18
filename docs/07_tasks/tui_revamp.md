# RIP TUI — Comprehensive Revamp Plan

Status
- Canonical execution plan for the TUI revamp (Phases A–D, 35 sequenced commits).
- Tracked as a "Now" item in `docs/07_tasks/roadmap.md`.
- Surface-only and capability-backed: every action references a capability id in `docs/03_contracts/capability_registry.md`; anything without backing ships *[deferred]* (see Parts 16–17).
- Scope is the RIP fullscreen TUI only. Runtime / capability contract work is explicitly out of scope — if the revamp wants a capability that does not exist, it is deferred here and promoted through docs-first capability work separately.
- Active: 2026-04-18. Amendments land as commits to this file so the history reflects plan evolution.

## Context

RIP is a continuity OS, not a chat app. The current fullscreen TUI is architecturally sound — frame-driven state, a clean `rip-tui` vs `rip-cli` seam, responsive golden snapshots at xs/s/m — but the rendering layer is 1990s Borland: stacked bordered boxes, a debug-style status bar (`view:canvas session:s1 seq:6 hdr:- fb:- evt:-`), a `String`-backed canvas with a hardcoded `"You: "` prefix and byte-range "prompt ranges", overlays that double-frame the chrome, and a global `canvas` vs `xray` mode switch pretending to be a drill-down. It looks like every other agent TUI — which is to say, like nobody designed it with intent.

This plan replaces the entire rendering layer with a distinctive, restrained, typographically-literate workspace. The architecture stays; the skin, the interaction model, the chrome, and the palette engine change. No existing design docs under `docs/02_architecture/tui/` remain — they describe the old direction and are deleted as part of this work.

Goal: a TUI that feels like it was designed by one person with taste, not assembled from ecosystem patterns. Unique. Sleek. Never-seen-before. Not tacky.

The change set:

- Framework bumped to latest (Ratatui 0.30 + crossterm 0.29) as the first commit — everything else lands on the new foundation.
- Canvas becomes a structured `Vec<CanvasMessage>` with inline tool cards, streaming markdown blocks, cached renders.
- Chrome becomes borderless and typographic — no boxes, only rhythm and gutters.
- Palette becomes the primary control plane with five modes (Command · Go To · Models · Threads · Options) + Help overlay; every action is reachable by type-ahead.
- Overlays become a proper stack with in-UI provider-error recovery.
- X-ray becomes a per-item drill-down overlay, not a global mode.
- Theme becomes semantic tokens; two shipped themes (Graphite, Ink); NO_COLOR and ANSI-16 verified.
- A disciplined glyph vocabulary + a 3-col left "gutter" that narrates the conversation.
- Motion reduced to breath, streaming pulse, thinking cycle. No spinners, banners, or ornament.
- All old `docs/02_architecture/tui/` files (00_index, 01–07, `journeys/`, `screens/`, `widgets/`) deleted and replaced with a single `00_design.md`.
- The personal `ratatui-builder` skill under `~/.claude/skills/` is refreshed at the end so the next TUI task on this machine starts from the new end-state, not the old seams.

## Part 1 — North Star (the feeling)

**Quiet by default.** The canvas is the only thing that matters. Chrome disappears when not needed. Color means something; it never decorates.

**Typographic, not graphic.** Rhythm, weight, negative space, alignment do the heavy lifting. A terminal is a typographic instrument; we lean in.

**Gutter-narrated.** A 3-column left gutter carries role + state + provenance. Reading down the gutter narrates the conversation independent of content. Nobody else does this.

**Summoned, not stacked.** No tabs. No permanent panels. No always-on sidebars. One canvas. Everything else is summoned (palette, overlays) or ambient (activity strip, keylight).

**Palette-first.** Every command lives in the palette. Type-ahead discovery > memorized hotkeys. Hotkeys are aliases into palette actions.

**Zoom, not modes.** Press a key on a canvas item to zoom into its raw frames (X-ray). Never toggle a "raw mode" that rearranges the whole UI.

**Breath, not spinners.** Idle is a single pulsing dot. Streaming is a gutter glyph that pulses with token arrival. No rotating characters. No emoji soup. No progress bars.

**Continuity everywhere.** Turns don't reset ambient state. A background task started on turn 3 is still visible on turn 10. Errors persist until acknowledged.

**Surface only.** The TUI is a surface adapter. Every action — submit, retry, rotate cursor, switch model, switch thread, compaction, checkpoint, theme — flows through a capability (local runtime) or transport call (`--server`). The TUI never reads continuity state from disk, writes to the event log directly, or encodes business rules. Filesystem paths that persist UI-local prefs (theme, recents, input history) are explicitly not continuity truth and are documented as such.

**Multi-actor from day one.** The canvas is a stream of contributions by distinguishable actors — primary agent, subagent, reviewer, human collaborator — each carrying `actor_id` + `origin` provenance. Rendering must never assume "one human + one agent." The gutter already carries role; the model must extend to reviewers, retrievals, and plugin-contributed panels without structural change.

**Modular cognition.** Memory, retrieval, RLM, reviewer, subagent, compaction planning — these are runtime capabilities owned by the kernel / extension host. The TUI renders their jobs, their artifacts, their decisions, and any controls they expose; it never owns their strategy. A new cognition module should surface in the UI by implementing a capability and emitting structured frames, not by editing the TUI.

**Taste trumps breadth.** We'd rather ship one beautiful primitive than three good ones. No feature exists to match a competitor; every feature exists because it serves the conversation.

## Part 2 — Visual Identity

### 2.1 Two themes, both via semantic tokens

**Graphite (default, dark).** Near-black base, calm accents. Inspired by the discipline of Linear dark + Arc Browser's restraint.

```
bg_base         #0D1117   true / Black          16-color
bg_raised       #151B23   cards, overlays, selected row
bg_sunken       #0A0E14   keylight strip, separators, dim-behind-overlay
fg_primary      #E6EDF3   message body, title
fg_body         #C9D1D9   paragraph body
fg_muted        #7D8590   metadata, timestamps, model chip
fg_quiet        #484F58   rules, dividers
accent_user     #7EE787   user gutter + accent rule
accent_agent    #79C0FF   agent gutter + focus rule (primary agent)
accent_tool     #A5A5F8   tool gutter
accent_task     #D2A8FF   task gutter
accent_warn     #E3B341   warning / stall
accent_danger   #FF7B72   error
accent_success  #7EE787   completed
rule            #21262D   1-col left rule when unfocused
focus_tint      bg_raised layered over bg_base at 40%   (simulated)
```

**Ink (light).** Warm off-white base, deep warm neutral content, indigo-plum accent. Inspired by paper. Genuinely cream, not white-on-gray.

```
bg_base         #F4EFE6
bg_raised       #EEE7D9
bg_sunken       #E8DFCF
fg_primary      #1A1A1A
fg_body         #2C2C2C
fg_muted        #6B6B6B
fg_quiet        #9A9A9A
accent_user     #3F6E3F
accent_agent    #3B5FCF
accent_tool     #5E548E
accent_task     #7A4E99
accent_warn     #B8860B
accent_danger   #C42B1C
accent_success  #3F6E3F
rule            #D9CFBE
```

**Implementation.** `crates/rip-tui/src/theme.rs` owns a `Theme` struct with named fields exactly as above. `Theme::graphite()` / `Theme::ink()` constructors. Terminal capability detection at startup (`supports-color` crate detection of `COLORTERM`, `TERM`, `NO_COLOR`) yields a `ColorDepth` enum that themes degrade into: TrueColor → 256 → ANSI16 → Mono. Each theme has explicit per-depth mappings; no lossy approximation.

**No globals.** `render::*` functions take `&Theme` by reference. Theme switching is a palette action — applies immediately, persists to `~/.rip/state/tui.json`.

### 2.2 Glyph vocabulary (strict)

Every glyph has a meaning. No glyph is decoration. All have ASCII fallbacks (auto-detected via `LANG` / `LC_CTYPE`).

| Role          | Glyph | ASCII | Notes |
|---|---|---|---|
| User turn     | `›`   | `>`   | Gutter open; colored `accent_user` |
| Agent turn    | `◉`   | `*`   | Gutter open; colored `accent_agent` |
| Agent streaming | `◎` | `o`   | Pulses with token arrival |
| Agent thinking  | `◐◓◑◒` | `.`  | 400 ms quarter-fill cycle, pre-first-token only |
| Subagent      | `◈`   | `+`   | Secondary agent in multi-actor continuities |
| Tool (sync)   | `⟡`   | `~`   | Inline tool card gutter |
| Task (bg)     | `⧉`   | `#`   | Background task gutter |
| System notice | `·`   | `.`   | Quiet, `fg_muted` |
| Context       | `⌖`   | `@`   | Context compile notice |
| Error         | `▲`   | `!`   | `accent_danger` |
| Stall / idle  | `·`   | `.`   | Breath glyph in input gutter |
| Focus rule    | `▎`   | `\|`  | 1-col left rule on focused element |
| Provenance    | `◌`   | `o`   | Actor chip in header when >1 human |
| Artifact      | `⧉`   | `#`   | (Overloaded with task; disambiguated by context — tasks only appear in gutter, artifacts inline) |
| Card corners  | `╭ ╮ ╰ ╯` | `+ + + +` | Rounded (BorderType::Rounded) |
| Card rule     | `─`   | `-`   | Horizontal card top/bottom |

**Deleted from current codebase:** `✓`, `✗`, `⊘`, `⏸`, `◔`, `◯`, `⟳`, `📄`, `📎`, `⚠`, `⏸`, and the inconsistent mix of shapes. Status is conveyed by color + position, not emoji.

### 2.3 Rules, separators, rhythm

- **Message separator:** blank line(s); no `─` between messages.
- **Section break** (rare): `······` centered in `fg_quiet`, 1 blank above/below. Used only at compaction checkpoints.
- **Card top:** `╭─ title ──────────── meta ─╮` (rounded). **Bottom:** `╰──────────────╯`. Sides are implicit — card body is indented 2 cols from the card's left edge, no vertical rule.
- **Input:** 1-col left rule in `accent_agent` when focused, `fg_quiet` when a palette/overlay has focus. No border box.
- **Outer frame:** optional 1-row outer border at `L` breakpoint only (a discreet `rule` rule on all sides). At xs/s/m: no outer frame.

**Vertical rhythm (spacing between canvas items):**

```
User → Agent                  2 blank lines
Agent → Inline tool card      1 blank line
Tool card → Tool card         1 blank line
Tool card → Agent continues   1 blank line
Any → System notice           1 blank line
Compaction checkpoint         2 blank + `······` + 2 blank
```

**Indentation:** gutter occupies cols 0–2. Message body starts at col 3. Tool cards live inside the same indent (card corners start at col 3, card body at col 5).

### 2.4 Color discipline

- Color never conveys meaning alone. Every color has a glyph / text partner.
- `fg_primary` for titles; `fg_body` for paragraph body; `fg_muted` for metadata (timing, model, seq); `fg_quiet` for rules and separators.
- Accent usage: gutter glyphs, keylight active-key highlight, palette selection left-rule, artifact chip, provenance chips. **Not** on message body backgrounds. **Not** on card fills.
- No colored backgrounds on message bodies. The current `Color::Rgb(23, 34, 48)` ribbon on user turns is deleted. User turns are distinguished by gutter glyph + very light italic on `fg_primary`, not block tint.

## Part 3 — Interaction Architecture

### 3.1 One screen, four zones

```
┌─────────────────────────────────────────────────────────────────┐
│ slide-prep · rip · gpt-5                  streaming · ttft 120  │  (1) Hero  – 1 row, no border
│                                                                 │
│ ›  You                                                          │
│    Add a slide outline for a product launch.                    │
│                                                                 │  (2) Canvas
│ ◉  RIP                                                          │
│    Here's a 5-slide outline. I'll refine after a quick check    │
│    of the repo.                                                 │
│                                                                 │
│    ╭─ write · slides.md ─────────────── ✓ 120ms ───────╮       │
│      Opening: title + tagline                                   │
│      Problem: 30s framing                                       │
│      …                            5 artifacts · ⏎ expand       │
│    ╰─────────────────────────────────────────────────╯         │
│                                                                 │
│    Want me to draft slide 1?                                    │
│                                                                 │
│ ·  compiling context · 1 task running                           │  (3) Activity – 1 row, dim, auto-hide when idle
├─────────────────────────────────────────────────────────────────┤
│ ▎› _                                                            │  (4a) Input – left accent rule; grows to 6 rows
│    ? help    ⌘K command    ⌘M model    ⌘G go to                 │  (4b) Keylight – 1 row, context-aware
└─────────────────────────────────────────────────────────────────┘
```

No vertical rules. Zones are delineated by rhythm and the horizontal `rule`-colored separator above the input.

### 3.2 Zones in detail

**Hero (row 1).** Borderless. Left: `thread · agent · model` with `·` separators in `fg_quiet`. Right-aligned: state (`idle | thinking | streaming | stalled | error`) + latest TTFT. Truncation rule: thread shrinks first (to ≤20 chars with `…`), then agent becomes its glyph, then model to `…nano`. Mouse: click any segment → opens the corresponding palette mode.

**Canvas (rows 2 … n-3).** Vertical list of structured messages. 3-col gutter. Body at col 3. Scrollable by PgUp/PgDn, mouse wheel, and `⌘[` / `⌘]` (prev/next message jump). Follows tail by default; `⌃F` toggles follow.

**Activity (row n-2).** One row, `fg_muted`, borderless. Summarizes currently-running work: `⟡ write README.md · ⧉ 2 tasks · ⌖ ctx compiled`. Auto-hides when idle **and** transcript is at bottom. `accent_warn` when stalled; `accent_danger` when unhandled error. Click opens a full Activity overlay.

**Input zone (rows n-1, n).** Top row: editor with `▎` accent rule. Bottom row: keylight. See Part 3.6.

### 3.3 The gutter

The gutter is the narrative track. Every message gets a glyph in column 0 (center of 3-col gutter, 1 space before and after):

```
cols:   0 1 2  3 →
        ›       You
                Add a slide outline for a product launch.

        ◉       RIP
                Here's a 5-slide outline.
        ⟡       write · slides.md  ✓ 120ms
                …card body…
        ◉       Want me to draft slide 1?
```

Reading down column 0 — `› ◉ ⟡ ◉` — you already know the shape of the conversation. This is the signature move.

When something is focused (e.g. for keyboard navigation), a `▎` appears in column 1 of its gutter row in the element's accent color. Unfocused elements have blank col 1.

### 3.4 Palette is the control plane

Every action lives in the palette. `⌘K` opens `Command` mode. `/foo` in input or palette query switches modes. Five modes shipped in Phase C; engine supports unlimited more.

Hotkeys are aliases. `⌘M` literally calls `palette.open(Models)`. New commands never need a new root key — they get a palette entry.

### 3.5 Overlays are a stack

`Vec<Box<dyn Overlay>>` in `TuiState`. Top of stack gets all input. Esc pops one; Ctrl-C pops all.

Shipped overlays: `Palette`, `ToolDetail`, `TaskDetail`, `ErrorDetail`, `Xray`, `Help`, `ArtifactViewer`, `ThreadPicker`, `Debug`.

When stack is non-empty, the canvas area is tinted to `bg_sunken` (no blur available in TTY; tint + slight `fg` dim). This is the "focus peel." Hero, input, and keylight stay at `bg_base` so the user always sees the current context.

### 3.6 Keylight (the contextual shortcut row)

Below the editor, one row in `fg_muted`, reconfigured per state:

```
idle          ? help    ⌘K command    ⌘M model    ⌘G go to
typing        ⏎ send    ⇧⏎ newline    ⌘K command
thinking      ⎋ stop    ⌘← scroll
streaming     ⎋ stop    ⌘← scroll
tool running  ⎋ cancel  ⏎ inspect    ⌘← scroll
error         r retry   c rotate cursor   x raw    ⎋ dismiss
palette open  ↑↓ select  ⏎ apply    ⇥ mode    ⎋ close
overlay open  ⎋ close    ↑↓ scroll    x raw
```

Keys are `fg_primary`; their labels are `fg_muted`. On xs, truncate right-to-left but never below 2 shortcuts.

This replaces the current "help crammed into input box title bar" pattern. Keylight is a first-class primitive, not a static help line.

### 3.7 Spatial palette

Palette modal is `min(60, area.width - 4) × min(18, area.height - 6)`, centered or biased toward its summoning origin:

| Summoned via | Position |
|---|---|
| `⌘K` (from anywhere) | top-center |
| `⌘M` (Models) | top-right (aligns with model chip in hero) |
| `⌘G` (Go To) | center |
| `⌘T` (Threads) | top-left (aligns with thread chip in hero) |
| `/` in input | bottom-center, flush above input |
| Command-mode action "Open …" | same position as whatever spawned that action |

Subtle; nobody does it; feels alive. Memory is: where it was summoned from, the palette "came from" that place.

### 3.8 Breath and motion

When **completely** idle (no streaming, no tools running, no overlays open, no pending submit), the breath glyph `·` in the input gutter fades `fg_quiet → fg_muted → fg_quiet` on a 2400 ms cycle. That's the entire ambient motion.

During **thinking** (agent started, no tokens yet), the agent gutter cycles `◐ ◓ ◑ ◒` at 400 ms per frame.

During **streaming**, the agent gutter is `◎`. Its color pulses toward `fg_primary` with token arrival (content-driven, not clock-driven) and relaxes back to `accent_agent` between tokens. On stream end, snap to solid `◉`.

New canvas messages **fade in** over 2 frames: `fg_muted → fg_primary` for message body, gutter glyph flashes at `accent_{role}` briefly.

Palette **open** animates height `0 → 18 rows` in 3 frames (~48ms).

Overlay **dim-behind** is a snap, not a fade. Single-frame change.

**Nothing else animates.** No spinners (`| / - \`), no bouncing cursors, no shimmer, no ASCII art banners, no gradient dithering, no typewriter reveals, no auto-scroll "bounce."

## Part 4 — Canvas Model

### 4.1 CanvasMessage enum

Replaces `output_text: String` + `prompt_ranges: Vec<(usize, usize)>` with a structured list.

```rust
pub enum AgentRole {
    Primary,
    Subagent { parent_run_id: String },
    Reviewer { target_message_id: String },
    // Extension-contributed roles are represented as `Extension { kind: String }`
    Extension { kind: String },
}

pub enum CanvasMessage {
    UserTurn {
        message_id: String,
        actor_id: String,
        origin: String,              // "cli" / "tui" / "sdk" / "hook"
        blocks: Vec<Block>,
        submitted_at_ms: u64,
    },
    AgentTurn {
        run_session_id: String,
        agent_id: Option<String>,
        role: AgentRole,             // Primary | Subagent | Reviewer | Extension — drives gutter glyph + accent
        actor_id: String,            // provenance — distinguishes two parallel primary agents in shared continuities
        model: Option<String>,
        blocks: Vec<Block>,
        streaming: bool,
        started_at_ms: u64,
        ended_at_ms: Option<u64>,
    },
    ToolCard {
        tool_id: String,
        tool_name: String,
        args_block: Block,
        status: ToolCardStatus,       // Running | Succeeded { duration_ms, exit_code } | Failed { error }
        body: Vec<Block>,             // stdout/stderr/pty
        expanded: bool,
        artifact_ids: Vec<String>,
        started_seq: u64,
        started_at_ms: u64,
    },
    TaskCard {
        task_id: String,
        tool_name: String,
        title: Option<String>,
        execution_mode: ToolTaskExecutionMode,
        status: TaskCardStatus,
        body: Vec<Block>,
        expanded: bool,
        artifact_ids: Vec<String>,
        started_at_ms: Option<u64>,
    },
    /// Background kernel job (summarizer, indexer, retriever, reviewer planner,
    /// memory writer, compaction planner). The TUI displays status + artifacts;
    /// the strategy logic lives in the kernel / extension host.
    ///
    /// Fields mirror `ContinuityJobSpawned` / `ContinuityJobEnded` exactly:
    /// everything here is derived from frame contents, nothing invented.
    JobNotice {
        job_id: String,
        job_kind: String,             // kernel-assigned; no fixed enum — glyph lookup is a TUI concern
        details: Option<Value>,       // raw from frame; shown in JobDetail / X-ray
        status: JobStatus,            // derived: Running while only Spawned seen; Succeeded/Failed/Cancelled after Ended
        result: Option<Value>,        // present only after Ended
        error: Option<String>,        // present only after Ended with status=failed
        actor_id: String,
        origin: String,
        started_at_ms: Option<u64>,
        ended_at_ms: Option<u64>,
    },
    SystemNotice {
        level: NoticeLevel,           // Quiet | Info | Warn | Danger
        text: String,
        origin_event_kind: String,
        seq: u64,
    },
    ContextNotice {
        run_session_id: String,
        strategy: String,             // e.g. "default" | "retrieval-augmented" | extension-defined
        status: ContextStatus,
        bundle_artifact_id: Option<String>,
        contributed_artifact_ids: Vec<String>,   // retrieval / RLM / memory artifacts folded into context
    },
    CompactionCheckpoint {
        checkpoint_id: String,
        from_seq: u64,
        to_seq: u64,
        summary_artifact_id: String,
    },
    /// A plugin/extension-contributed UI panel. Rendered in a sandboxed slot
    /// with a narrow render contract (lines + styles + key routes). Strategy
    /// and state belong to the extension; the TUI owns only layout + focus.
    ExtensionPanel {
        panel_id: String,
        extension_id: String,
        title: String,
        placement: PanelPlacement,   // Inline (canvas card) | Overlay | ActivityChip
        lines: Vec<StyledLine>,      // pre-rendered by the extension via capability
        keys: Vec<(String, String)>, // capability-namespaced actions the extension exposes
        artifact_ids: Vec<String>,
    },
}

pub enum Block {
    Paragraph(CachedText),
    Heading { level: u8, text: CachedText },
    Markdown(CachedText),              // for mixed inline content
    CodeFence { lang: Option<String>, text: CachedText /* syntect-highlighted */ },
    BlockQuote(Vec<Block>),
    List { ordered: bool, items: Vec<Vec<Block>> },
    Thematic,
    ToolArgsJson(CachedText),          // pretty-printed JSON with dimmed keys
    ToolStdout(CachedText),
    ToolStderr(CachedText),            // accent_danger tint
    ArtifactChip { artifact_id: String, bytes: Option<u64> },
}

/// Pre-rendered ratatui::text::Text with a hash of its source for invalidation.
pub struct CachedText {
    text: ratatui::text::Text<'static>,
    source_hash: u64,
}
```

The cache holds pre-built `Text`. Rendering blits cached `Text`; only the tail block of a streaming turn is re-parsed frame-to-frame.

### 4.2 Ingestion rules (`TuiState::update(event)` → messages)

Rule of thumb: **the TUI reads frames and produces `CanvasMessage`s; it never consults external state to decide what a frame means.** Any interpretation — actor identity, job kind, inline policy, extension panel placement — must come from fields on the frame itself (which the kernel / extension host is responsible for emitting correctly).

- `SessionStarted`: if pending `UserTurn` exists (from `begin_pending_turn`) do nothing; else push `UserTurn` derived from `input`. Then push `AgentTurn { streaming: true, blocks: [], role }` where `role` is resolved from the session's `parent_run_id` (→ `Subagent`), `reviewer_target_message_id` (→ `Reviewer`), or defaults to `Primary`.
- `OutputTextDelta`: feed delta into `StreamCollector`; collector emits stable `Block`s which append to the tail `AgentTurn.blocks`; a transient tail block renders beneath stable blocks.
- `SessionEnded`: mark tail `AgentTurn.streaming = false`, stamp `ended_at_ms`.
- `ToolStarted`: push `ToolCard { status: Running }` at current tail.
- `ToolStdout` / `ToolStderr`: push preview into matching card's `body`.
- `ToolEnded`: flip status to `Succeeded`, fold artifacts.
- `ToolFailed`: flip to `Failed`, attach error.
- `ToolTaskSpawned` / `ToolTaskStatus` / `ToolTaskOutputDelta` / `ToolTaskCancel*`: create/update `TaskCard`.
- `ProviderEvent` with error status: push `SystemNotice(Danger)` **and** auto-open `ErrorDetail` overlay.
- `ContinuityContextSelectionDecided` + `ContinuityContextCompiled`: push a single `ContextNotice` (replaces any prior one for the same `run_session_id`). `contributed_artifact_ids` lists retrieval / RLM / memory artifacts that fed context — visible in the Activity strip + X-ray.
- `ContinuityCompactionCheckpointCreated`: push `CompactionCheckpoint` divider.
- `ContinuityJobSpawned { job_kind, details, … }` / `ContinuityJobEnded { job_kind, status, result, error, … }`: push / update `JobNotice`. The frame carries `job_kind` (kernel-assigned string) and optional `details` Value — the TUI dispatches on `job_kind` to pick a glyph / inline policy but **does not** invent new kinds. Retrieval, reviewer, memory-writer, compaction-planner, indexer, and subagent job work all flow through these two frames with different `job_kind` values; the TUI needs zero new ingestion code when a new `job_kind` ships (it falls back to a generic chip with the kind as label).

  There are **no** dedicated `ContinuityRetrieval*` / `ContinuityReviewer*` / `ContinuityMemory*` frame kinds today and the plan does not add any — those are runtime contract work that lives outside the revamp. If and when the kernel adds a finer-grained taxonomy, the ingestion rule will dispatch on the new variant; until then, everything rides `ContinuityJob*`.

  *Progress ticks mid-job:* the kernel does not currently emit a `ContinuityJobStatus` frame. The TUI shows "running since <timestamp>" and lets the X-ray overlay show the raw Spawned/Ended pair; it does **not** synthesize progress from side-channels.

- *(Deferred — no ingestion rule yet.)* `ExtensionPanel` mount/update/unmount frames will flow through the P2 `extension.ui` capability once it ships. Until then, `CanvasMessage::ExtensionPanel` is declared but never produced by the ingestion layer; rendering handles the variant so no dispatch panics if a future frame starts emitting it, and no filesystem/hack route is used to populate it in the interim.
- `CheckpointCreated` / `CheckpointFailed`: reflected via `SystemNotice` only on failure; success is ambient-only.
- Everything else: consumed by `FrameStore` for X-ray; no canvas message.

### 4.3 **Critical:** `begin_pending_turn` no longer resets ambient state

Current `state.rs:529` calls `reset_session_state()` on every submit — clearing frames, tools, tasks, jobs, context, artifacts, errors. This contradicts "one chat forever." The new behavior:

```rust
pub fn begin_pending_turn(&mut self, input: &str) {
    let prompt = input.trim();
    if prompt.is_empty() { return; }
    self.messages.push(CanvasMessage::UserTurn { /* … */ });
    self.pending_prompt = Some(prompt.to_string());
    self.awaiting_response = true;
    self.set_status_message("sending…");
    // Ambient state (tools, tasks, jobs, context, artifacts, last_error_seq) persists.
    // FrameStore also persists; a new run appends frames, not replaces them.
}
```

Run-specific state (timing for the NEW run) is tracked per-`run_session_id` on messages, not on global `TuiState` fields. Old fields like `start_ms`, `first_output_ms`, `end_ms`, `openresponses_*_ms` become per-`AgentTurn` fields.

### 4.4 StreamCollector + block cache

```rust
pub struct StreamCollector {
    pending_tail: String,          // not-yet-terminated raw chunk
    stable_blocks: Vec<Block>,     // parsed blocks for the stable region
    stable_text_len: usize,        // bytes consumed into stable_blocks
}

impl StreamCollector {
    pub fn push(&mut self, delta: &str) -> CollectorStep;
    pub fn finalize(&mut self);
    pub fn tail_block(&self, theme: &Theme) -> Option<Block>;
    pub fn stable_blocks(&self) -> &[Block];
}
```

`push(delta)` accumulates into `pending_tail`. On detecting a block boundary (double-newline for paragraphs, end-of-fence for code blocks, end of heading line, etc.), transfers complete blocks from `pending_tail` to `stable_blocks` by invoking `pulldown-cmark` over just the new region. Returns a `CollectorStep` that tells the canvas "N new stable blocks appended, tail is now X." Canvas re-renders only the new stable blocks + the current tail.

**Code fences** use `syntect`. A shared `SyntaxSet::load_defaults_newlines()` is loaded at startup (one-time cost ~30ms amortized). Highlight theme derived from current `Theme` (two syntect themes bundled: `graphite.tmTheme`, `ink.tmTheme`, generated from our token palette).

### 4.5 Tool card widget

`struct ToolCardWidget<'a> { card: &'a ToolCard, theme: &'a Theme, focused: bool }` implementing ratatui `Widget`.

**Collapsed (default):**

```
╭─ write · slides.md ──────────────── ✓ 120ms ─╮
 5 artifacts · ⏎ expand · x raw
╰──────────────────────────────────────────────╯
```

**Expanded (⏎ focused):**

```
╭─ write · slides.md ──────────────── ✓ 120ms ─╮
 args
   path: "slides.md"
   content: "# Product Launch\n…"
 stdout
   wrote 312 bytes
 artifacts
   ⧉ 3a4f… · 312 B
╰──────────────────────────────────────────────╯
```

Card color: by status (`accent_success` / `accent_warn` / `accent_danger` / `accent_tool` running). Body indented 2 cols from card corners. No right-side border.

### 4.6 Artifact chips + viewer

Inline: `⧉ 3a4f…` where id is truncated to 4 hex. On canvas item focus, press `A` to cycle through the item's artifacts; Enter opens `ArtifactViewer` overlay (range-fetched content, MIME-detected render: code → syntect; markdown → markdown renderer; binary → hex; unknown → first 2KB of bytes).

### 4.7 Markdown support (intentional subset)

**Supported:** ATX headings (# ## ### → semantic heading tokens), paragraphs, bold / italic / strikethrough, inline code, code fences (with highlighting), unordered + ordered lists, block quotes, thematic break (`---`), inline links (rendered as `text ↗`; full URL shown on X-ray).

**Not supported (intentional):** tables (deferred; width handling is hairy), images (not renderable in TTY), raw HTML (stripped), footnotes (rare in agent output).

## Part 5 — Palette Engine

### 5.1 PaletteMode trait

```rust
pub trait PaletteMode: Send {
    fn id(&self) -> &'static str;                          // "command" / "models" / "go-to" / "threads" / "options"
    fn label(&self) -> &str;
    fn placeholder(&self) -> &str;
    fn entries(&self, ctx: &PaletteCtx) -> Vec<Entry>;
    fn apply(&self, entry: &Entry, ctx: &mut PaletteCtx) -> PaletteAction;
    fn preview(&self, entry: &Entry, ctx: &PaletteCtx) -> Option<Preview> { None }
    fn empty_state(&self) -> &str { "No results" }
    fn allow_custom(&self) -> Option<&str> { None }        // Some("Use typed route") — Models uses this
}

pub enum PaletteAction {
    ClosePalette,
    ClosePaletteAnd(Action),                // Action dispatched by TuiDriver
    SwitchMode(&'static str, String /* initial query */),
    Noop,
}
```

`PaletteCtx` gives read-only access to state + a mutable channel for dispatched actions. Modes never touch `TuiState` directly (keeps them pure for future surfaces: GUI, web, MCP).

### 5.2 Five shipped modes

**Command** — workspace actions. Grouped headings: Canvas / Threads / Models / Compaction / Debug / System. Full inventory (and per-entry backing capability) in Parts 16 and 17. Extensions will contribute additional entries under an `EXTENSIONS` heading once the P2 `extension.commands` capability ships; until then the heading is hidden. The TUI does not discover extensions by reading the filesystem.

**Go To** — fuzzy over canvas items: every UserTurn, AgentTurn, ToolCard, TaskCard, ArtifactChip, CompactionCheckpoint. Applying scrolls the target into view + focuses it.

**Models** — catalog routes + recents + favorites + custom typed routes. Recents pinned at top (up to 5). Current entry chipped `current`. Applies via per-turn override (preserves existing `fullscreen.rs::apply_model_palette_selection` logic, refactored behind the trait).

**Threads** — continuity browser. "New thread" row; current thread pinned; then recent threads with a one-line preview of their last user message + timestamp + model. Apply switches `continuity_id` + reloads frames from the continuity log via `thread.*` capabilities.

**Options** — fast toggles: theme (graphite / ink), auto-follow, reasoning visible (show `reasoning_summary_text_delta` in AgentTurn), streaming view mode (live / after-complete), vim input mode, mouse capture.

**Help** (overlay, not a mode strictly): searchable keybinding + command table; grouped by category. Enter on a row triggers the command. Press `?` or `/help` to open.

### 5.3 Recents / favorites / variants — local state, not config

Opencode's good idea (one we cherry-pick as CONCEPT, not style): recents/favorites are UI memory, not config. Stored at `~/.rip/state/tui.json`:

```json
{
  "recent_models": [
    { "provider": "openai", "model": "gpt-5-nano-2025-08-07", "last_used_ms": 17135... },
    { "provider": "openrouter", "model": "openai/gpt-oss-20b", "last_used_ms": 17134... }
  ],
  "favorite_models": [
    { "provider": "openai", "model": "gpt-5-pro" }
  ],
  "recent_commands": [
    { "id": "threads.new", "last_used_ms": 17135... }
  ],
  "theme": "graphite",
  "auto_follow": true,
  "reasoning_visible": false,
  "vim_input_mode": false
}
```

Caps: recents 8; favorites unlimited. File is created lazily; absence is zero state. Never affects continuity log.

### 5.4 Slash-prefix mode switching

- In input (palette closed): typing `/` at BOF pops palette with `/` already present. Typing `/m` narrows to `models` mode; Enter enters Models mode with empty query. `/m gpt-4` enters Models mode with query `gpt-4`.
- In palette query (palette open): same rule. `⇥` cycles through modes round-robin.

### 5.5 Rendering

```
╭─ Command ──────────────────────────────────────── 12/42 ─╮
│ › search commands                                        │
│                                                          │
│ CANVAS                                                   │
│ ▎ scroll to bottom                            ⌘⇥        │
│   follow tail                                  ⌃f        │
│   copy last message                                      │
│                                                          │
│ THREADS                                                  │
│   new thread                                             │
│   switch thread                                ⌘T        │
│ …                                                        │
╰─ ⇥ mode   ↑↓ select   ⏎ apply   ⎋ close ────────────────╯
```

- Rounded border. Title includes mode + filtered/total count.
- Query row: `›` in accent color; `_` cursor.
- Section headings in `fg_muted` tracked-caps.
- Selected entry: `▎` left rule in accent color + `bg_raised` tint row. No `▸`.
- Right-aligned shortcut chips in `fg_quiet`.
- Footer is a keylight strip scoped to the palette.

At M+, if mode provides `preview()`, an optional right preview pane shows 10 lines of preview context for the selected entry.

## Part 6 — Overlay System

### 6.1 Overlay trait

```rust
pub enum OverlayOutcome {
    Continue,
    Pop,
    PopAll,
    Apply(Action),
}

pub trait Overlay: Send {
    fn id(&self) -> &'static str;
    fn title(&self) -> &str;
    fn on_key(&mut self, k: KeyEvent, ctx: &OverlayCtx) -> OverlayOutcome;
    fn on_mouse(&mut self, m: MouseEvent, ctx: &OverlayCtx) -> OverlayOutcome { OverlayOutcome::Continue }
    fn render(&self, area: Rect, buf: &mut Buffer, theme: &Theme);
    fn focus_cursor(&self) -> Option<(u16, u16)> { None }
    fn keylight(&self) -> Vec<(&'static str, &'static str)>;  // contextual keys
}
```

### 6.2 Shipped overlays

- **Palette** (with five modes).
- **ToolDetail** — summoned by ⏎ on a focused `ToolCard`. Full args JSON, full stdout/stderr, artifact list with `A` shortcut to cycle, timing, provenance. `X` opens `Xray` on the frame range.
- **TaskDetail** — same for `TaskCard`; adds PTY log tail.
- **JobDetail** — mirrors TaskDetail for `JobNotice`: status timeline, contributed artifacts, extension-exposed controls routed back to the owning capability.
- **ErrorDetail** — auto-opens on provider error or on `CheckpointFailed`. In-UI actions (see 6.4).
- **Xray** — summoned by `X` on any canvas item. Scoped to that item's frame range. Panes: timeline list + raw JSON inspector + artifact viewer.
- **Help** — searchable keymap + command reference (Part 9.2).
- **ArtifactViewer** — range-fetch + MIME-aware render. Fetch goes through `tool.output_fetch` (range-capable; P2 planned) once that capability ships. Until then, ArtifactViewer is declared but disabled: clicking an artifact chip shows a "preview available once `tool.output_fetch` ships" stub rather than reading from disk. Never a direct filesystem read.
- **ThreadPicker** — richer view than palette `Threads` mode: thread previews with first/last message, tags, size. Backed by `thread.list` + `thread.get` (both supported). Any field the capabilities don't yet return (e.g. "last message preview", "size") is rendered as "—" rather than synthesized from disk.
- *(Deferred)* **ExtensionOverlay** — host slot for plugin-contributed overlays (memory browser, retrieval inspector, reviewer workbench). Lands when the P2 `extension.ui` capability ships with a render contract. Until then the overlay trait + slot exist in the code but there is no mount path — no ad-hoc plugin wiring, no direct filesystem discovery.
- **Debug** — surfaces current debug tokens (TTFT, seq, hdr/fb/evt, stalled, theme, etc.). Replaces the current status-bar soup; reachable via `Command > Show debug info`.

### 6.3 Dim-behind-overlay

When the overlay stack is non-empty, canvas area repaints with `bg_sunken` fill under the existing characters (effective "dim" — we lower `fg` saturation by routing fg through a theme-aware dim transform). Hero, input, keylight stay at `bg_base` to keep the user's bearings. No blur.

### 6.4 In-UI error recovery

```
▲  Provider error · openresponses
   InvalidJson
   3 parse errors · 1 response error

   The provider returned malformed JSON partway through the response.
   The current run is paused.

   ▎ Retry turn (same model)          r
     Rotate provider cursor            c
     Switch model                      m
     Open X-ray on raw frames          x
     Dismiss                           ⎋
```

Actions map to capabilities already in the registry — **all flow through the capability contract** (local runtime or `--server` transport), not through direct filesystem reads or shell invocations:

- **Retry turn** → `thread.post_message` (re-appends the last user message; the capability contract already states "may trigger a new run and returns linkage", so the kernel spawns the run — the TUI does not call a separate `run.spawn`).
- **Rotate cursor** → `thread.provider_cursor.rotate` (supported).
- **Switch model** → opens Palette(Models), which applies per-turn overrides on the next `thread.post_message` via the existing driver path.
- **X-ray** → `Xray` overlay scoped to the error's `seq` ± a small window; frames come from the in-memory `FrameStore`, not disk.
- **Dismiss** → pop overlay; error chip persists in Activity strip.

If the capability call fails (e.g. `--server` is down), the overlay surfaces the failure text without trying a filesystem fallback.

The current `rip threads events <continuity_id> --max-events 200` shell punt is deleted from `render.rs`; it violated surface-only posture by asking the user to drop out to a subprocess.

## Part 7 — Input Editor

Backed by `ratatui-textarea` (workspace-level approved; version `0.9.x`).

- **Multi-line** with soft wrap. Default submit on `⏎`; `⇧⏎` inserts newline. (Alternate "submit-on-⇧⏎, newline-on-⏎" modeled as an Option.)
- **Height**: 1 row when empty; grows to 6 rows as content expands; internal scroll beyond. Canvas shrinks accordingly; activity strip hides when input >2 rows.
- **Placeholder**: context-aware. Empty canvas: "Ask anything". Mid-thread: "Continue the thread". After long idle: "Pick back up, or /threads for a new one". After error: "Retry, or r for recovery".
- **History**: `↑` at BOF recalls prior submission; `↓` advances. Stored at `~/.rip/state/tui_history.jsonl`, bounded 200 entries.
- **Slash**: typing `/` at BOF pops palette with `/` prefixed. `⎋` closes palette without consuming the slash (so user can type a normal message starting with `/` by pressing `⎋` after the open).
- **Paste**: bracketed paste detection. Multi-line paste auto-expands editor to 4 rows.
- **Emacs bindings**: `⌃a` BOL, `⌃e` EOL, `⌃k` kill-line, `⌃u` kill-bol, `⌃w` kill-word.
- **Vim opt-in** (Options toggle): `⎋` enters command mode; `i / a / o / dd / yy / p / gg / G` supported.
- **Cursor**: `█` block in `accent_agent` when focused.
- **Accent rule**: 1-col `▎` at col 0 in `accent_agent` when editor focused; `fg_quiet` when overlay has focus.

## Part 8 — Screen Composition (responsive)

### 8.1 Breakpoints (from existing contract — preserved)

```rust
enum Shell { Xs, S, M, L }

fn shell(area: Rect) -> Shell {
    match (area.width, area.height) {
        (w, _) if w < 80  => Shell::Xs,
        (w, _) if w < 100 => Shell::S,
        (w, _) if w < 140 => Shell::M,
        _                 => Shell::L,
    }
}
```

### 8.2 XS (60×20) — phone SSH

```
slide-prep · gpt-5                     streaming 120
                                                    
›  You                                              
   Add a slide outline for a product launch.       
                                                    
◉  RIP                                              
   Here's a 5-slide outline. I'll refine after     
   a quick check.                                   
                                                    
   ╭─ write · slides.md ────── ✓ 120ms ─╮         
     5 artifacts · ⏎ expand                         
   ╰────────────────────────────────────╯           
                                                    
   Want me to draft slide 1?                        
                                                    
▎› _                                                
  ? help  ⌘K command                                
```

No outer frame. No activity strip (dot on hero right side instead). Keylight truncated to 2 actions. Palette opens full-screen with 1-col margin.

### 8.3 S (80×24)

Same as M layout but no pinned rail ever, and palette modal is `70% width × 16 rows` centered.

### 8.4 M (100×31 to 139×45) — mockup in 3.1.

Optional palette preview pane appears at ≥120 cols.

### 8.5 L (140×46+)

Thin outer frame in `rule` appears (a 1-row gutter around the viewport). Activity strip can be pinned as a 28-col rail on the right via `Command → Pin Activity rail`. Pinning moves activity information from the bottom strip to a persistent vertical list; unpin returns it.

### 8.6 Layout contract

Render computes `shell` once at top. Each zone is given a `Rect`. Zones are pure (theme in; Rect in; Buffer mutated; no state mutation). Overlays composite on top via the overlay stack.

## Part 9 — Keymap + Help

### 9.1 Top-level keymap (reduced)

| Key | Command |
|---|---|
| `⌘K` / `⌃K` | Palette: Command |
| `⌘M` / `⌃M` | Palette: Models |
| `⌘G` / `⌃G` | Palette: Go To |
| `⌘T` / `⌃T` | Palette: Threads |
| `⌘/` / `⌃/` | Palette: Command (search) |
| `?` | Help overlay |
| `X` (on focused item) | X-ray overlay |
| `A` (on focused card) | Cycle artifacts |
| `⏎` | Submit (input) / Expand (focused card) / Apply (palette) |
| `⇧⏎` | Newline |
| `⎋` | Pop overlay / cancel pending send |
| `⌃C` | Pop all overlays / quit |
| `↑` `↓` | Scroll canvas / navigate list |
| `⌘[` `⌘]` | Prev / next message |
| `PgUp` `PgDn` | Scroll canvas page |
| `⌃F` | Toggle follow-tail |
| `⇧T` | Toggle theme (hot shortcut; also in Options) |

**Deleted from defaults:** `⌃B`, `⌃R`, `⌃Y`, `M-t`, `Tab` (as details-mode toggle). All their functions become palette actions. `Tab` is freed for palette mode cycling.

Existing `~/.rip/keybindings.json` load mechanism (in `keymap.rs`) preserved. The `Command` enum in `keymap.rs` expands to include new palette-opening variants (`PaletteCommand`, `PaletteModels`, `PaletteGoTo`, `PaletteThreads`, `PaletteOptions`, `PaletteHelp`). Old variants (`ToggleActivity`, `ToggleTasks`, `ToggleDetailsMode`, etc.) stay as aliases into palette actions for back-compat of user config.

### 9.2 Help overlay

```
Help                                         search: _

CANVAS
  ↑↓           scroll canvas
  ⌘[ ⌘]       prev / next message
  ⏎           expand focused card
  x            X-ray focused item

COMMANDS
  ⌘K          palette (Command)
  ⌘M          palette (Models)
  ⌘G          palette (Go To)
  ⌘T          palette (Threads)
  /foo         palette with mode prefilled

INPUT
  ⏎           send
  ⇧⏎         newline
  ↑ (empty)    recall previous
  ⌃a / ⌃e     BOL / EOL

THREADS
  new / switch / branch / handoff
  compaction (run / schedule / status)

…
```

Two columns: key + description. Fuzzy search narrows both. Enter on a row triggers the command.

Backed by metadata: each `Command` in `keymap.rs` grows a `description()` and `category()` method. Help is a projection over the keymap + palette mode listings.

## Part 10 — Animation & Motion (policy)

**We do:**

- Breath (`·` in input gutter, 2400 ms cycle, idle only).
- Thinking cycle (`◐◓◑◒` on agent gutter, 400 ms frames, pre-first-token only).
- Streaming pulse (`◎` color modulates with token rate, content-driven).
- New message fade-in (2 frames, `fg_muted → fg_primary`).
- Palette open (3-frame height expansion).

**We don't:**

- Spinners (`| / - \`, `⠋ ⠙ ⠹…` Braille spinners, `◐◓◑◒` outside thinking).
- ASCII/text progress bars.
- ASCII banners on startup.
- Gradient dithering (256-color simulated gradients look cheap in TTY).
- Shimmer or glow effects.
- Typewriter reveals of static text.
- Auto-scroll "bounce" feedback.
- Color cycling for emphasis.

Rule: motion reflects real work or guides attention to a just-changed element. No decoration.

## Part 11 — Docs Cleanup

### Delete (pre-execute Phase A)

- `docs/02_architecture/tui/00_index.md`
- `docs/02_architecture/tui/01_design_principles.md`
- `docs/02_architecture/tui/02_navigation_model.md`
- `docs/02_architecture/tui/03_interaction_patterns.md`
- `docs/02_architecture/tui/04_graceful_degradation.md`
- `docs/02_architecture/tui/05_performance_considerations.md`
- `docs/02_architecture/tui/06_experience_review.md`
- `docs/02_architecture/tui/07_canvas_and_xray.md`
- `docs/02_architecture/tui/journeys/` (directory)
- `docs/02_architecture/tui/screens/` (directory)
- `docs/02_architecture/tui/widgets/` (directory)

All describe the old direction (chat-wrapper + bordered chrome + mode-toggle X-ray). Keeping them alongside the new code guarantees drift. `journeys/` and `screens/` contain UI mockups tied to the deleted layout; `widgets/` describes widget anatomy that is superseded by Parts 4–6 of this plan.

### Create (Phase D final step)

`docs/02_architecture/tui/00_design.md` — a single concise source of truth: identity, interaction model, canvas model, palette engine, overlays, input editor, responsive behavior, animation discipline, glyph vocabulary, theme tokens. Effectively a committed, condensed version of this plan minus the implementation phases.

### Update

- `docs/00_index.md`, `docs/00_doc_map.md` — drop references to deleted files; add pointer to `00_design.md`.
- `agent_state.md` — log the revamp; note that `reset_session_state` on submit is removed (behavioral change).
- `docs/07_tasks/roadmap.md` — move "TUI: UX v1 experience review journeys" to Done (superseded by revamp); add "TUI: design language revamp (Phase A–D)" as Now.

Journey names (`follow_a_run`, `background_tasks`, `recover_error`) are preserved — they describe real user paths, not old UI.

## Part 12 — Phased Implementation

Four phases, each committed directly to `main` as a coherent slice (no PRs). After each phase the repo must still pass `scripts/check` and pre-commit hooks (`scripts/check-fast` runs automatically; see AGENTS.md). Each phase is internally sequenced into numbered steps so a single interruption lands on a safe boundary.

Work the steps in order. When a step lands, commit with a message that names the phase and step (e.g. `tui: A.0 bump to ratatui 0.30`). `agent_state.md` gets updated at the end of each phase.

### Phase A — Foundations (quiet refactor; no user-visible change)

**A.0 — Framework upgrade (lands first, in its own commit).** Bump all TUI-path dependencies to latest before any refactor or visual change:

- `ratatui` 0.29 → **0.30** (breaking: `ratatui::layout::Alignment` → `HorizontalAlignment` — any glob imports break; `Block` API rework; crossterm conversions now via `FromCrossterm`/`IntoCrossterm` traits; `Flex::SpaceAround` → CSS-flex semantics, use `SpaceEvenly` for old behavior; `Layout::init_cache` gated behind `layout-cache` feature; `WidgetRef`/`StatefulWidgetRef` still behind `unstable-widget-ref`).
- `crossterm` 0.28 → **0.29** (companion to ratatui 0.30; re-export compatibility).
- `ratatui-textarea` — add at the latest version that targets ratatui 0.30 (prefer this over `tui-textarea` which is pinned to older ratatui; see `references/official-ecosystem.md`).
- `pulldown-cmark` — latest 0.x (markdown parser; wired in Phase B).
- `syntect` — latest 5.x (code fence highlighting; wired in Phase B).
- `supports-color` — latest 3.x (color-depth detection; wired in A.2).

Verify at crates.io before committing — do not pin versions from memory. After the bump commit, every `cargo check -p rip-tui -p rip-cli` error is fixed by mechanical import / API updates (no behavioral changes yet). All existing snapshot tests must still match after A.0. This phase is intentionally boring; it unblocks the revamp.

**A.1** Split `crates/rip-tui/src/render.rs` (1500+ lines) into a `render/` directory:
- `mod.rs` (entry + shell detection)
- `theme.rs` (tokens + Graphite / Ink constructors + NO_COLOR + ANSI16 fallbacks)
- `hero.rs`
- `canvas.rs` (still consumes `output_text` during A)
- `activity.rs`
- `input.rs`
- `keylight.rs`
- `overlays/{mod, palette, tool_detail, task_detail, error, xray, help, artifact_viewer, thread_picker, debug}.rs`
- `widgets/{tool_card, task_card, artifact_chip, gutter}.rs`
- `glyphs.rs` (with `UnicodeLevel::{Full, Ascii}` detection from `LANG` / `LC_CTYPE`)

**A.2** Semantic `Theme` struct; replace all hardcoded `Style::default().fg(...)` and `Color::Rgb(…)` with `theme.foo` or `theme.tint(foo, level)` for color-depth degradation.

**A.3** `Overlay` trait + `Vec<Box<dyn Overlay>>` stack in `TuiState`; port the 7 existing `Overlay::*` variants to trait impls one-by-one. Keep the enum as a thin shim during migration (drop at end of A).

**A.4** `PaletteMode` trait + `PaletteCtx`. Migrate existing `ModelPaletteCatalog` logic in `crates/rip-cli/src/fullscreen.rs` to `ModelsMode` in `crates/rip-tui/src/palette/modes/models.rs`. Palette engine owns a `Box<dyn PaletteMode>` (not an enum).

**A.5** Delete existing TUI design docs (Part 11). No replacement doc yet — waits for Phase D once the shape stabilizes.

**Acceptance.**
- All existing snapshot tests still match (chrome identical visually during A, including after A.0's ratatui 0.30 bump).
- `cargo test -p rip-tui -p rip-cli` green.
- Dependencies at latest: `ratatui 0.30`, `crossterm 0.29`, `ratatui-textarea` (latest compatible with 0.30, wired in C), `pulldown-cmark` (latest, wired in B), `syntect 5` (wired in B), `supports-color` (latest, used in A.2).
- No user-visible regression.

**Commit cadence.** A.0 is its own commit (mechanical 0.30 port). A.1–A.5 land as separate commits, each green.

### Phase B — Canvas Overhaul (the unlock)

**B.1** Add `CanvasMessage` enum + `messages: Vec<CanvasMessage>` + `next_message_id` to `TuiState`. Write `ingest(&mut self, event: &Event)` that derives messages per Part 4.2. Keep `output_text` + `prompt_ranges` temporarily (populated alongside).

**B.2** Rewrite `render::canvas` to walk `messages`. Delete `prompt_ranges`, the `"You: "` literal, and eventually `output_text`. Gutter column rendering with per-message glyph + accent color.

**B.3** **Delete the `reset_session_state` call from `begin_pending_turn`.** Ambient state persists across turns. Per-run timing moves to `AgentTurn.started_at_ms` / `ended_at_ms` fields. Snapshot test `journey_multi_turn_continuity` asserts a task spawned on turn 1 is still visible as an ambient chip on turn 3.

**B.4** `ToolCardWidget` + `TaskCardWidget`. Inline tool cards. Collapsed by default; `⏎` on a focused card expands; `X` opens `XrayOverlay` scoped to the card's frame range.

**B.5** `StreamCollector` with block-level cache (Part 4.4). Feed `OutputTextDelta` through it; stable blocks append to tail `AgentTurn.blocks`; tail rendered transiently.

**B.6** `pulldown-cmark` parser for agent blocks: paragraphs, headings, lists, block quotes, fences, emphasis, inline code, thematic break.

**B.7** `syntect` code-fence highlighting. Shared `SyntaxSet` loaded once. Two bundled themes (graphite.tmTheme, ink.tmTheme, derived from token palette).

**B.8** `CachedText` block invalidation on theme change (blow cache; re-parse stable blocks once).

**B.9** Regenerate snapshots. New journeys:
- `journey_follow_a_run_v2` × xs/s/m × graphite/ink/nocolor
- `journey_background_tasks_v2` × xs/s/m × graphite/ink/nocolor
- `journey_recover_error_v2` × xs/s/m × graphite/ink/nocolor
- `journey_multi_turn_continuity` × xs/s/m (new)
- `journey_markdown_rendering` × s/m (new)
- `journey_tool_card_expand` × s/m (new)

Old snapshots move to `crates/rip-tui/tests/snapshots/archive/` for reference (optional, can be deleted).

**Acceptance.**
- Structured messages with inline tool cards.
- Ambient state persists across turns (asserted).
- Markdown + code highlighting visible in M snapshots.
- New snapshots gated in CI via `scripts/check-fast`.
- `cargo test -p rip-tui -p rip-cli` green.

**Commit cadence.** B.1–B.9 as separate commits; snapshots land with the step that produced them.

### Phase C — Chrome, Palette, Input, Error Recovery

**C.1** Replace status bar with borderless `Hero` strip (Part 2.3). Status-bar debug tokens migrate to `Debug` overlay reachable via `Command → Show debug info`.

**C.2** Activity strip (Part 3.2): 1 row, borderless, auto-hide, colored by worst current state.

**C.3** Keylight (Part 3.6): state-driven shortcut row above input.

**C.4** Multi-line editor via `ratatui-textarea`: history, placeholder, paste handling, slash-prefix trigger, emacs bindings.

**C.5** Ship four remaining palette modes: `Command`, `Go To`, `Threads`, `Options`. Wire slash-prefix mode switching + `Tab` mode cycling + `⌘K/M/G/T` aliases.

**C.6** Spatial palette positioning (Part 3.7).

**C.7** `Help` overlay backed by `Command::description()` / `category()` metadata.

**C.8** Remove `OutputViewMode::Raw` as a global mode. `Ctrl-R` becomes `X-ray focused canvas item` (opens `XrayOverlay`).

**C.9** Breath / thinking / streaming pulse / new-message fade (Part 10).

**C.10** In-UI error recovery (Part 6.4). Remove `rip threads events <id>` shell breadcrumb.

**Acceptance.**
- No bordered status bar, no bordered canvas, no bordered input. Only card corners + (at L) a discreet outer frame.
- Palette reachable via `⌘K` and all four aliases; all five modes functional; Command exposes ≥25 actions.
- `?` Help overlay populated and searchable.
- Provider errors recoverable in-UI (Retry / Rotate / Switch / X-ray / Dismiss).
- `cargo test -p rip-tui -p rip-cli` green; new snapshots accepted.

**Commit cadence.** C.1–C.10 as separate commits.

### Phase D — Polish

**D.1** Ink theme finalization: verified against real terminals (iTerm2, Alacritty, Kitty, GNOME Terminal). 16-color + NO_COLOR fallback visual QA.

**D.2** `ArtifactViewer` overlay with range-fetch + MIME detection.

**D.3** `ThreadPicker` overlay (richer than palette Threads mode): thread previews, size chips, actor chips, age chips.

**D.4** Subagent color palette (up to 4 simultaneous agents distinguishable by gutter color).

**D.5** Vim-keymap opt-in in editor.

**D.6** Canvas virtualization (render only visible messages) if 10k+ message benchmark regresses.

**D.7** Mouse polish: click-to-focus on canvas items, scroll-over on activity strip opens Activity overlay, click on hero segments opens their palette modes, drag-select for copy.

**D.8** Create `docs/02_architecture/tui/00_design.md`.

**D.9** Update `docs/07_tasks/roadmap.md` + `agent_state.md`.

**D.10** Refresh the `ratatui-builder` skill at `~/.claude/skills/ratatui-builder/` to reflect the shipped end-state — the skill is personal tooling, not repo docs, but it goes stale relative to RIP if we don't update it:

- `references/rip-seams.md` — rewrite from "current seams" to "end-state seams". Update the seam list to include the new layout: `crates/rip-tui/src/{state,lib}.rs`, `crates/rip-tui/src/render/{mod,hero,canvas,activity,input,keylight}.rs`, `crates/rip-tui/src/render/overlays/`, `crates/rip-tui/src/render/widgets/`, `crates/rip-tui/src/canvas/{mod,stream_collector,markdown}.rs`, `crates/rip-tui/src/palette/{mod,modes/*}.rs`, `crates/rip-tui/src/overlays/`, `crates/rip-tui/src/theme.rs`, `crates/rip-tui/src/glyphs.rs`. Add breakpoint `L` (≥140 cols) to the list. Update the "Places worth a fresh look" section — palette foundation and help overlay are now shipped, not candidates. Keep the verification grep commands since paths will continue to drift.
- `scripts/palette.rs` — update the `PaletteMode` trait signature in the comment block to match RIP's richer version (`PaletteCtx` param, `preview()`, `empty_state()`, `allow_custom()`), with a note that the script remains a generic starter and RIP-specific details live in `rip-seams.md`.
- `SKILL.md` — change the RIP-specific Ratatui note to "Ratatui 0.30" (post-A.0). Under "Default recommendations", cross-reference `rip-seams.md` for the RIP-specific palette / canvas / overlay signatures once they are authoritative.
- `references/repo-patterns.md` — add a note under "OpenCode" that for RIP we take the **concept** of recents/favorites/variants as local state (not config), not the style of the opencode UI itself.

This keeps the skill useful for the next TUI task on this machine without the skill re-teaching us patterns we've already committed to.

**Commit cadence.** D.1–D.10 as separate commits. D.8 (design doc) and D.10 (skill refresh) are the closing commits of the revamp.

**Total: 35 sequenced commits across four phases** (A×6, B×9, C×10, D×10). No PRs; direct-to-`main` per commit with `scripts/check-fast` green and hooks running. Expect the whole revamp to land in a handful of hours of focused work.

## Part 13 — Constraints Preserved (do not change)

- **Frame-driven state.** Canvas is a function of the event log. All UI state derives from frames or UI-local prefs (theme, overlay stack, scroll, follow).
- **Surface-only posture.** The TUI calls capabilities (local runtime) or transport (`--server`) for every action. No direct filesystem reads of continuity state. No direct writes to the event log. No TUI-owned business rules. UI-local prefs (theme, recents, input history) are stored under `~/.rip/state/` and are documented as **non-continuity** — losing that directory must leave the continuity log intact.
- **Strategy lives in the kernel / extension host.** Memory, retrieval, RLM, reviewer planning, subagent orchestration, compaction planning — all capability-owned. The TUI *renders* their jobs, artifacts, decisions, and controls; it never computes them. A new cognition module ships as a capability + frame schema; the TUI should pick it up via `JobNotice` / `ExtensionPanel` without structural changes.
- **Multi-actor canvas.** `UserTurn.actor_id` + `UserTurn.origin` and `AgentTurn.role` + `AgentTurn.actor_id` are required fields, not optional ornament. Rendering branches on `role`; nothing assumes a single agent or a single human.
- **Per-turn OpenResponses overrides.** New Models mode preserves this; never mutates global config.
- **`rip-tui` (state + render + palette modes + overlay trait + theme) vs `rip-cli` (driver + keymap + transport + SSE) seam.** Palette modes must not import from `rip-cli`. Both crates depend only on the kernel's capability contract + frame schema, never on transport internals.
- **Snapshot-test discipline at xs/s/m.** Every canvas, overlay, palette state snapshot-gated in CI.
- **Canvas vs X-ray.** Canvas default; X-ray is a drill-down overlay scoped to a canvas item.
- **No hidden mutable state.** UI actions correspond to frames or declared UI-local prefs.
- **Append-only transcript.** Messages do not mutate in place.
- **Breakpoints 60×20 / 80×24 / 120×40.** Phone-SSH-first. Nothing drops below XS.
- **Ratatui 0.30 pin** + latest ecosystem. Workspace is currently on 0.29; Phase A.0 upgrades all TUI-path dependencies to their latest compatible versions before any revamp work begins (see Part 12).
- **Event log capacity, frame_store retention** (current defaults preserved).
- **`~/.rip/keybindings.json` load mechanism** preserved; `Command` enum extended, not replaced.
- **Shutdown safety**: raw mode / alt-screen / RAII `Drop` + panic hook.
- **Mouse capture + bracketed paste + Kitty flags.** Unchanged.
- **`rip run`, `rip serve`, SDK, local authority**. Untouched.

## Part 14 — Testing & Verification

### 14.1 Snapshot journeys

Each journey × breakpoint × theme generates one golden file. Journeys:

- `follow_a_run` — user submits → stream → tool call → done.
- `background_tasks` — spawn task → running → task detail.
- `recover_error` — provider error → recovery overlay → retry.
- `multi_turn_continuity` — prove B.3 (ambient state across turns).
- `markdown_rendering` — heading, bold, list, code fence.
- `tool_card_expand` — collapsed → expanded.
- `palette_command` — Command mode open.
- `palette_models` — Models mode with current + recents.
- `palette_go_to` — Go To over mixed canvas.
- `xray_overlay` — X-ray on a tool card.

Breakpoints: xs / s / m. Themes: graphite / ink / nocolor. **Total: 10 × 3 × 3 = 90 snapshots** (~1 KB each ≈ 90 KB text).

### 14.2 Unit tests

- `TuiState::update` fed synthetic frame sequences (journey fixtures).
- `StreamCollector::push` block-boundary detection under varied chunk sizes.
- Palette mode filtering, selection, apply, custom-candidate routes across all five modes.
- Overlay stack push / pop / outcome propagation + dim-behind tint.
- Theme switching produces same layout, different styles (layout invariance).
- Glyph ASCII fallback under `UnicodeLevel::Ascii`.

### 14.3 Interaction tests (TestBackend + synthetic KeyEvents)

`crates/rip-cli/tests/tui_interactions.rs`:
- `/models` typed + Enter cycles catalog.
- `⌘K` + `go ` filters Go To.
- `X` on tool card opens XrayOverlay.
- `Esc` pops one; `⌃C` pops all.
- `⏎` on focused ToolCard toggles expand.
- After provider error: `r` triggers retry (mock `thread.post_message`; the kernel spawns the resulting run per the capability contract).

### 14.4 Performance

`crates/rip-tui/benches/canvas.rs`:
- Render 10,000 canvas messages with mixed blocks → <16ms per frame budget.
- `StreamCollector::push(1 KB delta)` → <50µs.
- Theme hot-swap → no flicker.

### 14.5 Manual QA

1. `cargo run -p rip-cli` against `fixtures/repo_small`.
2. At 60×20 (resize): verify XS collapse.
3. At 80×24: verify S activity strip.
4. At 140×50: verify L optional pinned rail via `Command → Pin Activity rail`.
5. `NO_COLOR=1 cargo run -p rip-cli`: verify monochrome + ASCII glyph fallback.
6. With `TERM=xterm-16color`: verify ANSI-16 degradation.
7. Live provider run (OpenAI + OpenRouter): observe streaming pulse, tool card inlining, markdown rendering.
8. Provider error injection (malformed JSON fixture): verify ErrorDetail overlay and `r` recovery.
9. Multi-turn: spawn a task on turn 1; send turn 2; verify task chip still in Activity.
10. Theme swap: `⇧T` or Options → Ink. Verify no flicker, all colors coherent.

### 14.6 CI gates

- `scripts/check-fast` adds snapshot assertion for all 90 new goldens.
- `scripts/check` runs full suite + benches.
- TTFT + end-to-end bench budgets unchanged.

## Part 15 — Risks & Mitigations

- **Ratatui 0.30 breaking changes.** Glob imports of `ratatui::layout::Alignment`, direct `From<crossterm::event::KeyEvent>` usage, `Flex::SpaceAround` old behavior, and ungated `Layout::init_cache` all break. Mitigate by doing A.0 as a single mechanical-fix commit with snapshots asserting pixel-identical output before/after; if a 0.30 feature (`WidgetRef`, layout cache) is wanted later, that's a separate opt-in step.
- **Snapshot churn.** 90 new goldens. Mitigate with RIP's bespoke regeneration flow (`RIPTUI_UPDATE_SNAPSHOTS=1 cargo test -p rip-tui` rewrites all goldens in-place; see `crates/rip-tui/tests/golden.rs:339`) and careful per-phase commits (snapshots land with their code).
- **`syntect` load time.** ~30ms at startup. Mitigate with lazy-init on first code fence (acceptable; most sessions see code soon anyway).
- **`pulldown-cmark` correctness for partial input.** Mitigate via `StreamCollector`'s boundary detection — parser only sees complete blocks.
- **Overlay stack focus leaks.** Mitigate with `OverlayCtx` that forbids reaching past the top; all input routes through `top.on_key`.
- **Theme hex values on ANSI-16 terminals.** Mitigate with explicit 16-color mapping table per theme (not lossy quantization).
- **Doc deletion breaks external references.** Mitigate by updating `docs/00_index.md` + `docs/00_doc_map.md` + `agent_state.md` in the same commit as the deletions (Phase A.5).

## Part 16 — Appendix: Palette Command inventory (Phase C target ≥25)

Each entry lists its backing (capability id or "UI-local"). Entries marked *[deferred]* ship with the palette entry present but **disabled** — selecting one shows a single-line toast explaining the missing capability, never a local hack. Entries marked *[P2]* light up when their capability reaches `supported` in the registry.

**Canvas** — UI-local, no capability calls
1. Scroll to top
2. Scroll to bottom
3. Toggle follow-tail
4. Jump to previous message
5. Jump to next message
6. Jump to previous tool call
7. Jump to next error
8. Copy last message (→ `ui.clipboard`, supported)
9. Copy selection (→ `ui.clipboard`, supported)
10. Clear selection

**Threads**
11. *[deferred]* New thread — no `thread.create` / `thread.new` capability in the registry today. `thread.ensure` is idempotent get-or-create of the default continuity, not a "fresh thread" primitive. Entry disabled until docs-first capability work lands it.
12. Switch thread → Palette(Threads) (`thread.list`, `thread.get`, supported)
13. Branch current thread (`thread.branch`, supported)
14. Handoff to new thread (`thread.handoff`, supported)
15. *[deferred]* Rename current thread — no `thread.rename` capability. Entry disabled.
16. Thread compaction: run now (`compaction.manual`, supported)
17. Thread compaction: schedule (`compaction.auto.schedule`, supported)
18. Thread compaction: status (`compaction.status`, supported)

**Runs / Models**
19. Retry last turn (`thread.post_message`, supported — re-posts the previous user message; kernel spawns a new run per the capability contract)
20. Stop streaming (`session.cancel`, supported)
21. Switch model → Palette(Models) (per-turn override, applied via `thread.post_message` on the next submit — same path the current driver uses)
22. Rotate provider cursor (`thread.provider_cursor.rotate`, supported)
23. Provider cursor status (`thread.provider_cursor.status`, supported)
24. Context selection status (`thread.context_selection.status`, supported)
25. Run config doctor (`config.doctor`, supported)

**Options** — UI-local prefs under `~/.rip/state/tui.json` (explicitly non-continuity; losing the file leaves the event log intact)
26. Toggle theme (graphite / ink)
27. Toggle auto-follow
28. Toggle reasoning visibility
29. Toggle vim input mode
30. Toggle mouse capture
31. Pin activity rail (L only)

**Debug** — UI-local + in-memory FrameStore
32. Open X-ray on current canvas item (FrameStore, in-memory)
33. Show debug info (overlay with TTFT, seq, stalled, timings; derived from the frame stream)
34. Show frame store stats (in-memory)
35. Copy last error breadcrumb (`ui.clipboard`)

**System** — UI-local + supported capabilities
36. Reload keybindings (UI-local; re-reads `~/.rip/keybindings.json`)
37. Reload theme (UI-local; re-reads `~/.rip/themes/…` via existing loader)
38. Quit

Count: **38 palette entries** at Phase C acceptance, of which **36 are active** on day one and **2 are visible-but-disabled** pending capability work (11, 15). Each entry has a title, subtitle (one-line description), shortcut chip (if bound), and is searchable. Disabled entries are dim with a `unavailable` chip in `fg_quiet`.

## Part 17 — Capability Backing Matrix

This is the definitive list of every TUI → runtime call the revamp introduces, paired with the capability it calls and its current registry status. It exists to make the "surface only" constraint auditable: if an action is missing from this table, it is not allowed in the TUI.

The TUI must never call a capability that is not `supported` in `docs/03_contracts/capability_registry.md`. If a palette entry or overlay action wants a capability that is `planned` (or absent), it ships as *[deferred]* (see Part 16 rules) — never as a local hack, a filesystem read of continuity truth, a direct event-log write, or a shell subprocess.

| TUI action | Capability | Registry status | Notes |
| --- | --- | --- | --- |
| Submit / Retry turn | `thread.post_message` | supported | Retry re-posts the last user message; the kernel triggers a new run per the capability contract. |
| Ensure a default continuity exists | `thread.ensure` | supported | Called once at startup; idempotent. |
| List threads (palette `Threads`, `ThreadPicker`) | `thread.list` | supported | |
| Get thread metadata | `thread.get` | supported | Any field the capability doesn't expose (size, last-message-preview, tags) renders as "—" in the picker. |
| Branch current thread | `thread.branch` | supported | |
| Handoff current thread | `thread.handoff` | supported | |
| Stop / cancel streaming | `session.cancel` | supported | |
| Stream session events | `session.stream_events` | supported | Already used by the current driver. |
| Rotate provider cursor | `thread.provider_cursor.rotate` | supported | |
| Provider cursor status | `thread.provider_cursor.status` | supported | |
| Context selection status / ContextNotice enrichment | `thread.context_selection.status` | supported | |
| Compaction run now | `compaction.manual` | supported | |
| Compaction schedule | `compaction.auto.schedule` | supported | |
| Compaction status | `compaction.status` | supported | |
| Tool task spawn / status / stream / cancel / stdin / resize / signal | `tool.task_*` | supported | Already exercised by existing task overlays; kept in the revamp's TaskCard/TaskDetail path. |
| Checkpoint create / rewind | `checkpoint.auto` / `checkpoint.rewind` | supported | Only failures render as `SystemNotice`; successes are ambient-only. |
| Config doctor | `config.doctor` | supported | Debug overlay + palette entry. |
| Clipboard read/write | `ui.clipboard` | supported (tui) | |
| Theme switching | `ui.theme` | supported (tui) | |
| Keybindings load/reload | `ui.keybindings` | supported (tui) | |
| Raw/rendered view toggle (legacy) → X-ray | `ui.raw_events` | supported (tui) | Revamp replaces "raw mode" with per-item X-ray overlay; the capability stays the same. |
| Palette infrastructure | `ui.palette` | planned (tui) | The revamp is what takes this from planned → supported; landing Phase C flips the registry entry. |
| Multi-line input, paste, queueing | `ui.multiline`, `ui.editor` | planned (tui) | Flipped to supported when Phase C C.4 lands. |
| Artifact fetch for ArtifactViewer | `tool.output_fetch` | planned (P2) | ArtifactViewer ships visible-but-disabled; enabled when this capability flips to supported. |
| Artifact store for full tool outputs | `tool.output_store` | planned (P2) | Same gate as above. |
| Reviewer panel | `ui.review` | planned (P2) | Not shipped by the revamp; `AgentRole::Reviewer` is prepared in the data model so it lights up trivially later. |
| Thread tree / map UI | `ui.thread_tree`, `ui.thread_map` | planned (P2) | Out of scope for the revamp; ThreadPicker is flat for now. |
| Background tasks UI | `ui.background_tasks` | planned | Activity strip covers most of this; Phase D may expand. |
| New blank thread | *(no capability)* | missing | Palette entry 11 is *[deferred]*. |
| Rename thread | *(no capability)* | missing | Palette entry 15 is *[deferred]*. |
| Extension-contributed palette entries | `extension.commands` | planned (P2) | `EXTENSIONS` palette heading hidden until capability flips to supported. |
| Extension-contributed panels / overlays | `extension.ui` | planned (P2) | `CanvasMessage::ExtensionPanel` variant and `ExtensionOverlay` slot are declared but not wired. |
| Extension tool renderers | `extension.tool_renderers` | planned (P2) | Orthogonal to this revamp. |
| Subagent spawn / invoke | `subagent.spawn`, `subagent.invoke` | planned (P2) | `AgentRole::Subagent` is prepared; ingestion dispatches on `SessionStarted.parent_run_id` (or equivalent kernel-supplied field) when the capability ships. |
| Retrieval / memory / RLM UI | `search.retrieval`, `memory.store` | planned (P3) | Render via `CanvasMessage::JobNotice` with the kernel-assigned `job_kind` when `ContinuityJobSpawned { job_kind: "retrieval" \| "memory" \| … }` frames start flowing; no retrieval logic in the TUI. |

**Ground rules for additions to this matrix:**

- Any new TUI action added after the revamp lands must name a capability id from the registry (not an invented one) and its registry status. A commit that adds an action without a row here should be rejected by review.
- If the wanted capability is missing, the right answer is to write an ADR / capability-contract update first, bump the registry, and only then wire the TUI.
- Flipping a palette entry from *[deferred]* to active is a one-line change here plus the enable-guard in the palette — no "maybe the capability is there if I probe it" heuristics in the TUI.

## Verification steps (end-to-end, post-implementation)

1. **Build + tests.** `scripts/check` green locally. `scripts/check-fast` under 60s.
2. **Interactive smoke.** `cargo run -p rip-cli` at default terminal size — verify hero, canvas, activity, input, keylight layout matches the mockup in Part 3.1. No borders except card corners.
3. **Palette walk.** `⌘K` → type "scroll" → Enter; `/models` → arrows → Enter; `/go ` → arrows → Enter. Each exits palette and performs its action.
4. **Multi-turn continuity.** Submit 3 messages in a row; verify after turn 3 that an ambient task from turn 1 is still reported on the Activity strip.
5. **Markdown + code.** Force a response with a code fence; verify syntect highlighting in `graphite`; theme-swap to `ink`; verify highlighting re-paints.
6. **Provider error.** Inject `InvalidJson` via test fixture; verify `ErrorDetail` overlay auto-opens; press `r` → retry; verify new run starts; press `c` → cursor rotates via capability; press `x` → X-ray opens on the error's seq.
7. **X-ray on tool card.** Scroll to a tool card; press `X`; verify X-ray overlay shows the card's frame range.
8. **Responsive.** Resize to 60×20: verify XS layout. Resize to 140×50: verify L and pinning.
9. **Snapshots.** First run on a clean machine: `RIPTUI_UPDATE_SNAPSHOTS=1 cargo test -p rip-tui` rewrites goldens in place (bespoke flow — not insta). Re-runs without the env var must match.
10. **Docs.** `docs/02_architecture/tui/` contains exactly `00_design.md`. Old files gone. `docs/00_doc_map.md` + `docs/00_index.md` updated.

## Critical files to modify

- `crates/rip-tui/src/state.rs` — replace `output_text/prompt_ranges` with `messages`; add `ingest()`; remove `reset_session_state` from `begin_pending_turn`; add overlay stack + palette engine references.
- `crates/rip-tui/src/render.rs` — delete; split into `render/` directory per Part 12 A.1.
- `crates/rip-tui/src/theme.rs` — new; semantic tokens + Graphite / Ink + depth degradation.
- `crates/rip-tui/src/canvas/mod.rs` — new; `CanvasMessage`, `Block`, `CachedText`.
- `crates/rip-tui/src/canvas/stream_collector.rs` — new.
- `crates/rip-tui/src/canvas/markdown.rs` — new; pulldown-cmark + syntect wiring.
- `crates/rip-tui/src/palette/mod.rs`, `palette/modes/{command,models,go_to,threads,options}.rs` — new.
- `crates/rip-tui/src/overlays/{mod,palette,tool_detail,task_detail,error,xray,help,artifact_viewer,thread_picker,debug}.rs` — new.
- `crates/rip-tui/src/widgets/{tool_card,task_card,artifact_chip,gutter}.rs` — new.
- `crates/rip-tui/src/glyphs.rs` — new.
- `crates/rip-tui/src/lib.rs` — re-exports updated.
- `crates/rip-cli/src/fullscreen.rs` — `ModelPaletteCatalog` logic moves into `rip-tui::palette::modes::models`; driver shrinks to event loop + keymap routing + transport.
- `crates/rip-cli/src/fullscreen/keymap.rs` — extend `Command` enum with palette openers + `description()` / `category()` methods for Help.
- `crates/rip-tui/Cargo.toml` — add `pulldown-cmark`, `syntect`, `supports-color`.
- `crates/rip-cli/Cargo.toml` — add `ratatui-textarea`.
- `docs/02_architecture/tui/{00_index,01_design_principles,02_navigation_model,03_interaction_patterns,04_graceful_degradation,05_performance_considerations,06_experience_review,07_canvas_and_xray}.md` — delete.
- `docs/02_architecture/tui/{journeys,screens,widgets}/` — delete (directories).
- `docs/02_architecture/tui/00_design.md` — create (Phase D.8).
- `docs/00_index.md`, `docs/00_doc_map.md`, `agent_state.md`, `docs/07_tasks/roadmap.md` — update references.
- `~/.claude/skills/ratatui-builder/references/rip-seams.md`, `~/.claude/skills/ratatui-builder/references/repo-patterns.md`, `~/.claude/skills/ratatui-builder/SKILL.md`, `~/.claude/skills/ratatui-builder/scripts/palette.rs` — refresh (Phase D.10).

---

Plan end. On approval, execute starting at Phase A.0 (framework bump) and commit straight through to D.10.
