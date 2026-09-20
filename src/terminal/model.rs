use crate::terminal::content::{
  CursorState, IndexedCell, SelectionRange, TerminalContent, TerminalEvent, TerminalPoint,
};
use crate::terminal::pty::{Pty, TerminalSize};
use crate::terminal::title::{DEFAULT_TERMINAL_TITLE, normalize_title};
use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use async_channel::{Receiver, Sender, unbounded};
use async_lock::Mutex;
use gpui::*;
use std::future::{Future, poll_fn};
use std::pin::pin;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;
use tracing::debug;

/// 默认滚动历史行数
const DEFAULT_SCROLL_HISTORY_LINES: usize = 10_000;

/// 从首块输出开始计时，为 120Hz 帧预算保留解析和绘制时间。
pub(super) const OUTPUT_BATCH_WINDOW: Duration = Duration::from_millis(3);
/// 达到上限时提前解析，避免持续输出导致 buffer 和单次持锁时间无限增长。
/// 保留完整 PTY 数据块，因此实际批次最多会超过此阈值一个数据块。
const OUTPUT_BATCH_MAX_BYTES: usize = 256 * 1024;

/// 空闲时等待首块数据；窗口内复用 buffer 合并字节，EOF 时先交付剩余输出。
async fn read_output_batch(
  reader: &Receiver<Vec<u8>>,
  executor: &BackgroundExecutor,
  buffer: &mut Vec<u8>,
) -> bool {
  buffer.clear();
  while buffer.is_empty() {
    let Ok(data) = reader.recv().await else {
      return false;
    };
    buffer.extend_from_slice(&data);
  }

  let deadline = executor.now() + OUTPUT_BATCH_WINDOW;
  let mut timer = pin!(executor.timer(OUTPUT_BATCH_WINDOW));
  while buffer.len() < OUTPUT_BATCH_MAX_BYTES {
    let mut receive = pin!(reader.recv());
    let data = poll_fn(|cx| {
      // 优先检查固定截止时间，持续就绪的 reader 不能饿死定时器。
      if executor.now() >= deadline || timer.as_mut().poll(cx).is_ready() {
        return Poll::Ready(None);
      }
      receive.as_mut().poll(cx).map(Result::ok)
    })
    .await;
    let Some(data) = data else { break };
    buffer.extend_from_slice(&data);
  }
  true
}

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

  pub fn advance(&mut self, data: &[u8]) {
    self.parser.advance(&mut self.term, data)
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
///   → background_spawn: 合并首块起 3ms 内的字节（达到大小上限时提前提交）
///   → background_spawn: term.lock().advance(data)   ← 只做 VTE 解析
///   → entity.update: cx.notify()                     ← 每批只发一次信号，不提取数据
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
  pty: Option<Arc<dyn Pty>>,
  terminal_size: Option<TerminalSize>,
  default_title: String,
  application_title: Option<String>,
  user_has_scrolled: bool,
  selection: Option<SelectionRange>,
  closed: bool,
  _reader_task: Task<()>,
  _event_task: Task<()>,
}

impl Terminal {
  /// 创建新的终端
  #[cfg(test)]
  pub fn new(pty: Arc<dyn Pty>, cx: &mut Context<Self>) -> Result<Self> {
    Self::new_with_default_title(pty, DEFAULT_TERMINAL_TITLE, cx)
  }

  /// 使用启动命令生成的默认标题创建终端。
  ///
  /// 应用发送的 OSC 标题优先于默认标题；清空或重置 OSC 标题后回退。
  pub fn new_with_default_title(
    pty: Arc<dyn Pty>,
    default_title: impl Into<String>,
    cx: &mut Context<Self>,
  ) -> Result<Self> {
    let initial_size = pty.initial_size();
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

    let pty_reader = pty.reader();

    // PTY 读取任务：「生产」侧
    // 后台合并 PTY 原始数据 → 每批一次 VTE 解析 → notify UI 线程
    // 这里不提取渲染数据，提取放在 prepaint 阶段按需执行
    let event_term = term.clone();
    let reader_task = cx.spawn(async move |entity, cx| {
      let term = event_term;
      let mut buffer = Vec::new();
      loop {
        let term = term.clone();
        let reader = pty_reader.clone();
        let executor = cx.background_executor().clone();
        let batch = cx
          .background_spawn(async move {
            if !read_output_batch(&reader, &executor, &mut buffer).await {
              return None;
            }
            term.lock().await.advance(&buffer);
            Some(buffer)
          })
          .await;
        let Some(data) = batch else { break };
        buffer = data;

        if entity.update(cx, |_, cx| cx.notify()).is_err() {
          return;
        }
      }

      let _ = entity.update(cx, |terminal, cx| terminal.mark_closed(cx));
    });

    // alacritty 事件处理
    // 处理 PTY 回写（光标位置响应等）、标题变更、响铃、退出等异步事件
    let event_task = cx.spawn(async move |entity, cx| {
      use alacritty_terminal::event::Event;
      loop {
        let event = match events_rx.recv().await {
          Ok(event) => event,
          Err(_) => break,
        };

        match event {
          Event::Title(title) => {
            let title = normalize_title(&title);
            if entity
              .update(cx, |terminal, cx| {
                terminal.set_application_title(title, cx);
              })
              .is_err()
            {
              break;
            }
          }
          Event::ResetTitle => {
            let entity_dropped = entity
              .update(cx, |terminal, cx| {
                terminal.set_application_title(None, cx);
              })
              .is_err();
            if entity_dropped {
              break;
            }
          }
          Event::PtyWrite(data) => {
            let pty = entity
              .read_with(cx, |terminal, _| terminal.pty.clone())
              .ok()
              .flatten();
            if let Some(pty) = pty
              && pty.needs_terminal_responses()
            {
              let _ = pty.write(data.into_bytes()).await;
            }
          }
          Event::Wakeup => {
            let _ = entity.update(cx, |_, cx| cx.notify());
          }
          Event::Bell => {
            let _ = entity.update(cx, |_, cx| {
              cx.emit(TerminalEvent::Bell);
            });
          }
          Event::Exit | Event::ChildExit(_) => {
            let _ = entity.update(cx, |terminal, cx| terminal.mark_closed(cx));
            break;
          }
          _ => {}
        }
      }
    });

    let content = TerminalContent::new();

    let default_title =
      normalize_title(&default_title.into()).unwrap_or_else(|| DEFAULT_TERMINAL_TITLE.to_string());

    Ok(Self {
      content,
      term,
      pty: Some(pty),
      terminal_size: None,
      default_title,
      application_title: None,
      user_has_scrolled: false,
      selection: None,
      closed: false,
      _reader_task: reader_task,
      _event_task: event_task,
    })
  }

  fn set_application_title(&mut self, title: Option<String>, cx: &mut Context<Self>) {
    if self.application_title == title {
      return;
    }

    let previous_title = self.title().to_string();
    self.application_title = title;
    if self.title() != previous_title {
      cx.emit(TerminalEvent::TitleChanged);
      cx.notify();
    }
  }

  fn mark_closed(&mut self, cx: &mut Context<Self>) {
    if self.closed {
      return;
    }

    self.closed = true;
    self.pty.take();
    cx.emit(TerminalEvent::Closed);
    cx.notify();
  }

  /// 发送输入数据到终端
  pub fn input(&mut self, cx: &mut Context<Self>, data: Vec<u8>) {
    if data.is_empty() {
      return;
    }

    self.scroll_to_bottom(false, cx);
    self.clear_selection(cx);

    let Some(pty) = self.pty.clone() else {
      return;
    };
    cx.spawn(async move |_, _| pty.write(data).await).detach();
  }

  /// 粘贴文本到终端
  pub fn paste(&mut self, cx: &mut Context<Self>, text: String) {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let data = if self.content.mode.contains(TermMode::BRACKETED_PASTE) {
      // ESC 可以在剪贴正文中构造 ESC[201~ 提前结束 bracketed paste，
      // 使后续字节被 shell 当作普通按键执行。保留可见文本，但移除 ESC。
      let sanitized = normalized.replace('\x1b', "");
      let mut data = Vec::with_capacity(sanitized.len() + 12);
      data.extend_from_slice(b"\x1b[200~");
      data.extend_from_slice(sanitized.as_bytes());
      data.extend_from_slice(b"\x1b[201~");
      data
    } else {
      normalized.into_bytes()
    };

    self.input(cx, data);
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

    if let Some(pty) = self.pty.clone() {
      cx.spawn(async move |_, _| {
        let _ = pty.resize(new_size).await;
      })
      .detach();
    }
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
    self
      .application_title
      .as_deref()
      .unwrap_or(&self.default_title)
  }

  /// 子进程是否已退出
  pub fn is_closed(&self) -> bool {
    self.closed
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
  #[allow(dead_code)]
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
      for c in cols.values() {
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
  c.is_whitespace()
}

impl EventEmitter<TerminalEvent> for Terminal {}

#[cfg(test)]
mod tests {
  use super::{OUTPUT_BATCH_MAX_BYTES, OUTPUT_BATCH_WINDOW, Pty, Terminal, is_word_boundary};
  use crate::terminal::content::{IndexedCell, TerminalPoint};
  use crate::terminal::fake_pty::FakePty;
  use crate::terminal::{LocalPty, PtyCommand, TerminalSize};
  use alacritty_terminal::term::TermMode;
  use gpui::{AppContext as _, Bounds, Entity, TestAppContext, point, px, size};
  use std::cell::Cell;
  use std::path::Path;
  use std::process::Command;
  use std::rc::Rc;
  use std::sync::Arc;
  use std::time::{Duration, Instant};

  /// 用 FakePty 创建一个 Terminal 实体。
  fn make_terminal(cx: &mut TestAppContext) -> Entity<Terminal> {
    let pty = Arc::new(FakePty::new()) as Arc<dyn Pty>;
    cx.new(|cx| Terminal::new(pty, cx).expect("create terminal"))
  }

  fn first_row(terminal: &Entity<Terminal>, cx: &mut TestAppContext) -> String {
    terminal.update(cx, |terminal, cx| {
      terminal.refresh_content(cx);
      terminal
        .content()
        .cells
        .iter()
        .filter(|cell| cell.point.line.0 == 0)
        .map(|cell| cell.cell.c)
        .collect::<String>()
        .trim_end()
        .to_string()
    })
  }

  fn count_notifications(
    terminal: &Entity<Terminal>,
    cx: &mut TestAppContext,
  ) -> (Rc<Cell<usize>>, gpui::Subscription) {
    let count = Rc::new(Cell::new(0));
    let observed = count.clone();
    let subscription = cx.update(|cx| {
      cx.observe(terminal, move |_, _| {
        observed.set(observed.get() + 1);
      })
    });
    (count, subscription)
  }

  #[gpui::test]
  fn output_window_merges_chunks_without_extending_deadline(cx: &mut TestAppContext) {
    let fake = Arc::new(FakePty::new());
    let terminal = cx.new(|cx| Terminal::new(fake.clone(), cx).unwrap());
    let (notifications, _subscription) = count_notifications(&terminal, cx);

    for text in ["one", " two", " three"] {
      fake.push_bytes(text).unwrap();
      cx.run_until_parked();
      assert_eq!(notifications.get(), 0);
      assert_eq!(first_row(&terminal, cx), "");
      cx.background_executor
        .advance_clock(Duration::from_millis(1));
    }
    cx.run_until_parked();
    assert_eq!(notifications.get(), 1);
    assert_eq!(first_row(&terminal, cx), "one two three");

    cx.background_executor
      .advance_clock(Duration::from_millis(9));
    cx.run_until_parked();
    assert_eq!(notifications.get(), 1, "idle terminals should not notify");

    fake.push_bytes("!").unwrap();
    crate::terminal::flush_pty_output(cx);
    assert_eq!(notifications.get(), 2, "a lone chunk must also be flushed");
    assert_eq!(first_row(&terminal, cx), "one two three!");
  }

  #[gpui::test]
  fn output_size_limit_flushes_early_and_preserves_following_bytes(cx: &mut TestAppContext) {
    let fake = Arc::new(FakePty::new());
    let terminal = cx.new(|cx| Terminal::new(fake.clone(), cx).unwrap());
    let (notifications, _subscription) = count_notifications(&terminal, cx);

    // 多个小块填满批次；回车不增加滚动历史，末尾可见字符验证数据顺序。
    for _ in 0..OUTPUT_BATCH_MAX_BYTES / 4096 {
      fake.push_output(vec![b'\r'; 4096]).unwrap();
    }
    fake.push_bytes("tail").unwrap();
    cx.run_until_parked();
    assert_eq!(notifications.get(), 1, "full batches flush before 3ms");
    assert_eq!(first_row(&terminal, cx), "");

    crate::terminal::flush_pty_output(cx);
    assert_eq!(notifications.get(), 2);
    assert_eq!(first_row(&terminal, cx), "tail");
  }

  #[gpui::test]
  fn eof_flushes_buffer_before_closed_event(cx: &mut TestAppContext) {
    let fake = Arc::new(FakePty::new());
    let terminal = cx.new(|cx| Terminal::new(fake.clone(), cx).unwrap());
    let closed_row = Rc::new(std::cell::RefCell::new(None));
    let observed = closed_row.clone();
    let _subscription = cx.update(|cx| {
      cx.subscribe(&terminal, move |terminal, event, cx| {
        if matches!(event, crate::terminal::content::TerminalEvent::Closed) {
          terminal.update(cx, |terminal, cx| {
            terminal.refresh_content(cx);
            let text: String = terminal
              .content()
              .cells
              .iter()
              .filter(|cell| cell.point.line.0 == 0)
              .map(|cell| cell.cell.c)
              .collect();
            *observed.borrow_mut() = Some(text.trim_end().to_string());
          });
        }
      })
    });

    fake.push_bytes("final ").unwrap();
    cx.run_until_parked();
    fake.push_bytes("output").unwrap();
    fake.close_reader();
    cx.run_until_parked();

    assert!(terminal.read_with(cx, |terminal, _| terminal.is_closed()));
    assert_eq!(closed_row.borrow().as_deref(), Some("final output"));
  }

  #[gpui::test]
  fn batching_preserves_split_utf8_and_escape_sequences(cx: &mut TestAppContext) {
    let fake = Arc::new(FakePty::new());
    let terminal = cx.new(|cx| Terminal::new(fake.clone(), cx).unwrap());

    // UTF-8 字符与 CSI 都跨越批次边界；后半个 CSI 还分散在多个 PTY 块中。
    fake.push_output(vec![0xe4]).unwrap();
    crate::terminal::flush_pty_output(cx);
    fake
      .push_output(vec![0xbd, 0xa0, 0x1b, b'[', b'?'])
      .unwrap();
    crate::terminal::flush_pty_output(cx);
    fake.push_bytes("2004").unwrap();
    fake.push_bytes("h!").unwrap();
    crate::terminal::flush_pty_output(cx);

    assert!(first_row(&terminal, cx).starts_with('你'));
    assert!(first_row(&terminal, cx).ends_with('!'));
    assert!(terminal.read_with(cx, |terminal, _| {
      terminal.content().mode.contains(TermMode::BRACKETED_PASTE)
    }));

    fake.push_bytes("\x1b[").unwrap();
    fake.push_bytes("6n").unwrap();
    cx.run_until_parked();
    assert!(fake.writes().is_empty());
    cx.background_executor.advance_clock(OUTPUT_BATCH_WINDOW);
    cx.run_until_parked();
    assert_eq!(fake.writes_string(), "\x1b[1;4R");
  }

  #[gpui::test]
  fn dropping_terminal_cancels_pending_output_batch(cx: &mut TestAppContext) {
    let fake = Arc::new(FakePty::new());
    let terminal = cx.new(|cx| Terminal::new(fake.clone(), cx).unwrap());
    let weak = terminal.downgrade();
    fake.push_bytes("pending").unwrap();
    cx.run_until_parked();

    drop(terminal);
    crate::terminal::flush_pty_output(cx);
    assert!(weak.upgrade().is_none());
  }

  fn echo_pty_command() -> Option<PtyCommand> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/echo-pty.js");
    let node = std::env::var("CATUS_NODE").unwrap_or_else(|_| "node".to_string());
    if !Command::new(&node)
      .arg("--version")
      .output()
      .ok()?
      .status
      .success()
    {
      return None;
    }
    Some(PtyCommand::program(
      node,
      [script.to_string_lossy().into_owned()],
    ))
  }

  /// 构造一个指定字符的单元格。
  fn cell_with(c: char) -> alacritty_terminal::term::cell::Cell {
    alacritty_terminal::term::cell::Cell {
      c,
      ..Default::default()
    }
  }

  /// 构造位于 (line, col) 的单元格。
  fn indexed(line: i32, col: usize, c: char) -> IndexedCell {
    IndexedCell {
      point: TerminalPoint {
        line: alacritty_terminal::index::Line(line),
        column: alacritty_terminal::index::Column(col),
      },
      cell: cell_with(c),
    }
  }

  #[gpui::test]
  fn is_word_boundary_classifies_chars() {
    // 简单版本：仅以空格类字符作为单词边界，标点不分割单词
    assert!(is_word_boundary(' '));
    assert!(is_word_boundary('\t'));
    assert!(is_word_boundary('\n'));
    assert!(!is_word_boundary('.'));
    assert!(!is_word_boundary(','));
    assert!(!is_word_boundary('/'));
    assert!(!is_word_boundary('_'));
    assert!(!is_word_boundary('a'));
    assert!(!is_word_boundary('Z'));
    assert!(!is_word_boundary('0'));
  }

  #[gpui::test]
  fn reader_eof_marks_terminal_closed(cx: &mut TestAppContext) {
    let fake = Arc::new(FakePty::new());
    let pty = fake.clone() as Arc<dyn Pty>;
    let terminal = cx.new(|cx| Terminal::new(pty, cx).expect("create terminal"));

    fake.close_reader();
    cx.run_until_parked();

    assert!(terminal.read_with(cx, |terminal, _| terminal.is_closed()));
  }

  #[gpui::test]
  fn reader_task_does_not_keep_terminal_alive(cx: &mut TestAppContext) {
    let terminal = make_terminal(cx);
    let weak_terminal = terminal.downgrade();

    drop(terminal);
    cx.run_until_parked();

    assert!(weak_terminal.upgrade().is_none());
  }

  #[gpui::test]
  fn point_from_pixel_maps_to_grid(cx: &mut TestAppContext) {
    let terminal = make_terminal(cx);
    terminal.update(cx, |t, _cx| {
      // bounds 从 (10,20) 开始，char 宽 8、高 16
      let bounds = Bounds {
        origin: point(px(10.), px(20.)),
        size: size(px(800.), px(384.)),
      };
      let p = t.point_from_pixel(
        point(px(10. + 3. * 8.), px(20. + 5. * 16.)),
        bounds,
        px(8.),
        px(16.),
      );
      assert_eq!(p.line.0, 5);
      assert_eq!(p.column.0, 3);
    });
  }

  #[gpui::test]
  fn point_from_pixel_clamps_negative(cx: &mut TestAppContext) {
    let terminal = make_terminal(cx);
    terminal.update(cx, |t, _cx| {
      let bounds = Bounds {
        origin: point(px(10.), px(20.)),
        size: size(px(800.), px(384.)),
      };
      let p = t.point_from_pixel(point(px(0.), px(0.)), bounds, px(8.), px(16.));
      assert_eq!(p.line.0, 0);
      assert_eq!(p.column.0, 0);
    });
  }

  #[gpui::test]
  fn echo_input_appears_in_content(cx: &mut TestAppContext) {
    let Some(command) = echo_pty_command() else {
      eprintln!("skipping real PTY echo test because Node.js is unavailable");
      return;
    };
    let pty = Arc::new(
      LocalPty::new_with_command(TerminalSize::default_size(), command).expect("create echo PTY"),
    ) as Arc<dyn Pty>;
    let terminal = cx.new(|cx| Terminal::new(pty, cx).expect("create terminal"));

    // 先等待 OSC 标题，确认 Node 脚本已启动并进入 raw mode，
    // 避免输入被 PTY 默认的内核 echo 回显而产生假阳性。
    let title_deadline = Instant::now() + Duration::from_secs(5);
    while terminal.read_with(cx, |terminal, _| terminal.title() != "Echo") {
      assert!(
        Instant::now() < title_deadline,
        "echo helper did not initialize"
      );
      crate::terminal::flush_pty_output(cx);
      std::thread::sleep(Duration::from_millis(10));
    }

    terminal.update(cx, |t, cx| t.input(cx, b"hi".to_vec()));
    let mut row0 = String::new();
    let echo_deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < echo_deadline {
      crate::terminal::flush_pty_output(cx);
      terminal.update(cx, |t, cx| t.refresh_content(cx));
      row0 = terminal.read_with(cx, |t, _| {
        t.content()
          .cells
          .iter()
          .filter(|c| c.point.line.0 == 0)
          .map(|c| c.cell.c)
          .collect()
      });
      if row0.starts_with("hi") {
        break;
      }
      std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
      row0.starts_with("hi"),
      "row should start with 'hi', got: {:?}",
      row0
    );
  }

  #[gpui::test]
  fn title_updates_from_osc_sequence(cx: &mut TestAppContext) {
    // 保留底层 FakePty 引用，以便在 Terminal 构造后向其 reader 注入输出。
    let fake = Arc::new(FakePty::new());
    let pty_dyn: Arc<dyn Pty> = fake.clone();
    let terminal = cx.new(|cx| Terminal::new(pty_dyn, cx).expect("create terminal"));

    // 注入 OSC 标题序列: ESC ] 2 ; My Title BEL
    fake.push_bytes("\x1b]2;My Title\x07").unwrap();
    crate::terminal::flush_pty_output(cx);

    let title = terminal.read_with(cx, |t, _| t.title().to_string());
    assert_eq!(title, "My Title");
  }

  #[gpui::test]
  fn empty_title_osc_restores_default_title(cx: &mut TestAppContext) {
    let fake = Arc::new(FakePty::new());
    let pty_dyn: Arc<dyn Pty> = fake.clone();
    let terminal =
      cx.new(|cx| Terminal::new_with_default_title(pty_dyn, "zsh", cx).expect("create terminal"));

    assert_eq!(terminal.read_with(cx, |t, _| t.title().to_string()), "zsh");

    // 先设置一个标题
    fake.push_bytes("\x1b]2;Real Title\x07").unwrap();
    crate::terminal::flush_pty_output(cx);
    assert_eq!(
      terminal.read_with(cx, |t, _| t.title().to_string()),
      "Real Title"
    );

    // 再注入空标题，清除应用标题并回退到启动程序。
    fake.push_bytes("\x1b]2;   \x07").unwrap();
    crate::terminal::flush_pty_output(cx);
    assert_eq!(terminal.read_with(cx, |t, _| t.title().to_string()), "zsh");
  }

  #[gpui::test]
  fn reset_title_event_restores_default_title(cx: &mut TestAppContext) {
    let fake = Arc::new(FakePty::new());
    let pty_dyn: Arc<dyn Pty> = fake.clone();
    let terminal =
      cx.new(|cx| Terminal::new_with_default_title(pty_dyn, "zsh", cx).expect("create terminal"));

    // 保存初始的 None 标题、设置应用标题，然后从标题栈恢复 None。
    fake
      .push_bytes("\x1b[22t\x1b]2;Temporary\x07\x1b[23t")
      .unwrap();
    crate::terminal::flush_pty_output(cx);

    assert_eq!(terminal.read_with(cx, |t, _| t.title().to_string()), "zsh");
  }

  #[gpui::test]
  fn application_title_is_trimmed(cx: &mut TestAppContext) {
    let fake = Arc::new(FakePty::new());
    let pty_dyn: Arc<dyn Pty> = fake.clone();
    let terminal = cx.new(|cx| Terminal::new(pty_dyn, cx).expect("create terminal"));

    fake.push_bytes("\x1b]2;  dev server  \x07").unwrap();
    crate::terminal::flush_pty_output(cx);

    assert_eq!(
      terminal.read_with(cx, |t, _| t.title().to_string()),
      "dev server"
    );
  }

  #[gpui::test]
  fn selected_text_extracts_selection(cx: &mut TestAppContext) {
    let terminal = make_terminal(cx);
    terminal.update(cx, |t, cx| {
      // 手动填充 content.cells
      t.content.cells = vec![
        indexed(0, 0, 'h'),
        indexed(0, 1, 'e'),
        indexed(0, 2, 'l'),
        indexed(0, 3, 'l'),
        indexed(0, 4, 'o'),
        indexed(1, 0, 'w'),
        indexed(1, 1, 'o'),
        indexed(1, 2, 'r'),
        indexed(1, 3, 'l'),
        indexed(1, 4, 'd'),
      ];
      t.set_selection_start(
        TerminalPoint {
          line: alacritty_terminal::index::Line(0),
          column: alacritty_terminal::index::Column(1),
        },
        cx,
      );
      t.set_selection_end(
        TerminalPoint {
          line: alacritty_terminal::index::Line(1),
          column: alacritty_terminal::index::Column(2),
        },
        cx,
      );
    });

    let text = terminal.read_with(cx, |t, _| t.selected_text());
    assert_eq!(text, "ello\nwor");
  }

  #[gpui::test]
  fn selected_text_empty_without_selection(cx: &mut TestAppContext) {
    let terminal = make_terminal(cx);
    let text = terminal.read_with(cx, |t, _| t.selected_text());
    assert_eq!(text, "");
  }

  #[gpui::test]
  fn clear_selection_resets(cx: &mut TestAppContext) {
    let terminal = make_terminal(cx);
    terminal.update(cx, |t, cx| {
      t.set_selection_start(TerminalPoint::default(), cx);
      assert!(t.selection().is_some());
      t.clear_selection(cx);
      assert!(t.selection().is_none());
    });
  }

  #[gpui::test]
  fn select_word_at_selects_contiguous_word(cx: &mut TestAppContext) {
    let terminal = make_terminal(cx);
    terminal.update(cx, |t, cx| {
      // 行：foo bar baz（以空格分隔）
      t.content.cells = vec![
        indexed(0, 0, 'f'),
        indexed(0, 1, 'o'),
        indexed(0, 2, 'o'),
        indexed(0, 3, ' '),
        indexed(0, 4, 'b'),
        indexed(0, 5, 'a'),
        indexed(0, 6, 'r'),
        indexed(0, 7, ' '),
        indexed(0, 8, 'b'),
        indexed(0, 9, 'a'),
        indexed(0, 10, 'z'),
      ];
      // 点击 'b' (col 4)，应当选中 "bar"
      t.select_word_at(
        TerminalPoint {
          line: alacritty_terminal::index::Line(0),
          column: alacritty_terminal::index::Column(4),
        },
        cx,
      );
      let sel = t.selection().expect("selection set");
      assert_eq!(sel.start.column.0, 4);
      assert_eq!(sel.end.column.0, 6);
      assert_eq!(t.selected_text(), "bar");
    });
  }

  #[gpui::test]
  fn select_word_at_on_boundary_selects_single_cell(cx: &mut TestAppContext) {
    let terminal = make_terminal(cx);
    terminal.update(cx, |t, cx| {
      t.content.cells = vec![
        indexed(0, 0, 'a'),
        indexed(0, 1, 'b'),
        indexed(0, 2, ' '),
        indexed(0, 3, 'c'),
      ];
      // 点击空格（单词边界）
      t.select_word_at(
        TerminalPoint {
          line: alacritty_terminal::index::Line(0),
          column: alacritty_terminal::index::Column(2),
        },
        cx,
      );
      let sel = t.selection().expect("selection set");
      assert_eq!(sel.start, sel.end);
      assert_eq!(sel.start.column.0, 2);
    });
  }

  #[gpui::test]
  fn paste_wraps_with_brackets_when_mode_enabled(cx: &mut TestAppContext) {
    let fake = Arc::new(FakePty::new());
    let pty_dyn: Arc<dyn Pty> = fake.clone();
    let terminal = cx.new(|cx| Terminal::new(pty_dyn, cx).expect("create terminal"));

    // 开启 bracketed paste 模式
    fake.push_bytes("\x1b[?2004h").unwrap();
    crate::terminal::flush_pty_output(cx);
    terminal.update(cx, |t, cx| t.refresh_content(cx));
    let bracketed = terminal.read_with(cx, |t, _| {
      t.content().mode.contains(TermMode::BRACKETED_PASTE)
    });
    assert!(bracketed, "bracketed paste mode should be enabled");

    // 粘贴文本，应当被 bracket 包裹
    terminal.update(cx, |t, cx| t.paste(cx, "hello".to_string()));
    cx.run_until_parked();

    let written = fake.writes_string();
    assert!(
      written.contains("\x1b[200~hello\x1b[201~"),
      "expected bracketed paste, got: {:?}",
      written
    );
  }

  #[gpui::test]
  fn bracketed_paste_removes_embedded_escape_sequences(cx: &mut TestAppContext) {
    let fake = Arc::new(FakePty::new());
    let pty = fake.clone() as Arc<dyn Pty>;
    let terminal = cx.new(|cx| Terminal::new(pty, cx).expect("create terminal"));

    fake.push_bytes("\x1b[?2004h").unwrap();
    crate::terminal::flush_pty_output(cx);
    terminal.update(cx, |terminal, cx| terminal.refresh_content(cx));

    terminal.update(cx, |terminal, cx| {
      terminal.paste(cx, "safe\x1b[201~printf injected\r".to_string())
    });
    cx.run_until_parked();

    let written = fake.writes_string();
    assert_eq!(written.matches("\x1b[201~").count(), 1);
    assert_eq!(written, "\x1b[200~safe[201~printf injected\n\x1b[201~");
  }

  #[gpui::test]
  fn paste_without_brackets_when_mode_disabled(cx: &mut TestAppContext) {
    let fake = Arc::new(FakePty::new());
    let pty_dyn: Arc<dyn Pty> = fake.clone();
    let terminal = cx.new(|cx| Terminal::new(pty_dyn, cx).expect("create terminal"));

    // 默认未开启 bracketed paste
    terminal.update(cx, |t, cx| t.paste(cx, "hello\r\nworld".to_string()));
    cx.run_until_parked();

    let written = fake.writes_string();
    assert!(!written.contains("\x1b[200~"));
    assert!(written.contains("hello\nworld"));
  }

  #[gpui::test]
  fn sync_size_resizes_alacritty_and_pty(cx: &mut TestAppContext) {
    let fake = Arc::new(FakePty::new());
    let pty_dyn: Arc<dyn Pty> = fake.clone();
    let terminal = cx.new(|cx| Terminal::new(pty_dyn, cx).expect("create terminal"));

    terminal.update(cx, |t, cx| {
      t.sync_size(
        Bounds {
          origin: point(px(0.), px(0.)),
          size: size(px(800.), px(384.)),
        },
        px(8.),
        px(16.),
        cx,
      );
    });
    cx.run_until_parked();

    // 800/8 = 100 列，384/16 = 24 行
    let resizes = fake.resizes();
    assert_eq!(resizes.len(), 1);
    assert_eq!(resizes[0].cols, 100);
    assert_eq!(resizes[0].rows, 24);
  }

  #[gpui::test]
  fn sync_size_skips_when_unchanged(cx: &mut TestAppContext) {
    let fake = Arc::new(FakePty::new());
    let pty_dyn: Arc<dyn Pty> = fake.clone();
    let terminal = cx.new(|cx| Terminal::new(pty_dyn, cx).expect("create terminal"));

    let bounds = Bounds {
      origin: point(px(0.), px(0.)),
      size: size(px(800.), px(384.)),
    };
    terminal.update(cx, |t, cx| t.sync_size(bounds, px(8.), px(16.), cx));
    cx.run_until_parked();
    assert_eq!(fake.resizes().len(), 1);

    // 同样尺寸再次调用不应触发 resize
    terminal.update(cx, |t, cx| t.sync_size(bounds, px(8.), px(16.), cx));
    cx.run_until_parked();
    assert_eq!(fake.resizes().len(), 1);
  }

  #[gpui::test]
  fn scroll_sets_user_scrolled_flag(cx: &mut TestAppContext) {
    let terminal = make_terminal(cx);
    terminal.update(cx, |t, cx| {
      assert!(!t.user_has_scrolled());
      t.scroll_lines(3, true, cx);
      assert!(t.user_has_scrolled());
    });
  }

  #[gpui::test]
  fn terminal_starts_not_closed(cx: &mut TestAppContext) {
    let terminal = make_terminal(cx);
    let closed = terminal.read_with(cx, |t, _| t.is_closed());
    assert!(!closed);
    let title = terminal.read_with(cx, |t, _| t.title().to_string());
    assert_eq!(title, "Terminal");
  }
}
