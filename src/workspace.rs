use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use gpui::{AppContext, Entity};
use gpui_component::IconName;

use crate::id::ID;
use crate::terminal::{LocalPty, Terminal, TerminalSize};

static TAB_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

pub fn generate_tab_id() -> TabId {
  TabId(TAB_ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}

#[derive(Clone)]
pub enum TabType {
  Terminal(ID<Terminal>),
  Sftp,
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
  pub tab_type: TabType,
}

impl TabItem {
  pub fn new(cx: &mut gpui::Context<Workspace>, icon: IconName, tab_type: TabType) -> Self {
    let state = cx.new(|_cx| TabState::new(icon));

    Self {
      id: generate_tab_id(),
      state,
      tab_type,
    }
  }

  pub fn new_terminal(cx: &mut gpui::Context<Workspace>, terminal_id: ID<Terminal>) -> Self {
    Self {
      id: generate_tab_id(),
      state: cx.new(|_cx| TabState::new(IconName::File)),
      tab_type: TabType::Terminal(terminal_id),
    }
  }

  pub fn new_sftp(cx: &mut gpui::Context<Workspace>) -> Self {
    Self {
      id: generate_tab_id(),
      state: cx.new(|_cx| TabState::new(IconName::Folder)),
      tab_type: TabType::Sftp,
    }
  }
}

pub struct Workspace {
  pub tabs: Vec<TabItem>,
  pub active_tab_id: Option<TabId>,
  terminals: HashMap<ID<Terminal>, Entity<Terminal>>,
}

impl Workspace {
  pub fn new(cx: &mut gpui::Context<Self>) -> Self {
    let mut terminals = HashMap::new();
    let terminal_id = ID::<Terminal>::generate();

    let tab = match Self::create_terminal_entity(cx, terminal_id, &mut terminals, 24, 80) {
      Ok(()) => TabItem::new_terminal(cx, terminal_id),
      Err(e) => {
        eprintln!("Failed to create default terminal tab: {}", e);
        return Self {
          tabs: vec![],
          active_tab_id: None,
          terminals,
        };
      }
    };

    let active_tab_id = Some(tab.id);

    Self {
      tabs: vec![tab],
      active_tab_id,
      terminals,
    }
  }

  pub fn add_tab(&mut self, tab: TabItem) -> TabId {
    let id = tab.id;
    self.tabs.push(tab);
    self.active_tab_id = Some(id);
    id
  }

  pub fn close_tab(&mut self, id: TabId) -> bool {
    if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
      let tab = &self.tabs[index];
      if let TabType::Terminal(terminal_id) = &tab.tab_type {
        self.terminals.remove(terminal_id);
      }

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
    let terminal_id = ID::<Terminal>::generate();
    Self::create_terminal_entity(cx, terminal_id, &mut self.terminals, 24, 80)?;
    let tab = TabItem::new_terminal(cx, terminal_id);
    Ok(self.add_tab(tab))
  }

  pub fn add_sftp_tab(&mut self, cx: &mut gpui::Context<Self>) -> TabId {
    let tab = TabItem::new_sftp(cx);
    self.add_tab(tab)
  }

  pub fn terminal(&self, id: ID<Terminal>) -> Option<&Entity<Terminal>> {
    self.terminals.get(&id)
  }

  fn create_terminal_entity(
    cx: &mut gpui::Context<Self>,
    terminal_id: ID<Terminal>,
    terminals: &mut HashMap<ID<Terminal>, Entity<Terminal>>,
    rows: usize,
    cols: usize,
  ) -> Result<(), String> {
    let size = TerminalSize::new(rows as u16, cols as u16, 0, 0);
    let pty = LocalPty::new(size, None).map_err(|e| format!("Failed to create PTY: {}", e))?;
    let terminal_entity =
      cx.new(|cx| Terminal::new(Arc::new(pty), cx).expect("Failed to create terminal"));
    terminals.insert(terminal_id, terminal_entity);
    Ok(())
  }
}
