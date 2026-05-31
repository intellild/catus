use gpui::*;

use crate::terminal::TerminalView;

#[derive(Clone)]
pub enum PaneView {
  Terminal(Entity<TerminalView>),
}

impl PaneView {
  pub fn title(&self, cx: &App) -> String {
    match self {
      PaneView::Terminal(view) => view.read(cx).title(cx),
    }
  }
}
