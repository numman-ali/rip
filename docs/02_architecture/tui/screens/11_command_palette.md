# Command Palette

Status: **Sketch** | Phase: 2

This screen doc is conceptual. Canonical UX gates are the journey specs in `docs/02_architecture/tui/journeys/` plus [Canvas + X-ray](../07_canvas_and_xray.md).

## Purpose

Quick access to all commands, actions, threads, and files via fuzzy search. The central navigation hub inspired by VS Code's Ctrl+P/Ctrl+K.

## Entry Conditions

- User presses `Ctrl+K` (global shortcut)
- User types `/` at start of input (commands only)
- User types `@` in input (files only)

## Capabilities Used

| Capability | Usage |
|------------|-------|
| `ui.palette` | Palette interface |
| `ui.command_palette` | Custom commands |
| `ui.theme` | Theme selection via `set:theme` |
| `command.registry` | Command lookup |
| `command.slash` | Slash command execution |
| `ui.autocomplete` | Completion |

---

## Wireframe (Full Palette)

```
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                     │
│                                                                                     │
│     ┌─ Command Palette ─────────────────────────────────────────────────────────┐  │
│     │                                                                           │  │
│     │  > bran█                                                                  │  │
│     │                                                                           │  │
│     ├───────────────────────────────────────────────────────────────────────────┤  │
│     │                                                                           │  │
│     │  Commands                                                                 │  │
│     │  ─────────────────────────────────────────────────────────────────────    │  │
│     │  ▸ /branch             Branch from current point              Ctrl+B     │  │
│     │    /branch:handoff     Handoff with context summary                      │  │
│     │    /branch:select      Choose branch point interactively                 │  │
│     │                                                                           │  │
│     │  Threads                                                                  │  │
│     │  ─────────────────────────────────────────────────────────────────────    │  │
│     │    thread:branch-exp   Resume "branch-experiment"             1d ago     │  │
│     │                                                                           │  │
│     │  Skills                                                                   │  │
│     │  ─────────────────────────────────────────────────────────────────────    │  │
│     │    skill:branch-review Review changes before branching                   │  │
│     │                                                                           │  │
│     │  Recent                                                                   │  │
│     │  ─────────────────────────────────────────────────────────────────────    │  │
│     │    /compact            (used 2h ago)                                      │  │
│     │    /model gpt-4.1      (used 3h ago)                                      │  │
│     │                                                                           │  │
│     └───────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│     [Enter] execute    [Tab] autocomplete    [Esc] close    [↑↓] navigate          │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Search Categories

The palette searches across multiple categories:

| Category | Prefix | Example |
|----------|--------|---------|
| Commands | `/` | `/branch`, `/model`, `/review` |
| Threads | `thread:` | `thread:feat/auth` |
| Files | `@` or `file:` | `@src/auth.ts` |
| Skills | `skill:` | `skill:code-review` |
| Actions | `action:` | `action:copy-output` |
| Settings | `set:` | `set:theme`, `set:model` |

Without prefix, searches all categories.

---

## Command Results

```
┌─ Commands ─────────────────────────────────────────────────────────────────────────┐
│                                                                                    │
│  ▸ /branch             Branch from current point                      Ctrl+B      │
│    /branch:handoff     Handoff with context summary                               │
│    /cancel             Cancel current operation                       Ctrl+C      │
│    /clear              Clear output                                   Ctrl+L      │
│    /compact            Trigger context compaction                                 │
│    /export             Export session                                             │
│    /help               Show help                                      ?           │
│    /map                Open thread map                                            │
│    /model <id>         Change model                                               │
│    /review             Open review panel                                          │
│    /tasks              Show background tasks                          Ctrl+T      │
│    /threads            Open thread browser                                        │
│                                                                                    │
└────────────────────────────────────────────────────────────────────────────────────┘
```

---

## File Results

```
┌─ Files ────────────────────────────────────────────────────────────────────────────┐
│                                                                                    │
│  > @auth█                                                                          │
│                                                                                    │
│  Project Files                                                                     │
│  ─────────────────────────────────────────────────────────────────────────────     │
│  ▸ src/auth.ts                     Modified in this session                       │
│    src/auth/index.ts                                                              │
│    src/auth/middleware.ts          Modified in this session                       │
│    src/types/auth.ts               Created in this session                        │
│    tests/auth.test.ts                                                             │
│                                                                                    │
│  Recent                                                                            │
│  ─────────────────────────────────────────────────────────────────────────────     │
│    src/auth.ts                     Referenced 5m ago                              │
│    src/middleware.ts               Referenced 12m ago                             │
│                                                                                    │
└────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Thread Results

```
┌─ Threads ──────────────────────────────────────────────────────────────────────────┐
│                                                                                    │
│  > thread:auth█                                                                    │
│                                                                                    │
│  Matching Threads                                                                  │
│  ─────────────────────────────────────────────────────────────────────────────     │
│  ▸ feat/auth             24 turns    "OAuth2 implementation"           2h ago    │
│    fix/auth-bug          8 turns     "Session timeout fix"             1d ago    │
│    auth-experiment       4 turns     "Trying different approach"       3d ago    │
│                                                                                    │
│  [Enter] switch    [b] branch from    [h] handoff                                 │
│                                                                                    │
└────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Skill Results

```
┌─ Skills ───────────────────────────────────────────────────────────────────────────┐
│                                                                                    │
│  > skill:review█                                                                   │
│                                                                                    │
│  Available Skills                                                                  │
│  ─────────────────────────────────────────────────────────────────────────────     │
│  ▸ code-review           Review code for issues and improvements                  │
│    security-review       Check for security vulnerabilities                       │
│    pr-review             Prepare pull request description                         │
│                                                                                    │
│  [Enter] invoke    [?] show details                                               │
│                                                                                    │
└────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Command with Arguments

Some commands accept arguments:

```
┌─ /model ───────────────────────────────────────────────────────────────────────────┐
│                                                                                    │
│  > /model █                                                                        │
│                                                                                    │
│  Select Model                                                                      │
│  ─────────────────────────────────────────────────────────────────────────────     │
│  ▸ gpt-4.1              Current model                                  ✓          │
│    gpt-4.1-mini         Faster, lower cost                                        │
│    claude-3-opus        Alternative provider                                       │
│    claude-3-sonnet      Fast alternative                                           │
│                                                                                    │
│  [Enter] select    [Tab] confirm and close                                        │
│                                                                                    │
└────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Fuzzy Matching

The palette uses fuzzy matching:

| Query | Matches |
|-------|---------|
| `br` | `/branch`, `/branch:handoff`, `thread:branch-exp` |
| `mdl` | `/model` |
| `auth.ts` | `src/auth.ts`, `tests/auth.test.ts` |
| `cmp` | `/compact`, `/compare` |

Matching is case-insensitive and matches non-contiguous characters.

---

## Result Grouping

Results are grouped by category with headers:

1. **Commands** (if any match)
2. **Threads** (if any match)
3. **Files** (if any match)
4. **Skills** (if any match)
5. **Recent** (always shown at bottom)

Empty groups are hidden.

---

## Actions

| Key | Action | Effect |
|-----|--------|--------|
| `Enter` | Execute | Run command or navigate |
| `Tab` | Autocomplete | Complete current selection |
| `↑/↓` | Navigate | Move through results |
| `Esc` | Close | Return to previous view |
| `Ctrl+Enter` | Execute + close | Run and immediately close |
| `?` | Details | Show command/skill details |

---

## Inline Preview

For some items, show preview on selection:

```
┌─ Command Palette ───────────────────────────────────────────────────────────────────┐
│                                                                                     │
│  > /branch█                                                                         │
│                                                                                     │
│  ▸ /branch             Branch from current point                                   │
│    /branch:handoff     Handoff with context summary                                │
│                                                                                     │
│  ┌─ Preview ──────────────────────────────────────────────────────────────────┐   │
│  │                                                                            │   │
│  │  /branch                                                                   │   │
│  │                                                                            │   │
│  │  Create a new branch from the current conversation point.                  │   │
│  │                                                                            │   │
│  │  Usage: /branch [name]                                                     │   │
│  │                                                                            │   │
│  │  Options:                                                                  │   │
│  │    --from <turn>    Branch from specific turn                              │   │
│  │    --tag <tag>      Add tag to new branch                                  │   │
│  │                                                                            │   │
│  └────────────────────────────────────────────────────────────────────────────┘   │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Considerations for Implementers

- **Search performance**: Fuzzy search over many items should feel instant.
- **Result ranking**: Recent + relevance should influence ordering.
- **Keyboard focus**: Ensure palette captures all keys except Esc.
- **Async loading**: Thread/file lists may need async loading.

---

## Surface Parity

| TUI | CLI | Headless | SDK |
|-----|-----|----------|-----|
| Visual palette | `rip <command>` | Direct command | Method calls |
| Fuzzy search | `rip help --search` | N/A | N/A |
| File completion | `@file` in prompt | `--context file` | Context param |
| Command list | `rip help commands` | `--list-commands` | `client.getCommands()` |
