use gpui::*;

use crate::terminal::TerminalView;
use crate::title_bar::{TitleBarRoot, TitleBarTabs};
use crate::workspace::{TabId, TabType, Workspace};

pub struct MainView {
  workspace: Entity<Workspace>,
  title_bar: Entity<TitleBarRoot>,
  current_terminal_view: Option<(TabId, Entity<TerminalView>)>,
}

impl MainView {
  pub fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
    let title_bar_content = cx.new(|_| TitleBarTabs::new(workspace.clone()));
    let title_bar = cx.new(|_| TitleBarRoot::new(title_bar_content.into()));

    Self {
      workspace,
      title_bar,
      current_terminal_view: None,
    }
  }

  fn render_active_tab_content(
    &mut self,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> impl IntoElement {
    let workspace = self.workspace.read(cx);
    let active_tab = workspace.active_tab().cloned();
    let terminal_entity = active_tab.as_ref().and_then(|tab| {
      if let TabType::Terminal(terminal_id) = &tab.tab_type {
        workspace.terminal(*terminal_id).cloned()
      } else {
        None
      }
    });

    if let (Some(tab), Some(terminal)) = (active_tab, terminal_entity) {
      let terminal_view = match &self.current_terminal_view {
        Some((id, view)) if *id == tab.id => view.clone(),
        _ => {
          let view = cx.new(|cx| TerminalView::new(terminal.clone(), cx));
          self.current_terminal_view = Some((tab.id, view.clone()));
          view
        }
      };

      terminal_view.focus_handle(cx).focus(window);

      div()
        .flex_1()
        .size_full()
        .child(terminal_view)
        .into_any_element()
    } else if let Some(_tab) = workspace.active_tab() {
      match _tab.tab_type {
        TabType::Sftp => div()
          .flex_1()
          .size_full()
          .flex()
          .items_center()
          .justify_center()
          .child("SFTP view not implemented yet")
          .into_any_element(),
        _ => div()
          .flex_1()
          .size_full()
          .flex()
          .items_center()
          .justify_center()
          .child("Terminal not found")
          .into_any_element(),
      }
    } else {
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
    if let Some((id, _)) = &self.current_terminal_view {
      if let Some(active_tab) = self.workspace.read(cx).active_tab() {
        if active_tab.id != *id {
          self.current_terminal_view = None;
        }
      } else {
        self.current_terminal_view = None;
      }
    }

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
          .child(self.render_active_tab_content(window, cx)),
      )
  }
}
