use gpui::{AppContext, Entity};

use crate::workspace::Workspace;
use crate::workspace_kind::WorkspaceKind;

/// App 管理多个 Workspace，每个 Workspace 拥有独立的 Tab/Pane/终端集合。
///
/// 默认包含一个本地的 `Local` Workspace。
///
/// 当任何 Workspace 变更（tab 切换、终端标题变化等）时，App 通过
/// `cx.notify()` 通知所有观察者（MainView、WorkspaceSidebar、TitleBarTabs）。
pub struct App {
  pub workspaces: Vec<Entity<Workspace>>,
  pub active_index: Option<usize>,
}

impl App {
  /// 创建一个新的 App，包含一个默认的本地 Workspace。
  pub fn new(cx: &mut gpui::Context<Self>) -> Self {
    let local = cx.new(|cx| Workspace::new(WorkspaceKind::Local, cx));
    Self::observe_workspace(&local, cx);
    Self {
      workspaces: vec![local],
      active_index: Some(0),
    }
  }

  /// 观察 Workspace 的变化，转发为 App 的 notify。
  fn observe_workspace(ws: &Entity<Workspace>, cx: &mut gpui::Context<Self>) {
    cx.observe(ws, |_, _, cx| {
      cx.notify();
    })
    .detach();
  }

  /// 当前激活的 Workspace 实体。
  pub fn active_workspace(&self) -> Option<&Entity<Workspace>> {
    self.active_index.and_then(|i| self.workspaces.get(i))
  }

  /// 添加一个 Workspace 并设为激活。
  pub fn add_workspace(
    &mut self,
    kind: WorkspaceKind,
    cx: &mut gpui::Context<Self>,
  ) -> Result<Entity<Workspace>, String> {
    let workspace = cx.new(|cx| Workspace::new(kind, cx));

    // 终端创建失败时拒绝添加空 workspace
    if workspace.read(cx).tabs.is_empty() {
      return Err("Failed to create terminal for workspace".to_string());
    }

    Self::observe_workspace(&workspace, cx);
    self.workspaces.push(workspace.clone());
    self.active_index = Some(self.workspaces.len() - 1);
    cx.notify();
    Ok(workspace)
  }

  /// 激活指定索引的 Workspace。
  pub fn activate_workspace(&mut self, index: usize, cx: &mut gpui::Context<Self>) -> bool {
    if index < self.workspaces.len() {
      self.active_index = Some(index);
      cx.notify();
      true
    } else {
      false
    }
  }

  /// 关闭指定索引的 Workspace。始终保留至少一个 Workspace。
  /// 返回是否执行了关闭。
  pub fn close_workspace(&mut self, index: usize, cx: &mut gpui::Context<Self>) -> bool {
    if self.workspaces.len() <= 1 || index >= self.workspaces.len() {
      return false;
    }
    self.workspaces.remove(index);
    // 调整激活索引：优先保持原索引，越界则回退到上一个。
    self.active_index = match self.active_index {
      Some(active) if active == index => Some(index.min(self.workspaces.len() - 1)),
      Some(active) if active > index => Some(active - 1),
      other => other,
    };
    cx.notify();
    true
  }
}
