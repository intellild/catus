use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use gpui::{AppContext, Entity, SharedString};
use gpui_component::IconName;

use crate::pane::PaneGroup;
use crate::terminal::{LocalPty, Terminal, TerminalSize, TerminalView, TerminalViewEvent};
use crate::workspace_kind::WorkspaceKind;

static TAB_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

pub fn generate_tab_id() -> TabId {
  TabId(TAB_ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}

#[derive(Clone)]
pub struct TabItem {
  pub id: TabId,
  pub pane_group: Entity<PaneGroup>,
}

pub struct Workspace {
  pub kind: WorkspaceKind,
  pub tabs: Vec<TabItem>,
  pub active_tab_id: Option<TabId>,
}

impl Workspace {
  pub fn new(kind: WorkspaceKind, cx: &mut gpui::Context<Self>) -> Self {
    match Self::make_tab(cx, &kind) {
      Ok(tab) => {
        let active_tab_id = Some(tab.id);
        Self {
          kind,
          tabs: vec![tab],
          active_tab_id,
        }
      }
      Err(e) => {
        eprintln!("Failed to create default terminal: {}", e);
        Self {
          kind,
          tabs: vec![],
          active_tab_id: None,
        }
      }
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

  fn make_tab(cx: &mut gpui::Context<Self>, kind: &WorkspaceKind) -> Result<TabItem, String> {
    let terminal_view = Self::create_terminal_view(cx, kind)?;
    let workspace_handle = cx.entity().clone();
    let pane_group = cx.new(|cx| PaneGroup::new(workspace_handle, terminal_view, cx));
    Ok(TabItem {
      id: generate_tab_id(),
      pane_group,
    })
  }

  pub fn add_tab(&mut self, tab: TabItem, cx: &mut gpui::Context<Self>) -> TabId {
    let id = tab.id;
    self.tabs.push(tab);
    self.active_tab_id = Some(id);
    cx.notify();
    id
  }

  pub fn close_tab(&mut self, id: TabId, cx: &mut gpui::Context<Self>) -> bool {
    if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
      self.tabs.remove(index);
      if self.active_tab_id == Some(id) {
        self.active_tab_id = self.tabs.get(index.saturating_sub(1)).map(|t| t.id);
      }
      cx.notify();
      return true;
    }
    false
  }

  pub fn activate_tab(&mut self, id: TabId, cx: &mut gpui::Context<Self>) -> bool {
    if self.tabs.iter().any(|t| t.id == id) {
      self.active_tab_id = Some(id);
      cx.notify();
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
    let tab = Self::make_tab(cx, &self.kind)?;
    Ok(self.add_tab(tab, cx))
  }

  /// 创建 Terminal + TerminalView 实体，并订阅 TerminalViewEvent。
  ///
  /// 当终端标题变更或子进程退出时，通过 `cx.notify()` 触发 Workspace
  /// 重新渲染，进而通知 App → MainView / TitleBarTabs。
  pub fn create_terminal_view(
    cx: &mut gpui::Context<Self>,
    kind: &WorkspaceKind,
  ) -> Result<Entity<TerminalView>, String> {
    let pty = LocalPty::new(TerminalSize::default_size(), kind.command())
      .map_err(|e| format!("Failed to create PTY: {}", e))?;
    let terminal =
      cx.new(|cx| Terminal::new(Arc::new(pty), cx).expect("Failed to create terminal"));
    let view = cx.new(|cx| TerminalView::new(terminal, cx));

    cx.subscribe(&view, |_, _, event: &TerminalViewEvent, cx| {
      if matches!(
        event,
        TerminalViewEvent::TitleChanged | TerminalViewEvent::Closed
      ) {
        cx.notify();
      }
    })
    .detach();

    Ok(view)
  }
}
