# Permissions

Status: **Sketch** | Phase: 2

This screen doc is conceptual. Canonical UX gates are the journey specs in `docs/02_architecture/tui/journeys/` plus [Canvas + X-ray](../07_canvas_and_xray.md).

## Purpose

Request user approval for tool executions based on policy rules. Shows what will happen and allows approve/deny/edit decisions.

## Entry Conditions

- Agent requests tool execution requiring approval
- Policy rule triggers `ask` permission mode
- Destructive or sensitive operation detected

## Capabilities Used

| Capability | Usage |
|------------|-------|
| `policy.permissions.rules` | Rule evaluation |
| `security.permissions` | Permission engine |
| `ui.permission_modes` | Mode display |
| `tool.permissions` | Tool-specific policies |

---

## Wireframe (Standard Request)

```
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                     │
│                                                                                     │
│     ┌─ Permission Required ─────────────────────────────────────────────────────┐  │
│     │                                                                           │  │
│     │  Tool:     bash                                                           │  │
│     │  Action:   Execute shell command                                          │  │
│     │                                                                           │  │
│     │  ┌─ Command ────────────────────────────────────────────────────────────┐ │  │
│     │  │                                                                      │ │  │
│     │  │  rm -rf node_modules && npm install                                  │ │  │
│     │  │                                                                      │ │  │
│     │  └──────────────────────────────────────────────────────────────────────┘ │  │
│     │                                                                           │  │
│     │  ⚠  Warning: This will delete the node_modules directory                 │  │
│     │                                                                           │  │
│     │  Policy:   ask (file deletions require approval)                         │  │
│     │  Matched:  rule "destructive_ops" in ~/.rip/policy.json                  │  │
│     │                                                                           │  │
│     │  ────────────────────────────────────────────────────────────────────    │  │
│     │                                                                           │  │
│     │  [y] Allow once                                                          │  │
│     │  [a] Allow for this session                                              │  │
│     │  [A] Always allow (update policy)                                        │  │
│     │  [n] Deny                                                                │  │
│     │  [e] Edit command                                                        │  │
│     │                                                                           │  │
│     │  [ ] Remember this choice for similar commands                           │  │
│     │                                                                           │  │
│     └───────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Information Displayed

| Field | Description |
|-------|-------------|
| Tool | Tool being invoked |
| Action | Human-readable description |
| Command/Args | Actual parameters |
| Warning | Risk indicator (if applicable) |
| Policy | Which policy rule triggered |
| Matched | Rule source location |

---

## Warning Levels

### Low Risk (Informational)
```
│  ℹ  This will read files in the src/ directory                                │
```

### Medium Risk (Caution)
```
│  ⚠  This will modify 3 files in your project                                  │
```

### High Risk (Warning)
```
│  ⚠  Warning: This will delete files and cannot be undone                      │
│     Affected: node_modules/ (15,234 files)                                    │
```

### Critical Risk (Danger)
```
│  ⛔ DANGER: This command has elevated privileges                              │
│     Running: sudo rm -rf /                                                    │
│     This is extremely dangerous and likely unintended.                        │
```

---

## Decision Options

| Key | Action | Effect |
|-----|--------|--------|
| `y` | Allow once | Execute this instance only |
| `a` | Allow session | Auto-approve similar until TUI closes |
| `A` | Always allow | Update policy to allow permanently |
| `n` | Deny | Block execution, return error to agent |
| `e` | Edit | Modify command before execution |
| `?` | Explain | Show why this rule matched |

---

## Edit Mode

When pressing `e`:

```
┌─ Edit Command ──────────────────────────────────────────────────────────────────┐
│                                                                                 │
│  Original:                                                                      │
│  rm -rf node_modules && npm install                                            │
│                                                                                 │
│  Modified:                                                                      │
│  ┌────────────────────────────────────────────────────────────────────────────┐│
│  │ rm -rf node_modules && npm install --legacy-peer-deps█                     ││
│  └────────────────────────────────────────────────────────────────────────────┘│
│                                                                                 │
│  [Enter] approve modified    [Esc] cancel edit                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Batch Approval

When multiple permissions are pending:

```
┌─ Permissions Required (3 pending) ──────────────────────────────────────────────┐
│                                                                                 │
│  The agent wants to perform multiple operations:                                │
│                                                                                 │
│  1. [bash] npm test --coverage                           ○ pending             │
│  2. [write] Update src/auth.ts (+24 -8 lines)           ○ pending             │
│  3. [bash] npm run build                                 ○ pending             │
│                                                                                 │
│  ────────────────────────────────────────────────────────────────────────────  │
│                                                                                 │
│  [1-3] Review individual    [Y] Approve all    [N] Deny all                    │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## File Edit Permission

Special handling for file modifications:

```
┌─ Permission Required ───────────────────────────────────────────────────────────┐
│                                                                                 │
│  Tool:     apply_patch                                                          │
│  Action:   Modify file                                                          │
│  File:     src/auth.ts                                                          │
│                                                                                 │
│  ┌─ Changes ──────────────────────────────────────────────────────────────────┐│
│  │                                                                            ││
│  │   45 │-    return db.query(user, pass);                                    ││
│  │   45 │+    const sanitized = sanitize(user);                               ││
│  │   46 │+    return db.query(sanitized, hash(pass));                         ││
│  │                                                                            ││
│  │   +2 lines, -1 line                                                        ││
│  └────────────────────────────────────────────────────────────────────────────┘│
│                                                                                 │
│  Checkpoint: Will create cp_19 before applying                                 │
│                                                                                 │
│  [y] Allow    [n] Deny    [d] View full diff    [e] Edit patch                │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Policy Mode Indicator

In status bar, show current mode:

```
│ ● feat/auth │ gpt-4.1 │ 🔒 ask │
```

Mode indicators:
- `🔓 auto` - Auto-approve (full trust)
- `🔒 ask` - Ask for permissions (default)
- `🛡️ safe` - Deny destructive by default
- `⚙️ custom` - Custom policy active

---

## Timeout Behavior

If user doesn't respond:

```
┌─ Permission Required ───────────────────────────────────────────────────────────┐
│                                                                                 │
│  Tool:     bash                                                                 │
│  Command:  npm test                                                            │
│                                                                                 │
│  ⏱  Auto-deny in: 45s                                                          │
│                                                                                 │
│  [y] Allow    [n] Deny    [+] Extend timeout                                   │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Considerations for Implementers

- **Overlay priority**: Permission modals should interrupt other overlays.
- **Timeout handling**: Consider default action and configurability.
- **Audit trail**: Log all permission decisions for replay/debugging.
- **Pattern matching**: "Similar commands" matching should be configurable.

---

## Surface Parity

| TUI | CLI | Headless | SDK |
|-----|-----|----------|-----|
| Visual modal | Interactive prompt | `--allow` flags | Callback function |
| Edit command | Prompt editor | N/A | Modify before call |
| Policy display | `rip policy show` | `--policy-json` | `client.getPolicy()` |
| Mode toggle | `rip policy mode <m>` | `--policy-mode` | `client.setMode()` |
