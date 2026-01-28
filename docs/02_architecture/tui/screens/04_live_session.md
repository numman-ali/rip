# Live Session (Canvas + X-ray)

## Purpose

The primary workspace. Where users observe agent activity, read the narrative output, and send messages.

Default posture: **Canvas** (beautiful, semantic, calm).

Peek inside: **X-ray** (full fidelity internals: frames, artifacts, provenance).

Status: **Design** | Phase: 2 | Last updated: 2026-01-27

---

## Entry Conditions

- New thread created
- Existing thread resumed
- Attached to running session

---

## Capabilities Used

| Capability | Usage |
|------------|-------|
| `session.stream_events` | Real-time frame display (Canvas + X-ray) |
| `session.send_input` | Message submission |
| `session.cancel` | Cancel current operation |
| `tool.*` | Tool execution rendering + drill-down |
| `tool.task_*` | Background task awareness + drill-down |
| `checkpoint.*` | Checkpoint indicators + rewind actions |
| `ui.raw_events` | Rendered (Canvas) ↔ raw (X-ray) |
| `ui.clipboard` | Copy selected content |

---

## Default Layout: Canvas

The Canvas is a single narrative stream. It can include small inline “chips” that summarize background activity.

### XS (60×20 → 79×23)

```
┌─ RIP ───────────────────────────┐
│ ● streaming   tools:1  tasks:1  │
├─────────────────────────────────┤
│ Canvas (rendered)               │
│                                 │
│ I'll take care of that. First…  │
│                                 │
│ chips: [⟳ bash] [tasks:1] [⚙ ctx]│
├─────────────────────────────────┤
│ > Add a slide outline for…      │
└─────────────────────────────────┘
```

### S (80×24 → 99×30)

```
┌─ RIP ────────────────────────────────────────────────┐
│ ● streaming   tools:1  tasks:1      [?] help          │
├───────────────────────────────────────────────────────┤
│ Canvas (rendered)                                     │
│ …a readable story of what RIP is doing…               │
│                                                       │
│ chips: [⟳ bash] [tasks:1] [jobs:2] [📄4] [⚠ error?]   │
├───────────────────────────────────────────────────────┤
│ > Continue. Also make it shorter.                     │
└───────────────────────────────────────────────────────┘
```

### M (100×31+)

Canvas remains primary. An Activity rail can be pinned without turning the whole UI into a cockpit.

```
┌─ RIP ──────────────────────────────────────────────────────────────────────────────┐
│ ● streaming   tools:1  tasks:1  jobs:2    [Ctrl+K] palette   [?] help               │
├───────────────────────────────────────────────────────────┬─────────────────────────┤
│ Canvas (rendered)                                         │ Activity (pinned)       │
│ …a readable story of what RIP is doing…                   │ ⟳ bash                  │
│                                                           │ ⟳ task: build           │
│                                                           │ ⚙ context compiling     │
│                                                           │ 📄 patch.diff           │
│                                                           │ ⚠ provider error        │
├───────────────────────────────────────────────────────────┴─────────────────────────┤
│ > Ok, now turn it into a 5-slide outline.                                           │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Activity Drawer (Overlay)

The Activity drawer is the “what’s going on?” index: tools, tasks, background jobs/subagents, context, artifacts, errors.

```
┌─ Activity ───────────────────────────────────────────────┐
│ tools (1)   tasks (1)   jobs (2)   context   artifacts (4)│
├───────────────────────────────────────────────────────────┤
│ ▸ ⟳ tool  bash          started 00:12                      │
│   ⟳ task  build         running 02:14                      │
│   ⚙ ctx   compiled      bundle $art_…                      │
│   ✓ job   summarizer    done                               │
│   ⚠ err   provider      invalid json                       │
│                                                           │
│ [Enter] details   [e] filter errors   [Esc] close          │
└───────────────────────────────────────────────────────────┘
```

Selecting an item opens a detail overlay.

---

## X-ray (Peek Inside)

X-ray is the full internals view: timeline (frames) + inspector (decoded/raw JSON). It is always available, but not the default.

```
┌─ X-ray ───────────────────────────────────────────────────────────────────────────┐
│ Timeline (frames)                          │ Inspector (decoded/raw)              │
├────────────────────────────────────────────┼──────────────────────────────────────┤
│  122 provider_event  response.created      │ type: tool_started                    │
│  123 output_delta    "I'll help…"          │ tool: bash                             │
│ ▸124 tool_started    bash: "npm test"      │ args: {"command":"…"}               │
│  125 tool_stdout     "PASS …"             │ …                                      │
│                                            │                                      │
│ [p] provider  [t] tool  [e] errors  [0] clear  [/] search  [Esc] back             │
└───────────────────────────────────────────────────────────────────────────────────┘
```

---

## Notes

- Canonical for v0 snapshots: **Canvas-first behavior** (chips + drill-down).
- Anything that causes side-effects must map to a cross-surface capability (CLI/server/SDK), not TUI-only logic.
