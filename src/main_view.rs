use std::collections::HashMap;

use crate::placeholder::PlaceholderView;
use crate::terminal::TerminalView;
use crate::tiles::{Tile, TileView};
use crate::workspace::{TabId, TabType, Workspace};
use gpui::*;
use gpui_component::WindowExt;
use gpui_component::notification::Notification;
use gpui_component::{ActiveTheme as _, Icon, IconName, StyledExt as _, tab::*, *};

/// Main view
pub struct MainView {
  pub workspace: Entity<Workspace>,
  /// Cache terminal views by tab ID so they aren't recreated on every render
  terminal_views: HashMap<TabId, Entity<TerminalView>>,
}

impl MainView {
  pub fn new(workspace: Entity<Workspace>) -> Self {
    Self {
      workspace,
      terminal_views: HashMap::new(),
    }
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

  fn render_active_tab_content(
    &mut self,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> impl IntoElement {
    let active_tab = self.workspace.read(cx).active_tab().cloned();

    if let Some(tab) = active_tab {
      match &tab.tab_type {
        TabType::Terminal(terminal) => {
          // Reuse existing TerminalView or create one
          let terminal_view = self
            .terminal_views
            .entry(tab.id)
            .or_insert_with(|| cx.new(|cx| TerminalView::new(terminal.clone(), cx)))
            .clone();

          // Ensure the terminal view is focused so it receives key events
          // terminal_view.focus_handle(cx).focus(window);

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
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    // Clean up terminal views for closed tabs
    let tab_ids: std::collections::HashSet<TabId> =
      self.workspace.read(cx).tabs.iter().map(|t| t.id).collect();
    self.terminal_views.retain(|id, _| tab_ids.contains(id));

    let placeholder = cx.new(|_| PlaceholderView);
    let root_tile = cx.new(|_| Tile::Content(placeholder.into()));

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
          .child(cx.new(|cx| TileView::new(root_tile, None, cx))),
      )
  }
}
