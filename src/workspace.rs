use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use gpui::{AppContext, Entity, SharedString};
use gpui_component::IconName;

use crate::pane::PaneGroup;
use crate::terminal::{LocalPty, Terminal, TerminalSize, TerminalView, TerminalViewEvent};
use crate::workspace_kind::WorkspaceKind;

static TAB_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

pub fn generate_tab_id() -> TabId {
  TabId(TAB_ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}

#[derive(Clone)]
pub struct TabItem {
  pub id: TabId,
  pub pane_group: Entity<PaneGroup>,
}

pub struct Workspace {
  pub kind: WorkspaceKind,
  pub tabs: Vec<TabItem>,
  pub active_tab_id: Option<TabId>,
}

impl Workspace {
  pub fn new(kind: WorkspaceKind, cx: &mut gpui::Context<Self>) -> Self {
    match Self::make_tab(cx, &kind) {
      Ok(tab) => {
        let active_tab_id = Some(tab.id);
        Self {
          kind,
          tabs: vec![tab],
          active_tab_id,
        }
      }
      Err(e) => {
        eprintln!("Failed to create default terminal: {}", e);
        Self {
          kind,
          tabs: vec![],
          active_tab_id: None,
        }
      }
    }
  }

  /// 侧边栏展示用的名称。
  pub fn display_name(&self) -> SharedString {
    self.kind.display_name()
  }

  /// 侧边栏展示用的图标。
  pub fn icon(&self) -> IconName {
    self.kind.icon()
  }

  fn make_tab(cx: &mut gpui::Context<Self>, kind: &WorkspaceKind) -> Result<TabItem, String> {
    let terminal_view = Self::create_terminal_view(cx, kind)?;
    let workspace_handle = cx.entity().clone();
    let pane_group = cx.new(|cx| PaneGroup::new(workspace_handle, terminal_view, cx));
    Ok(TabItem {
      id: generate_tab_id(),
      pane_group,
    })
  }

  pub fn add_tab(&mut self, tab: TabItem, cx: &mut gpui::Context<Self>) -> TabId {
    let id = tab.id;
    self.tabs.push(tab);
    self.active_tab_id = Some(id);
    cx.notify();
    id
  }

  pub fn close_tab(&mut self, id: TabId, cx: &mut gpui::Context<Self>) -> bool {
    if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
      self.tabs.remove(index);
      if self.active_tab_id == Some(id) {
        self.active_tab_id = self.tabs.get(index.saturating_sub(1)).map(|t| t.id);
      }
      cx.notify();
      return true;
    }
    false
  }

  pub fn activate_tab(&mut self, id: TabId, cx: &mut gpui::Context<Self>) -> bool {
    if self.tabs.iter().any(|t| t.id == id) {
      self.active_tab_id = Some(id);
      cx.notify();
      true
    } else {
      false
    }
  }

  pub fn active_tab(&self) -> Option<&TabItem> {
    self
      .active_tab_id
      .and_then(|id| self.tabs.iter().find(|t| t.id == id))
  }

  pub fn active_index(&self) -> Option<usize> {
    self
      .active_tab_id
      .and_then(|id| self.tabs.iter().position(|t| t.id == id))
  }

  pub fn add_terminal_tab(&mut self, cx: &mut gpui::Context<Self>) -> Result<TabId, String> {
    let tab = Self::make_tab(cx, &self.kind)?;
    Ok(self.add_tab(tab, cx))
  }

  /// 创建 Terminal + TerminalView 实体，并订阅 TerminalViewEvent。
  ///
  /// 当终端标题变更或子进程退出时，通过 `cx.notify()` 触发 Workspace
  /// 重新渲染，进而通知 App → MainView / TitleBarTabs。
  pub fn create_terminal_view(
    cx: &mut gpui::Context<Self>,
    kind: &WorkspaceKind,
  ) -> Result<Entity<TerminalView>, String> {
    let pty = LocalPty::new(TerminalSize::default_size(), kind.command())
      .map_err(|e| format!("Failed to create PTY: {}", e))?;
    Self::create_terminal_view_with_pty(cx, Arc::new(pty))
  }

  /// 用给定的 PTY 创建 Terminal + TerminalView 实体，并订阅 TerminalViewEvent。
  ///
  /// 生产代码中由 [`create_terminal_view`] 调用，传入 `LocalPty`；
  /// 测试中可传入 `FakePty` 以避免启动真实子进程。
  pub(crate) fn create_terminal_view_with_pty(
    cx: &mut gpui::Context<Self>,
    pty: Arc<dyn crate::terminal::Pty>,
  ) -> Result<Entity<TerminalView>, String> {
    let terminal = cx.new(|cx| Terminal::new(pty, cx).expect("Failed to create terminal"));
    let view = cx.new(|cx| TerminalView::new(terminal, cx));

    cx.subscribe(&view, |_, _, event: &TerminalViewEvent, cx| {
      if matches!(
        event,
        TerminalViewEvent::TitleChanged | TerminalViewEvent::Closed
      ) {
        cx.notify();
      }
    })
    .detach();

    Ok(view)
  }
}

#[cfg(test)]
impl Workspace {
  /// 用 FakePty 创建一个 Workspace，避免测试中启动真实 shell。
  pub(crate) fn new_with_fake_pty(kind: WorkspaceKind, cx: &mut gpui::Context<Self>) -> Self {
    match Self::make_tab_with_fake_pty(cx) {
      Ok(tab) => {
        let active_tab_id = Some(tab.id);
        Self {
          kind,
          tabs: vec![tab],
          active_tab_id,
        }
      }
      Err(e) => {
        eprintln!("Failed to create default terminal: {}", e);
        Self {
          kind,
          tabs: vec![],
          active_tab_id: None,
        }
      }
    }
  }

  /// 用给定的 PTY 创建初始 tab，便于测试向终端注入输出（如 OSC 标题序列）。
  pub(crate) fn new_with_pty(
    kind: WorkspaceKind,
    pty: Arc<dyn crate::terminal::Pty>,
    cx: &mut gpui::Context<Self>,
  ) -> Self {
    match Self::make_tab_with_pty(cx, pty) {
      Ok(tab) => {
        let active_tab_id = Some(tab.id);
        Self {
          kind,
          tabs: vec![tab],
          active_tab_id,
        }
      }
      Err(e) => {
        eprintln!("Failed to create default terminal: {}", e);
        Self {
          kind,
          tabs: vec![],
          active_tab_id: None,
        }
      }
    }
  }

  fn make_tab_with_fake_pty(cx: &mut gpui::Context<Self>) -> Result<TabItem, String> {
    let pty = Arc::new(crate::terminal::FakePty::new()) as Arc<dyn crate::terminal::Pty>;
    Self::make_tab_with_pty(cx, pty)
  }

  /// 用给定的 PTY 创建初始 tab，供需要向终端注入数据的测试使用。
  fn make_tab_with_pty(
    cx: &mut gpui::Context<Self>,
    pty: Arc<dyn crate::terminal::Pty>,
  ) -> Result<TabItem, String> {
    let terminal_view = Self::create_terminal_view_with_pty(cx, pty)?;
    let workspace_handle = cx.entity().clone();
    let pane_group = cx.new(|cx| PaneGroup::new(workspace_handle, terminal_view, cx));
    Ok(TabItem {
      id: generate_tab_id(),
      pane_group,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::workspace_kind::WorkspaceKind;
  use gpui::TestAppContext;

  /// 创建一个使用 FakePty 的 Workspace 实体。
  fn make_workspace(cx: &mut TestAppContext, kind: WorkspaceKind) -> Entity<Workspace> {
    use gpui::AppContext as _;
    cx.new(|cx| Workspace::new_with_fake_pty(kind, cx))
  }

  #[gpui::test]
  fn new_workspace_has_single_active_tab(cx: &mut TestAppContext) {
    let ws = make_workspace(cx, WorkspaceKind::Local);
    let (tabs, active) = ws.read_with(cx, |w, _| (w.tabs.len(), w.active_tab_id));
    assert_eq!(tabs, 1);
    assert!(active.is_some());
  }

  #[gpui::test]
  fn active_tab_and_index_are_consistent(cx: &mut TestAppContext) {
    let ws = make_workspace(cx, WorkspaceKind::Local);
    ws.update(cx, |w, cx| {
      let id = w.active_tab_id.expect("has active tab");
      assert_eq!(w.active_tab().map(|t| t.id), Some(id));
      assert_eq!(w.active_index(), Some(0));
      let _ = cx;
    });
  }

  #[gpui::test]
  fn activate_tab_returns_false_for_unknown(cx: &mut TestAppContext) {
    let ws = make_workspace(cx, WorkspaceKind::Local);
    ws.update(cx, |w, cx| {
      let original = w.active_tab_id;
      let ok = w.activate_tab(TabId(9999), cx);
      assert!(!ok);
      assert_eq!(w.active_tab_id, original);
    });
  }

  #[gpui::test]
  fn close_tab_falls_back_to_previous(cx: &mut TestAppContext) {
    let ws = make_workspace(cx, WorkspaceKind::Local);
    // 预置两个额外的 tab：直接构造 TabItem 并添加，避免再次创建终端
    ws.update(cx, |w, cx| {
      // 借用现有 tab 的 pane_group 作为占位，仅用于测试 close 索引逻辑
      let placeholder_pane = w.tabs[0].pane_group.clone();
      let tab1 = TabItem {
        id: generate_tab_id(),
        pane_group: placeholder_pane.clone(),
      };
      let tab2 = TabItem {
        id: generate_tab_id(),
        pane_group: placeholder_pane,
      };
      w.add_tab(tab1, cx);
      w.add_tab(tab2, cx);
    });

    ws.update(cx, |w, cx| {
      // 现在有 3 个 tab，激活的是最后一个（tab2）
      let active_id = w.active_tab_id.expect("active");
      // 关闭激活的 tab，应当回退到上一个
      assert!(w.close_tab(active_id, cx));
      assert_eq!(w.tabs.len(), 2);
      assert!(w.active_tab_id.is_some());
    });
  }

  #[gpui::test]
  fn close_unknown_tab_returns_false(cx: &mut TestAppContext) {
    let ws = make_workspace(cx, WorkspaceKind::Local);
    ws.update(cx, |w, cx| {
      assert!(!w.close_tab(TabId(9999), cx));
      assert_eq!(w.tabs.len(), 1);
    });
  }

  #[gpui::test]
  fn workspace_display_name_and_icon_delegate_to_kind(cx: &mut TestAppContext) {
    let ws = make_workspace(cx, WorkspaceKind::Ssh("ssh h".to_string()));
    ws.read_with(cx, |w, _| {
      assert_eq!(w.display_name().as_ref(), "ssh h");
    });
  }

  /// 注入 OSC 标题序列后，验证 tab 实际展示的标题（即 PaneGroup::active_leaf_title）
  /// 能跟随更新。覆盖链路：
  /// PTY 输出 → Terminal 解析 OSC → TerminalEvent::TitleChanged →
  /// TerminalViewEvent::TitleChanged → Workspace/PaneGroup 订阅 → active_leaf_title。
  #[gpui::test]
  fn tab_title_updates_from_osc_sequence(cx: &mut TestAppContext) {
    use crate::terminal::fake_pty::EchoMode;
    use gpui::AppContext as _;

    // 保留底层 FakePty 引用，以便注入输出
    let fake = Arc::new(crate::terminal::FakePty::with_echo_mode(EchoMode::None));
    let pty_dyn: Arc<dyn crate::terminal::Pty> = fake.clone();
    let ws = cx.new(|cx| Workspace::new_with_pty(WorkspaceKind::Local, pty_dyn, cx));

    // 初始 tab 标题应为 "Terminal"
    let initial = ws.read_with(cx, |w, cx| {
      let pane = w.active_tab().expect("has tab").pane_group.clone();
      pane.read(cx).active_leaf_title(cx)
    });
    assert_eq!(initial.as_deref(), Some("Terminal"));

    // 注入 OSC 标题序列: ESC ] 2 ; My Tab Title BEL
    fake.push_bytes("\x1b]2;My Tab Title\x07").unwrap();
    cx.run_until_parked();

    // tab 标题应已更新为新标题
    let updated = ws.read_with(cx, |w, cx| {
      let pane = w.active_tab().expect("has tab").pane_group.clone();
      pane.read(cx).active_leaf_title(cx)
    });
    assert_eq!(updated.as_deref(), Some("My Tab Title"));
  }

  /// 验证空标题的 OSC 序列不会把 tab 标题覆盖成空字符串。
  #[gpui::test]
  fn tab_title_not_overwritten_by_empty_osc(cx: &mut TestAppContext) {
    use crate::terminal::fake_pty::EchoMode;
    use gpui::AppContext as _;

    let fake = Arc::new(crate::terminal::FakePty::with_echo_mode(EchoMode::None));
    let pty_dyn: Arc<dyn crate::terminal::Pty> = fake.clone();
    let ws = cx.new(|cx| Workspace::new_with_pty(WorkspaceKind::Local, pty_dyn, cx));

    // 先设置一个标题
    fake.push_bytes("\x1b]2;Real Title\x07").unwrap();
    cx.run_until_parked();
    let title = ws.read_with(cx, |w, cx| {
      let pane = w.active_tab().expect("has tab").pane_group.clone();
      pane.read(cx).active_leaf_title(cx)
    });
    assert_eq!(title.as_deref(), Some("Real Title"));

    // 再注入空标题，tab 标题不应被覆盖
    fake.push_bytes("\x1b]2;   \x07").unwrap();
    cx.run_until_parked();
    let title = ws.read_with(cx, |w, cx| {
      let pane = w.active_tab().expect("has tab").pane_group.clone();
      pane.read(cx).active_leaf_title(cx)
    });
    assert_eq!(title.as_deref(), Some("Real Title"));
  }
}
