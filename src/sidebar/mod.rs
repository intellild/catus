use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::tooltip::Tooltip;
use gpui_component::{ActiveTheme, Icon, IconName, Sizable};

use crate::add_workspace_dialog::open_add_workspace_dialog;
use crate::app::App;

/// 左侧侧边栏：纵向排列的 Workspace 图标栏。
/// 每行仅显示图标；名称等文字描述通过 hover 触发的 popover（tooltip）展示。
/// 底部有一个 `+` 按钮用于打开「添加 Workspace」面板。
pub struct WorkspaceSidebar {
  app: Entity<App>,
}

impl WorkspaceSidebar {
  pub fn new(app: Entity<App>, cx: &mut Context<Self>) -> Self {
    cx.observe(&app, |_, _, cx| {
      cx.notify();
    })
    .detach();
    Self { app }
  }

  fn handle_select(&mut self, index: usize, cx: &mut Context<Self>) {
    self
      .app
      .update(cx, |app, cx| app.activate_workspace(index, cx));
    cx.notify();
  }

  fn handle_close(&mut self, index: usize, cx: &mut Context<Self>) {
    self
      .app
      .update(cx, |app, cx| app.close_workspace(index, cx));
    cx.notify();
  }

  fn handle_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    open_add_workspace_dialog(self.app.clone(), window, cx);
  }

  fn render_row(
    &self,
    index: usize,
    is_active: bool,
    name: SharedString,
    icon: IconName,
    closeable: bool,
    cx: &mut Context<Self>,
  ) -> impl IntoElement {
    let theme = cx.theme();
    let group_name = format!("ws-row-{}", index);
    let mut row = div()
      .id(("workspace-row", index))
      .group(group_name.clone())
      .relative()
      .flex()
      .items_center()
      .justify_center()
      .w(px(36.))
      .h(px(36.))
      .rounded_md()
      .text_color(theme.foreground)
      .when(is_active, |this| {
        this.bg(theme.accent).text_color(theme.accent_foreground)
      })
      .when(!is_active, |this| {
        this.hover(|style| style.bg(theme.secondary_hover))
      })
      .on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, _, _, cx| {
          cx.stop_propagation();
          this.handle_select(index, cx);
        }),
      )
      // 文字描述收到 hover 触发的 popover（tooltip）里。
      .tooltip(move |window, cx| Tooltip::new(name.clone()).build(window, cx))
      .child(Icon::new(icon).with_size(px(16.)));

    // 关闭按钮：始终保留至少一个 Workspace，因此仅在有多个时显示。
    // 行内没有放置文字的空间，关闭按钮以角标形式悬浮在行的右上角，
    // 默认隐藏，鼠标悬停在该行时通过 group_hover 显示出来。
    if closeable {
      row = row.child(
        div()
          .id(("workspace-close", index))
          .absolute()
          .top(px(1.))
          .right(px(1.))
          .flex()
          .items_center()
          .justify_center()
          .w(px(14.))
          .h(px(14.))
          .rounded_full()
          .border_1()
          .border_color(theme.border)
          .bg(theme.background)
          .text_color(theme.foreground)
          .opacity(0.0)
          .group_hover(group_name, |style| style.opacity(1.0))
          .hover(|style| {
            style
              .bg(theme.danger_hover)
              .text_color(theme.danger_foreground)
          })
          .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
              cx.stop_propagation();
              this.handle_close(index, cx);
            }),
          )
          .child(Icon::new(IconName::Close).with_size(px(9.))),
      );
    }
    row
  }
}

impl Render for WorkspaceSidebar {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    // 先收集每个 workspace 的不可变数据，避免在构建元素时持续借用 `cx`。
    let (rows, sidebar_bg, sidebar_border) = {
      let app = self.app.read(cx);
      let theme = cx.theme();
      let rows: Vec<(usize, bool, SharedString, IconName)> = app
        .workspaces
        .iter()
        .enumerate()
        .map(|(index, workspace)| {
          let ws = workspace.read(cx);
          (
            index,
            app.active_index == Some(index),
            ws.display_name(),
            ws.icon(),
          )
        })
        .collect();
      (rows, theme.secondary, theme.border)
    };
    let closeable = rows.len() > 1;

    // 在进入元素构建链之前，先把每行渲染成 AnyElement，避免在 .children() 闭包里持续借用 self/cx。
    let mut row_elements: Vec<AnyElement> = Vec::with_capacity(rows.len());
    for (index, is_active, name, icon) in rows {
      row_elements.push(
        self
          .render_row(index, is_active, name, icon, closeable, cx)
          .into_any_element(),
      );
    }

    div()
      .id("workspace-sidebar")
      .flex()
      .flex_col()
      .h_full()
      .w(px(48.))
      .flex_shrink_0()
      .bg(sidebar_bg)
      .border_r_1()
      .border_color(sidebar_border)
      // 上方：Workspace 图标列表（可滚动）
      .child(
        div()
          .id("workspace-list")
          .flex_1()
          .min_h_0()
          .overflow_y_scroll()
          .child(
            div()
              .flex()
              .flex_col()
              .items_center()
              .gap_1()
              .p(px(6.))
              .children(row_elements),
          ),
      )
      // 底部：添加按钮
      .child(
        div()
          .flex()
          .items_center()
          .justify_center()
          .p(px(6.))
          .border_t_1()
          .border_color(sidebar_border)
          .child(
            Button::new("add-workspace")
              .ghost()
              .small()
              .icon(Icon::new(IconName::Plus))
              .tooltip("Add a new workspace")
              .on_click(cx.listener(|this, _, window, cx| {
                this.handle_add(window, cx);
              })),
          ),
      )
  }
}
