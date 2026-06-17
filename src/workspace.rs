use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use gpui::{App, AppContext, Entity, SharedString};
use gpui_component::IconName;

use crate::id::ID;
use crate::pane::PaneGroup;
use crate::terminal::{LocalPty, Terminal, TerminalSize, TerminalView};
use crate::workspace_kind::WorkspaceKind;

static TAB_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

pub fn generate_tab_id() -> TabId {
  TabId(TAB_ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}

#[derive(Clone)]
pub struct TabState {
  pub icon: IconName,
}

impl TabState {
  pub fn new(icon: IconName) -> Self {
    Self { icon }
  }
}

#[derive(Clone)]
pub struct TabItem {
  pub id: TabId,
  pub state: Entity<TabState>,
  pub pane_group: Entity<PaneGroup>,
}

pub struct Workspace {
  pub kind: WorkspaceKind,
  pub tabs: Vec<TabItem>,
  pub active_tab_id: Option<TabId>,
  terminals: HashMap<ID<Terminal>, Entity<Terminal>>,
}

impl Workspace {
  pub fn new(kind: WorkspaceKind, cx: &mut gpui::Context<Self>) -> Self {
    let mut terminals = HashMap::new();
    let terminal_id = ID::<Terminal>::generate();

    let terminal =
      match Self::create_terminal_entity(cx, terminal_id, &mut terminals, 24, 80, &kind) {
        Ok(t) => t,
        Err(e) => {
          eprintln!("Failed to create default terminal: {}", e);
          return Self {
            kind,
            tabs: vec![],
            active_tab_id: None,
            terminals,
          };
        }
      };

    let tab = Self::make_tab(cx, terminal, &terminals);
    let active_tab_id = Some(tab.id);

    Self {
      kind,
      tabs: vec![tab],
      active_tab_id,
      terminals,
    }
  }

  /// 侧边栏展示用的名称。
  pub fn display_name(&self) -> SharedString {
    self.kind.display_name()
  }

  /// 侧边栏展示用的图标。
  pub fn icon(&self) -> IconName {
    self.kind.icon()
  }

  fn make_tab(
    cx: &mut gpui::Context<Self>,
    terminal: Entity<Terminal>,
    _terminals: &HashMap<ID<Terminal>, Entity<Terminal>>,
  ) -> TabItem {
    let terminal_view = cx.new(|cx| TerminalView::new(terminal, cx));
    let workspace_handle = cx.entity().clone();
    let pane_group = cx.new(|cx| PaneGroup::new(workspace_handle, terminal_view, cx));
    TabItem {
      id: generate_tab_id(),
      state: cx.new(|_cx| TabState::new(IconName::File)),
      pane_group,
    }
  }

  pub fn create_terminal(&mut self, cx: &mut App) -> Result<Entity<Terminal>, String> {
    let terminal_id = ID::<Terminal>::generate();
    Self::create_terminal_entity(cx, terminal_id, &mut self.terminals, 24, 80, &self.kind)
  }

  pub fn add_tab(&mut self, tab: TabItem) -> TabId {
    let id = tab.id;
    self.tabs.push(tab);
    self.active_tab_id = Some(id);
    id
  }

  pub fn close_tab(&mut self, id: TabId) -> bool {
    if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
      self.tabs.remove(index);
      if self.active_tab_id == Some(id) {
        self.active_tab_id = self.tabs.get(index.saturating_sub(1)).map(|t| t.id);
      }
      return true;
    }
    false
  }

  pub fn activate_tab(&mut self, id: TabId) -> bool {
    if self.tabs.iter().any(|t| t.id == id) {
      self.active_tab_id = Some(id);
      true
    } else {
      false
    }
  }

  pub fn active_tab(&self) -> Option<&TabItem> {
    self
      .active_tab_id
      .and_then(|id| self.tabs.iter().find(|t| t.id == id))
  }

  pub fn active_index(&self) -> Option<usize> {
    self
      .active_tab_id
      .and_then(|id| self.tabs.iter().position(|t| t.id == id))
  }

  pub fn add_terminal_tab(&mut self, cx: &mut gpui::Context<Self>) -> Result<TabId, String> {
    let terminal = self.create_terminal(cx)?;
    let tab = Self::make_tab(cx, terminal, &self.terminals);
    Ok(self.add_tab(tab))
  }

  fn create_terminal_entity(
    cx: &mut App,
    terminal_id: ID<Terminal>,
    terminals: &mut HashMap<ID<Terminal>, Entity<Terminal>>,
    rows: usize,
    cols: usize,
    kind: &WorkspaceKind,
  ) -> Result<Entity<Terminal>, String> {
    let size = TerminalSize::new(rows as u16, cols as u16, 0, 0);
    let pty =
      LocalPty::new(size, kind.command()).map_err(|e| format!("Failed to create PTY: {}", e))?;
    let terminal_entity =
      cx.new(|cx| Terminal::new(Arc::new(pty), cx).expect("Failed to create terminal"));
    terminals.insert(terminal_id, terminal_entity.clone());
    Ok(terminal_entity)
  }
}
