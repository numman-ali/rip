# Thread Map

Status: **Sketch** | Phase: 2

This screen doc is conceptual. Canonical UX gates are the journey specs in `docs/02_architecture/tui/journeys/` plus [Canvas + X-ray](../07_canvas_and_xray.md).

## Purpose

Visual graph showing thread relationships: branches, merges, and the conversation tree structure.

## Entry Conditions

- User selects "Map" from Thread Browser
- User invokes `/map` command from Live Session
- User presses `m` on a thread

## Capabilities Used

| Capability | Usage |
|------------|-------|
| `thread.map` | Graph data structure |
| `thread.branch` | Branch from node |
| `session.resume` | Jump to node |
| `ui.thread_map` | Visual rendering |

---

## Wireframe

```
┌─────────────────────────────────────── RIP ────────────────────────────────────────┐
│ Thread Map                                                        [Esc] back       │
├────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                    │
│                                                                                    │
│     ●━━━━━●━━━━━●━━━━━●━━━━━●━━━━━●━━━━━●━━━━━●  main                             │
│           │           │     │           │                                          │
│           │           │     │           └━━━━━●━━━━━●  hotfix/urgent              │
│           │           │     │                                                      │
│           │           │     └━━━━━●━━━━━●━━━━━●  fix/login (merged ✓)             │
│           │           │                                                            │
│           │           └━━━━━●━━━━━●━━━━━●━━━━━●━━━━━◉  feat/auth ◀ current        │
│           │                       │                                                │
│           │                       └━━━━━●━━━━━●  experiment (abandoned)            │
│           │                                                                        │
│           └━━━━━●━━━━━●━━━━━●━━━━━●━━━━━●  refactor/db                            │
│                                                                                    │
│                                                                                    │
│                                                                                    │
│                                                                                    │
│  Legend:  ● checkpoint   ◉ selected   ◀ current thread   ━ lineage   │ branch     │
│           ✓ merged       ✗ abandoned                                              │
│                                                                                    │
├────────────────────────────────────────────────────────────────────────────────────┤
│ ┌─ Selected ─────────────────────────────────────────────────────────────────────┐ │
│ │ feat/auth  │  turn 24  │  "Add refresh token rotation"  │  gpt-4.1  │  $0.47  │ │
│ └────────────────────────────────────────────────────────────────────────────────┘ │
│                                                                                    │
│ [Enter] jump   [b] branch from   [h] handoff   [d] diff with main   [r] resume    │
└────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Graph Elements

| Symbol | Meaning |
|--------|---------|
| `●` | Checkpoint (navigable point) |
| `◉` | Currently selected node |
| `◀` | Current active thread |
| `━` | Direct lineage |
| `│` | Branch point |
| `✓` | Merged back to parent |
| `✗` | Abandoned/archived |

---

## Navigation

| Key | Action |
|-----|--------|
| `h/←` | Move left (earlier in time) |
| `l/→` | Move right (later in time) |
| `j/↓` | Move to lower branch |
| `k/↑` | Move to upper branch |
| `g` | Go to root (earliest point) |
| `G` | Go to latest point on current branch |
| `Enter` | Jump to selected point |
| `/` | Search for thread by name |

---

## Selected Node Detail

The bottom panel shows details of the currently selected node:

```
┌─ Selected ─────────────────────────────────────────────────────────────────────┐
│ Thread:      feat/auth                                                         │
│ Turn:        24                                                                │
│ Message:     "Add refresh token rotation with 7-day expiry window"            │
│ Model:       gpt-4.1                                                           │
│ Cost:        $0.47                                                             │
│ Files:       src/auth.ts, src/middleware.ts (+2 more)                          │
│ Timestamp:   Jan 15, 2024 14:47:23                                            │
└────────────────────────────────────────────────────────────────────────────────┘
```

---

## Actions

| Key | Action | Effect |
|-----|--------|--------|
| `Enter` | Jump | → Live Session at selected point |
| `b` | Branch | → Branch flow from selected point |
| `h` | Handoff | → Handoff flow with context from point |
| `d` | Diff | Show diff between selected and main |
| `r` | Resume | → Live Session continuing from point |
| `Esc` | Back | → Previous screen |

---

## Diff View (Overlay)

When pressing `d`:

```
┌─ Diff: feat/auth vs main ─────────────────────────────────────────────────────────┐
│                                                                                   │
│  feat/auth is 18 commits ahead, 3 commits behind main                            │
│                                                                                   │
│  Files changed: 4                                                                 │
│  ─────────────────────────────────────────────────────────────────────────────    │
│    M  src/auth.ts              +45 -12                                           │
│    M  src/middleware.ts        +23 -8                                            │
│    A  src/types/auth.ts        +67 -0                                            │
│    M  tests/auth.test.ts       +89 -4                                            │
│                                                                                   │
│  [Enter] view file diff    [m] merge to main    [Esc] close                      │
└───────────────────────────────────────────────────────────────────────────────────┘
```

---

## Zoom Levels

For large graphs, support zoom:

### Zoomed Out (Overview)
```
●━●━●━●━●━●━●━●━●━●  main
    │   │ │
    │   │ └●━●━●  fix
    │   │
    │   └●━●━●━●━◉  feat ◀
    │       │
    │       └●━●  exp
    │
    └●━●━●━●━●  refactor
```

### Zoomed In (Detail)
```
          ┌─ turn 18 ─┐     ┌─ turn 19 ─┐     ┌─ turn 20 ─┐
●━━━━━━━━━●━━━━━━━━━━━●━━━━━●━━━━━━━━━━━●━━━━━●━━━━━━━━━━━●━━━━
          │           │                 │
          │     "implement"       "add tests"
          │      auth flow
          │
          └━━━━●━━━━━●  experiment branch
              │
        "try different
           approach"
```

| Key | Action |
|-----|--------|
| `+` / `=` | Zoom in |
| `-` | Zoom out |
| `0` | Reset zoom |

---

## Considerations for Implementers

- **Graph layout**: Laying out a DAG in ASCII is non-trivial. Consider libraries or algorithms.
- **Large graphs**: Hundreds of branches need virtualization or collapsing.
- **Performance**: Graph computation should be cached; re-layout only on change.
- **Terminal width**: Graph needs to adapt to available width gracefully.
- **Color**: Use color to distinguish branches, but ensure symbols work without color.

---

## Surface Parity

| TUI | CLI | Headless | SDK |
|-----|-----|----------|-----|
| Visual map | `rip threads --tree` | `--tree --json` (structured) | `client.getThreadMap()` |
| Node selection | N/A | N/A | Programmatic access |
| Branch from point | `rip branch --from <id>` | Same | `client.branch(fromId)` |
| Diff | `rip diff <thread> main` | Same with `--json` | `client.diffThreads(a, b)` |
