use gpui::*;
use gpui_component::button::Button;
use gpui_component::notification::Notification;
use gpui_component::tab::{Tab, TabBar, TabVariant};
use gpui_component::{ActiveTheme, Icon, IconName, Sizable, WindowExt};

use crate::app::App;
use crate::workspace::Workspace;

pub struct TitleBarTabs {
  app: Entity<App>,
}

impl TitleBarTabs {
  pub fn new(app: Entity<App>, cx: &mut Context<Self>) -> Self {
    cx.observe(&app, |_, _, cx| {
      cx.notify();
    })
    .detach();
    Self { app }
  }

  /// 对当前激活的 workspace 执行操作；返回操作结果。
  fn with_active_workspace<R>(
    &self,
    cx: &mut Context<Self>,
    f: impl FnOnce(&mut Workspace, &mut gpui::Context<Workspace>) -> R,
  ) -> Option<R> {
    self.app.update(cx, |app, cx| {
      app
        .active_workspace()
        .map(|ws| ws.update(cx, |ws, cx| f(ws, cx)))
    })
  }

  fn handle_tab_click(&mut self, index: usize, _window: &mut Window, cx: &mut Context<Self>) {
    let id = self
      .app
      .read(cx)
      .active_workspace()
      .and_then(|ws| ws.read(cx).tabs.get(index))
      .map(|tab| tab.id);
    if let Some(id) = id
      && self
        .with_active_workspace(cx, |ws, cx| ws.activate_tab(id, cx))
        .unwrap_or(false)
    {
      cx.notify();
    }
  }

  fn handle_tab_close(&mut self, index: usize, _window: &mut Window, cx: &mut Context<Self>) {
    let id = self
      .app
      .read(cx)
      .active_workspace()
      .and_then(|ws| ws.read(cx).tabs.get(index))
      .map(|tab| tab.id);
    if let Some(id) = id
      && self
        .with_active_workspace(cx, |ws, cx| ws.close_tab(id, cx))
        .unwrap_or(false)
    {
      cx.notify();
    }
  }

  fn handle_add_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let result = self.with_active_workspace(cx, |ws, cx| ws.add_terminal_tab(cx));
    match result {
      Some(Err(error_msg)) => {
        window.push_notification(Notification::error(error_msg), cx);
      }
      Some(Ok(_)) => cx.notify(),
      None => {}
    }
  }
}

impl Render for TitleBarTabs {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let active_workspace = self.app.read(cx).active_workspace();
    let (tabs_len, active_index) = match active_workspace {
      Some(ws) => {
        let ws = ws.read(cx);
        (ws.tabs.len(), ws.active_index().unwrap_or(0))
      }
      None => (0, 0),
    };

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
          .children((0..tabs_len).map(|ix| {
            // 当前 workspace 的每个 tab 渲染一个 Tab。
            let (icon, title) = active_workspace
              .and_then(|ws| ws.read(cx).tabs.get(ix))
              .and_then(|tab| tab.pane_group.read(cx).first_leaf_title(cx))
              .map(|title| (IconName::SquareTerminal, title))
              .unwrap_or((IconName::SquareTerminal, "Terminal".to_string()));
            let title: SharedString = title.into();

            Tab::new().label(title).icon(icon).suffix(
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
