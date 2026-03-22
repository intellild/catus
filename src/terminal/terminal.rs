use crate::terminal::content::{TerminalBounds, TerminalContent, TerminalEvent, TerminalPoint};
use crate::terminal::pty::{Pty, TerminalSize};
use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::Config;
use alacritty_terminal::vte::ansi::Processor;
use async_channel::{Sender, unbounded};
use async_lock::Mutex;
use gpui::*;
use std::sync::Arc;

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
    // 使用 ok() 忽略发送失败（接收端已关闭的情况）
    let _ = self.0.send_blocking(event);
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
  content: TerminalContent,
  /// alacritty 终端状态（使用 Arc<Mutex> 以便在后台任务中访问）
  term: Arc<Mutex<Term>>,
  pty: Arc<dyn Pty>,
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
  pub fn new(pty: Arc<dyn Pty>, cx: &mut Context<Self>) -> Result<Self> {
    // 创建初始尺寸
    let initial_size = TerminalSize::default_size();
    let term_dimensions = TermDimensions::from(initial_size);

    // 创建终端配置
    let term_config = Config {
      scrolling_history: DEFAULT_SCROLL_HISTORY_LINES,
      ..Config::default()
    };

    // 创建事件通道（alacritty → Terminal）
    let (events_tx, events_rx) = unbounded::<alacritty_terminal::event::Event>();

    let term = Arc::new(Mutex::new(Term::new(
      term_config,
      &term_dimensions,
      EventProxy(events_tx),
    )));

    // 获取实体句柄（用于后台任务更新内容）
    let entity = cx.entity().clone();

    // 启动 PTY 读取器
    let pty_reader = pty.reader();

    let event_term = term.clone();
    cx.spawn(async move |_, cx| -> Result<()> {
      let term = event_term;
      loop {
        let data = pty_reader.recv().await?;
        let term = term.clone();
        cx.background_spawn(async move {
          term.clone().lock().await.advance(data);
        })
        .await;

        entity.update(cx, |_, cx| {
          cx.notify();
          cx.emit(TerminalEvent::Wakeup);
        })?;
      }
    })
    .detach();

    cx.spawn(async move |entity, cx| -> Result<()> {
      use alacritty_terminal::event::Event;
      loop {
        let event = events_rx.recv().await?;

        entity.update(cx, |terminal, cx| {
          match event {
            Event::Title(title) => {
              terminal.title = title.clone();
              cx.emit(TerminalEvent::TitleChanged(title));
            }
            Event::Wakeup => {
              cx.notify();
            }
            Event::Bell => {
              cx.emit(TerminalEvent::Bell);
            }
            Event::Exit | Event::ChildExit(_) => {
              cx.emit(TerminalEvent::Closed);
            }
            _ => {}
          }
        })?;
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
}

impl EventEmitter<TerminalEvent> for Terminal {}
