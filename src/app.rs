use std::path::PathBuf;

use gpui::{AppContext, Entity};
use tracing::{info, warn};

use crate::config::{AppConfig, WorkspaceConfig, default_config_path};
use crate::workspace::Workspace;
use crate::workspace_kind::WorkspaceKind;

/// App 管理多个 Workspace，每个 Workspace 拥有独立的 Tab/Pane/终端集合。
///
/// 启动时从 `~/.config/catus/config.toml` 读取 workspace 列表并激活第一个；
/// 新增/关闭 workspace 时把当前列表整体写回配置文件。
///
/// 当任何 Workspace 变更（tab 切换、终端标题变化等）时，App 通过
/// `cx.notify()` 通知所有观察者（MainView、WorkspaceSidebar、TitleBarTabs）。
pub struct App {
  pub workspaces: Vec<Entity<Workspace>>,
  pub active_index: Option<usize>,
  /// 配置文件路径；`None`（测试构造）时增删 workspace 不持久化。
  config_path: Option<PathBuf>,
}

impl App {
  /// 从默认配置文件创建 App。
  pub fn new(cx: &mut gpui::Context<Self>) -> Self {
    Self::from_config_with(default_config_path(), cx, |kind, cx| {
      cx.new(|cx| Workspace::new(kind, cx))
    })
  }

  /// 从配置文件创建 workspace 列表：第一个条目激活。
  ///
  /// 某条目终端创建失败时跳过该条目；全部失败或配置为空时回退到默认本地
  /// workspace，保证至少有一个图标和一个 tab。
  fn from_config_with(
    path: PathBuf,
    cx: &mut gpui::Context<Self>,
    spawn_workspace: impl Fn(WorkspaceKind, &mut gpui::App) -> Entity<Workspace>,
  ) -> Self {
    let config = AppConfig::load(&path);
    let mut workspaces = Vec::new();
    for entry in &config.workspaces {
      let kind = WorkspaceKind::from_command_line(&entry.command);
      let workspace = spawn_workspace(kind, cx);
      if workspace.read(cx).tabs.is_empty() {
        warn!(
          target: "catus",
          "skipping workspace command {:?}: no terminal created",
          entry.command
        );
        continue;
      }
      Self::observe_workspace(&workspace, cx);
      workspaces.push(workspace);
    }
    if workspaces.is_empty() {
      let workspace = spawn_workspace(WorkspaceKind::from_command_line(""), cx);
      Self::observe_workspace(&workspace, cx);
      workspaces.push(workspace);
    }
    let active_index = if workspaces.is_empty() { None } else { Some(0) };
    Self {
      workspaces,
      active_index,
      config_path: Some(path),
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

  /// 添加一个 Workspace 并设为激活，成功时持久化到配置文件。
  pub fn add_workspace(
    &mut self,
    kind: WorkspaceKind,
    cx: &mut gpui::Context<Self>,
  ) -> Result<Entity<Workspace>, String> {
    self.add_workspace_inner(kind, cx, |kind, cx| cx.new(|cx| Workspace::new(kind, cx)))
  }

  fn add_workspace_inner(
    &mut self,
    kind: WorkspaceKind,
    cx: &mut gpui::Context<Self>,
    spawn_workspace: impl Fn(WorkspaceKind, &mut gpui::App) -> Entity<Workspace>,
  ) -> Result<Entity<Workspace>, String> {
    let workspace = spawn_workspace(kind, cx);

    // 终端创建失败时拒绝添加空 workspace
    if workspace.read(cx).tabs.is_empty() {
      return Err("Failed to create terminal for workspace".to_string());
    }

    Self::observe_workspace(&workspace, cx);
    self.workspaces.push(workspace.clone());
    self.active_index = Some(self.workspaces.len() - 1);
    info!(target: "catus", "added workspace (index {}, total {})", self.workspaces.len() - 1, self.workspaces.len());
    self.persist_config(cx);
    cx.notify();
    Ok(workspace)
  }

  /// 激活指定索引的 Workspace。
  pub fn activate_workspace(&mut self, index: usize, cx: &mut gpui::Context<Self>) -> bool {
    if index < self.workspaces.len() {
      self.active_index = Some(index);
      info!(target: "catus", "activated workspace index {}", index);
      cx.notify();
      true
    } else {
      false
    }
  }

  /// 关闭指定索引的 Workspace 并从配置文件移除。始终保留至少一个 Workspace。
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
    info!(target: "catus", "closed workspace index {} (remaining {})", index, self.workspaces.len());
    self.persist_config(cx);
    cx.notify();
    true
  }

  /// 把当前 workspace 列表（启动命令）整体写回配置文件。
  fn persist_config(&self, cx: &gpui::App) {
    let Some(path) = &self.config_path else {
      return;
    };
    let config = AppConfig {
      workspaces: self
        .workspaces
        .iter()
        .map(|ws| WorkspaceConfig {
          command: ws.read(cx).kind.to_command_line(),
        })
        .collect(),
    };
    if let Err(e) = config.save(path) {
      warn!(target: "catus", "failed to save config {}: {}", path.display(), e);
    }
  }
}

#[cfg(test)]
impl App {
  /// 用预置的 Workspace 列表构造 App，避免测试中启动真实 shell。
  /// active_index 默认指向最后一个 workspace；不带配置持久化。
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
      config_path: None,
    }
  }

  /// 从配置文件创建 App，但 workspace 用 FakePty，避免测试启动真实 shell。
  pub(crate) fn from_config_with_fake_pty(path: PathBuf, cx: &mut gpui::Context<Self>) -> Self {
    Self::from_config_with(path, cx, |kind, cx| {
      cx.new(|cx| {
        Workspace::new_with_pty(
          kind,
          std::sync::Arc::new(crate::terminal::FakePty::new()),
          cx,
        )
      })
    })
  }

  /// 添加使用 FakePty 的 workspace，用于测试配置持久化。
  pub(crate) fn add_workspace_with_fake_pty(
    &mut self,
    kind: WorkspaceKind,
    cx: &mut gpui::Context<Self>,
  ) -> Result<Entity<Workspace>, String> {
    self.add_workspace_inner(kind, cx, |kind, cx| {
      cx.new(|cx| {
        Workspace::new_with_pty(
          kind,
          std::sync::Arc::new(crate::terminal::FakePty::new()),
          cx,
        )
      })
    })
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

  /// 每个测试独立的临时配置路径，避免并行测试互相干扰。
  fn temp_config_path(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("catus-app-test-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("config.toml")
  }

  fn workspace_command(entry_command: &str) -> WorkspaceConfig {
    WorkspaceConfig {
      command: entry_command.to_string(),
    }
  }

  #[gpui::test]
  fn startup_loads_workspaces_from_config_and_activates_first(cx: &mut TestAppContext) {
    let path = temp_config_path("startup");
    AppConfig {
      workspaces: vec![workspace_command(""), workspace_command("ssh host1")],
    }
    .save(&path)
    .unwrap();
    let app = cx.new(|cx| App::from_config_with_fake_pty(path.clone(), cx));
    app.read_with(cx, |app, cx| {
      assert_eq!(app.workspaces.len(), 2);
      assert_eq!(app.active_index, Some(0));
      assert!(matches!(
        app.workspaces[0].read(cx).kind,
        WorkspaceKind::Local
      ));
      assert!(matches!(
        app.workspaces[1].read(cx).kind,
        WorkspaceKind::Ssh(_)
      ));
    });
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
  }

  #[gpui::test]
  fn missing_config_bootstraps_default_single_workspace(cx: &mut TestAppContext) {
    let path = temp_config_path("bootstrap");
    let app = cx.new(|cx| App::from_config_with_fake_pty(path.clone(), cx));
    app.read_with(cx, |app, _| {
      assert_eq!(app.workspaces.len(), 1);
      assert_eq!(app.active_index, Some(0));
    });
    // 首次启动写入默认配置文件
    assert_eq!(AppConfig::load(&path), AppConfig::default());
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
  }

  #[gpui::test]
  fn empty_config_falls_back_to_single_workspace(cx: &mut TestAppContext) {
    let path = temp_config_path("empty");
    AppConfig { workspaces: vec![] }.save(&path).unwrap();
    let app = cx.new(|cx| App::from_config_with_fake_pty(path.clone(), cx));
    app.read_with(cx, |app, cx| {
      assert_eq!(app.workspaces.len(), 1);
      assert!(matches!(
        app.workspaces[0].read(cx).kind,
        WorkspaceKind::Local
      ));
    });
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
  }

  #[gpui::test]
  fn add_workspace_persists_command_to_config(cx: &mut TestAppContext) {
    let path = temp_config_path("add");
    let app = cx.new(|cx| App::from_config_with_fake_pty(path.clone(), cx));
    app.update(cx, |app, cx| {
      app
        .add_workspace_with_fake_pty(WorkspaceKind::Ssh("ssh user@host".into()), cx)
        .unwrap();
    });
    assert_eq!(
      AppConfig::load(&path).workspaces,
      vec![workspace_command(""), workspace_command("ssh user@host")],
    );
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
  }

  #[gpui::test]
  fn close_workspace_persists_removal_to_config(cx: &mut TestAppContext) {
    let path = temp_config_path("close");
    AppConfig {
      workspaces: vec![workspace_command(""), workspace_command("ssh host1")],
    }
    .save(&path)
    .unwrap();
    let app = cx.new(|cx| App::from_config_with_fake_pty(path.clone(), cx));
    app.update(cx, |app, cx| {
      assert!(app.close_workspace(1, cx));
    });
    assert_eq!(
      AppConfig::load(&path).workspaces,
      vec![workspace_command("")],
    );
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
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
