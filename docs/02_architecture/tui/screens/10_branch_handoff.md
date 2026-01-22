# Branch & Handoff

## Purpose

Create new conversation branches or hand off context to fresh threads. Manages context carryover and thread relationships.

## Entry Conditions

- User invokes `/branch` command
- User presses `b` on a thread/checkpoint
- User invokes `/handoff` command

## Capabilities Used

| Capability | Usage |
|------------|-------|
| `thread.branch` | Create branch |
| `thread.handoff` | Handoff with summary |
| `thread.reference` | Context reference |
| `context.refs.thread` | Thread referencing |
| `compaction.branch_summary` | Auto-summarize |

---

## Branch Flow Wireframe

```
┌─────────────────────────────────────────────────────────────────────────────────────┐
│ Branch from: feat/auth @ turn 18                               [Esc] cancel         │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─ Branch Point ───────────────────────────────────────────────────────────────┐  │
│  │                                                                              │  │
│  │  Thread:     feat/auth                                                       │  │
│  │  Turn:       18 of 24                                                        │  │
│  │  Message:    "Add refresh token rotation with 7-day expiry"                  │  │
│  │  Files:      src/auth.ts, src/middleware.ts                                  │  │
│  │                                                                              │  │
│  └──────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│  ┌─ New Branch ─────────────────────────────────────────────────────────────────┐  │
│  │                                                                              │  │
│  │  Name: fix/token-expiry█                                                     │  │
│  │                                                                              │  │
│  │  Tags: #auth #bugfix                                                         │  │
│  │                                                                              │  │
│  └──────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│  ┌─ Context Carryover ──────────────────────────────────────────────────────────┐  │
│  │                                                                              │  │
│  │  [x] Conversation history (turns 1-18)              ~12,000 tokens          │  │
│  │  [x] File context (4 files)                         ~3,200 tokens           │  │
│  │  [x] Recent tool outputs (last 5)                   ~1,800 tokens           │  │
│  │  [x] Checkpoints (8 available)                      metadata only           │  │
│  │                                                                              │  │
│  │  Estimated context: ~17,000 tokens                                          │  │
│  │                                                                              │  │
│  └──────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│  ┌─ Initial Message (optional) ─────────────────────────────────────────────────┐  │
│  │                                                                              │  │
│  │  The refresh tokens seem to be expiring too quickly. Can you investigate    │  │
│  │  the TTL configuration and check if the rotation logic is correct?          │  │
│  │  █                                                                          │  │
│  │                                                                              │  │
│  └──────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│  [Enter] create branch    [Tab] next field    [Ctrl+Enter] create and switch       │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Handoff Flow Wireframe

```
┌─────────────────────────────────────────────────────────────────────────────────────┐
│ Handoff from: feat/auth                                        [Esc] cancel         │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─ Current Thread Summary ─────────────────────────────────────────────────────┐  │
│  │                                                                              │  │
│  │  Thread:     feat/auth (24 turns, $0.47)                                     │  │
│  │  Started:    Jan 15, 14:23                                                   │  │
│  │  Goal:       Implement OAuth2 authentication flow                            │  │
│  │  Status:     Core auth complete, refresh tokens working                      │  │
│  │                                                                              │  │
│  │  Key files:                                                                  │  │
│  │    • src/auth.ts (created login, logout, refresh functions)                 │  │
│  │    • src/middleware.ts (added auth middleware)                              │  │
│  │    • src/types/auth.ts (defined Session, Token interfaces)                  │  │
│  │                                                                              │  │
│  └──────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│  ┌─ New Thread ─────────────────────────────────────────────────────────────────┐  │
│  │                                                                              │  │
│  │  Name: feat/auth-ui█                                                         │  │
│  │                                                                              │  │
│  │  Purpose: Now that auth is done, build the login UI components              │  │
│  │                                                                              │  │
│  └──────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│  ┌─ Context Transfer ───────────────────────────────────────────────────────────┐  │
│  │                                                                              │  │
│  │  [x] Auto-generated summary                         ~800 tokens             │  │
│  │  [x] File structure awareness                       ~400 tokens             │  │
│  │  [ ] Full conversation history                      ~12,000 tokens          │  │
│  │  [ ] All tool outputs                               ~5,000 tokens           │  │
│  │                                                                              │  │
│  │  ( ) Minimal context (summary only)                                         │  │
│  │  (•) Standard context (recommended)                                         │  │
│  │  ( ) Full context (everything)                                              │  │
│  │                                                                              │  │
│  └──────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│  [Enter] create handoff    [Tab] next field    [e] edit summary                    │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Context Carryover Options

### Branch (Fork)
Inherits everything up to branch point:
- Full conversation history to that turn
- All file context at that point
- Checkpoint state
- Same model/config

### Handoff (Fresh Start)
Curated context transfer:
- Auto-generated summary of accomplishments
- Key file references (not content)
- Goal/purpose statement
- Clean conversation start

---

## Summary Editor

When pressing `e` on auto-summary:

```
┌─ Edit Handoff Summary ──────────────────────────────────────────────────────────────┐
│                                                                                     │
│  This summary will be included in the new thread's context:                         │
│                                                                                     │
│  ┌──────────────────────────────────────────────────────────────────────────────┐  │
│  │ ## Previous Thread: feat/auth                                                │  │
│  │                                                                              │  │
│  │ ### Accomplishments                                                          │  │
│  │ - Implemented login/logout with session tokens                               │  │
│  │ - Added refresh token rotation (7-day expiry)                                │  │
│  │ - Created auth middleware for protected routes                               │  │
│  │ - All tests passing (12/12)                                                  │  │
│  │                                                                              │  │
│  │ ### Key Files                                                                │  │
│  │ - `src/auth.ts` - Core auth functions                                        │  │
│  │ - `src/middleware.ts` - Express middleware                                   │  │
│  │ - `src/types/auth.ts` - TypeScript interfaces                                │  │
│  │                                                                              │  │
│  │ ### Notes for Next Steps                                                     │  │
│  │ - Auth API is complete and tested                                            │  │
│  │ - Ready for frontend integration█                                            │  │
│  └──────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│  [Ctrl+Enter] save    [Esc] cancel    [Ctrl+R] regenerate                          │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Branch Point Selection

When branching from a specific point:

```
┌─ Select Branch Point ───────────────────────────────────────────────────────────────┐
│                                                                                     │
│  Thread: feat/auth                                                                  │
│                                                                                     │
│  Turn │ Message                                              │ Files  │ Time       │
│  ─────┼──────────────────────────────────────────────────────┼────────┼─────────── │
│    1  │ "Help me implement OAuth2 auth"                      │ 0      │ 14:23      │
│    5  │ "Let's start with the session model"                 │ 1      │ 14:28      │
│   12  │ "Add input validation to login"                      │ 2      │ 14:45      │
│ ▸ 18  │ "Add refresh token rotation"                         │ 3      │ 15:12      │
│   24  │ "Latest: All tests passing"                          │ 4      │ 15:34      │
│                                                                                     │
│  [j/k] select    [Enter] branch from here    [Esc] cancel                          │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

---

## After Creation

```
┌─ Branch Created ────────────────────────────────────────────────────────────────────┐
│                                                                                     │
│  ✓ Created: fix/token-expiry                                                       │
│                                                                                     │
│  Branched from feat/auth @ turn 18                                                 │
│  Context: 17,000 tokens carried over                                               │
│                                                                                     │
│  ┌────────────────────────────────────────────────────────────────────────────┐   │
│  │  [Enter] Switch to new branch    [Esc] Stay in current thread             │   │
│  └────────────────────────────────────────────────────────────────────────────┘   │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Actions

| Key | Action | Context |
|-----|--------|---------|
| `Tab` | Next field | Form navigation |
| `Shift+Tab` | Previous field | Form navigation |
| `Enter` | Create | Final confirmation |
| `Ctrl+Enter` | Create and switch | Immediate switch |
| `Space` | Toggle checkbox | Context options |
| `e` | Edit summary | Handoff summary |
| `Esc` | Cancel | Close flow |

---

## Considerations for Implementers

- **Token counting**: Provide estimates but don't block on exact counts.
- **Summary generation**: May require LLM call; show loading state.
- **Branch visualization**: Update thread map after creation.
- **Context limits**: Warn if context exceeds model limits.

---

## Surface Parity

| TUI | CLI | Headless | SDK |
|-----|-----|----------|-----|
| Visual flow | `rip threads branch <thread_id> --from-message-id <message_id> --title <title>` | Same + `--server <url>` | `rip.threadBranch(parentThreadId, req, opts)` |
| Handoff | `rip threads handoff <thread_id> --summary-markdown "<text>" --title <title>` | Same + `--server <url>` | `rip.threadHandoff(fromThreadId, req, opts)` |
| Context selection | Checkboxes | `--context minimal/standard/full` | Options param |
| Summary edit | Editor | `--summary-markdown "<text>"` | `summary_markdown` |
