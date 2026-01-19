# Navigation Model

## Screen Hierarchy

The TUI has three levels of UI:

1. **Screens** — Full terminal views (Start, Live Session, Thread Browser)
2. **Panels** — Regions within a screen (Timeline, Output, Inspector)
3. **Overlays** — Modal layers on top of screens (Command Palette, Tool Detail)

```
┌─────────────────────────────────────────────────────────────────┐
│                         SCREEN                                  │
│  ┌─────────────┐ ┌─────────────────────┐ ┌─────────────────┐   │
│  │   PANEL     │ │       PANEL         │ │     PANEL       │   │
│  │             │ │                     │ │                 │   │
│  │             │ │                     │ │                 │   │
│  └─────────────┘ └─────────────────────┘ └─────────────────┘   │
│                                                                 │
│         ┌─────────────────────────────┐                        │
│         │         OVERLAY             │                        │
│         │    (on top of screen)       │                        │
│         └─────────────────────────────┘                        │
└─────────────────────────────────────────────────────────────────┘
```

---

## State Diagram

```
                                ┌──────────────┐
                                │    START     │
                                │              │
                                └──────┬───────┘
                                       │
              ┌────────────────────────┼────────────────────────┐
              │                        │                        │
              ▼                        ▼                        ▼
      ┌───────────────┐      ┌─────────────────┐      ┌─────────────────┐
      │   NEW THREAD  │      │ THREAD BROWSER  │      │ ATTACH/RESUME   │
      │               │      │                 │      │                 │
      └───────┬───────┘      └────────┬────────┘      └────────┬────────┘
              │                       │                        │
              │              ┌────────┴────────┐               │
              │              │                 │               │
              │              ▼                 │               │
              │      ┌───────────────┐        │               │
              │      │  THREAD MAP   │◄───────┼───────────────┤
              │      │  (optional)   │        │               │
              │      └───────┬───────┘        │               │
              │              │                │               │
              └──────────────┼────────────────┘               │
                             │                                │
                             ▼                                │
                 ┌───────────────────────┐                    │
                 │                       │◄───────────────────┘
                 │    LIVE SESSION       │
                 │    (primary view)     │
                 │                       │
                 └───────────┬───────────┘
                             │
       ┌─────────────────────┼─────────────────────┐
       │                     │                     │
       ▼                     ▼                     ▼
┌─────────────┐      ┌─────────────┐      ┌─────────────┐
│  OVERLAYS   │      │   PANELS    │      │  EXPANDED   │
│             │      │  (toggle)   │      │   VIEWS     │
│ - Palette   │      │             │      │             │
│ - Tool Det. │      │ - Sidebar   │      │ - Tasks     │
│ - Artifact  │      │ - Tasks     │      │ - Review    │
│ - Perms     │      │ - Inspector │      │             │
│ - Help      │      │             │      │             │
└─────────────┘      └─────────────┘      └─────────────┘
       │                     │                     │
       └─────────────────────┼─────────────────────┘
                             │
                             ▼
                     (return to Live Session)
```

---

## Transitions

### From Start Screen

| Action | Destination |
|--------|-------------|
| Select "New thread" | Live Session (new) |
| Select "Attach" | Live Session (existing) |
| Select "Resume" | Live Session (from checkpoint) |
| Select "Browse" | Thread Browser |

### From Thread Browser

| Action | Destination |
|--------|-------------|
| Select thread + Enter | Live Session |
| Select thread + `b` | Branch flow → Live Session |
| Select thread + `m` | Thread Map (focused) |
| `Esc` | Start Screen |

### From Live Session

| Action | Destination |
|--------|-------------|
| `Ctrl+K` | Command Palette overlay |
| `Enter` on frame | Tool Detail overlay |
| `Enter` on artifact | Artifact Viewer overlay |
| Permission required | Permission overlay |
| `Ctrl+B` | Toggle sidebar panel |
| `Ctrl+T` | Toggle/expand tasks panel |
| `/review` | Review Panel expanded |
| `/branch` | Branch flow overlay |
| `/map` | Thread Map screen |
| `?` | Help overlay |
| `Ctrl+D` | Quit confirmation |

### Overlay Behavior

- `Esc` always closes the topmost overlay
- Overlays stack (palette → tool detail → nested view)
- Maximum 3 overlay depth recommended
- Background remains visible but dimmed

---

## Focus Model

Within a screen, focus moves between panels:

```
┌─────────────────────────────────────────────────────────────────┐
│ [1] Sidebar      │ [2] Timeline        │ [3] Inspector         │
│                  │                     │                       │
│                  │                     │                       │
│                  ├─────────────────────┤                       │
│                  │ [4] Output          │                       │
│──────────────────┴─────────────────────┴───────────────────────│
│ [5] Input                                                      │
└─────────────────────────────────────────────────────────────────┘

Tab order: 1 → 2 → 3 → 4 → 5 → 1 (cycles)
Shift+Tab: reverse
```

### Focus Rules

- Only one panel has focus at a time
- Focused panel has highlighted border
- Panel-specific keys only work when focused
- Global keys work regardless of focus
- Input captures most keys when focused (except globals)

---

## Responsive Behavior

The TUI should adapt to terminal size:

### Minimum (80x24)
- Sidebar hidden by default
- Inspector collapsed
- Single-column layout

### Standard (120x40)
- Full three-column layout
- All panels visible
- Comfortable spacing

### Large (160x50+)
- Wider panels
- More visible content
- Same structure

### Considerations for Implementers
- Detect terminal resize events
- Gracefully reflow content
- Preserve focus and scroll position on resize
- Consider which panels collapse first when space is tight
