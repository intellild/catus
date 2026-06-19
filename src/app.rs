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

#[cfg(test)]
impl App {
  /// 用预置的 Workspace 列表构造 App，避免测试中启动真实 shell。
  /// active_index 默认指向最后一个 workspace。
  pub(crate) fn with_workspaces(
    workspaces: Vec<Entity<Workspace>>,
    cx: &mut gpui::Context<Self>,
  ) -> Self {
    for ws in &workspaces {
      Self::observe_workspace(ws, cx);
    }
    let active_index = if workspaces.is_empty() {
      None
    } else {
      Some(workspaces.len() - 1)
    };
    Self {
      workspaces,
      active_index,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::workspace::Workspace;
  use crate::workspace_kind::WorkspaceKind;
  use gpui::TestAppContext;

  /// 创建一个使用 FakePty 的 App（单个本地 workspace）。
  fn make_app(cx: &mut TestAppContext) -> Entity<App> {
    let ws = cx.new(|cx| Workspace::new_with_fake_pty(WorkspaceKind::Local, cx));
    cx.new(|cx| App::with_workspaces(vec![ws], cx))
  }

  /// 用 N 个 workspace 构造 App。
  fn make_app_with_n(cx: &mut TestAppContext, n: usize) -> Entity<App> {
    let mut workspaces = Vec::new();
    for i in 0..n {
      let kind = if i == 0 {
        WorkspaceKind::Local
      } else {
        WorkspaceKind::Ssh(format!("ssh host{}", i))
      };
      workspaces.push(cx.new(|cx| Workspace::new_with_fake_pty(kind, cx)));
    }
    cx.new(|cx| App::with_workspaces(workspaces, cx))
  }

  #[gpui::test]
  fn active_workspace_returns_some(cx: &mut TestAppContext) {
    let app = make_app(cx);
    app.read_with(cx, |a, _| {
      assert!(a.active_workspace().is_some());
      assert_eq!(a.active_index, Some(0));
    });
  }

  #[gpui::test]
  fn activate_workspace_within_bounds(cx: &mut TestAppContext) {
    let app = make_app_with_n(cx, 3);
    app.update(cx, |a, cx| {
      assert!(a.activate_workspace(0, cx));
      assert_eq!(a.active_index, Some(0));
      assert!(a.activate_workspace(2, cx));
      assert_eq!(a.active_index, Some(2));
    });
  }

  #[gpui::test]
  fn activate_workspace_out_of_bounds_returns_false(cx: &mut TestAppContext) {
    let app = make_app_with_n(cx, 2);
    app.update(cx, |a, cx| {
      let original = a.active_index;
      assert!(!a.activate_workspace(5, cx));
      assert_eq!(a.active_index, original);
    });
  }

  #[gpui::test]
  fn close_workspace_refuses_when_only_one(cx: &mut TestAppContext) {
    let app = make_app(cx);
    app.update(cx, |a, cx| {
      assert!(!a.close_workspace(0, cx));
      assert_eq!(a.workspaces.len(), 1);
    });
  }

  #[gpui::test]
  fn close_workspace_out_of_bounds_returns_false(cx: &mut TestAppContext) {
    let app = make_app_with_n(cx, 2);
    app.update(cx, |a, cx| {
      assert!(!a.close_workspace(9, cx));
      assert_eq!(a.workspaces.len(), 2);
    });
  }

  #[gpui::test]
  fn close_active_workspace_falls_back(cx: &mut TestAppContext) {
    let app = make_app_with_n(cx, 3);
    // with_workspaces 默认激活最后一个（index 2）
    app.update(cx, |a, cx| {
      assert_eq!(a.active_index, Some(2));
      assert!(a.close_workspace(2, cx));
      // 关闭激活的，回退到 min(2, len-1=1) = 1
      assert_eq!(a.workspaces.len(), 2);
      assert_eq!(a.active_index, Some(1));
    });
  }

  #[gpui::test]
  fn close_workspace_before_active_shifts_index(cx: &mut TestAppContext) {
    let app = make_app_with_n(cx, 3);
    app.update(cx, |a, cx| {
      a.activate_workspace(2, cx);
      // 关闭 index 0（在激活之前），激活应变为 1
      assert!(a.close_workspace(0, cx));
      assert_eq!(a.workspaces.len(), 2);
      assert_eq!(a.active_index, Some(1));
    });
  }

  #[gpui::test]
  fn close_workspace_after_active_keeps_index(cx: &mut TestAppContext) {
    let app = make_app_with_n(cx, 3);
    app.update(cx, |a, cx| {
      a.activate_workspace(0, cx);
      // 关闭 index 2（在激活之后），激活保持 0
      assert!(a.close_workspace(2, cx));
      assert_eq!(a.workspaces.len(), 2);
      assert_eq!(a.active_index, Some(0));
    });
  }

  #[gpui::test]
  fn close_workspace_until_one_remains(cx: &mut TestAppContext) {
    let app = make_app_with_n(cx, 3);
    app.update(cx, |a, cx| {
      assert!(a.close_workspace(2, cx));
      assert_eq!(a.workspaces.len(), 2);
      // 只剩两个时，再关闭仍可（保留至少一个）
      assert!(a.close_workspace(1, cx));
      assert_eq!(a.workspaces.len(), 1);
      // 此时不能再关闭
      assert!(!a.close_workspace(0, cx));
    });
  }
}
