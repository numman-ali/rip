# Navigation Model

Canonical postures are defined in [Canvas + X-ray](07_canvas_and_xray.md).

## Screen Hierarchy

The TUI has three levels of UI:

1. **Screens** — Full terminal views (Start, Live Session, Thread Browser)
2. **Panels** — Optional pinned regions within a screen (Activity rail, Timeline, Inspector)
3. **Overlays/Drawers** — Modal layers on top of screens (Command Palette, Activity drawer, Tool Detail)

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
                 │   LIVE SESSION        │
                 │   (Canvas default)    │
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
│ - Activity  │      │ - Activity  │      │ - X-ray     │
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
| `Enter` on event/chip | Detail overlay (tool/task/error/provider/context) |
| `Enter` on artifact ref | Artifact Viewer overlay |
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

Within a screen, focus moves between panels when they are present/pinned. In the Canvas default, focus is usually just **Canvas ↔ Input**.

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

### XS (60×20 → 79×23)
- Canvas only; everything else via overlays/drawers.

### S (80×24 → 99×30)
- Canvas remains primary.
- Activity (timeline/tasks/errors) via overlay/drawer by default.

### M (100×31+)
- Canvas remains primary.
- Optional pinned Activity rail and/or Inspector (X-ray) as a power-user preset.

### Considerations for Implementers
- Detect terminal resize events
- Gracefully reflow content
- Preserve focus and scroll position on resize
- Consider which panels collapse first when space is tight
