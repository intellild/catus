use gpui::{AppContext, Entity};

use crate::workspace::Workspace;
use crate::workspace_kind::WorkspaceKind;

/// App 管理多个 Workspace，每个 Workspace 拥有独立的 Tab/Pane/终端集合。
///
/// 默认包含一个本地的 `Local` Workspace。
pub struct App {
  pub workspaces: Vec<Entity<Workspace>>,
  pub active_index: Option<usize>,
}

impl App {
  /// 创建一个新的 App，包含一个默认的本地 Workspace。
  pub fn new(cx: &mut gpui::Context<Self>) -> Self {
    let local = cx.new(|cx| Workspace::new(WorkspaceKind::Local, cx));
    Self {
      workspaces: vec![local],
      active_index: Some(0),
    }
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
    self.workspaces.push(workspace.clone());
    self.active_index = Some(self.workspaces.len() - 1);
    Ok(workspace)
  }

  /// 激活指定索引的 Workspace。
  pub fn activate_workspace(&mut self, index: usize) -> bool {
    if index < self.workspaces.len() {
      self.active_index = Some(index);
      true
    } else {
      false
    }
  }

  /// 关闭指定索引的 Workspace。始终保留至少一个 Workspace。
  /// 返回是否执行了关闭。
  pub fn close_workspace(&mut self, index: usize) -> bool {
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
    true
  }
}
