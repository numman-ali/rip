# Background Tasks

## Purpose

Monitor and control asynchronous tool executions. Shows running, completed, and failed tasks with live output streaming.

## Entry Conditions

- User expands Tasks panel (`Ctrl+T`)
- User invokes `/tasks` command
- Task notification clicked

## Capabilities Used

| Capability | Usage |
|------------|-------|
| `tool.task_spawn` | Task creation |
| `tool.task_status` | Status monitoring |
| `tool.task_cancel` | Cancellation |
| `ui.background_tasks` | Status display |

---

## Wireframe (Expanded Panel)

```
┌─────────────────────────────────────────────────────────────────────────────────────┐
│ Background Tasks                                                   [Esc] minimize   │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│ ┌─ Active (2) ──────────────────────────────────────────────────────────────────┐  │
│ │                                                                               │  │
│ │  task_id   │ tool     │ status    │ elapsed │ progress        │ actions      │  │
│ │ ───────────┼──────────┼───────────┼─────────┼─────────────────┼───────────── │  │
│ │ ▸ tsk_a1f  │ npm test │ running   │ 2:14    │ 45/48 suites    │ [c] [f] [l] │  │
│ │   tsk_b2e  │ cargo    │ compiling │ 0:47    │ ████████░░ 80%  │ [c] [f] [l] │  │
│ │                                                                               │  │
│ └───────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│ ┌─ tsk_a1f: npm test (live) ────────────────────────────────────────────────────┐  │
│ │                                                                               │  │
│ │  PASS src/auth.test.ts (4 tests)                                              │  │
│ │  PASS src/middleware.test.ts (6 tests)                                        │  │
│ │  PASS src/handlers.test.ts (12 tests)                                         │  │
│ │  RUNS src/integration.test.ts                                                 │  │
│ │    ✓ user registration flow (234ms)                                           │  │
│ │    ✓ login flow (89ms)                                                        │  │
│ │    ◌ password reset flow...                                                   │  │
│ │  █                                                                            │  │
│ │                                                                               │  │
│ └───────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│ ┌─ Completed (3) ───────────────────────────────────────────────────────────────┐  │
│ │                                                                               │  │
│ │  task_id   │ tool      │ result    │ duration │ actions                      │  │
│ │ ───────────┼───────────┼───────────┼──────────┼───────────────────────────── │  │
│ │   tsk_c3d  │ lint      │ ✓ pass    │ 12s      │ [l] log  [r] rerun           │  │
│ │   tsk_d4e  │ typecheck │ ✓ pass    │ 8s       │ [l] log  [r] rerun           │  │
│ │   tsk_e5f  │ build     │ ✗ fail    │ 34s      │ [l] log  [r] rerun  [e] err  │  │
│ │                                                                               │  │
│ └───────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│ [Enter] expand task   [c] cancel   [a] cancel all   [C] clear completed            │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Task States

| State | Icon | Description |
|-------|------|-------------|
| `pending` | `◯` | Queued, not started |
| `running` | `⟳` | Actively executing |
| `compiling` | `◐` | Preparation phase |
| `success` | `✓` | Completed successfully |
| `failed` | `✗` | Completed with error |
| `cancelled` | `⊘` | User cancelled |
| `timeout` | `⏱` | Exceeded time limit |

---

## Task List Columns

### Active Tasks

| Column | Description |
|--------|-------------|
| task_id | Short identifier |
| tool | Tool name |
| status | Current state |
| elapsed | Running time |
| progress | Progress indicator (if available) |
| actions | Quick action keys |

### Completed Tasks

| Column | Description |
|--------|-------------|
| task_id | Short identifier |
| tool | Tool name |
| result | Success/failure |
| duration | Total run time |
| actions | Available actions |

---

## Live Output Panel

When a task is selected, shows streaming output:

```
┌─ tsk_a1f: npm test (live) ────────────────────────────────────────────────────┐
│                                                                               │
│  Test Suites: 45 passed, 0 failed, 3 remaining                               │
│  Tests:       312 passed, 0 failed                                           │
│  Time:        2:14                                                            │
│                                                                               │
│  RUNS src/integration.test.ts                                                 │
│    ✓ user registration flow (234ms)                                           │
│    ✓ login flow (89ms)                                                        │
│    ◌ password reset flow...                                                   │
│  █                                                                            │
│                                                                               │
│  [c] cancel    [p] pause output    [f] full screen                           │
└───────────────────────────────────────────────────────────────────────────────┘
```

---

## Progress Indicators

Tasks can report progress in different ways:

### Percentage Bar
```
████████████░░░░░░░░ 60%
```

### Count Progress
```
45/48 suites
```

### Spinner (Unknown Progress)
```
⟳ processing...
```

### Phase Indicator
```
compiling → linking → done
```

---

## Failed Task Detail

```
┌─ tsk_e5f: cargo build (failed) ─────────────────────────────────────────────────┐
│                                                                                 │
│  Status:    ✗ Failed (exit code 101)                                           │
│  Duration:  34s                                                                 │
│                                                                                 │
│  Error Output:                                                                  │
│  ─────────────────────────────────────────────────────────────────────────────  │
│  error[E0382]: borrow of moved value: `config`                                 │
│    --> src/main.rs:45:12                                                       │
│     |                                                                          │
│  45 |     let x = config;                                                      │
│     |             ------ value moved here                                      │
│  46 |     println!("{}", config.name);                                         │
│     |                    ^^^^^^ value borrowed here after move                 │
│                                                                                 │
│  [r] rerun    [l] full log    [y] copy error    [Esc] close                   │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Collapsed Sidebar View

In the sidebar, tasks show minimal info:

```
┌─ Tasks (2) ────────────┐
│                        │
│ ⟳ npm test      2:14   │
│ ⟳ cargo build   0:47   │
│ ✓ lint          done   │
│ ✗ typecheck     fail   │
│                        │
│ [Ctrl+T] expand        │
└────────────────────────┘
```

---

## Actions

| Key | Action | Context |
|-----|--------|---------|
| `Enter` | Expand/select | Task list |
| `c` | Cancel task | Selected active task |
| `a` | Cancel all | Any (confirmation) |
| `l` | View full log | Any task |
| `r` | Rerun task | Completed task |
| `e` | View error | Failed task |
| `C` | Clear completed | Task list |
| `f` | Full screen output | Expanded task |
| `p` | Pause/resume output | Live output |
| `Esc` | Minimize panel | Panel focused |

---

## Notifications

When tasks complete, show notification:

```
┌───────────────────────────────────────────────────────────────────────────┐
│                                                                           │
│  ✓ npm test completed (2:34)                          [Enter] view [x]   │
│                                                                           │
└───────────────────────────────────────────────────────────────────────────┘
```

Failed tasks are more prominent:

```
┌───────────────────────────────────────────────────────────────────────────┐
│                                                                           │
│  ✗ cargo build failed (exit 101)                      [Enter] view [x]   │
│    error[E0382]: borrow of moved value                                    │
│                                                                           │
└───────────────────────────────────────────────────────────────────────────┘
```

---

## Considerations for Implementers

- **Output buffering**: Stream output efficiently without overwhelming the UI.
- **Multiple active tasks**: Consider how to show multiple live outputs.
- **Task persistence**: Should completed tasks persist across TUI restarts?
- **Notification timing**: Auto-dismiss notifications after delay? User preference?

---

## Surface Parity

| TUI | CLI | Headless | SDK |
|-----|-----|----------|-----|
| Visual task panel | `rip tasks` | `--tasks --json` | `client.listTasks()` |
| Live output | Streaming | JSON events | Event stream |
| Cancel | `[c]` key | `rip task cancel <id>` | `client.cancelTask(id)` |
| Rerun | `[r]` key | `rip task rerun <id>` | `client.rerunTask(id)` |
