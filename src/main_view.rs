use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{ActiveTheme as _, Icon, IconName, StyledExt as _, tab::*, *};

use crate::app_state::{AppState, TabItem};
use crate::terminal::TerminalView;

/// Main view
pub struct MainView {
  pub state: Entity<AppState>,
}

impl MainView {
  pub fn new(app_state: Entity<AppState>) -> Self {
    Self { state: app_state }
  }

  fn handle_tab_click(&mut self, index: usize, _window: &mut Window, cx: &mut Context<Self>) {
    // cx.background_spawn()
    if let Some(tab) = self.state.read(cx).tabs.get(index) {
      let id = tab.id;
      if self.state.as_mut(cx).activate_tab(id) {
        cx.notify();
      }
    }
  }

  fn handle_tab_close(&mut self, index: usize, _window: &mut Window, cx: &mut Context<Self>) {
    if let Some(tab) = self.state.read(cx).tabs.get(index) {
      let id = tab.id;
      if self.state.as_mut(cx).close_tab(id) {
        cx.notify();
      }
    }
  }

  fn handle_add_tab(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
    let new_tab =
      TabItem::new(format!("Tab {}", self.state.tabs.len() + 1)).with_icon(IconName::File);
    self.state.add_tab(new_tab);
    cx.notify();
  }

  fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let height = px(34.);

    // Left padding for macOS traffic lights or general spacing
    #[cfg(target_os = "macos")]
    let left_padding = px(80.);
    #[cfg(not(target_os = "macos"))]
    let left_padding = px(12.);

    let tabs = self.state.tabs.clone();
    let active_index = self.state.active_index().unwrap_or(0);

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
                this.handle_add_tab(window, cx);
              }))
              .child(Icon::new(IconName::Plus).small()),
          ),
      )
  }
}

impl Render for MainView {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    // 获取或创建 TerminalView
    let terminal_view = cx.new(|cx| TerminalView::new(cx));

    div()
      .v_flex()
      .size_full()
      .child(self.render_title_bar(cx))
      .child(
        // Main content area with terminal
        div()
          .v_flex()
          .flex_1()
          .child(
            div()
              .text_xl()
              .font_weight(FontWeight::SEMIBOLD)
              .p_4()
              .child(
                self
                  .state
                  .active_tab()
                  .map(|t| t.state.read(cx).title.clone())
                  .unwrap_or_else(|| "No Tab".into()),
              ),
          )
          .child(div().flex_1().p_4().child(terminal_view)),
      )
  }
}
