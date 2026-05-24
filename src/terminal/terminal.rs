use crate::terminal::content::{
  CursorState, IndexedCell, TerminalBounds, TerminalContent, TerminalEvent, TerminalPoint,
};
use crate::terminal::pty::{Pty, TerminalSize};
use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::{Config, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use async_channel::{Sender, unbounded};
use async_lock::Mutex;
use gpui::*;
use std::sync::Arc;
use tracing::debug;

/// 默认滚动历史行数
const DEFAULT_SCROLL_HISTORY_LINES: usize = 10_000;
/// 最大滚动历史行数
pub const MAX_SCROLL_HISTORY_LINES: usize = 100_000;

/// 终端尺寸结构，用于 alacritty 的 Dimensions trait
#[derive(Clone, Copy, Debug)]
struct TermDimensions {
  columns: usize,
  screen_lines: usize,
}

impl Dimensions for TermDimensions {
  fn total_lines(&self) -> usize {
    self.screen_lines
  }

  fn screen_lines(&self) -> usize {
    self.screen_lines
  }

  fn columns(&self) -> usize {
    self.columns
  }
}

impl From<TerminalBounds> for TermDimensions {
  fn from(bounds: TerminalBounds) -> Self {
    Self {
      columns: bounds.num_columns(),
      screen_lines: bounds.num_lines(),
    }
  }
}

impl From<TerminalSize> for TermDimensions {
  fn from(size: TerminalSize) -> Self {
    Self {
      columns: size.cols as usize,
      screen_lines: size.rows as usize,
    }
  }
}

/// 内部事件（类似 Zed 的 InternalEvent）
#[derive(Clone, Debug)]
enum InternalEvent {
  /// 调整终端大小
  Resize(TerminalBounds),
  /// 滚动
  Scroll(alacritty_terminal::grid::Scroll),
  /// 设置选区
  SetSelection(Option<alacritty_terminal::selection::Selection>),
  /// 更新选区
  UpdateSelection(TerminalPoint),
  /// 清除屏幕
  Clear,
  /// 复制选区
  Copy,
  /// 粘贴
  Paste(String),
}

/// 终端事件监听器 - 使用 mpsc channel 转发 alacritty 事件到后台任务
#[derive(Clone)]
struct EventProxy(Sender<alacritty_terminal::event::Event>);

impl EventListener for EventProxy {
  fn send_event(&self, event: alacritty_terminal::event::Event) {
    if let Err(e) = self.0.send_blocking(event) {
      debug!(target: "terminal", "Failed to send terminal event: {}", e);
    }
  }
}

struct Term {
  term: alacritty_terminal::Term<EventProxy>,
  parser: Processor<alacritty_terminal::vte::ansi::StdSyncHandler>,
}

impl Term {
  pub fn new<D: Dimensions>(
    config: alacritty_terminal::term::Config,
    dimensions: &D,
    event_proxy: EventProxy,
  ) -> Self {
    Self {
      term: alacritty_terminal::Term::new(config, dimensions, event_proxy),
      parser: Processor::new(),
    }
  }

  pub fn advance(&mut self, data: Vec<u8>) {
    self.parser.advance(&mut self.term, &data)
  }

  pub fn resize(&mut self, dimensions: &TermDimensions) {
    self.term.resize(*dimensions);
  }

  pub fn dimensions(&self) -> TermDimensions {
    TermDimensions {
      columns: self.term.columns() as usize,
      screen_lines: self.term.screen_lines() as usize,
    }
  }

  pub fn extract(&self) -> ExtractedTerminalData {
    let content = self.term.renderable_content();
    let mut cells = Vec::new();
    for indexed in content.display_iter {
      cells.push(IndexedCell {
        point: TerminalPoint {
          line: indexed.point.line - content.display_offset,
          column: indexed.point.column,
        },
        cell: indexed.cell.clone(),
      });
    }
    let cursor = content.cursor;
    let cursor_state = CursorState {
      point: TerminalPoint {
        line: cursor.point.line - content.display_offset,
        column: cursor.point.column,
      },
      shape: cursor.shape,
    };
    let cursor_char = cells
      .iter()
      .find(|cell| {
        cell.point.line == cursor_state.point.line && cell.point.column == cursor_state.point.column
      })
      .map(|cell| cell.cell.c)
      .unwrap_or(' ');
    ExtractedTerminalData {
      cells,
      cursor_state,
      cursor_char,
      mode: content.mode,
      display_offset: content.display_offset,
    }
  }
}

struct ExtractedTerminalData {
  cells: Vec<IndexedCell>,
  cursor_state: CursorState,
  cursor_char: char,
  mode: TermMode,
  display_offset: usize,
}

/// 终端协调器
///
/// ## 数据流设计
///
/// 采用「生产-消费」分离模式，按需同步 alacritty 内部状态到可渲染的 TerminalContent：
///
/// ```text
/// PTY 数据到达
///   → background_spawn: term.lock().advance(data)   ← 只做 VTE 解析
///   → entity.update: cx.notify()                     ← 只发信号，不提取数据
///   → GPUI 帧循环触发 TerminalElement::prepaint()
///     → terminal.refresh_content(cx)
///       → term.lock().extract()                      ← 开锁提取渲染数据
///       → apply_extracted_data()
///     → 读取 content → paint
/// ```
///
/// 这个设计的优势：
/// - **按需同步**：只有被渲染的 Tab 才执行 extract，后台 Tab 白白保持 alacritty 状态但不消耗 extract 的 CPU
/// - **帧级合并**：一帧内无论收到多少 PTY 数据块，prepaint 只 extract 一次
/// - **职责分离**：PTY reader 管"生产"（advance），prepaint 管"消费"（extract + paint）
pub struct Terminal {
  content: TerminalContent,
  term: Arc<Mutex<Term>>,
  pty: Arc<dyn Pty>,
  terminal_size: Option<TerminalSize>,
  display_offset: usize,
  selection_head: Option<TerminalPoint>,
  title: String,
  mouse_mode: bool,
  user_has_scrolled: bool,
}

impl Terminal {
  /// 创建新的终端
  pub fn new(pty: Arc<dyn Pty>, cx: &mut Context<Self>) -> Result<Self> {
    let initial_size = TerminalSize::default_size();
    let term_dimensions = TermDimensions::from(initial_size);

    let term_config = Config {
      scrolling_history: DEFAULT_SCROLL_HISTORY_LINES,
      ..Config::default()
    };

    let (events_tx, events_rx) = unbounded::<alacritty_terminal::event::Event>();

    let term = Arc::new(Mutex::new(Term::new(
      term_config,
      &term_dimensions,
      EventProxy(events_tx),
    )));

    let entity = cx.entity().clone();
    let pty_reader = pty.reader();

    // PTY 读取任务：「生产」侧
    // 从 PTY 获取原始数据 → VTE 解析写入 alacritty Term → notify UI 线程
    // 这里不提取渲染数据，提取放在 prepaint 阶段按需执行
    let event_term = term.clone();
    cx.spawn(async move |_, cx| -> Result<()> {
      let term = event_term;
      loop {
        let data = pty_reader.recv().await?;
        let term = term.clone();
        cx.background_spawn(async move {
          term.lock().await.advance(data);
        })
        .await;

        entity.update(cx, |_, cx| cx.notify())?;
      }
    })
    .detach();

    // alacritty 事件处理
    // 处理 PTY 回写（光标位置响应等）、标题变更、响铃、退出等异步事件
    let pty_clone = pty.clone();
    cx.spawn(async move |entity, cx| -> Result<()> {
      use alacritty_terminal::event::Event;
      loop {
        let event = events_rx.recv().await?;

        match event {
          Event::Title(title) => {
            entity.update(cx, |terminal, cx| {
              terminal.title = title.clone();
              cx.emit(TerminalEvent::TitleChanged(title));
            })?;
          }
          Event::PtyWrite(data) => {
            pty_clone.write(data.into_bytes()).await?;
          }
          Event::Wakeup => {
            entity.update(cx, |_, cx| cx.notify())?;
          }
          Event::Bell => {
            entity.update(cx, |_, cx| {
              cx.emit(TerminalEvent::Bell);
            })?;
          }
          Event::Exit | Event::ChildExit(_) => {
            entity.update(cx, |_, cx| {
              cx.emit(TerminalEvent::Closed);
            })?;
          }
          _ => {}
        }
      }
    })
    .detach();

    let content = TerminalContent::new();

    Ok(Self {
      content,
      term,
      pty,
      terminal_size: None,
      display_offset: 0,
      selection_head: None,
      title: "Terminal".to_string(),
      mouse_mode: false,
      user_has_scrolled: false,
    })
  }

  /// 创建仅显示的终端（无 PTY，用于测试或显示静态内容）
  pub fn new_display_only(cx: &mut Context<Self>) -> anyhow::Result<Self> {
    use crate::terminal::local_pty::LocalPty;

    let size = TerminalSize::default_size();
    let pty = LocalPty::new(size, None)?;

    Self::new(Arc::new(pty), cx)
  }

  /// 发送输入数据到终端
  pub fn input(&mut self, cx: &mut Context<Self>, data: Vec<u8>) {
    self.scroll_to_bottom(false, cx);

    let pty = self.pty.clone();
    cx.spawn(async move |_, _| pty.write(data).await).detach();
  }

  /// 根据视图实际尺寸同步终端行列数
  ///
  /// 首次调用时（terminal_size 为 None）使用视图的实际尺寸替代默认的 24x80，
  /// 后续调用仅在尺寸变化时才 resize alacritty Term 和 PTY。
  pub fn sync_size(
    &mut self,
    bounds: Bounds<Pixels>,
    char_width: Pixels,
    char_height: Pixels,
    cx: &mut Context<Self>,
  ) {
    let height: f32 = bounds.size.height.into();
    let char_h: f32 = char_height.into();
    let rows = ((height / char_h).floor() as u16).max(1);
    let width: f32 = bounds.size.width.into();
    let char_w: f32 = char_width.into();
    let cols = ((width / char_w).floor() as u16).max(1);
    let new_size = TerminalSize::new(rows, cols, 0, 0);

    if self.terminal_size == Some(new_size) {
      return;
    }

    self.terminal_size = Some(new_size);

    let dimensions = TermDimensions::from(new_size);
    cx.background_executor()
      .block(self.term.lock())
      .resize(&dimensions);

    let pty = self.pty.clone();
    cx.spawn(async move |_, _| {
      let _ = pty.resize(new_size).await;
    })
    .detach();
  }

  /// 滚动终端
  pub fn scroll(
    &mut self,
    scroll: alacritty_terminal::grid::Scroll,
    user_initiated: bool,
    cx: &mut Context<Self>,
  ) {
    cx.background_executor()
      .block(self.term.lock())
      .term
      .scroll_display(scroll);
    if user_initiated {
      self.user_has_scrolled = true;
    }
    cx.notify();
  }

  /// 向上滚动一行
  pub fn scroll_line_up(&mut self, user_initiated: bool, cx: &mut Context<Self>) {
    use alacritty_terminal::grid::Scroll;
    self.scroll(Scroll::Delta(1), user_initiated, cx);
  }

  /// 向下滚动一行
  pub fn scroll_line_down(&mut self, user_initiated: bool, cx: &mut Context<Self>) {
    use alacritty_terminal::grid::Scroll;
    self.scroll(Scroll::Delta(-1), user_initiated, cx);
  }

  /// 向上滚动一页
  pub fn scroll_page_up(&mut self, user_initiated: bool, cx: &mut Context<Self>) {
    use alacritty_terminal::grid::Scroll;
    self.scroll(Scroll::PageUp, user_initiated, cx);
  }

  /// 向下滚动一页
  pub fn scroll_page_down(&mut self, user_initiated: bool, cx: &mut Context<Self>) {
    use alacritty_terminal::grid::Scroll;
    self.scroll(Scroll::PageDown, user_initiated, cx);
  }

  /// 滚动到顶部
  pub fn scroll_to_top(&mut self, user_initiated: bool, cx: &mut Context<Self>) {
    use alacritty_terminal::grid::Scroll;
    self.scroll(Scroll::Top, user_initiated, cx);
  }

  /// 滚动到底部
  pub fn scroll_to_bottom(&mut self, user_initiated: bool, cx: &mut Context<Self>) {
    use alacritty_terminal::grid::Scroll;
    self.scroll(Scroll::Bottom, user_initiated, cx);
  }

  /// 获取当前内容的引用
  pub fn content(&self) -> &TerminalContent {
    &self.content
  }

  /// 获取终端标题
  pub fn title(&self) -> &str {
    &self.title
  }

  /// 用户是否手动滚动过（即不在自动跟随底部状态）
  pub fn user_has_scrolled(&self) -> bool {
    self.user_has_scrolled
  }

  /// 是否滚动到顶部
  pub fn scrolled_to_top(&self) -> bool {
    self.content.scrolled_to_top
  }

  /// 是否滚动到底部
  pub fn scrolled_to_bottom(&self) -> bool {
    self.content.scrolled_to_bottom
  }

  /// 从 alacritty Term 提取最新内容并更新到 TerminalContent
  ///
  /// 在 prepaint 阶段调用，确保渲染前数据是最新的。
  /// 如果用户未手动滚动，自动滚动到底部以跟随新输出。
  pub fn refresh_content(&mut self, cx: &mut Context<Self>) {
    let mut term = cx.background_executor().block(self.term.lock());

    if !self.user_has_scrolled {
      term
        .term
        .scroll_display(alacritty_terminal::grid::Scroll::Bottom);
    }

    let extracted = term.extract();
    drop(term);

    if self.user_has_scrolled && extracted.display_offset == 0 {
      self.user_has_scrolled = false;
    }

    self.apply_extracted_data(extracted);
  }

  fn apply_extracted_data(&mut self, data: ExtractedTerminalData) {
    self.content.cells = data.cells;
    self.content.cursor = data.cursor_state;
    self.content.cursor_char = data.cursor_char;
    self.content.mode = data.mode;
    self.content.display_offset = data.display_offset;
  }
}

impl EventEmitter<TerminalEvent> for Terminal {}

crate::impl_id!(Terminal);
