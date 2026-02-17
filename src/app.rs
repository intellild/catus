use gpui::{AppContext, Entity};

use crate::workspace::Workspace;

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
