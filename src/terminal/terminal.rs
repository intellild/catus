use crate::terminal::content::{
  CursorState, IndexedCell, SelectionRange, TerminalContent, TerminalEvent, TerminalPoint,
};
use crate::terminal::pty::{Pty, TerminalSize};
use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use async_channel::{Sender, unbounded};
use async_lock::Mutex;
use gpui::*;
use std::sync::Arc;
use tracing::debug;

/// 默认滚动历史行数
const DEFAULT_SCROLL_HISTORY_LINES: usize = 10_000;

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

impl From<TerminalSize> for TermDimensions {
  fn from(size: TerminalSize) -> Self {
    Self {
      columns: size.cols as usize,
      screen_lines: size.rows as usize,
    }
  }
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

  pub fn extract(&self) -> ExtractedTerminalData {
    let content = self.term.renderable_content();
    let display_offset = content.display_offset;
    let screen_lines = self.term.screen_lines() as i32;
    let mut cells = Vec::new();
    for indexed in content.display_iter {
      let line: i32 = indexed.point.line.0 + display_offset as i32;
      if line < 0 || line >= screen_lines {
        continue;
      }
      cells.push(IndexedCell {
        point: TerminalPoint {
          line: alacritty_terminal::index::Line(line),
          column: indexed.point.column,
        },
        cell: indexed.cell.clone(),
      });
    }
    let cursor = content.cursor;
    let cursor_line = cursor.point.line.0 + display_offset as i32;
    let cursor_state = CursorState {
      point: TerminalPoint {
        line: alacritty_terminal::index::Line(cursor_line),
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
      display_offset,
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
  title: String,
  user_has_scrolled: bool,
  selection: Option<SelectionRange>,
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
              cx.emit(TerminalEvent::TitleChanged);
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
      title: "Terminal".to_string(),
      user_has_scrolled: false,
      selection: None,
    })
  }

  /// 发送输入数据到终端
  pub fn input(&mut self, cx: &mut Context<Self>, data: Vec<u8>) {
    self.scroll_to_bottom(false, cx);
    self.clear_selection(cx);

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

  /// 滚动指定行数（正数向上，负数向下）
  pub fn scroll_lines(&mut self, lines: i32, user_initiated: bool, cx: &mut Context<Self>) {
    use alacritty_terminal::grid::Scroll;
    self.scroll(Scroll::Delta(lines), user_initiated, cx);
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

  /// 是否滚动到底部
  pub fn scrolled_to_bottom(&self) -> bool {
    self.content.scrolled_to_bottom
  }

  /// 设置选择起点
  pub fn set_selection_start(&mut self, point: TerminalPoint, cx: &mut Context<Self>) {
    self.selection = Some(SelectionRange {
      start: point,
      end: point,
    });
    cx.notify();
  }

  /// 设置选择终点
  pub fn set_selection_end(&mut self, point: TerminalPoint, cx: &mut Context<Self>) {
    if let Some(ref mut selection) = self.selection {
      selection.end = point;
      cx.notify();
    }
  }

  /// 清除选择
  pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
    if self.selection.take().is_some() {
      cx.notify();
    }
  }

  /// 获取当前选择
  pub fn selection(&self) -> Option<SelectionRange> {
    self.selection
  }

  /// 将像素坐标转换为终端坐标
  pub fn point_from_pixel(
    &self,
    position: Point<Pixels>,
    bounds: Bounds<Pixels>,
    char_width: Pixels,
    char_height: Pixels,
  ) -> TerminalPoint {
    let relative_x = (position.x - bounds.origin.x).max(px(0.));
    let relative_y = (position.y - bounds.origin.y).max(px(0.));
    let col = (relative_x / char_width).floor() as usize;
    let row = (relative_y / char_height).floor() as usize;
    TerminalPoint {
      line: alacritty_terminal::index::Line(row as i32),
      column: alacritty_terminal::index::Column(col),
    }
  }

  /// 双击选中单词
  pub fn select_word_at(&mut self, point: TerminalPoint, cx: &mut Context<Self>) {
    let cells = &self.content.cells;
    let row = point.line.0;

    let mut row_cells: Vec<_> = cells
      .iter()
      .filter(|c| c.point.line.0 == row && !c.cell.flags.contains(Flags::WIDE_CHAR_SPACER))
      .collect();
    row_cells.sort_by_key(|c| c.point.column.0);

    let clicked_idx = row_cells
      .iter()
      .position(|c| c.point.column.0 == point.column.0)
      .unwrap_or(0);

    let clicked_char = row_cells.get(clicked_idx).map(|c| c.cell.c).unwrap_or(' ');

    if is_word_boundary(clicked_char) {
      self.selection = Some(SelectionRange {
        start: point,
        end: point,
      });
      cx.notify();
      return;
    }

    let mut start_idx = clicked_idx;
    for (i, _cell) in row_cells[..clicked_idx].iter().enumerate().rev() {
      if is_word_boundary(row_cells[i].cell.c) {
        start_idx = i + 1;
        break;
      }
      start_idx = i;
    }

    let mut end_idx = clicked_idx;
    for (i, _cell) in row_cells[clicked_idx + 1..].iter().enumerate() {
      if is_word_boundary(row_cells[clicked_idx + 1 + i].cell.c) {
        end_idx = clicked_idx + i;
        break;
      }
      end_idx = clicked_idx + 1 + i;
    }

    let start_col = row_cells[start_idx].point.column.0;
    let end_col = row_cells[end_idx].point.column.0;

    self.selection = Some(SelectionRange {
      start: TerminalPoint {
        line: point.line,
        column: alacritty_terminal::index::Column(start_col),
      },
      end: TerminalPoint {
        line: point.line,
        column: alacritty_terminal::index::Column(end_col),
      },
    });
    cx.notify();
  }

  /// 获取选中的文本
  pub fn selected_text(&self) -> String {
    let Some(selection) = self.selection else {
      return String::new();
    };

    let mut lines: std::collections::BTreeMap<i32, std::collections::BTreeMap<i32, char>> =
      std::collections::BTreeMap::new();

    for indexed in &self.content.cells {
      if indexed.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
        continue;
      }
      if selection.contains(indexed.point) {
        let row = indexed.point.line.0;
        let col = indexed.point.column.0 as i32;
        lines.entry(row).or_default().insert(col, indexed.cell.c);
      }
    }

    let mut text = String::new();
    for (i, (_row, cols)) in lines.iter().enumerate() {
      if i > 0 {
        text.push('\n');
      }
      for (_col, c) in cols {
        text.push(*c);
      }
    }

    text
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

    let was_user_scrolled = self.user_has_scrolled;
    if self.user_has_scrolled && extracted.display_offset == 0 {
      self.user_has_scrolled = false;
    }

    self.content.scrolled_to_bottom = extracted.display_offset == 0;
    self.apply_extracted_data(extracted);

    if was_user_scrolled && !self.user_has_scrolled {
      cx.notify();
    }
  }

  fn apply_extracted_data(&mut self, data: ExtractedTerminalData) {
    self.content = TerminalContent {
      cells: data.cells,
      mode: data.mode,
      display_offset: data.display_offset,
      cursor: data.cursor_state,
      cursor_char: data.cursor_char,
      scrolled_to_bottom: self.content.scrolled_to_bottom,
      selection: self.selection,
    };
  }
}

fn is_word_boundary(c: char) -> bool {
  c.is_whitespace() || c.is_ascii_punctuation()
}

impl EventEmitter<TerminalEvent> for Terminal {}

crate::impl_id!(Terminal);
