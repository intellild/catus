use std::collections::HashMap;

use gpui::*;

use crate::terminal::TerminalView;
use crate::title_bar::{TitleBarRoot, TitleBarTabs};
use crate::workspace::{TabId, TabType, Workspace};

/// Main view
pub struct MainView {
  workspace: Entity<Workspace>,
  title_bar: Entity<TitleBarRoot>,
  /// Cache terminal views by tab ID so they aren't recreated on every render
  terminal_views: HashMap<TabId, Entity<TerminalView>>,
}

impl MainView {
  pub fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
    let title_bar_content = cx.new(|_| TitleBarTabs::new(workspace.clone()));
    let title_bar = cx.new(|_| TitleBarRoot::new(title_bar_content.into()));

    Self {
      workspace,
      title_bar,
      terminal_views: HashMap::new(),
    }
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
          terminal_view.focus_handle(cx).focus(window);

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

    div()
      .flex()
      .flex_col()
      .size_full()
      .child(self.title_bar.clone())
      .child(
        // Main content area
        div()
          .flex()
          .flex_row()
          .flex_1()
          .size_full()
          .child(self.render_active_tab_content(window, cx)),
      )
  }
}
