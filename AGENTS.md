# Catus - 终端与 SFTP 客户端

## 项目简介

Catus 是一个基于 Rust 和 GPUI 框架构建的终端与 SFTP 客户端应用程序。它采用多 Tab 工作区设计，支持同时管理多个终端会话和文件传输任务。

## 技术栈

- **UI 框架**: GPUI
- **终端仿真**: alacritty_terminal
- **PTY 实现**: portable-pty
- **异步运行时**: tokio

## 终端架构

### 整体架构

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            UI Thread (GPUI)                                 │
│                                                                             │
│  ┌─────────────────┐       ┌─────────────────────────────────────────┐     │
│  │   TerminalView  │──────►│            Entity<Terminal>              │     │
│  │   (渲染 + 交互)  │       │  ┌───────────────────────────────────┐  │     │
│  └────────┬────────┘       │  │  content: TerminalContent (plain) │  │     │
│           │                │  │  term: Arc<Mutex<Term>>           │  │     │
│           │                │  │  pty: Arc<dyn Pty>                │  │     │
│           │                │  │  terminal_size: Option<...>       │  │     │
│           │                │  └───────────────────────────────────┘  │     │
│           │                └────────────────┬────────────────────────┘     │
│           │                                 │                              │
│           │                          cx.entity()                           │
│           │                                 │                              │
│  ┌────────▼────────┐       ┌────────────────▼────────────────────────┐     │
│  │ TerminalElement │       │    Entity<Terminal> (后台任务)            │     │
│  │   (paint渲染)   │       │  ┌────────────────────────────────────┐ │     │
│  │                 │       │  │ PTY reader task (cx.spawn)         │ │     │
│  │ prepaint():     │       │  │  loop {                            │ │     │
│  │  ├ sync_size()  │◄──────┤  │    data = pty_reader.recv()       │ │     │
│  │  ├ refresh_     │notify │  │    term.advance(data)  //VTE解析   │ │     │
│  │  │  content()   │       │  │    cx.notify()         //通知UI   │ │     │
│  │  └ read content │       │  │  }                                 │ │     │
│  └─────────────────┘       │  ├────────────────────────────────────┤ │     │
│                            │  │ Event handler task (cx.spawn)      │ │     │
│                            │  │  loop {                            │ │     │
│                            │  │    match event {                   │ │     │
│                            │  │      Title => 更新标题              │ │     │
│                            │  │      Wakeup => cx.notify()         │ │     │
│                            │  │      Bell => emit Bell             │ │     │
│                            │  │      Exit => emit Closed            │ │     │
│                            │  │    }                               │ │     │
│                            │  │  }                                 │ │     │
│                            │  └────────────────────────────────────┘ │     │
│                            └─────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      │ async_channel / cx.spawn
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                          Background Tasks                                    │
│                                                                             │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │                        LocalPty                                       │   │
│  │  ┌───────────────────┐    ┌──────────────────────────────────────┐   │   │
│  │  │   ReadThread      │    │         WriteThread                  │   │   │
│  │  │ std::thread::     │    │ std::thread::spawn {                 │   │   │
│  │  │ spawn {           │    │   loop {                             │   │   │
│  │  │   loop {          │    │     match cmd {                      │   │   │
│  │  │     reader.read() │    │       Write(data) => master.write() │   │   │
│  │  │     tx.send(buf)  │    │       Resize(size) => master.resize()│   │   │
│  │  │   }               │    │     }                                │   │   │
│  │  │ }                 │    │   }                                  │   │   │
│  │  └───────────────────┘    └──────────────────────────────────────┘   │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  ┌──────────────────────────────┐  ┌────────────────────────────────────┐   │
│  │ Term (alacritty_terminal)    │  │ VTE Processor                      │   │
│  │ ┌──────────────────────────┐ │  │ parser.advance(&mut term, data)    │   │
│  │ │ scrollback + grid + mode │ │  │                                    │   │
│  │ │ EventListener (EventProxy)│ │  │                                    │   │
│  │ └──────────────────────────┘ │  └────────────────────────────────────┘   │
│  └──────────────────────────────┘                                           │
│                                                                             │
│  ┌──────────────────────────────────────────────────────────────────┐       │
│  │                         PTY Owner                                 │       │
│  │ Box<dyn Child + Send + Sync>  (子进程，Drop 时自动 kill)          │       │
│  └──────────────────────────────────────────────────────────────────┘       │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 核心组件

#### 1. Term (alacritty 适配层)

包装 alacritty_terminal 的 `Term` 和 VTE `Processor`，提供统一接口：

```rust
struct Term {
    term: alacritty_terminal::Term<EventProxy>,
    parser: Processor<StdSyncHandler>,
}

impl Term {
    /// VTE 解析 PTY 输出
    pub fn advance(&mut self, data: Vec<u8>);
    /// 调整终端网格尺寸
    pub fn resize(&mut self, dimensions: &TermDimensions);
    /// 提取渲染数据（cells, cursor, mode 等）
    pub fn extract(&self) -> ExtractedTerminalData;
}
```

#### 2. Terminal (协调器)

```rust
pub struct Terminal {
    content: TerminalContent,             // 纯渲染状态（plain struct）
    term: Arc<Mutex<Term>>,              // alacritty 状态（在后台任务间共享）
    pty: Arc<dyn Pty>,                    // PTY 抽象
    terminal_size: Option<TerminalSize>,  // 当前 PTY 尺寸（None = 等待首次 resize）
    display_offset: usize,
    selection_head: Option<TerminalPoint>,
    title: String,
    mouse_mode: bool,
}

impl Terminal {
    pub fn new(pty: Arc<dyn Pty>, cx: &mut Context<Self>) -> Result<Self>;
    pub fn input(&mut self, cx: &mut Context<Self>, data: Vec<u8>);
    pub fn sync_size(&mut self, bounds, char_width, char_height, cx);
    pub fn refresh_content(&mut self, cx: &mut Context<Self>);
}
```

#### 3. TerminalContent (渲染状态)

```rust
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
```

#### 4. ExtractedTerminalData (渲染数据载体)

在持锁期间一次性提取所有渲染所需数据，之后无锁应用：

```rust
struct ExtractedTerminalData {
    cells: Vec<IndexedCell>,
    cursor_state: CursorState,
    cursor_char: char,
    mode: TermMode,
    display_offset: usize,
}
```

#### 5. Pty Trait

```rust
#[async_trait]
pub trait Pty: Send + Sync {
    async fn write(&self, data: Vec<u8>) -> Result<()>;
    async fn resize(&self, size: TerminalSize) -> Result<()>;
    fn reader(&self) -> async_channel::Receiver<Vec<u8>>;
    async fn close(&mut self) -> Result<()>;
    fn process_id(&self) -> Option<u32>;
}
```

注意：
- 所有方法使用 `&self` 而非 `&mut self`，内部通过 `Arc<Mutex<_>>` 实现可变性
- 需要 `Send + Sync` bound 以支持多线程访问

#### 6. LocalPty

```rust
pub struct LocalPty {
    process_id: Option<u32>,
    child: Box<dyn Child + Send + Sync>,
    reader_handle: JoinHandle<Result<()>>,
    reader_rx: Receiver<Vec<u8>>,
    writer_handle: JoinHandle<Result<()>>,
    writer_tx: Sender<WriteCommand>,
}
```

双线程架构：
- **Reader 线程**：`std::thread::spawn`，阻塞读取 PTY 输出，通过 `async_channel::Sender` 发送
- **Writer 线程**：`std::thread::spawn`，阻塞接收 `WriteCommand`（Write/Resize），写入 PTY master

### 数据流

#### 整体设计：「生产-消费」分离

采用帧级按需同步模式，PTY reader 只做 VTE 解析（生产），prepaint 时才提取渲染数据（消费）。

```
PTY数据到达
  │
  ▼
background_spawn: term.lock().advance(data)   ← 「生产」只做 VTE 解析
  │
  ▼
entity.update: cx.notify()                     ← 只发信号，不提取数据
  │
  ▼
GPUI 帧循环触发 TerminalElement::prepaint()
  ├─ terminal.sync_size(bounds, cw, ch, cx)   ← 检查/同步终端尺寸
  ├─ terminal.refresh_content(cx)              ← 「消费」提取渲染数据
  │    └─ term.lock().extract()
  │    └─ apply_extracted_data()
  └─ 读取 content → paint
```

这个设计的优势：
- **按需同步**：只有被渲染的 Tab 才执行 extract，后台 Tab 只保持 alacritty 状态但不消耗 extract CPU
- **帧级合并**：一帧内无论收到多少 PTY 数据块，prepaint 只 extract 一次
- **职责分离**：PTY reader 管"生产"（advance），prepaint 管"消费"（extract + paint）

#### 用户输入（按键 → Shell）

```
按键事件
    │
    ▼
TerminalView::handle_key_down()
    │
    ▼
terminal.input(data)
    │
    ▼
cx.spawn: pty.write(data)  // 异步写入 PTY，不阻塞 UI
    │
    ▼
Shell 进程
```

#### 终端输出（Shell → UI）

```
Shell 输出
    │
    ▼
LocalPty ReadThread (std::thread::spawn)
    │
    ▼
阻塞读取 → async_channel::Sender → pty_reader.recv()
    │
    ▼
cx.background_spawn: term.lock().advance(data)  // VTE 解析
    │
    ▼
entity.update: cx.notify()  // 通知 UI 线程
    │
    ▼
TerminalElement::prepaint()
  ├─ sync_size()    // 按需 resize
  └─ refresh_content()
       └─ term.lock().extract() → apply_extracted_data()
    │
    ▼
paint() 渲染到屏幕
```

#### 终端 Resize（Window 变化 → PTY → Shell）

```
窗口尺寸变化
    │
    ▼
TerminalElement::prepaint()
    │
    ▼
terminal.sync_size(bounds, char_width, char_height, cx)
    │
    ├─ 计算 rows = height / char_height, cols = width / char_width
    ├─ terminal_size 比较：相同则跳过
    ├─ term.lock().resize(&dimensions)    ← 同步更新 alacritty 网格
    └─ cx.spawn: pty.resize(new_size)    ← 异步通知 PTY → shell 收到 SIGWINCH
```

### 文件结构

```
src/terminal/
├── mod.rs              # 模块导出
├── terminal.rs         # Terminal 结构体（协调器）
├── content.rs          # TerminalContent（渲染状态）
├── input.rs            # TerminalInput 枚举
├── pty.rs              # Pty trait + TerminalSize
├── local_pty.rs        # LocalPty 实现
├── view.rs             # TerminalView（UI 组件）
└── terminal_element.rs # TerminalElement（渲染元素）
```

### 关键技术细节

#### 异步与并发

- 使用 `tokio` 运行时处理异步任务
- GPUI 提供单线程 UI 更新机制
- `LocalPty` 读取线程使用 `std::thread` 进行阻塞读取
- 用户输入写入 PTY 使用 `cx.spawn` 异步执行，不阻塞 UI
- 终端状态通过 `Arc<Mutex<Term>>` 在后台任务间共享

#### 进程间通信

- `async_channel` — PTY 数据通道（ReadThread → background_spawn）
- `async_channel` — PTY 写入命令通道（UI → WriteThread）
- alacritty 内部事件通过 `EventProxy` + `async_channel` 转发
- GPUI `cx.notify()` — 通知 UI 线程有新数据可供渲染

#### 终端仿真

- `alacritty_terminal::Term` — 终端状态管理（grid, scrollback, mode, cursor）
- `alacritty_terminal::vte::Processor` — ANSI/VTE 序列解析器
- `portable-pty` — 跨平台 PTY 实现
- `TermDimensions` — 适配 alacritty 的 `Dimensions` trait

#### PTY 生命周期管理

- `LocalPty::new(size, command)` 创建 PTY 并 spawn 子进程
- 默认使用初始尺寸 24x80，首次 `sync_size()` 调用时根据视图实际尺寸 resize 到正确大小
- `LocalPty` 在 Drop 时通过 `child.clone_killer().kill()` 终止子进程

#### 终端 Resize 机制

- `Terminal.terminal_size: Option<TerminalSize>` 记录当前 PTY 尺寸，`None` 表示等待首次 resize
- 首次渲染时 `sync_size()` 使用视图实际尺寸替代默认 24x80
- 后续每帧检查尺寸是否变化，仅在变化时才 resize（避免不必要的系统调用）
- `sync_size()` 同步 resize alacritty Term（当前线程 `block` 获取锁），异步 resize PTY（`cx.spawn`）

## 编码规范

### 后台任务处理

- 无特殊要求时，使用 `cx.background_spawn()` 处理后台异步任务

### 代码格式化

- 每次修改代码后，必须运行 `rustfmt` 格式化代码

## 参考实现

### Zed 编辑器

项目根目录下的 `zed` 目录包含 Zed 编辑器的源代码。在实现 terminal 和 SSH 相关功能时，应参考 Zed 编辑器的实现方式，借鉴其设计思路和代码组织方式。

详见 skill: `zed-terminal`
