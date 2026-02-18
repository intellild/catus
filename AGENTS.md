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
│                               UI Thread (GPUI)                              │
│                                                                             │
│  ┌─────────────────┐       ┌─────────────────────────────────────┐         │
│  │   TerminalView  │◄─────►│         Entity<Terminal>            │         │
│  │   (渲染 + 交互)  │       │  ┌───────────────────────────────┐  │         │
│  └────────┬────────┘       │  │  content: Entity<TerminalContent>│ │         │
│           │                │  │  pty_writer: Arc<Mutex<...>>  │  │         │
│           │                │  │  input_tx: mpsc::Sender<...>  │  │         │
│           │                │  └───────────────────────────────┘  │         │
│           │                └─────────────────┬───────────────────┘         │
│           │                                  │                             │
│           │ cx.observe()                     │                             │
│           │                                  │                             │
│  ┌────────▼────────┐       ┌─────────────────▼─────────────────────┐       │
│  │ TerminalElement │◄─────►│      Entity<TerminalContent>          │       │
│  │   (paint渲染)   │  read │  ┌───────────────────────────────┐    │       │
│  └─────────────────┘       │  │  cells, cursor, mode, title   │    │       │
│                            │  │  selection, bounds, ...       │    │       │
│                            │  └───────────────────────────────┘    │       │
│                            └───────────────────────────────────────┘       │
└─────────────────────────────────────────────────────────────────────────────┘
                                       │
                                       │ mpsc::channel<TerminalInput>
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Background Task (cx.background_spawn)               │
│                                                                             │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                    run_terminal_loop (async)                         │   │
│   │                                                                     │   │
│   │   loop {                                                            │   │
│   │       match input_rx.recv().await {                                 │   │
│   │           TerminalInput::PtyData(data) => {                         │   │
│   │               parser.advance(&mut term, &data);                     │   │
│   │               content_tx.send(make_content(&term));                 │   │
│   │           }                                                         │   │
│   │           TerminalInput::Resize(size) => {                          │   │
│   │               term.resize(dimensions);                              │   │
│   │           }                                                         │   │
│   │           TerminalInput::Shutdown => break,                         │   │
│   │       }                                                             │   │
│   │   }                                                                 │   │
│   │                                                                     │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                      │                                      │
│                    ┌─────────────────┼─────────────────┐                    │
│                    │                 │                 │                    │
│                    ▼                 ▼                 ▼                    │
│   ┌──────────────────────┐  ┌──────────────────┐  ┌──────────────┐         │
│   │   Term (alacritty)   │  │   Box<dyn Pty>   │  │  watch::tx   │         │
│   │   (终端状态 + VTE)   │  │   (pty抽象)       │  │  (UI读取)    │         │
│   └──────────────────────┘  └──────────────────┘  └──────────────┘         │
│                                     │                                       │
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
│  │  │    ReadThread         │  │  │  │       ReadThread            │    │   │
│  │  │  (阻塞读取 + channel)  │  │  │  │   (阻塞读取 + channel)       │    │   │
│  │  │  handle: JoinHandle   │  │  │  │   handle: JoinHandle        │    │   │
│  │  └───────────────────────┘  │  │  └─────────────────────────────┘    │   │
│  └─────────────────────────────┘  └─────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 核心组件

#### 1. Terminal (协调器)

```rust
pub struct Terminal {
    /// 内容实体（独立 Entity，可被观察）
    pub content: Entity<TerminalContent>,
    /// 向后台任务发送输入
    input_tx: mpsc::Sender<TerminalInput>,
    /// 从后台任务接收内容更新
    _content_rx: watch::Receiver<TerminalContent>,
    /// PTY writer，用于写入用户输入
    pty_writer: Arc<Mutex<Option<Box<dyn Pty>>>>,
    /// 后台任务句柄
    _task: Task<()>,
    /// UI 更新任务句柄
    _ui_task: Task<()>,
}
```

#### 2. TerminalContent (渲染状态)

```rust
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
    pub title: String,
}
```

#### 3. TerminalInput (输入枚举)

```rust
pub enum TerminalInput {
    /// PTY 输出数据（来自 read thread）
    PtyData(Vec<u8>),
    /// 调整终端大小
    Resize(TerminalSize),
    /// 获取当前内容（强制刷新）
    Sync,
    /// 关闭终端
    Shutdown,
}
```

#### 4. Pty Trait

```rust
pub trait Pty: Send {
    /// 写入数据（线程安全）
    fn write(&self, data: &[u8]) -> anyhow::Result<()>;
    /// 调整大小
    fn resize(&self, size: TerminalSize) -> anyhow::Result<()>;
    /// 启动读取循环，返回数据接收器（只能调用一次）
    fn start_reader(&mut self) -> mpsc::Receiver<Vec<u8>>;
    /// 关闭 PTY
    fn close(&self) -> anyhow::Result<()>;
    /// 获取进程 ID（本地 PTY 有效）
    fn process_id(&self) -> Option<u32>;
}
```

#### 5. LocalPty

```rust
pub struct LocalPty {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    process_id: Option<u32>,
    _reader_thread: Option<std::thread::JoinHandle<()>>,
    _child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    reader_rx: Option<mpsc::Receiver<Vec<u8>>>,
}
```

### 数据流

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
PTY::write(data)  // 直接写入，无需 channel
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
阻塞读取 → mpsc::channel → TerminalInput::PtyData
    │
    ▼
run_terminal_loop: parser.advance(&mut term, data)
    │
    ▼
watch::channel → TerminalContent Entity 更新
    │
    ▼
cx.emit(TerminalEvent::Wakeup)
    │
    ▼
UI 重渲染 (TerminalElement::paint)
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
- 终端 Worker 运行在独立线程
- GPUI 提供单线程 UI 更新机制
- `LocalPty` 读取线程使用 `std::thread` 进行阻塞读取

#### 进程间通信

- `tokio::sync::mpsc` - 输入命令通道（UI → Background）
- `tokio::sync::watch` - 终端内容广播（Background → UI）
- `std::sync::Mutex` - PTY writer 线程安全访问

#### 终端仿真

- `alacritty_terminal::Term` - VTE 解析和终端状态管理
- `alacritty_terminal::Processor` - ANSI/VTE 序列解析
- `portable-pty` - 跨平台 PTY 实现

#### PTY 生命周期管理

- `LocalPty` 保存读取线程 handle (`_reader_thread`) 和子进程 handle (`_child`)
- 当 `LocalPty` 被 drop 时，相关资源自动清理
- `reader_rx` 存储 PTY 输出接收器，`start_reader()` 只能调用一次

## 编码规范

### 后台任务处理

- 无特殊要求时，使用 `cx.background_spawn()` 处理后台异步任务

### 代码格式化

- 每次修改代码后，必须运行 `rustfmt` 格式化代码

## 参考实现

### Zed 编辑器

项目根目录下的 `zed` 目录包含 Zed 编辑器的源代码。在实现 terminal 和 SSH 相关功能时，应参考 Zed 编辑器的实现方式，借鉴其设计思路和代码组织方式。

详见 skill: `zed-terminal`
