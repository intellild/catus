use gpui::*;
use gpui_component::{ActiveTheme, Root};

use crate::app::App;
use crate::sidebar::WorkspaceSidebar;
use crate::title_bar::{TitleBarRoot, TitleBarTabs};
use crate::workspace::Workspace;

pub struct MainView {
  app: Entity<App>,
  sidebar: Entity<WorkspaceSidebar>,
  title_bar: Entity<TitleBarRoot>,
}

impl MainView {
  pub fn new(app: Entity<App>, cx: &mut Context<Self>) -> Self {
    // App 变更（workspace 切换、tab 切换、终端标题变化等）→ 重新渲染
    cx.observe(&app, |_, _, cx| {
      cx.notify();
    })
    .detach();

    let sidebar = cx.new(|cx| WorkspaceSidebar::new(app.clone(), cx));
    let title_bar_tabs = cx.new(|cx| TitleBarTabs::new(app.clone(), cx));
    let title_bar = cx.new(|_cx| TitleBarRoot::new(title_bar_tabs.into()));
    Self {
      app,
      sidebar,
      title_bar,
    }
  }
}

impl Render for MainView {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    let app = self.app.read(cx);
    let active_workspace: Option<&Entity<Workspace>> = app.active_workspace();

    let pane_group = active_workspace
      .and_then(|ws| ws.read(cx).active_tab())
      .map(|tab| tab.pane_group.clone());

    div()
      .size_full()
      .flex()
      .flex_col()
      .bg(theme.background)
      .text_color(theme.foreground)
      .child(self.title_bar.clone())
      .child(
        div()
          .flex_1()
          .min_h_0()
          .flex()
          .flex_row()
          // 左侧 workspace 侧边栏
          .child(self.sidebar.clone())
          // 右侧：当前 workspace 的 pane 区
          .child(div().flex_1().min_w_0().child(pane_group.map_or_else(
            || div().size_full().child("No active workspace"),
            |pg| div().size_full().child(pg),
          ))),
      )
      // 「添加 Workspace」对话框与错误通知由 Root 统一管理，
      // 必须在 Root 的子视图里渲染对应图层，否则 open_dialog 不可见。
      .children(Root::render_dialog_layer(window, cx))
      .children(Root::render_notification_layer(window, cx))
  }
}
