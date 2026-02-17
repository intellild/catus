use gpui::*;
use gpui_component::{ActiveTheme as _, Icon, IconName, StyledExt as _, tab::*, *};

use crate::terminal::TerminalView;
use crate::workspace::{TabType, Workspace};

/// Main view
pub struct MainView {
  pub workspace: Entity<Workspace>,
}

impl MainView {
  pub fn new(workspace: Entity<Workspace>) -> Self {
    Self { workspace }
  }

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

  fn handle_add_terminal(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
    self.workspace.update(cx, |workspace, cx| {
      workspace.add_terminal_tab(cx);
      cx.notify();
    });
  }

  fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let height = px(34.);

    // Left padding for macOS traffic lights or general spacing
    #[cfg(target_os = "macos")]
    let left_padding = px(80.);
    #[cfg(not(target_os = "macos"))]
    let left_padding = px(12.);

    let workspace = self.workspace.read(cx);
    let tabs = workspace.tabs.clone();
    let active_index = workspace.active_index().unwrap_or(0);

    div()
      .id("custom-title-bar")
      .flex()
      .flex_row()
      .items_center()
      .h(height)
      .bg(cx.theme().title_bar)
      .border_b_1()
      .border_color(cx.theme().title_bar_border)
      .child(
        // Left area: Drag region with some padding
        div()
          .id("title-bar-drag-region")
          .flex()
          .flex_row()
          .items_center()
          .h_full()
          .pl(left_padding)
          .flex_shrink_0()
          // This div acts as the drag handle for the window
          .child(
            div()
              .h_full()
              .w(px(60.)) // Extra drag area width
              .window_control_area(WindowControlArea::Drag),
          )
          // Tab bar using gpui_component's TabBar
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
          // Add tab button
          .child(
            div()
              .id("add-tab-btn")
              .flex()
              .items_center()
              .justify_center()
              .w(px(28.))
              .h(px(28.))
              .ml(px(4.))
              .rounded_md()
              .cursor_pointer()
              .hover(|style| style.bg(cx.theme().secondary_hover))
              .on_click(cx.listener(|this, _, window, cx| {
                this.handle_add_terminal(window, cx);
              }))
              .child(Icon::new(IconName::Plus).small()),
          ),
      )
  }

  fn render_active_tab_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let active_tab = self.workspace.read(cx).active_tab().cloned();

    if let Some(tab) = active_tab {
      match &tab.tab_type {
        TabType::Terminal(terminal) => {
          // 创建 TerminalView 来显示终端，使用 Tab 关联的 Terminal Entity
          let terminal_view = cx.new(|cx| TerminalView::new(terminal.clone(), cx));

          div()
            .flex_1()
            .size_full()
            .child(terminal_view)
            .into_any_element()
        }
        TabType::Sftp => {
          // TODO: 实现 SFTP 视图
          div()
            .flex_1()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child("SFTP view not implemented yet")
            .into_any_element()
        }
      }
    } else {
      // 没有激活的 Tab
      div()
        .flex_1()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child("No active tab")
        .into_any_element()
    }
  }
}

impl Render for MainView {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    div()
      .v_flex()
      .size_full()
      .child(self.render_title_bar(cx))
      .child(
        // Main content area
        div()
          .v_flex()
          .flex_1()
          .size_full()
          .child(self.render_active_tab_content(cx)),
      )
  }
}
