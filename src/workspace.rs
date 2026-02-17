use gpui::{AppContext, Entity};

use crate::app_state::{AppState, TabId, TabItem};

/// Workspace 代表一个工作区，包含多个 Tab
pub struct Workspace {
  /// Workspace 的 AppState，管理所有 Tab
  pub state: Entity<AppState>,
}

impl Workspace {
  /// 创建一个新的 Workspace
  /// 如果没有 Tab，会自动创建一个默认的 Terminal Tab
  pub fn new(cx: &mut gpui::Context<Self>) -> Self {
    let state = cx.new(|cx| AppState::new(cx));

    Self { state }
  }

  /// 添加一个新的 Terminal Tab
  pub fn add_terminal_tab(&mut self, cx: &mut gpui::Context<Self>) -> TabId {
    let tab = self.state.update(cx, |_state, cx| {
      TabItem::new_terminal(cx, 24, 80)
    });
    
    let id = tab.id;
    self.state.update(cx, |state, _cx| {
      state.add_tab(tab);
    });
    
    id
  }

  /// 添加一个新的 SFTP Tab
  pub fn add_sftp_tab(&mut self, cx: &mut gpui::Context<Self>) -> TabId {
    let tab = self.state.update(cx, |_state, cx| {
      TabItem::new_sftp(cx)
    });
    
    let id = tab.id;
    self.state.update(cx, |state, _cx| {
      state.add_tab(tab);
    });
    
    id
  }

  /// 关闭指定的 Tab
  pub fn close_tab(&mut self, id: TabId, cx: &mut gpui::Context<Self>) -> bool {
    self.state.update(cx, |state, _cx| state.close_tab(id))
  }

  /// 激活指定的 Tab
  pub fn activate_tab(&mut self, id: TabId, cx: &mut gpui::Context<Self>) -> bool {
    self.state.update(cx, |state, _cx| state.activate_tab(id))
  }

  /// 获取当前激活的 Tab ID
  pub fn active_tab_id(&self, cx: &gpui::App) -> Option<TabId> {
    self.state.read(cx).active_tab_id
  }
}

/// App 可以管理多个 Workspace，目前简化实现只支持一个
pub struct App {
  pub workspace: Entity<Workspace>,
}

impl App {
  /// 创建一个新的 App，包含一个默认的 Workspace
  pub fn new(cx: &mut gpui::Context<Self>) -> Self {
    let workspace = cx.new(|cx| Workspace::new(cx));

    Self { workspace }
  }

  /// 获取 Workspace 实体
  pub fn workspace(&self) -> &Entity<Workspace> {
    &self.workspace
  }
}
