use gpui::*;
use gpui_component::ActiveTheme;

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
    let sidebar = cx.new(|_| WorkspaceSidebar::new(app.clone()));
    let title_bar_tabs = cx.new(|_| TitleBarTabs::new(app.clone()));
    let title_bar = cx.new(|_| TitleBarRoot::new(title_bar_tabs.into()));
    Self {
      app,
      sidebar,
      title_bar,
    }
  }
}

impl Render for MainView {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
  }
}
