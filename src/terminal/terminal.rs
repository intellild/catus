use crate::terminal::content::{
  IndexedCell, TerminalContent, TerminalEvent, TerminalPoint, renderable_cursor_to_state,
};
use crate::terminal::input::TerminalInput;
use crate::terminal::pty::{Pty, TerminalSize};
use alacritty_terminal::{
  event::EventListener,
  grid::{Dimensions, GridCell},
  term::{Config, Term},
  vte::ansi::Processor,
};
use gpui::*;
use tokio::sync::{mpsc, watch};

/// 终端尺寸结构，用于 alacritty 的 Dimensions trait
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

/// 终端事件监听器
struct ChannelEventListener(mpsc::UnboundedSender<alacritty_terminal::event::Event>);

impl EventListener for ChannelEventListener {
  fn send_event(&self, event: alacritty_terminal::event::Event) {
    let _ = self.0.send(event);
  }
}

/// 终端协调器
pub struct Terminal {
  /// 内容实体（独立 Entity，可被观察）
  pub content: Entity<TerminalContent>,

  /// 向后台任务发送输入
  input_tx: mpsc::Sender<TerminalInput>,

  /// 从后台任务接收内容更新
  _content_rx: watch::Receiver<TerminalContent>,

  /// 后台任务句柄（Drop 时自动取消）
  _task: Task<()>,

  /// UI 更新任务句柄
  _ui_task: Task<()>,
}

impl Terminal {
  /// 创建新的终端，并附加 PTY
  pub fn new(pty: Box<dyn Pty>, cx: &mut App) -> Result<Self> {
    // 创建 TerminalContent Entity - 在同步上下文中创建，避免 RefCell borrow 冲突
    let content = cx.new(|_cx| TerminalContent::new());

    // 创建输入通道
    let (input_tx, input_rx) = mpsc::channel::<TerminalInput>(1024);

    // 创建 watch 通道用于内容同步
    let (content_tx, content_rx) = watch::channel(TerminalContent::new());

    // 获取 PTY reader
    let pty_reader = pty.start_reader();

    // 获取 AsyncApp 用于后台任务
    let async_cx = cx.to_async();

    // 启动后台任务（Term 和 Pty 在循环内部创建）
    let background_task = async_cx.background_spawn({
      let content_tx = content_tx.clone();
      async move {
        run_terminal_loop(input_rx, content_tx, pty).await;
      }
    });

    // 启动 PTY 读取任务
    let input_tx_clone = input_tx.clone();
    let pty_read_task = async_cx.background_spawn(async move {
      let mut reader = pty_reader;
      while let Some(data) = reader.recv().await {
        if input_tx_clone
          .send(TerminalInput::PtyData(data))
          .await
          .is_err()
        {
          break;
        }
      }
    });

    // 启动 UI 更新任务 - 使用 async_cx.spawn 在异步中检查 watch 更新
    let content_weak = content.downgrade();
    let mut content_rx_clone = content_rx.clone();
    let ui_update_task = async_cx.spawn(async move |cx| {
      loop {
        // 等待内容变化
        if content_rx_clone.changed().await.is_err() {
          // Sender 已关闭，退出循环
          break;
        }

        // 获取最新内容
        let new_content = content_rx_clone.borrow().clone();

        // 更新 Entity (WeakEntity 直接调用 update，失败时返回错误)
        let result = content_weak.update(cx, |content, cx| {
          *content = new_content;
          cx.emit(TerminalEvent::Wakeup);
        });

        // Entity 已被释放，退出循环
        if result.is_err() {
          break;
        }
      }
    });

    // 合并后台任务
    let combined_task = async_cx.background_spawn(async move {
      let _ = background_task.await;
      let _ = pty_read_task.await;
    });

    Ok(Self {
      content,
      input_tx,
      _content_rx: content_rx,
      _task: combined_task,
      _ui_task: ui_update_task,
    })
  }

  /// 写入输入数据（用户按键）
  pub fn input(&self, data: Vec<u8>) {
    // 发送到后台任务，由后台任务写入 PTY
    let _ = self.input_tx.try_send(TerminalInput::Write(data));
  }

  /// 调整终端大小
  pub fn resize(&self, size: TerminalSize) {
    let _ = self.input_tx.try_send(TerminalInput::Resize(size));
  }

  /// 获取当前内容
  pub fn current_content(&self, cx: &App) -> TerminalContent {
    self.content.read(cx).clone()
  }

  /// 同步终端状态（强制刷新）
  pub fn sync(&self) {
    let _ = self.input_tx.try_send(TerminalInput::Sync);
  }
}

impl EventEmitter<TerminalEvent> for Terminal {}

/// 终端后台循环
async fn run_terminal_loop(
  mut input_rx: mpsc::Receiver<TerminalInput>,
  content_tx: watch::Sender<TerminalContent>,
  pty: Box<dyn Pty>,
) {
  // 在循环内部创建 Term，避免使用 Arc<Mutex<_>>
  let config = Config::default();
  let dimensions = TermDimensions {
    columns: 80,
    screen_lines: 24,
  };
  let (event_tx, _event_rx) = mpsc::unbounded_channel();
  let listener = ChannelEventListener(event_tx);

  let mut term = Term::new(config, &dimensions, listener);
  let mut parser: Processor<alacritty_terminal::vte::ansi::StdSyncHandler> = Processor::new();

  // 处理 UI 输入
  while let Some(input) = input_rx.recv().await {
    match input {
      TerminalInput::Write(data) => {
        // 写入数据到 PTY
        if let Err(e) = pty.write(&data) {
          eprintln!("Failed to write to PTY: {}", e);
        }
      }
      TerminalInput::PtyData(data) => {
        // 解析 VTE 数据
        parser.advance(&mut term, &data);

        let content = make_content(&term);
        let _ = content_tx.send(content);
      }
      TerminalInput::Resize(size) => {
        // 调整终端大小
        let dimensions = TermDimensions {
          columns: size.cols as usize,
          screen_lines: size.rows as usize,
        };
        term.resize(dimensions);

        // 同时通知 PTY 调整大小
        let _ = pty.resize(size);
      }
      TerminalInput::Sync => {
        let content = make_content(&term);
        let _ = content_tx.send(content);
      }
      TerminalInput::Shutdown => {
        break;
      }
    }
  }
}

/// 从 Term 生成 TerminalContent
fn make_content(term: &Term<ChannelEventListener>) -> TerminalContent {
  let mut content = TerminalContent::new();

  // 获取网格内容
  let grid = term.grid();
  let rows = grid.screen_lines();
  let cols = grid.columns();

  content.terminal_bounds.rows = rows;
  content.terminal_bounds.cols = cols;

  // 收集可见单元格
  let mut cells = Vec::new();

  for indexed in grid.display_iter() {
    let cell = indexed.cell;
    if !cell.is_empty() {
      cells.push(IndexedCell {
        point: TerminalPoint {
          line: indexed.point.line,
          column: indexed.point.column,
        },
        cell: cell.clone(),
      });
    }
  }

  content.cells = cells;
  content.mode = *term.mode();

  // 获取光标
  let renderable_content = term.renderable_content();
  content.cursor = renderable_cursor_to_state(&renderable_content.cursor);

  // 获取光标处的字符
  let cursor_point = renderable_content.cursor.point;
  content.cursor_char = term.grid()[cursor_point].c;

  content
}
