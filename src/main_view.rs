use gpui::*;

use crate::title_bar::{TitleBarRoot, TitleBarTabs};
use crate::workspace::Workspace;

pub struct MainView {
  workspace: Entity<Workspace>,
  title_bar: Entity<TitleBarRoot>,
}

impl MainView {
  pub fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
    let title_bar_content = cx.new(|_| TitleBarTabs::new(workspace.clone()));
    let title_bar = cx.new(|_| TitleBarRoot::new(title_bar_content.into()));

    Self {
      workspace,
      title_bar,
    }
  }
}

impl Render for MainView {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let workspace = self.workspace.read(cx);
    let pane_group = workspace.active_tab().map(|tab| tab.pane_group.clone());

    div()
      .flex()
      .flex_col()
      .size_full()
      .child(self.title_bar.clone())
      .child(
        div()
          .flex()
          .flex_row()
          .flex_1()
          .size_full()
          .child(pane_group.map_or_else(
            || div().child("No active tab").into_any_element(),
            |pg| div().size_full().child(pg).into_any_element(),
          )),
      )
  }
}
