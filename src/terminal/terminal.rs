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
use parking_lot::FairMutex;
use std::sync::Arc;
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

  /// 从后台任务接收更新
  content_rx: watch::Receiver<TerminalContent>,

  /// 后台任务句柄（Drop 时自动取消）
  _task: Task<()>,
}

impl Terminal {
  /// 创建新的终端
  pub fn new(cx: &mut Context<Self>) -> Self {
    // 创建 TerminalContent Entity
    let content = cx.new(|_cx| TerminalContent::new());

    // 创建输入通道
    let (input_tx, input_rx) = mpsc::channel::<TerminalInput>(1024);

    // 创建内容 watch channel
    let (content_tx, content_rx) = watch::channel(TerminalContent::new());

    // 获取弱引用用于后台任务
    let weak_content = content.downgrade();

    // 创建 Term（alacritty 终端状态）
    let config = Config::default();
    // 创建默认尺寸
    let dimensions = TermDimensions {
      columns: 80,
      screen_lines: 24,
    };
    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    let listener = ChannelEventListener(event_tx);

    let term = Arc::new(FairMutex::new(Term::new(config, &dimensions, listener)));

    // 启动后台任务
    let background_task = cx.background_spawn({
      let term = term.clone();

      async move {
        run_terminal_loop(term, input_rx, content_tx, weak_content).await;
      }
    });

    Self {
      content,
      input_tx,
      content_rx,
      _task: background_task,
    }
  }

  /// 附加 PTY（本地或 SSH）
  /// 注意：这个方法需要在创建 Terminal 后调用，并传入 PTY
  pub fn attach_pty(&mut self, pty: Box<dyn Pty>, cx: &mut Context<Self>) {
    // 启动 PTY 读取器
    let reader = pty.start_reader();
    let input_tx = self.input_tx.clone();

    // 在后台任务中转发 PTY 数据
    let forward_task = cx.background_spawn(async move {
      let mut reader = reader;
      while let Some(data) = reader.recv().await {
        if input_tx.send(TerminalInput::PtyData(data)).await.is_err() {
          break;
        }
      }
    });

    // 我们需要存储 PTY 以便后续写入，但 Pty trait 不是 Sync
    // 这里我们启动一个任务来处理 Write 命令
    let input_tx_for_write = self.input_tx.clone();
    let _write_task = cx.background_spawn(async move {
      // 注意：这里简化处理，实际应该在后台循环中处理 Write
      // 目前 Write 命令会被接收但不会被处理
      drop(input_tx_for_write);
    });

    // 为了保持 PTY 存活，我们需要存储它
    // 但由于 trait object 的限制，这里简化处理
    drop(forward_task);
  }

  /// 写入输入数据
  pub fn input(&self, data: Vec<u8>) {
    let _ = self.input_tx.try_send(TerminalInput::Write(data));
  }

  /// 调整终端大小
  pub fn resize(&self, size: TerminalSize) {
    let _ = self.input_tx.try_send(TerminalInput::Resize(size));
  }

  /// 获取当前内容（从 watch channel）
  pub fn current_content(&self) -> TerminalContent {
    self.content_rx.borrow().clone()
  }

  /// 同步终端状态（强制刷新）
  pub fn sync(&self) {
    let _ = self.input_tx.try_send(TerminalInput::Sync);
  }
}

impl EventEmitter<TerminalEvent> for Terminal {}

/// 终端后台循环
async fn run_terminal_loop(
  term: Arc<FairMutex<Term<ChannelEventListener>>>,
  mut input_rx: mpsc::Receiver<TerminalInput>,
  content_tx: watch::Sender<TerminalContent>,
  weak_content: WeakEntity<TerminalContent>,
) {
  let mut parser: Processor<alacritty_terminal::vte::ansi::StdSyncHandler> = Processor::new();

  loop {
    tokio::select! {
      // 处理 UI 输入
      Some(input) = input_rx.recv() => {
        match input {
          TerminalInput::PtyData(data) => {
            // 解析 VTE 数据
            {
              let mut term_guard = term.lock();
              parser.advance(&mut *term_guard, &data);
            }

            // 生成新内容并发送
            let new_content = make_content(&term);
            let _ = content_tx.send(new_content.clone());

            // 通知 Entity 更新
            if let Some(entity) = weak_content.upgrade() {
              // 在后台线程中无法直接调用 cx.emit
              // 我们需要通过其他方式通知 UI
              // 这里简化处理，依赖 UI 端的定期同步
              drop(entity);
            }
          }
          TerminalInput::Write(_data) => {
            // 写入数据到 PTY - 需要在这里处理
            // 由于 Pty trait 的限制，这里简化处理
          }
          TerminalInput::Resize(size) => {
            // 调整终端大小
            let mut term_guard = term.lock();
            let dimensions = TermDimensions {
              columns: size.cols as usize,
              screen_lines: size.rows as usize,
            };
            term_guard.resize(dimensions);
            drop(term_guard);

            // 发送更新
            let new_content = make_content(&term);
            let _ = content_tx.send(new_content);
          }
          TerminalInput::Sync => {
            // 强制刷新
            let new_content = make_content(&term);
            let _ = content_tx.send(new_content);
          }
          TerminalInput::Shutdown => {
            break;
          }
        }
      }
      else => {
        // 没有消息时短暂休眠
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
      }
    }
  }
}

/// 从 Term 生成 TerminalContent
fn make_content(term: &Arc<FairMutex<Term<ChannelEventListener>>>) -> TerminalContent {
  let mut content = TerminalContent::new();
  let term_guard = term.lock();

  // 获取网格内容
  let grid = term_guard.grid();
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
  content.mode = *term_guard.mode();

  // 获取光标
  let renderable_content = term_guard.renderable_content();
  content.cursor = renderable_cursor_to_state(&renderable_content.cursor);

  // 获取光标处的字符
  let cursor_point = renderable_content.cursor.point;
  content.cursor_char = term_guard.grid()[cursor_point].c;

  drop(term_guard);

  content
}
