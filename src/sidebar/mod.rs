use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{ActiveTheme, Icon, IconName, Sizable};

use crate::add_workspace_dialog::open_add_workspace_dialog;
use crate::app::App;

/// 左侧侧边栏：纵向排列的 Workspace 列表，每行显示图标 + 名称。
/// 底部有一个 `+` 按钮用于打开「添加 Workspace」面板。
pub struct WorkspaceSidebar {
  app: Entity<App>,
}

impl WorkspaceSidebar {
  pub fn new(app: Entity<App>) -> Self {
    Self { app }
  }

  fn handle_select(&mut self, index: usize, cx: &mut Context<Self>) {
    if self.app.update(cx, |app, _| app.activate_workspace(index)) {
      cx.notify();
    }
  }

  fn handle_close(&mut self, index: usize, cx: &mut Context<Self>) {
    if self.app.update(cx, |app, _| app.close_workspace(index)) {
      cx.notify();
    }
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
    let row = div()
      .id(("workspace-row", index))
      .group(group_name.clone())
      .flex()
      .flex_row()
      .items_center()
      .gap(px(8.))
      .px(px(10.))
      .h(px(32.))
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
      .child(Icon::new(icon).with_size(px(16.)))
      .child(div().flex_1().text_sm().child(name).overflow_x_hidden());

    // 关闭按钮：始终保留至少一个 Workspace，因此仅在有多个时显示。
    // 默认隐藏，鼠标悬停在该行时通过 group_hover 显示出来。
    if closeable {
      row.child(
        div()
          .id(("workspace-close", index))
          .opacity(0.0)
          .group_hover(group_name, |style| style.opacity(1.0))
          .flex()
          .items_center()
          .justify_center()
          .w(px(18.))
          .h(px(18.))
          .rounded_full()
          .hover(|style| style.bg(theme.secondary_hover))
          .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
              cx.stop_propagation();
              this.handle_close(index, cx);
            }),
          )
          .child(Icon::new(IconName::Close).with_size(px(11.))),
      )
    } else {
      row
    }
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
      .w(px(180.))
      .flex_shrink_0()
      .bg(sidebar_bg)
      .border_r_1()
      .border_color(sidebar_border)
      // 上方：Workspace 列表（可滚动）
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
              .gap_1()
              .p(px(8.))
              .children(row_elements),
          ),
      )
      // 底部：添加按钮
      .child(
        div()
          .flex()
          .flex_row()
          .items_center()
          .px(px(8.))
          .py(px(8.))
          .border_t_1()
          .border_color(sidebar_border)
          .child(
            Button::new("add-workspace")
              .ghost()
              .small()
              .w_full()
              .justify_start()
              .gap(px(6.))
              .child(Icon::new(IconName::Plus).small())
              .child("Add Workspace")
              .tooltip("Add a new workspace")
              .on_click(cx.listener(|this, _, window, cx| {
                this.handle_add(window, cx);
              })),
          ),
      )
  }
}
