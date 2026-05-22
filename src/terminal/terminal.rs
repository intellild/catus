use crate::terminal::content::{
  CursorState, IndexedCell, TerminalBounds, TerminalContent, TerminalEvent, TerminalPoint,
};
use crate::terminal::pty::{Pty, TerminalSize};
use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::{Config, RenderableCursor, TermMode};
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

  pub fn extract(&self) -> ExtractedTerminalData {
    let content = self.term.renderable_content();
    let mut cells = Vec::new();
    for indexed in content.display_iter {
      cells.push(IndexedCell {
        point: TerminalPoint {
          line: indexed.point.line,
          column: indexed.point.column,
        },
        cell: indexed.cell.clone(),
      });
    }
    let cursor = content.cursor;
    let cursor_state = renderable_cursor_to_state(cursor);
    let cursor_char = cells
      .iter()
      .find(|cell| cell.point.line == cursor.point.line && cell.point.column == cursor.point.column)
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
  display_offset: usize,
  selection_head: Option<TerminalPoint>,
  title: String,
  mouse_mode: bool,
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
    // 处理标题变更、响铃、退出等异步事件
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
      display_offset: 0,
      selection_head: None,
      title: "Terminal".to_string(),
      mouse_mode: false,
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
    self.scroll_to_bottom();

    let pty = self.pty.clone();
    cx.spawn(async move |_, _| pty.write(data).await).detach();
  }

  /// 调整终端大小
  pub async fn resize(&mut self, size: TerminalSize) -> Result<()> {
    self.pty.resize(size).await
  }

  /// 滚动终端
  pub fn scroll(&mut self, scroll: alacritty_terminal::grid::Scroll) {
    // self.events.push_back(InternalEvent::Scroll(scroll));
  }

  /// 向上滚动一行
  pub fn scroll_line_up(&mut self) {
    use alacritty_terminal::grid::Scroll;
    self.scroll(Scroll::Delta(1));
  }

  /// 向下滚动一行
  pub fn scroll_line_down(&mut self) {
    use alacritty_terminal::grid::Scroll;
    self.scroll(Scroll::Delta(-1));
  }

  /// 向上滚动一页
  pub fn scroll_page_up(&mut self) {
    use alacritty_terminal::grid::Scroll;
    self.scroll(Scroll::PageUp);
  }

  /// 向下滚动一页
  pub fn scroll_page_down(&mut self) {
    use alacritty_terminal::grid::Scroll;
    self.scroll(Scroll::PageDown);
  }

  /// 滚动到顶部
  pub fn scroll_to_top(&mut self) {
    use alacritty_terminal::grid::Scroll;
    self.scroll(Scroll::Top);
  }

  /// 滚动到底部
  pub fn scroll_to_bottom(&mut self) {
    use alacritty_terminal::grid::Scroll;
    self.scroll(Scroll::Bottom);
  }

  /// 获取当前内容的引用
  pub fn content(&self) -> &TerminalContent {
    &self.content
  }

  /// 获取终端标题
  pub fn title(&self) -> &str {
    &self.title
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
  /// 注意：这里不调用 cx.notify()，因为 prepaint 本身就在帧循环中，
  /// 调用方（PTY reader / Event Wakeup）已经触发过 notify。
  pub fn refresh_content(&mut self, cx: &mut Context<Self>) {
    let extracted = cx.background_executor().block(self.term.lock()).extract();
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

/// 将 alacritty 的 RenderableCursor 转换为 CursorState
fn renderable_cursor_to_state(cursor: RenderableCursor) -> CursorState {
  CursorState {
    point: TerminalPoint {
      line: cursor.point.line,
      column: cursor.point.column,
    },
    shape: cursor.shape,
  }
}

impl EventEmitter<TerminalEvent> for Terminal {}

crate::impl_id!(Terminal);
