use crate::workspace::Workspace;
use gpui::*;
use gpui_component::button::Button;
use gpui_component::notification::Notification;
use gpui_component::tab::{Tab, TabBar, TabVariant};
use gpui_component::{ActiveTheme, Icon, IconName, Sizable, WindowExt};

pub struct TitleBarTabs {
  workspace: Entity<Workspace>,
}

impl TitleBarTabs {
  pub fn new(workspace: Entity<Workspace>) -> Self {
    Self { workspace }
  }
}

impl TitleBarTabs {
  fn handle_tab_click(&mut self, index: usize, _window: &mut Window, cx: &mut Context<Self>) {
    if let Some(tab) = self.workspace.read(cx).tabs.get(index) {
      let id = tab.id;
      if self
        .workspace
        .update(cx, |workspace, _cx| workspace.activate_tab(id))
      {
        cx.notify();
      }
    }
  }

  fn handle_tab_close(&mut self, index: usize, _window: &mut Window, cx: &mut Context<Self>) {
    if let Some(tab) = self.workspace.read(cx).tabs.get(index) {
      let id = tab.id;
      if self
        .workspace
        .update(cx, |workspace, _cx| workspace.close_tab(id))
      {
        cx.notify();
      }
    }
  }

  fn handle_add_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if let Err(error_msg) = self
      .workspace
      .update(cx, |workspace, cx| workspace.add_terminal_tab(cx))
    {
      // 显示错误通知
      window.push_notification(Notification::error(error_msg), cx);
    } else {
      cx.notify();
    }
  }
}

impl Render for TitleBarTabs {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let workspace = self.workspace.read(cx);
    let tabs = &workspace.tabs;
    let active_index = workspace.active_index().unwrap_or(0);

    div()
      .flex()
      .items_center()
      .child(
        TabBar::new("tab-bar")
          .with_variant(TabVariant::Tab)
          .selected_index(active_index)
          .on_click(cx.listener(|this, ix: &usize, window, cx| {
            this.handle_tab_click(*ix, window, cx);
          }))
          .children(tabs.iter().enumerate().map(|(ix, tab)| {
            let state = tab.state.read(cx);

            let tab_icon = state.icon.clone();
            let title = state.title.clone();

            Tab::new().label(title).icon(tab_icon).suffix(
              div()
                .id("tab-close")
                .flex()
                .items_center()
                .justify_center()
                .w(px(16.))
                .h(px(16.))
                .rounded_full()
                .hover(|style| style.bg(cx.theme().secondary_hover))
                .on_click(cx.listener(move |this, _, window, cx| {
                  cx.stop_propagation();
                  this.handle_tab_close(ix, window, cx);
                }))
                .child(Icon::new(IconName::Close).with_size(px(12.))),
            )
          })),
      )
      .child(
        Button::new("btn-secondary")
          .w(px(28.))
          .h(px(28.))
          .ml(px(4.))
          .on_click(cx.listener(|this, _, window, cx| {
            this.handle_add_terminal(window, cx);
          }))
          .child(Icon::new(IconName::Plus).small()),
      )
  }
}
