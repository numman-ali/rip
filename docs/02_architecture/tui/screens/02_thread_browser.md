# Thread Browser

Status: **Sketch** | Phase: 2

This screen doc is conceptual. Canonical UX gates are the journey specs in `docs/02_architecture/tui/journeys/` plus [Canvas + X-ray](../07_canvas_and_xray.md).

## Purpose

Search, filter, and manage all threads. The "file browser" for conversations.

## Entry Conditions

- User selects "Browse" from Start screen
- User invokes `/threads` command
- User presses thread browser shortcut from Live Session

## Capabilities Used

| Capability | Usage |
|------------|-------|
| `thread.search.text` | Keyword search |
| `thread.search.files` | Filter by files touched |
| `thread.tags` | Tag filtering |
| `thread.archive` | Archive action |
| `session.resume` | Resume selected thread |
| `thread.branch` | Branch from thread |
| `thread.handoff` | Handoff action |

---

## Wireframe

```
┌─────────────────────────────────────── RIP ────────────────────────────────────────┐
│ Thread Browser                                                    [Esc] back       │
├────────────────────────────────────────────────────────────────────────────────────┤
│ ┌─ Search ───────────────────────────────────────────────────────────────────────┐ │
│ │ > auth login█                                                                  │ │
│ │                                                                                │ │
│ │ Filters: [text] [files] [tags]        Sort: [recent] [name] [activity]        │ │
│ └────────────────────────────────────────────────────────────────────────────────┘ │
│                                                                                    │
│ ┌─ Results (12 matching) ────────────────────────┐┌─ Preview ───────────────────┐ │
│ │                                                ││                             │ │
│ │  name              │ updated │ turns │ tags    ││ feat/auth                   │ │
│ │ ───────────────────┼─────────┼───────┼──────── ││ ─────────────────────────── │ │
│ │ ▸ feat/auth        │ 2h ago  │ 24    │ #auth   ││                             │ │
│ │   fix/login-bug    │ 5h ago  │ 8     │ #bug    ││ Created:  Jan 15, 14:23     │ │
│ │   auth-experiment  │ 1d ago  │ 12    │ #auth   ││ Model:    gpt-4.1           │ │
│ │   login-refactor   │ 3d ago  │ 31    │ #auth   ││ Turns:    24                │ │
│ │   session-timeout  │ 5d ago  │ 6     │ #bug    ││ Cost:     $0.47             │ │
│ │   oauth-spike      │ 1w ago  │ 4     │ #exp    ││                             │ │
│ │                                                ││ Files touched:              │ │
│ │                                                ││   src/auth.ts               │ │
│ │                                                ││   src/middleware.ts         │ │
│ │                                                ││   src/types/auth.ts         │ │
│ │                                                ││   tests/auth.test.ts        │ │
│ │                                                ││                             │ │
│ │                                                ││ Last message:               │ │
│ │                                                ││ "Add refresh token          │ │
│ │                                                ││  rotation with 7-day        │ │
│ │                                                ││  expiry..."                 │ │
│ │                                                ││                             │ │
│ │                                                ││ Checkpoints: 12             │ │
│ │                                                ││ Branches: 2                 │ │
│ │ ───────────────────────────────────────────── ││                             │ │
│ │ showing 1-6 of 12                   [more ↓]  ││                             │ │
│ └────────────────────────────────────────────────┘└─────────────────────────────┘ │
│                                                                                    │
│ [Enter] resume   [b] branch   [h] handoff   [m] map   [a] archive   [t] edit tags │
└────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Information Displayed

### Results List

| Column | Description |
|--------|-------------|
| Name | Thread name/identifier |
| Updated | Time since last activity |
| Turns | Number of conversation turns |
| Tags | Applied tags |

### Preview Panel

| Field | Description |
|-------|-------------|
| Created | Creation timestamp |
| Model | Model used (or last model) |
| Turns | Total turn count |
| Cost | Cumulative cost (if tracked) |
| Files touched | Files modified in this thread |
| Last message | Truncated last user or assistant message |
| Checkpoints | Number of checkpoints |
| Branches | Child branches count |

---

## Search & Filter

### Text Search
Searches across:
- Thread name
- Message content
- File paths

### File Filter
```
┌─ Filter by files ─────────────────────────────────────────────┐
│ > src/auth█                                                   │
│                                                               │
│ Matching threads that touched:                                │
│   src/auth.ts                                                 │
│   src/auth/**                                                 │
└───────────────────────────────────────────────────────────────┘
```

### Tag Filter
```
┌─ Filter by tags ──────────────────────────────────────────────┐
│ [x] #auth    (8 threads)                                      │
│ [x] #bug     (5 threads)                                      │
│ [ ] #exp     (3 threads)                                      │
│ [ ] #refactor (2 threads)                                     │
└───────────────────────────────────────────────────────────────┘
```

---

## Actions

| Key | Action | Effect |
|-----|--------|--------|
| `Enter` | Resume | → Live Session with selected thread |
| `b` | Branch | → Branch flow from selected thread |
| `h` | Handoff | → Handoff flow from selected thread |
| `m` | Map | → Thread Map focused on selected |
| `a` | Archive | Archive selected (with confirmation) |
| `t` | Edit tags | Tag editor popup |
| `d` | Delete | Delete thread (with confirmation) |
| `/` | Search | Focus search input |
| `Tab` | Toggle filter type | Cycle text/files/tags |
| `j/k` | Navigate | Move selection in results |
| `Esc` | Back | → Start screen |

---

## Multi-Select Mode

Press `Space` to toggle selection, then batch actions:

```
┌─ Results (3 selected) ───────────────────────────────────────────────────────────┐
│                                                                                  │
│  [x] feat/auth        │ 2h ago  │ 24    │ #auth                                 │
│  [x] fix/login-bug    │ 5h ago  │ 8     │ #bug                                  │
│  [x] auth-experiment  │ 1d ago  │ 12    │ #auth                                 │
│  [ ] login-refactor   │ 3d ago  │ 31    │ #auth                                 │
│                                                                                  │
│ 3 selected: [a] archive all   [t] tag all   [d] delete all                      │
└──────────────────────────────────────────────────────────────────────────────────┘
```

---

## Tag Editor

```
┌─ Edit Tags: feat/auth ────────────────────────────────────────┐
│                                                               │
│  Current tags: #auth #feature                                 │
│                                                               │
│  Add tag: > █                                                 │
│                                                               │
│  Suggestions: #bug  #refactor  #experiment  #urgent          │
│                                                               │
│  [Enter] add    [x] remove selected    [Esc] done            │
└───────────────────────────────────────────────────────────────┘
```

---

## Empty State

```
┌─ Results ─────────────────────────────────────────────────────┐
│                                                               │
│                    No threads match your search               │
│                                                               │
│                    Try different keywords or                  │
│                    clear filters with [0]                     │
│                                                               │
└───────────────────────────────────────────────────────────────┘
```

---

## Transitions

| From | Action | To |
|------|--------|-----|
| Thread Browser | Enter on thread | Live Session |
| Thread Browser | `b` branch | Branch Flow |
| Thread Browser | `m` map | Thread Map |
| Thread Browser | Esc | Start Screen |

---

## Considerations for Implementers

- **Search performance**: With many threads, search should feel instant. Consider indexing.
- **Preview loading**: Preview content may require fetching; show loading state.
- **Sort stability**: Maintain selection when re-sorting.
- **Pagination vs virtual scroll**: For very large thread counts, consider approach.

---

## Surface Parity

| TUI | CLI | Headless | SDK |
|-----|-----|----------|-----|
| Visual browser | `rip threads` | `--list-threads --json` | `client.listThreads(query)` |
| Search | `rip threads --search "auth"` | Same with `--json` | `client.searchThreads(q)` |
| Tag filter | `rip threads --tag auth` | Same | `client.listThreads({tags})` |
| Archive | `rip thread archive <id>` | Same | `client.archiveThread(id)` |
