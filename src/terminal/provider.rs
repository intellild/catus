use alacritty_terminal::event::{Event as AlacTermEvent, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point as AlacPoint};
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::sync::FairMutex;

use alacritty_terminal::term::{
  Config, RenderableCursor, Term as AlacrittyTerm, TermMode, cell::Cell,
};
use alacritty_terminal::vte::ansi::Processor;
use anyhow::{Context as AnyhowContext, Result};
use gpui::{BackgroundExecutor, Bounds, Keystroke, Pixels};
use portable_pty::{CommandBuilder, ExitStatus, NativePtySystem, PtySize, PtySystem};
use std::ops::Deref;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{io::Read, thread};
use tokio::sync::{mpsc, watch};
use tokio::task::spawn_blocking;

pub enum ProviderCommand {
  /// 向 PTY 写入原始数据
  WriteData(Vec<u8>),
  /// 发送按键（会在 worker 线程中 encode 后写入 PTY）
  SendKey(Keystroke),
  /// 调整终端尺寸
  Resize {
    rows: usize,
    cols: usize,
  },
  /// 关闭终端
  Shutdown,
  Stop,
}

/// 带索引的单元格（类似 Zed 的 IndexedCell）
#[derive(Clone, Debug)]
pub struct IndexedCell {
  pub point: AlacPoint,
  pub cell: Cell,
}

impl Deref for IndexedCell {
  type Target = Cell;

  #[inline]
  fn deref(&self) -> &Cell {
    &self.cell
  }
}

/// 终端边界信息（类似 Zed 的 TerminalBounds）
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TerminalBounds {
  pub cell_width: Pixels,
  pub line_height: Pixels,
  pub bounds: Bounds<Pixels>,
}

impl TerminalBounds {
  pub fn new(line_height: Pixels, cell_width: Pixels, bounds: Bounds<Pixels>) -> Self {
    TerminalBounds {
      cell_width,
      line_height,
      bounds,
    }
  }

  pub fn num_lines(&self) -> usize {
    (self.bounds.size.height / self.line_height).floor() as usize
  }

  pub fn num_columns(&self) -> usize {
    (self.bounds.size.width / self.cell_width).floor() as usize
  }
}

impl Dimensions for TerminalBounds {
  fn total_lines(&self) -> usize {
    self.num_lines()
  }

  fn screen_lines(&self) -> usize {
    self.num_lines()
  }

  fn columns(&self) -> usize {
    self.num_columns()
  }
}

/// 终端内容快照（类似 Zed 的 TerminalContent）
#[derive(Clone)]
pub struct TerminalContent {
  pub cells: Vec<IndexedCell>,
  pub mode: TermMode,
  pub display_offset: usize,
  pub selection: Option<SelectionRange>,
  pub cursor: RenderableCursor,
  pub cursor_char: char,
  pub terminal_bounds: TerminalBounds,
  pub scrolled_to_top: bool,
  pub scrolled_to_bottom: bool,
}

impl Default for TerminalContent {
  fn default() -> Self {
    Self {
      cells: Vec::new(),
      mode: TermMode::default(),
      display_offset: 0,
      selection: None,
      cursor: RenderableCursor {
        shape: alacritty_terminal::vte::ansi::CursorShape::Block,
        point: AlacPoint::new(Line(0), Column(0)),
      },
      cursor_char: ' ',
      terminal_bounds: TerminalBounds::default(),
      scrolled_to_top: true,
      scrolled_to_bottom: true,
    }
  }
}

impl std::fmt::Debug for TerminalContent {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("TerminalContent")
      .field("cells_count", &self.cells.len())
      .field("mode", &self.mode)
      .field("display_offset", &self.display_offset)
      .field("selection", &self.selection)
      .field("cursor_char", &self.cursor_char)
      .field("terminal_bounds", &self.terminal_bounds)
      .field("scrolled_to_top", &self.scrolled_to_top)
      .field("scrolled_to_bottom", &self.scrolled_to_bottom)
      .finish()
  }
}

/// 终端更新事件
#[derive(Clone, Debug)]
pub enum TerminalEvent {
  Wakeup,
  TitleChanged(String),
  Bell,
}

/// TerminalProvider 发送给 UI 的更新（保留用于向后兼容）
#[derive(Clone)]
pub struct TerminalUpdate {
  pub content: TerminalContent,
  pub rows: usize,
  pub cols: usize,
}

impl TerminalUpdate {
  fn new(content: TerminalContent, rows: usize, cols: usize) -> Self {
    Self {
      content,
      rows,
      cols,
    }
  }
}

/// Channel 事件监听器 - 将 alacritty 事件转发到 watch channel
pub struct ChannelEventListener {
  tx: watch::Sender<AlacTermEvent>,
}

impl ChannelEventListener {
  pub fn new(tx: watch::Sender<AlacTermEvent>) -> Self {
    Self { tx }
  }
}

impl EventListener for ChannelEventListener {
  fn send_event(&self, event: AlacTermEvent) {
    // 通知有事件发生，UI 需要刷新（watch channel 只保留最新事件）
    let _ = self.tx.send(event);
  }
}

/// 终端尺寸实现
struct TermDimensions {
  rows: usize,
  cols: usize,
}

impl Dimensions for TermDimensions {
  fn total_lines(&self) -> usize {
    self.rows
  }

  fn screen_lines(&self) -> usize {
    self.rows
  }

  fn columns(&self) -> usize {
    self.cols
  }
}

/// 核心终端结构（类似 Zed 的 Terminal）
pub struct Terminal {
  term: Arc<FairMutex<AlacrittyTerm<ChannelEventListener>>>,
  pub last_content: TerminalContent,
  command_tx: mpsc::Sender<ProviderCommand>,
}

impl Terminal {
  /// 创建新的 Terminal
  fn new(
    term: Arc<FairMutex<AlacrittyTerm<ChannelEventListener>>>,
    command_tx: mpsc::Sender<ProviderCommand>,
  ) -> Self {
    Self {
      term,
      last_content: TerminalContent::default(),
      command_tx,
    }
  }

  /// 同步终端状态并更新 last_content（类似 Zed 的 sync 方法）
  pub fn sync(&mut self) {
    let term = self.term.clone();
    let terminal = term.lock();
    self.last_content = Self::make_content(&terminal, &self.last_content);
  }

  /// 生成 TerminalContent（类似 Zed 的 make_content）
  fn make_content(
    term: &AlacrittyTerm<ChannelEventListener>,
    last_content: &TerminalContent,
  ) -> TerminalContent {
    let content = term.renderable_content();

    // 预分配容量
    let estimated_size = content.display_iter.size_hint().0;
    let mut cells = Vec::with_capacity(estimated_size);

    cells.extend(content.display_iter.map(|ic| IndexedCell {
      point: ic.point,
      cell: ic.cell.clone(),
    }));

    TerminalContent {
      cells,
      mode: content.mode,
      display_offset: content.display_offset,
      selection: content.selection,
      cursor: content.cursor,
      cursor_char: term.grid()[content.cursor.point].c,
      terminal_bounds: last_content.terminal_bounds,
      scrolled_to_top: content.display_offset == term.history_size(),
      scrolled_to_bottom: content.display_offset == 0,
    }
  }

  /// 向 PTY 写入数据
  pub fn input(&self, data: impl Into<std::borrow::Cow<'static, [u8]>>) {
    let data = data.into();
    let _ = self
      .command_tx
      .try_send(ProviderCommand::WriteData(data.into_owned()));
  }

  /// 调整终端大小
  pub fn resize(&self, rows: usize, cols: usize) {
    let _ = self
      .command_tx
      .try_send(ProviderCommand::Resize { rows, cols });
  }

  /// 获取当前内容
  pub fn last_content(&self) -> &TerminalContent {
    &self.last_content
  }
}

impl TerminalProvider {
  /// 向 PTY 写入数据（便捷方法）
  pub fn input(&self, data: impl Into<std::borrow::Cow<'static, [u8]>>) {
    if let Some(ref terminal) = self.terminal {
      terminal.input(data);
    }
  }
}

pub struct TerminalProvider {
  pub command_tx: mpsc::Sender<ProviderCommand>,
  pub update_rx: watch::Receiver<TerminalUpdate>,
  pub event_rx: watch::Receiver<AlacTermEvent>,
  pub terminal: Option<Terminal>,
}

impl TerminalProvider {
  /// 创建新的 TerminalProvider 所需的数据和任务
  /// 返回 (command_tx, update_rx, event_rx)
  pub fn setup(
    executor: &BackgroundExecutor,
    rows: usize,
    cols: usize,
  ) -> (
    mpsc::Sender<ProviderCommand>,
    watch::Receiver<TerminalUpdate>,
    watch::Receiver<AlacTermEvent>,
    Arc<FairMutex<AlacrittyTerm<ChannelEventListener>>>,
  ) {
    let (command_tx, command_rx) = mpsc::channel::<ProviderCommand>(100);

    // 创建默认的初始值
    let initial_content = TerminalContent {
      cells: Vec::new(),
      mode: TermMode::default(),
      display_offset: 0,
      selection: None,
      cursor: RenderableCursor {
        shape: alacritty_terminal::vte::ansi::CursorShape::Block,
        point: AlacPoint::new(Line(0), Column(0)),
      },
      cursor_char: ' ',
      terminal_bounds: TerminalBounds::default(),
      scrolled_to_top: true,
      scrolled_to_bottom: true,
    };
    let initial_update = TerminalUpdate::new(initial_content, rows, cols);
    let (update_tx, update_rx) = watch::channel(initial_update);

    let (event_tx, event_rx) = watch::channel(AlacTermEvent::Wakeup);

    // 创建 alacritty Terminal 实例
    let config = Config::default();
    let dimensions = TermDimensions { rows, cols };
    let event_listener = ChannelEventListener::new(event_tx.clone());
    let term = Arc::new(FairMutex::new(AlacrittyTerm::new(
      config,
      &dimensions,
      event_listener,
    )));
    let term_clone = term.clone();

    // 使用 background_spawn 启动后台任务
    executor
      .spawn(async move {
        let _ = run_terminal_worker(rows, cols, command_rx, update_tx, event_tx, term_clone).await;
      })
      .detach();

    (command_tx, update_rx, event_rx, term)
  }

  /// 创建新的 TerminalProvider 实例（包含 Terminal）
  pub fn new(executor: &BackgroundExecutor, rows: usize, cols: usize) -> Self {
    let (command_tx, update_rx, event_rx, term) = Self::setup(executor, rows, cols);
    let terminal = Terminal::new(term, command_tx.clone());

    Self {
      command_tx,
      update_rx,
      event_rx,
      terminal: Some(terminal),
    }
  }

  /// 发送按键输入（异步）
  pub async fn send_key(&self, keystroke: Keystroke) -> Result<()> {
    self
      .command_tx
      .send(ProviderCommand::SendKey(keystroke))
      .await
      .context("Failed to send key")
  }

  /// 发送原始字节数据
  pub async fn write_data(&self, data: Vec<u8>) -> Result<()> {
    self
      .command_tx
      .send(ProviderCommand::WriteData(data))
      .await
      .context("Failed to write data")
  }

  /// 调整终端尺寸
  pub async fn resize(&self, rows: usize, cols: usize) -> Result<()> {
    self
      .command_tx
      .send(ProviderCommand::Resize { rows, cols })
      .await
      .context("Failed to resize")
  }

  /// 关闭终端
  pub async fn shutdown(&self) -> Result<()> {
    self
      .command_tx
      .send(ProviderCommand::Shutdown)
      .await
      .context("Failed to shutdown")
  }

  /// 获取当前更新（非阻塞，直接获取最新值）
  pub fn get_update(&self) -> TerminalUpdate {
    self.update_rx.borrow().clone()
  }

  /// 获取当前内容（从 Terminal 的 last_content）
  pub fn last_content(&self) -> Option<&TerminalContent> {
    self.terminal.as_ref().map(|t| &t.last_content)
  }

  /// 同步终端状态（更新 last_content）
  pub fn sync(&mut self) {
    if let Some(ref mut terminal) = self.terminal {
      terminal.sync();
    }
  }

  /// 等待更新变化（异步）
  pub async fn wait_for_update(&mut self) -> Result<TerminalUpdate> {
    self.update_rx.changed().await?;
    Ok(self.update_rx.borrow().clone())
  }

  /// 获取当前事件（非阻塞，直接获取最新值）
  pub fn get_event(&self) -> AlacTermEvent {
    self.event_rx.borrow().clone()
  }

  /// 等待事件变化（异步）
  pub async fn wait_for_event(&mut self) -> Result<AlacTermEvent> {
    self.event_rx.changed().await?;
    Ok(self.event_rx.borrow().clone())
  }
}

/// 在独立线程中运行终端
async fn run_terminal_worker(
  rows: usize,
  cols: usize,
  mut command_rx: mpsc::Receiver<ProviderCommand>,
  update_tx: watch::Sender<TerminalUpdate>,
  event_tx: watch::Sender<AlacTermEvent>,
  term: Arc<FairMutex<AlacrittyTerm<ChannelEventListener>>>,
) -> Result<ExitStatus> {
  let pty_system = NativePtySystem::default();
  let pair = pty_system
    .openpty(PtySize {
      rows: rows as u16,
      cols: cols as u16,
      pixel_width: 0,
      pixel_height: 0,
    })
    .context("Failed to open PTY")?;

  let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

  let mut cmd = CommandBuilder::new(&shell);
  cmd.arg("-l");
  let mut child = pair
    .slave
    .spawn_command(cmd)
    .context("Failed to spawn shell")?;

  let mut reader = pair
    .master
    .try_clone_reader()
    .context("Failed to clone reader")?;

  let mut writer = pair
    .master
    .take_writer()
    .context("Failed to take writer:")?;

  // Clone the Arc for the thread
  let term_for_thread = term.clone();

  let handle = thread::spawn(move || -> Result<ExitStatus> {
    let mut parser: Processor = Processor::new();

    let mut buf = Vec::new();
    buf.resize(4096, 0);

    loop {
      match reader.read(&mut buf).context("Error reading PTY")? {
        0 => {
          let start = Instant::now();

          loop {
            match child.try_wait()? {
              None => {
                thread::sleep(Duration::from_millis(500));

                if start.elapsed() >= Duration::from_secs(10) {
                  child.kill()?;
                }
              }
              Some(status) => {
                return Ok(status);
              }
            }
          }
        }
        n => {
          // 将数据写入 Terminal
          {
            let mut term = term_for_thread.lock();
            parser.advance(&mut *term, &buf[..n]);

            // 发送更新给 UI
            let rows = term.screen_lines();
            let cols = term.columns();
            let content = term.renderable_content();

            // 转换为 TerminalContent
            let estimated_size = content.display_iter.size_hint().0;
            let mut cells = Vec::with_capacity(estimated_size);
            cells.extend(content.display_iter.map(|ic| IndexedCell {
              point: ic.point,
              cell: ic.cell.clone(),
            }));

            let terminal_content = TerminalContent {
              cells,
              mode: content.mode,
              display_offset: content.display_offset,
              selection: content.selection,
              cursor: content.cursor,
              cursor_char: term.grid()[content.cursor.point].c,
              terminal_bounds: TerminalBounds::default(),
              scrolled_to_top: content.display_offset == term.history_size(),
              scrolled_to_bottom: content.display_offset == 0,
            };

            let update = TerminalUpdate::new(terminal_content, rows, cols);

            // 发送更新（watch channel 只保留最新值）
            drop(term);
            let _ = update_tx.send(update);
          }

          // 发送 Wakeup 事件
          let _ = event_tx.send(AlacTermEvent::Wakeup);
        }
      }
    }
  });

  'running: while let Some(cmd) = command_rx.recv().await {
    match cmd {
      ProviderCommand::WriteData(data) => {
        if writer.write_all(&data).is_err() || writer.flush().is_err() {
          break 'running;
        }
      }
      ProviderCommand::SendKey(keystroke) => {
        let data = encode_keystroke(&keystroke);
        if writer.write_all(&data).is_err() || writer.flush().is_err() {
          break 'running;
        }
      }
      ProviderCommand::Resize { rows, cols } => {
        let _ = pair.master.resize(PtySize {
          rows: rows as u16,
          cols: cols as u16,
          pixel_width: 0,
          pixel_height: 0,
        });

        // 更新终端大小
        let mut term = term.lock();
        let dimensions = TermDimensions { rows, cols };
        term.resize(dimensions);
      }
      ProviderCommand::Stop => {
        // kill
      }
      ProviderCommand::Shutdown => {
        break 'running;
      }
    }
  }

  let status = spawn_blocking(|| {
    handle.join().map_err(|err| {
      let msg = if let Some(s) = err.downcast_ref::<&str>() {
        format!("thread panicked: {}", s)
      } else if let Some(s) = err.downcast_ref::<String>() {
        format!("thread panicked: {}", s)
      } else {
        "thread panicked with unknown payload".to_string()
      };
      anyhow::Error::msg(msg)
    })
  })
  .await???;

  Ok(status)
}

/// 将 GPUI Keystroke 编码为字节序列
fn encode_keystroke(keystroke: &Keystroke) -> Vec<u8> {
  let key = keystroke.key.as_str();
  let modifiers = &keystroke.modifiers;

  match key {
    // 单字符输入（字母、数字、符号）
    k if k.len() == 1 => {
      let ch = k.chars().next().unwrap_or('\0');

      // 处理 Ctrl 修饰符
      if modifiers.control && ch.is_ascii_alphabetic() {
        let byte = ch.to_ascii_lowercase() as u8;
        vec![byte - b'a' + 1] // Ctrl+A = 0x01
      } else {
        k.as_bytes().to_vec()
      }
    }

    // 功能键
    "enter" | "return" => vec![b'\r'],
    "escape" | "esc" => vec![0x1b],
    "tab" => vec![b'\t'],
    "backspace" => vec![0x08],
    "delete" | "del" => vec![0x1b, b'[', b'3', b'~'],
    "insert" | "ins" => vec![0x1b, b'[', b'2', b'~'],

    // 方向键
    "up" => vec![0x1b, b'[', b'A'],
    "down" => vec![0x1b, b'[', b'B'],
    "right" => vec![0x1b, b'[', b'C'],
    "left" => vec![0x1b, b'[', b'D'],

    // Home/End
    "home" => vec![0x1b, b'[', b'H'],
    "end" => vec![0x1b, b'[', b'F'],

    // Page Up/Down
    "pageup" | "page up" => vec![0x1b, b'[', b'5', b'~'],
    "pagedown" | "page down" => vec![0x1b, b'[', b'6', b'~'],

    // 功能键 F1-F12
    "f1" => vec![0x1b, b'[', b'1', b'1', b'~'],
    "f2" => vec![0x1b, b'[', b'1', b'2', b'~'],
    "f3" => vec![0x1b, b'[', b'1', b'3', b'~'],
    "f4" => vec![0x1b, b'[', b'1', b'4', b'~'],
    "f5" => vec![0x1b, b'[', b'1', b'5', b'~'],
    "f6" => vec![0x1b, b'[', b'1', b'7', b'~'],
    "f7" => vec![0x1b, b'[', b'1', b'8', b'~'],
    "f8" => vec![0x1b, b'[', b'1', b'9', b'~'],
    "f9" => vec![0x1b, b'[', b'2', b'0', b'~'],
    "f10" => vec![0x1b, b'[', b'2', b'1', b'~'],
    "f11" => vec![0x1b, b'[', b'2', b'3', b'~'],
    "f12" => vec![0x1b, b'[', b'2', b'4', b'~'],

    // 其他未处理的键
    _ => vec![],
  }
}
