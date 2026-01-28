# Start Screen

Status: **Sketch** | Phase: 2

This screen doc is conceptual. Canonical UX gates are the journey specs in `docs/02_architecture/tui/journeys/` plus [Canvas + X-ray](../07_canvas_and_xray.md).

## Purpose

The entry point when launching `rip tui`. Provides quick access to common actions and recent threads.

## Entry Conditions

- TUI launched without session argument
- User returns from quit confirmation
- No active session attached

## Capabilities Used

| Capability | Usage |
|------------|-------|
| `session.create` | New thread action |
| `session.resume` | Resume thread action |
| `thread.search.text` | Recent threads list |
| `integrations.tui_attach` | Attach to running session |

---

## Wireframe

```
┌─────────────────────────────────────── RIP ────────────────────────────────────────┐
│                                                                                    │
│                                                                                    │
│                                                                                    │
│     ┌─ Start ────────────────────────────────────────────────────────────────┐    │
│     │                                                                        │    │
│     │                              ╱╲                                        │    │
│     │                             ╱  ╲                                       │    │
│     │                            ╱ RI ╲                                      │    │
│     │                           ╱  P   ╲                                     │    │
│     │                          ╱────────╲                                    │    │
│     │                                                                        │    │
│     │  ┌────────────────────────────────────────────────────────────────┐   │    │
│     │  │  [N] New thread              Start a fresh conversation        │   │    │
│     │  │  [A] Attach                  Connect to running session        │   │    │
│     │  │  [R] Resume                  Continue from where you left      │   │    │
│     │  │  [B] Browse                  Search all threads                │   │    │
│     │  └────────────────────────────────────────────────────────────────┘   │    │
│     │                                                                        │    │
│     ├────────────────────────────────────────────────────────────────────────┤    │
│     │  Recent                                                                │    │
│     │  ─────────────────────────────────────────────────────────────────     │    │
│     │  ▸ feat/auth             2h ago    "implement OAuth2 flow"            │    │
│     │    fix/login-bug         5h ago    "debug session timeout"            │    │
│     │    refactor/db           1d ago    "migrate to postgres"              │    │
│     │    api-cleanup           3d ago    "remove deprecated endpoints"      │    │
│     │                                                                        │    │
│     │  [/] search    [Enter] resume    [Tab] filter by tag                  │    │
│     └────────────────────────────────────────────────────────────────────────┘    │
│                                                                                    │
│                                                                                    │
│  workspace: ~/projects/myapp                                                       │
│  model: gpt-4.1  │  policy: standard  │  tools: 12 enabled  │  [?] help           │
└────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Information Displayed

| Element | Source | Notes |
|---------|--------|-------|
| Logo/branding | Static | Optional, can be config-disabled |
| Action menu | Static | Primary actions |
| Recent threads | Thread storage | Last 5-10, sorted by recency |
| Thread summary | Thread metadata | Name, age, last message preview |
| Workspace path | Current directory | Shows where RIP is running |
| Config summary | Active config | Model, policy, tool count |

---

## Actions

| Key | Action | Destination |
|-----|--------|-------------|
| `N` | New thread | → Live Session (new) |
| `A` | Attach | → Attach picker (if multiple) or Live Session |
| `R` | Resume | → Thread Browser (resume mode) |
| `B` | Browse | → Thread Browser |
| `Enter` | Resume selected recent | → Live Session |
| `/` | Search | Focus search input |
| `j/k` | Navigate recents | Move selection |
| `Tab` | Filter by tag | Show tag filter |
| `?` | Help | Help overlay |
| `Ctrl+D` | Quit | Exit |

---

## Transitions

```
                    ┌─────────────┐
                    │   START     │
                    └──────┬──────┘
                           │
       ┌───────────────────┼───────────────────┐
       │                   │                   │
       ▼                   ▼                   ▼
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│ Live Session│    │Thread Browse│    │Attach Picker│
│   (new)     │    │             │    │  (modal)    │
└─────────────┘    └─────────────┘    └─────────────┘
```

---

## Attach Picker (Sub-modal)

When multiple sessions are running:

```
┌─ Attach to Session ───────────────────────────────────────────┐
│                                                               │
│  Running sessions:                                            │
│  ─────────────────────────────────────────────────────────    │
│  ▸ session_a1b2    feat/auth      streaming    2m active     │
│    session_c3d4    debug/perf     idle         15m ago       │
│                                                               │
│  [Enter] attach    [Esc] cancel                              │
└───────────────────────────────────────────────────────────────┘
```

---

## Empty State

When no recent threads exist:

```
┌─ Start ──────────────────────────────────────────────────────────────┐
│                                                                      │
│                              Welcome to RIP                          │
│                                                                      │
│  No recent threads. Start a new conversation to begin.               │
│                                                                      │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │  [N] New thread              Start your first conversation     │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

---

## Considerations for Implementers

- **Recents loading**: Threads may be stored on disk; consider async loading with placeholder
- **Attach discovery**: Detecting running sessions requires server communication
- **Config display**: Should reflect actual active config, not defaults
- **Responsiveness**: This screen should appear instantly; defer heavy loading

---

## Surface Parity

| TUI | CLI | Headless | SDK |
|-----|-----|----------|-----|
| Visual start screen | `rip --help` | N/A | N/A |
| Recent threads | `rip threads --recent` | `--list-threads` JSON | `client.listThreads()` |
| Attach | `rip attach <id>` | `--session-id` | `client.attach(id)` |
| New thread | `rip` (default) | `rip run` | `client.createSession()` |
