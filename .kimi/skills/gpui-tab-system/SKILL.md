# GPUI Tab System Development

## Overview

This skill provides patterns and best practices for implementing a tab system in GPUI applications, similar to modern browsers and code editors.

## Use Cases

- Multi-view applications (file explorer + terminal + editor)
- Browser-like tabbed interfaces
- IDE workspace management
- Document editors with multiple files

## Core Architecture

### 1. Tab Identity

```rust
use std::sync::atomic::{AtomicU64, Ordering};

static TAB_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

pub fn generate_tab_id() -> TabId {
    TabId(TAB_ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}
```

### 2. Tab Kind Enumeration

```rust
pub enum TabKind {
    Explorer { path: PathBuf },
    Terminal { working_dir: PathBuf },
    SshTerminal { host: String, port: u16, username: String, working_dir: PathBuf },
}
```

### 3. Tab Manager

```rust
pub struct TabManager {
    tabs: Vec<Tab>,
    active_tab_id: Option<TabId>,
}

impl TabManager {
    pub fn add_tab(&mut self, kind: TabKind) -> TabId { ... }
    pub fn close_tab(&mut self, id: TabId) -> bool { ... }
    pub fn activate_tab(&mut self, id: TabId) -> bool { ... }
    pub fn next_tab(&mut self) { ... }
}
```

## Best Practices

1. Keep Tab State separate from View State
2. Lazy View Creation - create views only when needed
3. Proper Cleanup when closing tabs
4. Clone data when iterating to avoid borrow issues
5. Required imports: FluentBuilder, InteractiveElement, Sizable, Selectable, ButtonVariants

## Common Pitfalls

1. Forgetting `cx.notify()` after state modification
2. Wrong IconName values
3. Missing Trait imports
4. Borrow checker issues when iterating and modifying

## Example

See Catus project:
- `src/tab.rs` - Tab system
- `src/workspace.rs` - Workspace with TabBar
- `src/terminal_view.rs` - Terminal view
