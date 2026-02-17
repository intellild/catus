---
name: zed-terminal
description: Reference implementation of Terminal in Zed editor. Use when implementing terminal features, understanding terminal architecture, or optimizing terminal rendering performance.
---

# Zed Terminal Reference

This skill documents Zed editor's Terminal implementation for reference when building terminal applications with GPUI.

## Overview

Zed's Terminal implementation is split into two crates:
1. `terminal` - Core terminal logic and PTY management
2. `terminal_view` - UI layer with custom Element rendering

## Directory Structure

### 1. `zed/crates/terminal/` - Core Terminal Logic

| File | Responsibility |
|------|---------------|
| `terminal.rs` | Core Terminal struct, PTY management, event handling, terminal state, renderable content generation |
| `pty_info.rs` | PTY process info tracking, PID getter, title updates |
| `terminal_hyperlinks.rs` | Hyperlink detection and regex searches |
| `terminal_settings.rs` | Terminal settings (cursor shape, colors, scroll history) |
| `mappings/keys.rs` | Key mappings (Keystroke to ANSI escape sequences) |
| `mappings/colors.rs` | Color mappings (ANSI colors to GPUI Hsla) |
| `mappings/mouse.rs` | Mouse event mappings (mouse mode, scroll reporting) |

#### Key Design Patterns

**Terminal as Entity with Thread-Safe State:**
```rust
pub struct Terminal {
    term: Arc<FairMutex<Term<ZedListener>>>,
    // ... other fields
}
```

**Event Forwarding via Channel:**
```rust
pub struct ZedListener(pub UnboundedSender<AlacTermEvent>);

impl EventListener for ZedListener {
    fn send_event(&self, event: AlacTermEvent) {
        self.0.unbounded_send(event).ok();
    }
}
```

**Renderable Content Snapshot:**
```rust
pub struct TerminalContent {
    pub cells: Vec<IndexedCell>,
    pub mode: TermMode,
    pub cursor: RenderableCursor,
    pub selection: Option<SelectionRange>,
    pub terminal_bounds: TerminalBounds,
    // ...
}
```

**Bounds Management:**
```rust
pub struct TerminalBounds {
    pub cell_width: Pixels,
    pub line_height: Pixels,
    pub bounds: Bounds<Pixels>,
}

impl Dimensions for TerminalBounds {
    fn screen_lines(&self) -> usize {
        self.num_lines()
    }
    fn columns(&self) -> usize {
        self.num_columns()
    }
}
```

### 2. `zed/crates/terminal_view/` - UI View Layer

| File | Responsibility |
|------|---------------|
| `terminal_view.rs` | TerminalView component, user interaction, focus management, action dispatch |
| `terminal_element.rs` | **Core**: Custom Element implementation using `paint` for efficient rendering |
| `terminal_panel.rs` | Terminal panel management (bottom panel, center panel) |
| `terminal_scrollbar.rs` | Custom scrollbar implementation |
| `persistence.rs` | Terminal session persistence (database storage) |
| `terminal_path_like_target.rs` | Path-like target detection and hover handling |
| `terminal_slash_command.rs` | Slash command integration |

#### Key Design Patterns

**Custom Element Implementation:**
```rust
pub struct TerminalElement {
    terminal: Entity<Terminal>,
    terminal_view: Entity<TerminalView>,
    focus: FocusHandle,
    interactivity: Interactivity,
    // ...
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = LayoutState;

    fn request_layout(&mut self, ...) -> (LayoutId, Self::RequestLayoutState);
    fn prepaint(&mut self, ...) -> Self::PrepaintState;
    fn paint(&mut self, ...);
}
```

**Text Batching for Performance:**
```rust
pub struct BatchedTextRun {
    pub start_point: AlacPoint<i32, i32>,
    pub text: String,
    pub cell_count: usize,
    pub style: TextRun,
    pub font_size: AbsoluteLength,
}

impl BatchedTextRun {
    fn can_append(&self, other_style: &TextRun) -> bool {
        self.style.font == other_style.font
            && self.style.color == other_style.color
            && self.style.background_color == other_style.background_color
            && self.style.underline == other_style.underline
            && self.style.strikethrough == other_style.strikethrough
    }
}
```

**Background Region Merging:**
```rust
struct BackgroundRegion {
    start_line: i32,
    start_col: i32,
    end_line: i32,
    end_col: i32,
    color: Hsla,
}

fn merge_background_regions(regions: Vec<BackgroundRegion>) -> Vec<BackgroundRegion> {
    // Merge adjacent regions with same color to minimize paint_quad calls
}
```

**Layout State for Paint Reuse:**
```rust
pub struct LayoutState {
    hitbox: Hitbox,
    batched_text_runs: Vec<BatchedTextRun>,
    rects: Vec<LayoutRect>,
    cursor: Option<CursorLayout>,
    background_color: Hsla,
    dimensions: TerminalBounds,
    // ...
}
```

## Rendering Pipeline

```
TerminalView::render()
  └─► TerminalElement::new(...)
       ├─► request_layout()
       │    └─► Set size constraints (relative(1.) for full container)
       ├─► prepaint()
       │    ├─► Calculate char dimensions (measure 'm' width)
       │    ├─► Get latest content from Terminal
       │    ├─► layout_grid() - Process cells into BatchedTextRun
       │    │    ├─► Merge adjacent cells with same style
       │    │    └─► Collect background regions
       │    └─► LayoutState { ... }
       └─► paint()
            ├─► Paint background (paint_quad)
            ├─► Paint background rects (merged regions)
            ├─► Paint text batches (shape_line + paint)
            ├─► Paint selection highlights
            └─► Paint cursor (CursorLayout)
```

## Performance Optimizations

### 1. Text Batching
Merge adjacent cells with identical style into single `BatchedTextRun`:
- Reduces `shape_line` calls from N cells to M batches (M << N)
- Only breaks batch on: color change, font change, underline change, line break

### 2. Background Merging
Merge adjacent background regions:
- Horizontal merge: same row, adjacent columns, same color
- Vertical merge: same column span, adjacent rows, same color
- Reduces `paint_quad` calls significantly

### 3. Viewport Clipping
Only render cells within visible bounds:
```rust
let visible_bounds = window.content_mask().bounds;
let intersection = visible_bounds.intersect(&bounds);

// Skip if terminal entirely outside viewport
if intersection.size.height <= px(0.) || intersection.size.width <= px(0.) {
    return (Vec::new(), Vec::new());
}

// Calculate visible row range and filter cells
let rows_above_viewport = ...;
let visible_row_count = ...;
```

### 4. Contrast Adjustment
Ensure text readability with `ensure_minimum_contrast`:
```rust
fn cell_style(...) -> TextRun {
    let mut fg = convert_color(&fg, colors);
    if !is_decorative_character(indexed.c) {
        fg = ensure_minimum_contrast(fg, bg, minimum_contrast);
    }
    // ...
}
```

Skip contrast adjustment for decorative characters (Powerline symbols, box drawing):
```rust
fn is_decorative_character(ch: char) -> bool {
    matches!(ch as u32,
        0x2500..=0x257F  // Box Drawing
        | 0x2580..=0x259F  // Block Elements
        | 0x25A0..=0x25FF  // Geometric Shapes
        | 0xE0B0..=0xE0D7  // Powerline symbols
    )
}
```

## Component Mapping (Zed → Catus)

| Zed Component | Catus Component | Notes |
|--------------|-----------------|-------|
| `Terminal` | `TerminalProvider` | Core terminal logic + PTY management |
| `TerminalView` | `TerminalView` | View layer, interaction handling |
| `TerminalElement` | `TerminalElement` | Custom Element rendering |
| `ZedListener` | `ChannelEventListener` | alacritty event forwarding |
| `TerminalBounds` | Inline calculation | Terminal dimension management |
| `BatchedTextRun` | Simplified batching | Text merging optimization |

## Key Implementation Notes

### Thread Safety
- `Arc<FairMutex<Term>>` for thread-safe terminal state access
- Event loop runs on background thread
- UI updates via `cx.update_entity()` from event loop

### Mouse Handling
- Use `Hitbox` for mouse event hit detection
- Register mouse listeners in `paint()` via `interactivity.on_mouse_down/move/up`
- Support mouse mode for terminal applications (vim, tmux)

### Focus Management
- Track focus via `FocusHandle`
- Cursor visibility depends on focus state
- Blink manager for cursor blinking

### Text System Usage
```rust
// Measure character dimensions
let font_id = text_system.resolve_font(&font);
let advance = text_system.advance(font_id, font_size, 'm')?;

// Shape and paint text
let shaped_line = text_system.shape_line(
    text.into(),
    font_size,
    &[text_run],
    Some(cell_width),
)?;
shaped_line.paint(position, line_height, window, cx)?;
```
