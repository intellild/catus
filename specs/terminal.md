# Terminal 架构设计规格

## 1. 整体架构

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                               UI Thread (GPUI)                              │
│                                                                             │
│  ┌─────────────────┐       ┌─────────────────────────────────────┐         │
│  │   TerminalView  │◄─────►│         Entity<Terminal>            │         │
│  │   (渲染 + 交互)  │       │  ┌───────────────────────────────┐  │         │
│  └────────┬────────┘       │  │  content: TerminalContent     │  │         │
│           │                │  │  term: Arc<Mutex<Term>>       │  │         │
│           │                │  │  pty: Arc<dyn Pty>            │  │         │
│           │                │  └───────────────────────────────┘  │         │
│           │                └─────────────────┬───────────────────┘         │
│           │                                  │                             │
│           │ cx.spawn()                       │                             │
│           │                                  │                             │
│  ┌────────▼────────┐       ┌─────────────────▼─────────────────────┐       │
│  │ TerminalElement │◄─────►│      TerminalContent (直接存储)       │       │
│  │   (paint渲染)   │  read │  ┌───────────────────────────────┐    │       │
│  └─────────────────┘       │  │  cells, cursor, mode, title   │    │       │
│                            │  │  selection, bounds, ...       │    │       │
│                            │  └───────────────────────────────┘    │       │
│                            └───────────────────────────────────────┘       │
└─────────────────────────────────────────────────────────────────────────────┘
                                       │
                                       │ async_channel
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Background Task (cx.spawn)                          │
│                                                                             │
│   ┌─────────────────────────────────┐                                       │
│   │     PTY Read Loop               │                                       │
│   │  ┌─────────────────────────┐    │                                       │
│   │  │  pty.reader().recv()    │    │                                       │
│   │  │       │                 │    │                                       │
│   │  │       ▼                 │    │                                       │
│   │  │  while let Some(data) { │    │                                       │
│   │  │    term.lock().advance()│    │                                       │
│   │  │    cx.notify()          │────┼────► UI 重渲染                        │
│   │  │  }                      │    │                                       │
│   │  └─────────────────────────┘    │                                       │
│   │                                 │                                       │
│   └─────────────────────────────────┘                                       │
│                                                                             │
│   ┌──────────────────────┐  ┌──────────────────┐                           │
│   │   Arc<Mutex<Term>>   │  │   Arc<dyn Pty>   │                           │
│   │   (alacritty终端状态) │  │   (pty抽象)       │                           │
│   └──────────────────────┘  └──────────────────┘                           │
│                                      │                                      │
└─────────────────────────────────────┼───────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Pty 实现层                                      │
│                                                                             │
│  ┌─────────────────────────────┐  ┌─────────────────────────────────────┐   │
│  │       LocalPty              │  │           SshPty (TODO)             │   │
│  │  ┌───────────────────────┐  │  │  ┌─────────────────────────────┐    │   │
│  │  │ portable_pty::PtyPair │  │  │  │  ssh2::Session + Channel    │    │   │
│  │  │ ├─ master (write TX)  │  │  │  │                             │    │   │
│  │  │ └─ slave              │  │  │  └─────────────────────────────┘    │   │
│  │  └───────────────────────┘  │  │                                     │   │
│  │  ┌───────────────────────┐  │  │  ┌─────────────────────────────┐    │   │
│  │  │    Reader Thread      │  │  │  │       Reader Thread         │    │   │
│  │  │  (阻塞读取 + channel)  │  │  │  │   (阻塞读取 + channel)       │    │   │
│  │  │  handle: JoinHandle   │  │  │  │   handle: JoinHandle        │    │   │
│  │  └───────────────────────┘  │  │  └─────────────────────────────┘    │   │
│  │  ┌───────────────────────┐  │  │                                     │   │
│  │  │    Writer Thread      │  │  │                                     │   │
│  │  │  (channel 接收写入)    │  │  │                                     │   │
│  │  │  handle: JoinHandle   │  │  │                                     │   │
│  │  └───────────────────────┘  │  │                                     │   │
│  └─────────────────────────────┘  └─────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 2. 核心类型定义

### 2.1 TerminalContent

终端渲染状态，直接存储在 Terminal 中（非 Entity）。

```rust
/// 终端内容 - 纯渲染状态
#[derive(Clone)]
pub struct TerminalContent {
    pub cells: Vec<IndexedCell>,
    pub mode: TermMode,
    pub display_offset: usize,
    pub selection: Option<SelectionRange>,
    pub cursor: CursorState,
    pub cursor_char: char,
    pub terminal_bounds: TerminalBounds,
    pub scrolled_to_top: bool,
    pub scrolled_to_bottom: bool,
    pub title: String,
}

impl TerminalContent {
    pub fn new() -> Self;
    pub fn update_from_cells(&mut self, cells: Vec<IndexedCell>, cursor: CursorState, cursor_char: char);
    pub fn set_title(&mut self, title: String);
    pub fn set_bounds(&mut self, bounds: TerminalBounds);
}
```

### 2.2 CursorState

可渲染的光标状态。

```rust
#[derive(Clone, Debug)]
pub struct CursorState {
    pub point: TerminalPoint,
    pub shape: alacritty_terminal::vte::ansi::CursorShape,
}
```

### 2.3 Terminal

终端协调器 Entity，管理 PTY 和状态同步。

```rust
/// 终端协调器
pub struct Terminal {
    /// 终端内容（直接存储，非 Entity）
    pub content: TerminalContent,
    /// alacritty 终端状态
    term: Arc<Mutex<Term>>,
    /// PTY 实现
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
    pub fn new(pty: Arc<dyn Pty>, cx: &mut Context<Self>) -> Result<Self>;
    
    /// 创建仅显示的终端（无 PTY，用于测试）
    pub fn new_display_only(cx: &mut Context<Self>) -> anyhow::Result<Self>;
    
    /// 发送输入数据到终端
    pub fn input(&mut self, cx: &mut Context<Self>, data: Vec<u8>);
    
    /// 调整终端大小
    pub async fn resize(&mut self, size: TerminalSize) -> Result<()>;
    
    /// 滚动终端
    pub fn scroll(&mut self, scroll: alacritty_terminal::grid::Scroll);
    pub fn scroll_line_up(&mut self);
    pub fn scroll_line_down(&mut self);
    pub fn scroll_page_up(&mut self);
    pub fn scroll_page_down(&mut self);
    pub fn scroll_to_top(&mut self);
    pub fn scroll_to_bottom(&mut self);
    
    /// 获取当前内容的引用
    pub fn content(&self) -> &TerminalContent;
    
    /// 获取终端标题
    pub fn title(&self) -> &str;
    
    /// 是否滚动到顶部/底部
    pub fn scrolled_to_top(&self) -> bool;
    pub fn scrolled_to_bottom(&self) -> bool;
}

impl EventEmitter<TerminalEvent> for Terminal {}
```

### 2.4 TerminalInput (输入枚举)

定义所有从 UI 发送到后台的输入类型（当前定义但未在核心流程中使用）。

```rust
/// 终端输入事件（UI → Background）
pub enum TerminalInput {
    /// PTY 输出数据（来自 read thread）
    PtyData(Vec<u8>),
    
    /// 用户输入数据
    Write(Vec<u8>),
    
    /// 调整终端大小
    Resize(TerminalSize),
    
    /// 获取当前内容（强制刷新）
    Sync,
    
    /// 关闭终端
    Shutdown,
}
```

### 2.5 Pty Trait

PTY 抽象接口，支持本地和 SSH 两种实现。

```rust
/// PTY 抽象
#[async_trait]
pub trait Pty: Send + Sync {
    /// 写入数据到 PTY（异步）
    async fn write(&self, data: Vec<u8>) -> Result<()>;
    
    /// 调整 PTY 大小（异步）
    async fn resize(&self, size: TerminalSize) -> Result<()>;
    
    /// 获取数据接收器（克隆）
    fn reader(&self) -> Receiver<Vec<u8>>;
    
    /// 关闭 PTY（异步）
    async fn close(&mut self) -> Result<()>;
    
    /// 获取进程 ID（本地 PTY 有效）
    fn process_id(&self) -> Option<u32>;
}

/// 终端尺寸
#[derive(Clone, Copy, Debug)]
pub struct TerminalSize {
    pub rows: u16,
    pub cols: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}

impl TerminalSize {
    pub fn new(rows: u16, cols: u16, pixel_width: u16, pixel_height: u16) -> Self;
    pub fn default_size() -> Self;  // 24x80
}
```

### 2.6 LocalPty

本地 PTY 实现，基于 `portable-pty`。

```rust
/// 写入命令枚举
enum WriteCommand {
    Write(Vec<u8>),
    Resize(PtySize),
}

/// 本地 PTY 实现
pub struct LocalPty {
    process_id: Option<u32>,
    child: Box<dyn Child + Send + Sync>,
    reader_handle: JoinHandle<Result<()>>,
    reader_rx: Receiver<Vec<u8>>,
    writer_handle: JoinHandle<Result<()>>,
    writer_tx: Sender<WriteCommand>,
}

impl LocalPty {
    /// 创建本地 PTY
    pub fn new(size: TerminalSize, command: Option<&str>) -> Result<Self>;
}

#[async_trait]
impl Pty for LocalPty {
    async fn write(&self, data: Vec<u8>) -> Result<()>;
    async fn resize(&self, size: TerminalSize) -> Result<()>;
    fn reader(&self) -> Receiver<Vec<u8>>;
    async fn close(&mut self) -> Result<()>;
    fn process_id(&self) -> Option<u32>;
}

impl Drop for LocalPty {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
```

### 2.7 SshPty (TODO)

SSH PTY 实现，基于 `ssh2`（尚未实现）。

```rust
/// SSH 认证方式
pub enum SshAuth {
    Password(String),
    Key { private_key: PathBuf, passphrase: Option<String> },
    Agent,
}

/// SSH PTY
pub struct SshPty {
    // ssh2 内部实现
}

impl SshPty {
    /// 创建 SSH PTY（可能阻塞，应在 background_spawn 中调用）
    pub fn new(
        host: &str,
        user: &str,
        auth: SshAuth,
        size: TerminalSize,
    ) -> anyhow::Result<Self>;
}

#[async_trait]
impl Pty for SshPty {
    async fn write(&self, data: Vec<u8>) -> Result<()>;
    async fn resize(&self, size: TerminalSize) -> Result<()>;
    fn reader(&self) -> Receiver<Vec<u8>>;
    async fn close(&mut self) -> Result<()>;
    fn process_id(&self) -> Option<u32>;
}
```

## 3. 后台任务流程

```rust
// Terminal::new 中启动后台任务
let pty_reader = pty.reader();

cx.spawn(async move |_, cx| -> Result<()> {
    let term = event_term;
    loop {
        // 从 PTY 读取数据
        let data = pty_reader.recv().await?;
        let term = term.clone();
        
        // 在后台线程解析 VTE
        cx.background_spawn(async move {
            term.clone().lock().await.advance(data);
        }).await;
        
        // 通知 UI 重渲染
        entity.update(cx, |_, cx| {
            cx.notify();
        })?;
    }
}).detach();
```

## 4. UI 层接口

### 4.1 TerminalView

```rust
pub struct TerminalView {
    terminal: Entity<Terminal>,
    focus_handle: FocusHandle,
}

impl TerminalView {
    pub fn new(terminal: Entity<Terminal>, cx: &mut Context<Self>) -> Self;
    
    fn handle_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>);
    fn handle_paste(&mut self, text: &str, cx: &mut Context<Self>);
    
    pub fn terminal(&self) -> &Entity<Terminal>;
    
    // 滚动方法
    pub fn scroll_line_up(&mut self, cx: &mut Context<Self>);
    pub fn scroll_line_down(&mut self, cx: &mut Context<Self>);
    pub fn scroll_page_up(&mut self, cx: &mut Context<Self>);
    pub fn scroll_page_down(&mut self, cx: &mut Context<Self>);
    pub fn scroll_to_top(&mut self, cx: &mut Context<Self>);
    pub fn scroll_to_bottom(&mut self, cx: &mut Context<Self>);
    
    pub fn clear(&mut self, cx: &mut Context<Self>);
    pub fn copy(&mut self, cx: &mut Context<Self>);
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement;
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle;
}
```

### 4.2 TerminalElement

```rust
pub struct TerminalElement {
    terminal: Entity<Terminal>,
    content: TerminalContent,
    char_width: Pixels,
    char_height: Pixels,
    focus_handle: FocusHandle,
}

pub struct LayoutState {
    bounds: Bounds<Pixels>,
    content: TerminalContent,
    char_width: Pixels,
    char_height: Pixels,
    background_color: Hsla,
    cursor_visible: bool,
}

pub struct BatchedTextRun {
    pub start_row: usize,
    pub start_col: usize,
    pub text: String,
    pub cell_count: usize,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
    pub bold: bool,
}

impl TerminalElement {
    pub fn new(terminal: Entity<Terminal>, focus_handle: FocusHandle) -> Self;
    fn create_font() -> Font;
    fn calculate_char_dimensions(&mut self, window: &mut Window);
    fn create_text_run(len: usize, font: &Font, color: Hsla, bold: bool) -> TextRun;
    fn paint_cell_background(...);
    fn paint_cursor(...);
    fn layout_grid(content: &TerminalContent) -> Vec<BatchedTextRun>;
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = LayoutState;
    
    fn request_layout(&mut self, ...) -> (LayoutId, Self::RequestLayoutState);
    fn prepaint(&mut self, ...) -> Self::PrepaintState;
    fn paint(&mut self, ...);
}
```

## 5. 数据流

### 5.1 用户输入

```
按键事件
    │
    ▼
TerminalView::handle_key_down()
    │
    ▼
terminal.input(cx, data)
    │
    ▼
pty.write(data).await  // 异步写入
    │
    ▼
Writer Thread (via channel)
    │
    ▼
PTY Master
```

### 5.2 终端输出

```
PTY Master 有输出
    │
    ▼
Reader Thread (阻塞读取)
    │
    ▼
reader_tx.send(data)
    │
    ▼
cx.spawn 任务 recv()
    │
    ▼
term.lock().advance(data)  // 解析 VTE
    │
    ▼
cx.notify()  // 通知 UI
    │
    ▼
UI 重渲染 (TerminalElement::paint)
```

## 6. 文件结构

```
src/terminal/
├── mod.rs              # 模块导出
├── terminal.rs         # Terminal 结构体（Entity + 协调器）
├── content.rs          # TerminalContent（渲染状态）
├── input.rs            # TerminalInput 枚举
├── pty.rs              # Pty trait + TerminalSize
├── local_pty.rs        # LocalPty 实现
├── view.rs             # TerminalView
├── terminal_element.rs # TerminalElement
```

## 7. 使用示例

### 7.1 创建本地终端

```rust
use crate::terminal::{LocalPty, Terminal, TerminalSize};

let size = TerminalSize::default_size();
let pty = Arc::new(LocalPty::new(size, Some("/bin/bash")).unwrap());
let terminal = cx.new(|cx| Terminal::new(pty, cx).unwrap());
```

### 7.2 创建仅显示终端（测试用）

```rust
let terminal = cx.new(|cx| Terminal::new_display_only(cx).unwrap());
```

### 7.3 创建 SSH 终端 (TODO)

```rust
// 在后台创建 SSH 连接
cx.spawn(async move |this, cx| {
    let pty = cx.background_spawn(async move {
        SshPty::new("host:22", "user", SshAuth::Agent, size)
    }).await?;
    
    this.update(cx, |term, cx| {
        term.attach_pty(Arc::new(pty), cx);
    })?;
    
    Ok(())
}).detach();
```

## 8. 设计要点

1. **TerminalContent 直接存储**
   - 非 Entity，直接存储在 Terminal 中
   - 通过 `cx.notify()` 触发重渲染

2. **Pty 使用 Arc 包装**
   - 支持多线程共享
   - 使用 `async_trait` 定义异步方法

3. **LocalPty 双线程设计**
   - Reader Thread: 阻塞读取 PTY 输出
   - Writer Thread: 通过 channel 接收写入命令
   - 支持写入和 resize 操作

4. **VTE 解析在后台执行**
   - 使用 `cx.background_spawn` 在后台线程执行
   - 避免阻塞 UI 线程

5. **文本批处理渲染**
   - `BatchedTextRun` 合并相同样式的文本
   - 减少 draw call，提高渲染性能
