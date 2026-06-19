use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::ActiveTheme;

use crate::pane::any_view::PaneView;
use crate::pane::pane_node::{PaneLeafId, PaneNode, SplitDirection};
use crate::terminal::{TerminalView, TerminalViewEvent};
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
    Self::subscribe_to_view(cx, &initial_view);
    Self {
      root: PaneNode::new_leaf(leaf_id, PaneView::Terminal(initial_view)),
      active_leaf_id: Some(leaf_id),
      next_leaf_id: 2,
      workspace,
      focus_handle: cx.focus_handle(),
    }
  }

  /// 订阅 TerminalView 事件：标题变更时重新渲染，子进程退出时自动关闭对应 pane。
  fn subscribe_to_view(cx: &mut Context<Self>, view: &Entity<TerminalView>) {
    let view_clone = view.clone();
    cx.subscribe(
      view,
      move |this, _view, event: &TerminalViewEvent, cx| match event {
        TerminalViewEvent::TitleChanged => {
          cx.notify();
        }
        TerminalViewEvent::Closed => {
          this.close_leaf_by_view(&view_clone, cx);
        }
      },
    )
    .detach();
  }

  fn create_terminal_view(&mut self, cx: &mut Context<Self>) -> Option<Entity<TerminalView>> {
    let result = self
      .workspace
      .update(cx, |ws, cx| Workspace::create_terminal_view(cx, &ws.kind));
    match result {
      Ok(view) => {
        Self::subscribe_to_view(cx, &view);
        Some(view)
      }
      Err(e) => {
        eprintln!("Failed to create terminal: {}", e);
        None
      }
    }
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

  /// 按视图关闭对应的叶子节点。用于子进程退出时自动清理。
  fn close_leaf_by_view(&mut self, view: &Entity<TerminalView>, cx: &mut Context<Self>) {
    let target = PaneView::Terminal(view.clone());
    let Some(leaf_id) = self.root.find_leaf_id_by_view(&target) else {
      return;
    };
    // 只剩一个叶子时不自动关闭，保留 "Process exited" 显示
    if self.root.leaf_count() <= 1 {
      return;
    }
    let new_focus = self
      .root
      .next_leaf_after(leaf_id)
      .or_else(|| self.root.prev_leaf_before(leaf_id));
    self.root.remove_leaf(leaf_id);
    self.active_leaf_id = new_focus;
    cx.notify();
  }

  /// 获取当前激活叶子节点的终端标题（用于 tab 标题）。
  pub fn active_leaf_title(&self, cx: &App) -> Option<String> {
    self
      .active_leaf_id
      .and_then(|id| self.root.find_view_by_id(id))
      .map(|v| v.title(cx))
      .or_else(|| self.root.first_leaf_view().map(|v| v.title(cx)))
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

#[cfg(test)]
impl PaneGroup {
  /// 用 FakePty 在当前激活 leaf 上分割出一个新 pane，避免测试中启动真实 shell。
  /// 返回新 leaf 的 id。
  pub(crate) fn split_pane_with_fake_pty(
    &mut self,
    direction: SplitDirection,
    cx: &mut Context<Self>,
  ) -> Option<PaneLeafId> {
    let active_id = self.active_leaf_id?;
    let pty = std::sync::Arc::new(crate::terminal::FakePty::new())
      as std::sync::Arc<dyn crate::terminal::Pty>;
    let view = self
      .workspace
      .update(cx, |_ws, cx| {
        Workspace::create_terminal_view_with_pty(cx, pty)
      })
      .ok()?;
    Self::subscribe_to_view(cx, &view);
    let new_id = PaneLeafId(self.next_leaf_id);
    self.next_leaf_id += 1;
    let new_leaf = PaneNode::new_leaf(new_id, PaneView::Terminal(view));
    self.root.split_at(active_id, direction, new_leaf);
    self.active_leaf_id = Some(new_id);
    cx.notify();
    Some(new_id)
  }

  /// 测试用：暴露 close_active_pane。
  pub(crate) fn close_active_pane_for_test(&mut self, cx: &mut Context<Self>) {
    self.close_active_pane(cx);
  }

  /// 测试用：直接按视图关闭对应 leaf（模拟子进程退出触发的自动关闭）。
  pub(crate) fn close_leaf_by_view_for_test(
    &mut self,
    view: &Entity<TerminalView>,
    cx: &mut Context<Self>,
  ) {
    self.close_leaf_by_view(view, cx);
  }

  /// 测试用：当前 leaf 总数。
  pub(crate) fn leaf_count_for_test(&self) -> usize {
    self.root.leaf_count()
  }

  /// 测试用：当前激活 leaf id。
  pub(crate) fn active_leaf_id_for_test(&self) -> Option<PaneLeafId> {
    self.active_leaf_id
  }

  /// 测试用：根据 id 取对应 leaf 的 TerminalView。
  pub(crate) fn view_for_leaf(&self, id: PaneLeafId) -> Option<Entity<TerminalView>> {
    match self.root.find_view_by_id(id)? {
      PaneView::Terminal(view) => Some(view.clone()),
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
        cx,
      ))
  }
}

impl Focusable for PaneGroup {
  fn focus_handle(&self, _cx: &App) -> FocusHandle {
    self.focus_handle.clone()
  }
}

#[cfg(test)]
mod tests {
  use super::{PaneGroup, SplitDirection};
  use crate::terminal::FakePty;
  use crate::workspace::Workspace;
  use crate::workspace_kind::WorkspaceKind;
  use gpui::{AppContext as _, Entity, TestAppContext};
  use std::sync::Arc;

  /// 创建一个使用 FakePty 的 Workspace，并返回其激活 tab 的 PaneGroup 实体。
  fn make_pane_group(cx: &mut TestAppContext) -> Entity<PaneGroup> {
    let fake = Arc::new(FakePty::new());
    let pty_dyn: Arc<dyn crate::terminal::Pty> = fake.clone();
    let ws = cx.new(|cx| Workspace::new_with_pty(WorkspaceKind::Local, pty_dyn, cx));
    ws.read_with(cx, |w, _| {
      w.active_tab().expect("has tab").pane_group.clone()
    })
  }

  #[gpui::test]
  fn split_increments_leaf_count_and_switches_active(cx: &mut TestAppContext) {
    let group = make_pane_group(cx);
    let original_active = group.read_with(cx, |g, _| g.active_leaf_id_for_test());

    let new_id = group.update(cx, |g, cx| {
      g.split_pane_with_fake_pty(SplitDirection::Horizontal, cx)
    });
    assert!(new_id.is_some());

    group.read_with(cx, |g, _| {
      assert_eq!(g.leaf_count_for_test(), 2);
      // 激活应切到新 leaf
      assert_eq!(g.active_leaf_id_for_test(), new_id);
      assert_ne!(g.active_leaf_id_for_test(), original_active);
    });
  }

  #[gpui::test]
  fn close_active_pane_switches_focus_back(cx: &mut TestAppContext) {
    let group = make_pane_group(cx);
    let first = group.read_with(cx, |g, _| g.active_leaf_id_for_test().expect("active"));

    let new_id = group
      .update(cx, |g, cx| {
        g.split_pane_with_fake_pty(SplitDirection::Horizontal, cx)
      })
      .expect("split");
    assert_eq!(group.read_with(cx, |g, _| g.leaf_count_for_test()), 2);

    // 关闭激活（即新 leaf），焦点应回到兄弟 leaf
    group.update(cx, |g, cx| g.close_active_pane_for_test(cx));
    group.read_with(cx, |g, _| {
      assert_eq!(g.leaf_count_for_test(), 1);
      assert_eq!(g.active_leaf_id_for_test(), Some(first));
    });
    let _ = new_id;
  }

  #[gpui::test]
  fn close_active_pane_refuses_when_single_leaf(cx: &mut TestAppContext) {
    let group = make_pane_group(cx);
    group.update(cx, |g, cx| g.close_active_pane_for_test(cx));
    // 只剩一个 leaf 时不应关闭
    group.read_with(cx, |g, _| {
      assert_eq!(g.leaf_count_for_test(), 1);
      assert!(g.active_leaf_id_for_test().is_some());
    });
  }

  #[gpui::test]
  fn close_leaf_by_view_removes_target_when_multiple(cx: &mut TestAppContext) {
    let group = make_pane_group(cx);
    let new_id = group
      .update(cx, |g, cx| {
        g.split_pane_with_fake_pty(SplitDirection::Vertical, cx)
      })
      .expect("split");
    // 取出新 leaf 的 view，模拟其子进程退出触发自动关闭
    let target_view = group
      .read_with(cx, |g, _| g.view_for_leaf(new_id))
      .expect("view");

    group.update(cx, |g, cx| g.close_leaf_by_view_for_test(&target_view, cx));
    group.read_with(cx, |g, _| {
      assert_eq!(g.leaf_count_for_test(), 1);
    });
  }

  #[gpui::test]
  fn close_leaf_by_view_refuses_when_single_leaf(cx: &mut TestAppContext) {
    let group = make_pane_group(cx);
    let only_view = group.read_with(cx, |g, _| {
      g.view_for_leaf(g.active_leaf_id_for_test().expect("active"))
        .expect("view")
    });

    // 只剩一个 leaf 时不应自动关闭（保留 "Process exited" 显示）
    group.update(cx, |g, cx| g.close_leaf_by_view_for_test(&only_view, cx));
    group.read_with(cx, |g, _| {
      assert_eq!(g.leaf_count_for_test(), 1);
    });
  }

  #[gpui::test]
  fn active_leaf_title_returns_terminal_title(cx: &mut TestAppContext) {
    use crate::terminal::fake_pty::EchoMode;

    // 重新构造以便注入 OSC 标题
    let fake = Arc::new(FakePty::with_echo_mode(EchoMode::None));
    let pty_dyn: Arc<dyn crate::terminal::Pty> = fake.clone();
    let ws = cx.new(|cx| Workspace::new_with_pty(WorkspaceKind::Local, pty_dyn, cx));
    let group = ws.read_with(cx, |w, _| w.active_tab().expect("tab").pane_group.clone());

    // 初始标题为 "Terminal"
    let title = group.read_with(cx, |g, cx| g.active_leaf_title(cx));
    assert_eq!(title.as_deref(), Some("Terminal"));

    // 注入 OSC 标题后，active_leaf_title 跟随更新
    fake.push_bytes("\x1b]2;Pane Title\x07").unwrap();
    cx.run_until_parked();
    let title = group.read_with(cx, |g, cx| g.active_leaf_title(cx));
    assert_eq!(title.as_deref(), Some("Pane Title"));
  }
}
