use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use gpui::{AppContext, Entity, SharedString};
use gpui_component::IconName;

use crate::terminal::{LocalPty, Terminal, TerminalSize};

/// Tab ID generator
static TAB_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

pub fn generate_tab_id() -> TabId {
  TabId(TAB_ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Tab 类型
#[derive(Clone)]
pub enum TabType {
  /// 终端 Tab
  Terminal(Arc<Terminal>),
  /// SFTP Tab (TODO: 实现)
  Sftp,
}

/// Tab 状态（标题、图标等）
#[derive(Clone)]
pub struct TabState {
  pub title: SharedString,
  pub icon: IconName,
}

impl TabState {
  pub fn new(title: impl Into<SharedString>, icon: IconName) -> Self {
    Self {
      title: title.into(),
      icon,
    }
  }
}

/// Tab 项
#[derive(Clone)]
pub struct TabItem {
  pub id: TabId,
  pub state: Entity<TabState>,
  pub tab_type: TabType,
}

impl TabItem {
  /// 创建一个新的 TabItem
  pub fn new(
    cx: &mut gpui::Context<Workspace>,
    title: impl Into<SharedString>,
    icon: IconName,
    tab_type: TabType,
  ) -> Self {
    let state = cx.new(|_cx| TabState::new(title, icon));

    Self {
      id: generate_tab_id(),
      state,
      tab_type,
    }
  }

  /// 创建一个新的 Terminal Tab
  ///
  /// # Returns
  /// * `Ok(Self)` - 成功创建 Tab
  /// * `Err(String)` - 创建失败，返回错误信息
  pub fn new_terminal(
    cx: &mut gpui::Context<Workspace>,
    rows: usize,
    cols: usize,
  ) -> Result<Self, String> {
    // 创建 Terminal - 先不加 Arc
    let mut terminal =
      Terminal::new(cx).map_err(|e| format!("Failed to create terminal: {}", e))?;

    // 创建本地 PTY
    let size = TerminalSize::new(rows as u16, cols as u16, 0, 0);
    let pty = LocalPty::new(size, None).map_err(|e| format!("Failed to create PTY: {}", e))?;

    // 将 PTY 附加到 Terminal
    terminal.attach_pty(Box::new(pty), cx);

    // 现在包装成 Arc
    let terminal_arc = Arc::new(terminal);

    Ok(Self {
      id: generate_tab_id(),
      state: cx.new(|_cx| TabState::new("Terminal", IconName::File)),
      tab_type: TabType::Terminal(terminal_arc),
    })
  }

  /// 创建一个新的 SFTP Tab
  pub fn new_sftp(cx: &mut gpui::Context<Workspace>) -> Self {
    Self {
      id: generate_tab_id(),
      state: cx.new(|_cx| TabState::new("SFTP", IconName::Folder)),
      tab_type: TabType::Sftp,
    }
  }
}

/// Workspace 代表一个工作区，直接管理多个 Tab
pub struct Workspace {
  /// 所有 Tab
  pub tabs: Vec<TabItem>,
  /// 当前激活的 Tab ID
  pub active_tab_id: Option<TabId>,
}

impl Workspace {
  /// 创建一个新的 Workspace
  /// 如果没有 Tab，会自动创建一个默认的 Terminal Tab
  pub fn new(cx: &mut gpui::Context<Self>) -> Self {
    // 创建一个默认的 Terminal Tab
    let tabs = match TabItem::new_terminal(cx, 24, 80) {
      Ok(tab) => vec![tab],
      Err(e) => {
        eprintln!("Failed to create default terminal tab: {}", e);
        // 如果创建终端失败，创建一个空的 Workspace
        vec![]
      }
    };
    let active_tab_id = tabs.first().map(|tab| tab.id);

    Self {
      tabs,
      active_tab_id,
    }
  }

  /// 添加一个新的 Tab
  pub fn add_tab(&mut self, tab: TabItem) -> TabId {
    let id = tab.id;
    self.tabs.push(tab);
    self.active_tab_id = Some(id);
    id
  }

  /// 关闭指定的 Tab
  pub fn close_tab(&mut self, id: TabId) -> bool {
    if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
      self.tabs.remove(index);

      // Update active tab if needed
      if self.active_tab_id == Some(id) {
        self.active_tab_id = self.tabs.get(index.saturating_sub(1)).map(|t| t.id);
      }
      return true;
    }
    false
  }

  /// 激活指定的 Tab
  pub fn activate_tab(&mut self, id: TabId) -> bool {
    if self.tabs.iter().any(|t| t.id == id) {
      self.active_tab_id = Some(id);
      true
    } else {
      false
    }
  }

  /// 获取当前激活的 Tab
  pub fn active_tab(&self) -> Option<&TabItem> {
    self
      .active_tab_id
      .and_then(|id| self.tabs.iter().find(|t| t.id == id))
  }

  /// 获取当前激活的 Tab 索引
  pub fn active_index(&self) -> Option<usize> {
    self
      .active_tab_id
      .and_then(|id| self.tabs.iter().position(|t| t.id == id))
  }

  /// 添加一个新的 Terminal Tab
  ///
  /// # Returns
  /// * `Ok(TabId)` - 成功创建并添加 Tab
  /// * `Err(String)` - 创建失败，返回错误信息
  pub fn add_terminal_tab(&mut self, cx: &mut gpui::Context<Self>) -> Result<TabId, String> {
    let tab = TabItem::new_terminal(cx, 24, 80)?;
    Ok(self.add_tab(tab))
  }

  /// 添加一个新的 SFTP Tab
  pub fn add_sftp_tab(&mut self, cx: &mut gpui::Context<Self>) -> TabId {
    let tab = TabItem::new_sftp(cx);
    self.add_tab(tab)
  }
}
