use std::sync::atomic::{AtomicU64, Ordering};

use gpui::{AppContext, Entity, SharedString};
use gpui_component::IconName;

use crate::terminal::provider::TerminalProvider;

/// Tab ID generator
static TAB_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

pub fn generate_tab_id() -> TabId {
  TabId(TAB_ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Tab 类型
#[derive(Clone)]
pub enum TabType {
  /// 终端 Tab
  Terminal(Entity<TerminalProvider>),
  /// SFTP Tab (TODO: 实现 SFTP)
  Sftp,
}

#[derive(Clone)]
pub struct TabState {
  pub title: SharedString,
  pub icon: IconName,
}

impl TabState {
  pub fn new(
    title: impl Into<SharedString>,
    icon: IconName,
  ) -> Self {
    Self {
      title: title.into(),
      icon,
    }
  }
}

#[derive(Clone)]
pub struct TabItem {
  pub id: TabId,
  pub state: Entity<TabState>,
  pub tab_type: TabType,
}

impl TabItem {
  /// 创建一个新的 TabItem
  pub fn new(
    cx: &mut gpui::Context<AppState>,
    title: impl Into<SharedString>,
    icon: IconName,
    tab_type: TabType,
  ) -> Self {
    let state = cx.new(|_cx| TabState::new(title, icon));

    Self {
      id: generate_tab_id(),
      state,
      tab_type,
    }
  }

  /// 创建一个新的 Terminal Tab
  pub fn new_terminal(
    cx: &mut gpui::Context<AppState>,
    rows: usize,
    cols: usize,
  ) -> Self {
    // 创建 TerminalProvider
    let (command_tx, update_rx, event_rx) = TerminalProvider::setup(rows, cols);
    
    let provider = cx.new(|_cx| TerminalProvider {
      command_tx,
      update_rx,
      event_rx,
    });
    
    Self {
      id: generate_tab_id(),
      state: cx.new(|_cx| TabState::new("Terminal", IconName::File)),
      tab_type: TabType::Terminal(provider),
    }
  }

  /// 创建一个新的 SFTP Tab (TODO: 实现)
  pub fn new_sftp(cx: &mut gpui::Context<AppState>) -> Self {
    Self {
      id: generate_tab_id(),
      state: cx.new(|_cx| TabState::new("SFTP", IconName::Folder)),
      tab_type: TabType::Sftp,
    }
  }
}

/// Application state
pub struct AppState {
  pub tabs: Vec<TabItem>,
  pub active_tab_id: Option<TabId>,
}

impl AppState {
  pub fn new(cx: &mut gpui::Context<Self>) -> Self {
    // 创建一个默认的 Terminal Tab
    let tabs = vec![TabItem::new_terminal(cx, 24, 80)];
    let active_tab_id = Some(tabs[0].id);

    Self {
      tabs,
      active_tab_id,
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
      self.tabs.remove(index);

      // Update active tab if needed
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
}
