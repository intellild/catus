use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use gpui::{App, AppContext, AsyncApp, Entity, SharedString};
use gpui_component::IconName;
use gpui_component::dock::PanelStyle;

/// Tab ID generator
static TAB_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

pub fn generate_tab_id() -> TabId {
  TabId(TAB_ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}

#[derive(Clone)]
pub struct TabState {
  pub title: SharedString,
  pub icon: IconName,
}

impl TabState {
  pub fn new(
    cx: &mut AsyncApp,
    title: impl Into<SharedString>,
    icon: IconName,
  ) -> Result<Entity<Self>> {
    cx.new(move |_| Self {
      title: title.into(),
      icon,
    })
  }
}

#[derive(Clone)]
pub struct TabItem {
  pub id: TabId,
  pub state: Entity<TabState>,
}

impl TabItem {
  pub fn new(cx: &mut AsyncApp, title: impl Into<SharedString>) -> Result<Self> {
    let state = TabState::new(cx, title, IconName::WindowMaximize)?;

    Ok(Self {
      id: generate_tab_id(),
      state,
    })
  }
}

/// Application state
pub struct AppState {
  pub tabs: Vec<TabItem>,
  pub active_tab_id: Option<TabId>,
}

impl AppState {
  pub fn new(cx: &mut AsyncApp) -> Result<Self> {
    let tabs = vec![TabItem::new(cx, "Welcome")?];
    let active_tab_id = Some(tabs[0].id);

    Ok(Self {
      tabs,
      active_tab_id,
    })
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
