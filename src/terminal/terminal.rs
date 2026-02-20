use crate::terminal::content::{
  IndexedCell, TerminalBounds, TerminalContent, TerminalEvent, TerminalPoint,
  renderable_cursor_to_state,
};
use crate::terminal::input::TerminalInput;
use crate::terminal::pty::{Pty, TerminalSize};
use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use gpui::*;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

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
struct ChannelEventListener(mpsc::UnboundedSender<alacritty_terminal::event::Event>);

impl EventListener for ChannelEventListener {
  fn send_event(&self, event: alacritty_terminal::event::Event) {
    // 使用 ok() 忽略发送失败（接收端已关闭的情况）
    let _ = self.0.send(event);
  }
}

/// 终端后台任务句柄
struct TerminalTasks {
  /// 向后台任务发送输入
  input_tx: mpsc::Sender<TerminalInput>,
  /// 后台任务句柄
  _task: Task<()>,
  /// UI 更新任务句柄
  _ui_task: Task<()>,
}

/// 终端协调器 - 参考 Zed 的实现
///
/// Terminal 是 GPUI Entity，负责：
/// 1. 管理 alacritty Term 状态
/// 2. 处理内部事件队列
/// 3. 与后台 PTY 任务通信
/// 4. 生成可渲染的 TerminalContent
pub struct Terminal {
  /// 终端内容（直接存储，非 Entity）
  pub content: TerminalContent,
  /// alacritty 终端状态（使用 Arc<Mutex> 以便在后台任务中访问）
  term: Arc<async_lock::Mutex<Term<ChannelEventListener>>>,
  /// 终端配置
  term_config: Config,
  /// 内部事件队列（类似 Zed 的 events）
  events: VecDeque<InternalEvent>,
  /// 后台任务相关
  tasks: Option<TerminalTasks>,
  /// 当前显示偏移
  display_offset: usize,
  /// 选区头部位置
  selection_head: Option<TerminalPoint>,
  /// 终端标题
  title: String,
  /// 鼠标模式状态
  mouse_mode: bool,
}

impl Terminal {
  /// 创建新的终端，直接传入 PTY
  ///
  /// # Arguments
  /// * `pty` - PTY 实现
  /// * `cx` - GPUI Context
  pub fn new(pty: Box<dyn Pty>, cx: &mut Context<Self>) -> anyhow::Result<Self> {
    // 创建初始尺寸
    let initial_size = TerminalSize::default_size();
    let term_dimensions = TermDimensions::from(initial_size);

    // 创建终端配置
    let term_config = Config {
      scrolling_history: DEFAULT_SCROLL_HISTORY_LINES,
      ..Config::default()
    };

    // 创建事件通道（alacritty → Terminal）
    let (events_tx, mut events_rx) = mpsc::unbounded_channel::<alacritty_terminal::event::Event>();

    // 创建终端
    let term = Term::new(
      term_config.clone(),
      &term_dimensions,
      ChannelEventListener(events_tx),
    );
    let term = Arc::new(async_lock::Mutex::new(term));

    // 创建输入通道（UI → 后台任务）
    let (input_tx, mut input_rx) = mpsc::channel::<TerminalInput>(256);

    // 创建内容广播通道（后台任务 → UI）
    let (content_tx, _content_rx) = watch::channel(TerminalContent::new());

    // 克隆用于后台任务的 Arc
    let term_for_task = term.clone();

    // 获取实体句柄（用于后台任务更新内容）
    let entity = cx.entity().clone();

    // 启动后台任务处理 PTY 和终端事件
    let background_task = cx.background_spawn(async move {
      let pty = pty;
      let term = term_for_task;
      let mut parser = Processor::<alacritty_terminal::vte::ansi::StdSyncHandler>::new();

      // 启动 PTY 读取器
      let pty_reader = pty.start_reader();

      loop {
        tokio::select! {
            // 处理来自 UI 的输入
            Some(input) = input_rx.recv() => {
                match input {
                    TerminalInput::Write(data) => {
                        if let Err(e) = pty.write(&data) {
                            eprintln!("PTY write error: {}", e);
                        }
                    }
                    TerminalInput::Resize(size) => {
                        let dims = TermDimensions::from(size);
                        let mut term_guard = term.lock().await;
                        term_guard.resize(dims);
                        drop(term_guard);
                        if let Err(e) = pty.resize(size) {
                            eprintln!("PTY resize error: {}", e);
                        }
                        // 强制同步内容
                        let content = Self::make_content_sync(&term).await;
                        let _ = content_tx.send(content);
                    }
                    TerminalInput::PtyData(data) => {
                        // 处理 PTY 数据
                        let mut term_guard = term.lock().await;
                        parser.advance(&mut *term_guard, &data);
                        drop(term_guard);
                        // 更新内容
                        let content = Self::make_content_sync(&term).await;
                        let _ = content_tx.send(content);
                    }
                    TerminalInput::Sync => {
                        // 强制同步内容
                        let content = Self::make_content_sync(&term).await;
                        let _ = content_tx.send(content);
                    }
                    TerminalInput::Shutdown => {
                        let _ = pty.close();
                        break;
                    }
                }
            }

            // 处理 PTY 读取的数据
            Ok(data) = pty_reader.recv() => {
                let mut term_guard = term.lock().await;
                parser.advance(&mut *term_guard, &data);
                drop(term_guard);
                // 更新内容
                let content = Self::make_content_sync(&term).await;
                let _ = content_tx.send(content);
            }

            // 处理 alacritty 事件
            Some(event) = events_rx.recv() => {
                Self::process_alacritty_event(&event, &content_tx).await;
            }

            else => break,
        }
      }
    });

    // 启动 UI 更新任务 - 监听内容变化并更新 Terminal 实体
    let mut content_rx_for_ui = _content_rx.clone();
    let ui_task = cx.spawn(async move |this, cx| {
      loop {
        // 等待内容变化
        if content_rx_for_ui.changed().await.is_err() {
          break;
        }

        let content = content_rx_for_ui.borrow().clone();

        // 更新 Terminal 实体的 content 字段
        this.update(cx, |terminal, cx| {
          terminal.content = content;
          cx.emit(TerminalEvent::Wakeup);
          cx.notify();
        });
      }
    });

    let content = TerminalContent::new();

    Ok(Self {
      content,
      term,
      term_config,
      events: VecDeque::new(),
      tasks: Some(TerminalTasks {
        input_tx,
        _task: background_task,
        _ui_task: ui_task,
      }),
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

    Self::new(Box::new(pty), cx)
  }

  /// 处理 alacritty 事件（后台任务中调用）
  async fn process_alacritty_event(
    event: &alacritty_terminal::event::Event,
    _content_tx: &watch::Sender<TerminalContent>,
  ) {
    use alacritty_terminal::event::Event as AlacEvent;

    match event {
      AlacEvent::Title(_title) => {}
      AlacEvent::Wakeup => {}
      AlacEvent::Bell => {}
      AlacEvent::Exit => {}
      _ => {}
    }
  }

  /// 从 Term 生成 TerminalContent（后台任务中调用）
  async fn make_content_sync(
    term: &Arc<async_lock::Mutex<Term<ChannelEventListener>>>,
  ) -> TerminalContent {
    let term_guard = term.lock().await;
    let content = term_guard.renderable_content();

    let estimated_size = content.display_iter.size_hint().0;
    let mut cells = Vec::with_capacity(estimated_size);

    for indexed in content.display_iter {
      cells.push(IndexedCell {
        point: TerminalPoint {
          line: indexed.point.line,
          column: indexed.point.column,
        },
        cell: indexed.cell.clone(),
      });
    }

    let cursor_char = term_guard.grid()[content.cursor.point].c;

    let selection = content
      .selection
      .map(|range| crate::terminal::content::SelectionRange {
        start: TerminalPoint {
          line: range.start.line,
          column: range.start.column,
        },
        end: TerminalPoint {
          line: range.end.line,
          column: range.end.column,
        },
      });

    let scrolled_to_top = content.display_offset == term_guard.history_size();
    let scrolled_to_bottom = content.display_offset == 0;

    TerminalContent {
      cells,
      mode: content.mode,
      display_offset: content.display_offset,
      selection,
      cursor: renderable_cursor_to_state(&content.cursor),
      cursor_char,
      terminal_bounds: TerminalBounds::new(
        px(8.0),
        px(16.0),
        Bounds::default(),
        term_guard.screen_lines(),
        term_guard.columns(),
      ),
      scrolled_to_top,
      scrolled_to_bottom,
      title: "Terminal".to_string(),
    }
  }

  /// 同步终端状态 - 处理所有待处理的内部事件
  pub fn sync(&mut self, _cx: &mut Context<Self>) {
    while let Some(event) = self.events.pop_front() {
      self.process_internal_event(event);
    }
  }

  /// 处理内部事件
  fn process_internal_event(&mut self, event: InternalEvent) {
    match event {
      InternalEvent::Resize(bounds) => {
        let size = TerminalSize {
          rows: bounds.rows as u16,
          cols: bounds.cols as u16,
          pixel_width: f32::from(bounds.bounds.size.width) as u16,
          pixel_height: f32::from(bounds.bounds.size.height) as u16,
        };
        if let Some(tasks) = &self.tasks {
          let _ = tasks.input_tx.try_send(TerminalInput::Resize(size));
        }
        self.content.terminal_bounds = bounds;
      }
      InternalEvent::Scroll(_scroll) => {}
      InternalEvent::SetSelection(_selection) => {}
      InternalEvent::UpdateSelection(point) => {
        let _ = point;
      }
      InternalEvent::Clear => {
        if let Some(tasks) = &self.tasks {
          let _ = tasks.input_tx.try_send(TerminalInput::Sync);
        }
      }
      InternalEvent::Copy => {}
      InternalEvent::Paste(text) => {
        let data = if self.content.mode.contains(TermMode::BRACKETED_PASTE) {
          format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', ""))
        } else {
          text.replace("\r\n", "\r").replace('\n', "\r")
        };
        if let Some(tasks) = &self.tasks {
          let _ = tasks
            .input_tx
            .try_send(TerminalInput::Write(data.into_bytes()));
        }
      }
    }
  }

  /// 发送输入数据到终端
  pub fn input(&mut self, data: Vec<u8>) -> anyhow::Result<()> {
    self.scroll_to_bottom();
    self.set_selection(None);
    if let Some(tasks) = &self.tasks {
      tasks
        .input_tx
        .try_send(TerminalInput::Write(data))
        .map_err(|e| anyhow::anyhow!("Failed to send input: {}", e))
    } else {
      Ok(())
    }
  }

  /// 调整终端大小
  pub fn resize(&mut self, bounds: TerminalBounds) {
    let _ = self.events.push_back(InternalEvent::Resize(bounds));
  }

  /// 滚动终端
  pub fn scroll(&mut self, scroll: alacritty_terminal::grid::Scroll) {
    self.events.push_back(InternalEvent::Scroll(scroll));
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

  /// 清除屏幕
  pub fn clear(&mut self) {
    self.events.push_back(InternalEvent::Clear);
  }

  /// 粘贴文本
  pub fn paste(&mut self, text: &str) {
    self
      .events
      .push_back(InternalEvent::Paste(text.to_string()));
  }

  /// 设置选区
  pub fn set_selection(&mut self, selection: Option<alacritty_terminal::selection::Selection>) {
    self
      .events
      .push_back(InternalEvent::SetSelection(selection));
  }

  /// 复制选区
  pub fn copy(&mut self) {
    self.events.push_back(InternalEvent::Copy);
  }
}

impl EventEmitter<TerminalEvent> for Terminal {}
