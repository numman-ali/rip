# TUI Design Principles

## Foundational Stance: Build for Change

RIP is in an **experimental research phase**. We are building fast, learning fast, and pivoting fast. These designs exist to communicate intent and guide implementation, not to constrain it.

**For implementers**:

1. **Flexibility over precision** — If a design detail conflicts with a better technical approach, the technical approach wins. Flag it and move on.

2. **Modularity is non-negotiable** — Every widget, panel, and screen should be independently developable and testable. Avoid coupling.

3. **Capabilities will change** — The capability matrix is evolving. Build UI components that can appear, disappear, or transform without rewriting everything.

4. **Experiment freely** — These wireframes show one valid interpretation. If you find a better layout, interaction, or flow — try it. Document what you learn.

5. **Ship and iterate** — A working screen that covers 70% of the design is better than no screen. Get it in front of users.

---

## UX Philosophy

### Observability First

The primary job of the TUI is to show users **what is happening**. Before we make it interactive, it must be transparent:

- Every agent action should be visible
- Tool calls, their inputs, outputs, and status must be inspectable
- Errors should be obvious, not hidden
- The user should never wonder "what is it doing?"

### Keyboard Native

Terminal users expect keyboard efficiency:

- Every action reachable without mouse
- Consistent navigation patterns (j/k, arrows, tab)
- Command palette for discoverability
- Shortcuts for frequent actions

### Information Hierarchy

Not everything is equally important. Design for scanning:

- **Primary**: What's happening right now (streaming output, active tools)
- **Secondary**: Context (thread, model, session info)
- **Tertiary**: History and details (available on demand)

### Progressive Disclosure

Don't overwhelm. Show summary first, details on request:

- Timeline shows frame summaries → expand for full detail
- Output shows rendered text → toggle for raw frames
- Collapsed panels for less-used features

### Delight Without Distraction

RIP’s TUI should feel *obviously smarter than a chat log* while staying calm:

- **Conversational-first** by default; drill into tools/diffs/artifacts only when needed
- **Ambient signals** for background work (tools/tasks/subagents) using icons + restrained color
- **Fast “what’s going on?” answers** at a glance, without reading code

See: [Experience Review](06_experience_review.md) (defines “wow” gates + phone/SSH/web constraints).

---

## Visual Language

### Layout Conventions

```
┌─ Title ─────────────────────────────────────────────────────────┐
│ Content area                                                    │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│ Secondary area or actions                                       │
└─────────────────────────────────────────────────────────────────┘
```

- Borders define regions
- Titles in top border indicate panel purpose
- Status/actions typically at bottom
- Consistent padding within panels

### Indicators

| Symbol | Meaning |
|--------|---------|
| `▸` | Selected / current |
| `▾` | Expanded |
| `▹` | Collapsed |
| `●` | Active / connected |
| `○` | Inactive |
| `◐` | In progress |
| `✓` | Success / complete |
| `✗` | Failed / error |
| `⚠` | Warning |
| `⟳` | Running / loading |

### Focus States

- Focused panel: highlighted border or title
- Selected item: inverted colors or `▸` marker
- Disabled: dimmed text

---

## Surface Parity

Every TUI capability must have equivalents:

| TUI | CLI | Headless | SDK |
|-----|-----|----------|-----|
| Visual timeline | `--view output` | JSON stream | Event iterator |
| Command palette | Slash commands | N/A | Method calls |
| Tool detail modal | `--verbose` | Full frame JSON | Frame access |
| Permission prompt | Interactive prompt | `--allow` flags | Callback/policy |

When designing a screen, always ask: "How does a headless script get the same information or perform the same action?"

---

## Considerations for Implementers

These are flags, not mandates:

### On Large Lists
- Timelines, thread lists, and search results can grow large
- Consider how scrolling, filtering, and search will perform
- Users should feel instant response even with thousands of items

### On Real-time Updates
- Frames arrive continuously during active sessions
- New content shouldn't jar the user or lose their place
- Consider auto-follow vs manual scroll modes

### On Terminal Constraints
- Assume minimum 80x24, design for 120x40
- Colors: design for 256-color, degrade for 16-color
- No images (use ASCII art or placeholders)
- Unicode: assume modern terminals, have ASCII fallbacks

### On State
- TUI state should be reconstructable from the event stream
- Closing and reopening should restore position (within reason)
- Consider what state persists vs what resets

### On Testing
- Designs should be testable via golden snapshots
- Predictable layouts help automated testing
- Consider how to test keyboard flows

---

## Anti-Patterns to Avoid

- **Modal hell** — Avoid deep stacks of modals. Prefer inline expansion.
- **Mystery meat navigation** — Every key should be discoverable via `?` help.
- **Information overload** — If everything is visible, nothing stands out.
- **Coupling to capability versions** — UI shouldn't break if a capability changes.
- **Hardcoded layouts** — Panels should adapt to terminal size.
