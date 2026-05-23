use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::ActiveTheme;

use crate::pane::any_view::PaneView;
use crate::pane::pane_node::{PaneLeafId, PaneNode, SplitDirection};
use crate::terminal::TerminalView;
use crate::workspace::Workspace;

actions!(pane, [SplitRight, SplitDown, ClosePane]);

pub struct PaneGroup {
  root: PaneNode,
  active_leaf_id: Option<PaneLeafId>,
  next_leaf_id: u64,
  workspace: Entity<Workspace>,
  focus_handle: FocusHandle,
}

impl PaneGroup {
  pub fn new(
    workspace: Entity<Workspace>,
    initial_view: Entity<TerminalView>,
    cx: &mut Context<Self>,
  ) -> Self {
    let leaf_id = PaneLeafId(1);
    Self {
      root: PaneNode::new_leaf(leaf_id, PaneView::Terminal(initial_view)),
      active_leaf_id: Some(leaf_id),
      next_leaf_id: 2,
      workspace,
      focus_handle: cx.focus_handle(),
    }
  }

  fn create_terminal_view(&mut self, cx: &mut Context<Self>) -> Option<Entity<TerminalView>> {
    let terminal =
      self
        .workspace
        .update(cx, |workspace, cx| match workspace.create_terminal(cx) {
          Ok(t) => Some(t),
          Err(e) => {
            eprintln!("Failed to create terminal: {}", e);
            None
          }
        })?;
    let view = cx.new(|cx| TerminalView::new(terminal, cx));
    Some(view)
  }

  fn split_pane(&mut self, direction: SplitDirection, cx: &mut Context<Self>) {
    let Some(active_id) = self.active_leaf_id else {
      return;
    };
    let Some(new_view) = self.create_terminal_view(cx) else {
      return;
    };
    let new_id = PaneLeafId(self.next_leaf_id);
    self.next_leaf_id += 1;
    let new_leaf = PaneNode::new_leaf(new_id, PaneView::Terminal(new_view));
    self.root.split_at(active_id, direction, new_leaf);
    self.active_leaf_id = Some(new_id);
    cx.notify();
  }

  fn close_active_pane(&mut self, cx: &mut Context<Self>) {
    let Some(active_id) = self.active_leaf_id else {
      return;
    };
    if self.root.leaf_count() <= 1 {
      return;
    }
    let new_focus = self
      .root
      .next_leaf_after(active_id)
      .or_else(|| self.root.prev_leaf_before(active_id));
    self.root.remove_leaf(active_id);
    self.active_leaf_id = new_focus;
    cx.notify();
  }

  fn on_action_split_right(&mut self, _: &SplitRight, _: &mut Window, cx: &mut Context<Self>) {
    self.split_pane(SplitDirection::Horizontal, cx);
  }

  fn on_action_split_down(&mut self, _: &SplitDown, _: &mut Window, cx: &mut Context<Self>) {
    self.split_pane(SplitDirection::Vertical, cx);
  }

  fn on_action_close_pane(&mut self, _: &ClosePane, _: &mut Window, cx: &mut Context<Self>) {
    self.close_active_pane(cx);
  }

  fn render_node(node: &PaneNode, has_siblings: bool, cx: &App) -> AnyElement {
    match node {
      PaneNode::Leaf { view, .. } => {
        let terminal_el = match view {
          PaneView::Terminal(entity) => entity.clone().into_any_element(),
        };
        if has_siblings {
          let title = view.title(cx);
          div()
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .child(
              div()
                .h(px(28.))
                .bg(cx.theme().title_bar)
                .border_b_1()
                .border_color(cx.theme().title_bar_border)
                .px(px(8.))
                .flex()
                .items_center()
                .child(
                  div()
                    .text_size(px(11.))
                    .text_color(cx.theme().foreground)
                    .child(title),
                ),
            )
            .child(div().flex_1().w_full().overflow_hidden().child(terminal_el))
            .into_any_element()
        } else {
          div()
            .size_full()
            .bg(cx.theme().background)
            .overflow_hidden()
            .child(terminal_el)
            .into_any_element()
        }
      }
      PaneNode::Split {
        direction,
        children,
        ..
      } => {
        let is_h = *direction == SplitDirection::Horizontal;
        let n = children.len();
        div()
          .size_full()
          .flex()
          .when(is_h, |d: Div| d.flex_row())
          .when(!is_h, |d: Div| d.flex_col())
          .children(children.iter().enumerate().flat_map(|(i, child)| {
            let child_el = Self::render_node(child, true, cx);
            let mut elements: Vec<AnyElement> = vec![
              div()
                .flex_1()
                .min_w(px(100.))
                .min_h(px(50.))
                .overflow_hidden()
                .child(child_el)
                .into_any_element(),
            ];
            if i < n - 1 {
              elements.push(
                div()
                  .when(is_h, |d: Div| d.w(px(4.)).h_full())
                  .when(!is_h, |d: Div| d.h(px(4.)).w_full())
                  .bg(cx.theme().border)
                  .into_any_element(),
              );
            }
            elements
          }))
          .into_any_element()
      }
    }
  }
}

impl Render for PaneGroup {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    div()
      .id("pane-group")
      .key_context("Pane")
      .size_full()
      .track_focus(&self.focus_handle)
      .on_action(cx.listener(Self::on_action_split_right))
      .on_action(cx.listener(Self::on_action_split_down))
      .on_action(cx.listener(Self::on_action_close_pane))
      .child(Self::render_node(
        &self.root,
        self.root.leaf_count() > 1,
        &cx,
      ))
  }
}

impl Focusable for PaneGroup {
  fn focus_handle(&self, _cx: &App) -> FocusHandle {
    self.focus_handle.clone()
  }
}
