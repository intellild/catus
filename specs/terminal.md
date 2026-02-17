# Terminal 架构设计规格

## 1. 整体架构

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                               UI Thread (GPUI)                              │
│                                                                             │
│  ┌─────────────────┐       ┌─────────────────────────────────────┐         │
│  │   TerminalView  │◄─────►│         Entity<Terminal>            │         │
│  │   (渲染 + 交互)  │       │  ┌───────────────────────────────┐  │         │
│  └────────┬────────┘       │  │  content: Entity<TerminalContent>│ │         │
│           │                │  │  command_tx: mpsc::Sender<...>  │  │         │
│           │                │  │  content_tx: watch::Sender<...> │  │         │
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
│   │                         async闭包                                    │   │
│   │                                                                     │   │
│   │   loop {                                                            │   │
│   │       tokio::select! {                                              │   │
│   │           Some(input) = input_rx.recv() => {                        │   │
│   │               match input {                                         │   │
│   │                   TerminalInput::PtyData(data) => process_data(),   │   │
│   │                   TerminalInput::Resize(size) => resize_term(),     │   │
│   │                   TerminalInput::Write(data) => pty.write(),        │   │
│   │               }                                                     │   │
│   │           }                                                         │   │
│   │       }                                                             │   │
│   │       // 生成新内容                                                 │   │
│   │       let content = make_content(&term);                            │   │
│   │       content_tx.send(content);                                     │   │
│   │   }                                                                 │   │
│   │                                                                     │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                      │                                      │
│                    ┌─────────────────┼─────────────────┐                    │
│                    │                 │                 │                    │
│                    ▼                 ▼                 ▼                    │
│   ┌──────────────────────┐  ┌──────────────────┐  ┌──────────────┐         │
│   │   Arc<FairMutex<Term>>│  │   Box<dyn Pty>   │  │  watch::rx   │         │
│   │   (alacritty终端状态) │  │   (pty抽象)       │  │  (UI读取)    │         │
│   └──────────────────────┘  └──────────────────┘  └──────────────┘         │
│                                     │                                       │
└─────────────────────────────────────┼───────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Pty 实现层                                      │
│                                                                             │
│  ┌─────────────────────────────┐  ┌─────────────────────────────────────┐   │
│  │       LocalPty              │  │           SshPty                    │   │
│  │  ┌───────────────────────┐  │  │  ┌─────────────────────────────┐    │   │
│  │  │ portable_pty::PtyPair │  │  │  │ ssh2::Session + Channel     │    │   │
│  │  │ ├─ master (write TX)  │  │  │  │ ├─ session: Arc<Mutex<>>    │    │   │
│  │  │ └─ slave              │  │  │  │ └─ channel: Arc<Mutex<>>    │    │   │
│  │  └───────────────────────┘  │  │  └─────────────────────────────┘    │   │
│  │  ┌───────────────────────┐  │  │  ┌─────────────────────────────┐    │   │
│  │  │    ReadThread         │  │  │  │       ReadThread            │    │   │
│  │  │  (阻塞读取 + channel)  │  │  │  │   (阻塞读取 + channel)       │    │   │
│  │  │  handle: JoinHandle   │  │  │  │   handle: JoinHandle        │    │   │
│  │  └───────────────────────┘  │  │  └─────────────────────────────┘    │   │
│  └─────────────────────────────┘  └─────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 2. 核心类型定义

### 2.1 TerminalContent

终端渲染状态，作为独立的 Entity 存在。

```rust
/// 终端内容实体 - 纯渲染状态
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

impl EventEmitter<TerminalEvent> for TerminalContent {}
```

### 2.2 Terminal

终端协调器 Entity，管理后台任务和状态同步。

```rust
/// 终端协调器
pub struct Terminal {
    /// 内容实体（独立 Entity，可被观察）
    pub content: Entity<TerminalContent>,
    
    /// 向后台任务发送输入
    pub input_tx: mpsc::Sender<TerminalInput>,
    
    /// 从后台任务接收更新
    pub content_rx: watch::Receiver<TerminalContent>,
    
    /// 后台任务句柄（Drop 时自动取消）
    _task: Task<()>,
}

impl Terminal {
    /// 创建新的终端
    pub fn new(cx: &mut Context<Self>) -> Self;
    
    /// 附加 PTY（本地或 SSH）
    pub fn attach_pty(&mut self, pty: Box<dyn Pty>, cx: &mut Context<Self>);
    
    /// 写入输入数据
    pub fn input(&self, data: Vec<u8>);
    
    /// 调整终端大小
    pub fn resize(&self, size: TerminalSize);
    
    /// 获取当前内容（从 watch channel）
    pub fn current_content(&self) -> TerminalContent;
}

impl EventEmitter<TerminalEvent> for Terminal {}
```

### 2.3 TerminalInput (输入枚举)

定义所有从 UI 发送到后台的输入类型。

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

### 2.4 Pty Trait

PTY 抽象接口，支持本地和 SSH 两种实现。

```rust
/// PTY 抽象
pub trait Pty: Send + Sync {
    /// 写入数据（线程安全）
    fn write(&self, data: &[u8]) -> anyhow::Result<()>;
    
    /// 调整大小
    fn resize(&self, size: TerminalSize) -> anyhow::Result<()>;
    
    /// 启动读取循环，返回数据接收器
    /// 在内部创建独立线程进行阻塞读取
    fn start_reader(self: Box<Self>) -> mpsc::Receiver<Vec<u8>>;
    
    /// 关闭 PTY
    fn close(&self) -> anyhow::Result<()>;
    
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
```

### 2.5 LocalPty

本地 PTY 实现，基于 `portable-pty`。

```rust
/// 本地 PTY
pub struct LocalPty {
    // portable_pty 内部实现
}

impl LocalPty {
    /// 创建本地 PTY
    pub fn new(size: TerminalSize, shell: &str) -> anyhow::Result<Self>;
}

impl Pty for LocalPty {
    fn write(&self, data: &[u8]) -> anyhow::Result<()>;
    fn resize(&self, size: TerminalSize) -> anyhow::Result<()>;
    fn start_reader(self: Box<Self>) -> mpsc::Receiver<Vec<u8>>;
    fn close(&self) -> anyhow::Result<()>;
    fn process_id(&self) -> Option<u32>;
}
```

### 2.6 SshPty

SSH PTY 实现，基于 `ssh2`。

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

impl Pty for SshPty {
    fn write(&self, data: &[u8]) -> anyhow::Result<()>;
    fn resize(&self, size: TerminalSize) -> anyhow::Result<()>;
    fn start_reader(self: Box<Self>) -> mpsc::Receiver<Vec<u8>>;
    fn close(&self) -> anyhow::Result<()>;
    fn process_id(&self) -> Option<u32>;
}
```

## 3. 后台任务流程

```rust
// Terminal::new 中启动后台任务
let background_task = cx.background_spawn({
    let input_rx = input_rx;           // mpsc::Receiver<TerminalInput>
    let content_tx = content_tx;       // watch::Sender<TerminalContent>
    let weak_content = content.downgrade();
    
    async move {
        // 初始化 Term（alacritty）
        let term = Arc::new(FairMutex::new(Term::new(...)));
        let mut pty: Option<Box<dyn Pty>> = None;
        let mut pty_reader: Option<mpsc::Receiver<Vec<u8>>> = None;
        
        loop {
            tokio::select! {
                // 1. 处理 UI 输入
                Some(input) = input_rx.recv() => {
                    match input {
                        TerminalInput::AttachPty(new_pty) => {
                            pty_reader = Some(new_pty.start_reader());
                            pty = Some(new_pty);
                        }
                        TerminalInput::Write(data) => {
                            if let Some(ref p) = pty {
                                let _ = p.write(&data);
                            }
                        }
                        TerminalInput::Resize(size) => {
                            if let Some(ref p) = pty {
                                let _ = p.resize(size);
                            }
                            term.lock().resize(size);
                        }
                        TerminalInput::Shutdown => break,
                        _ => {}
                    }
                }
                
                // 2. 处理 PTY 输出（如果已附加）
                Some(data) = async {
                    match pty_reader {
                        Some(ref mut rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    // 解析 VTE
                    let mut parser = Processor::new();
                    let mut term = term.lock();
                    parser.advance(&mut *term, &data);
                    drop(term);
                    
                    // 生成新内容
                    let new_content = make_content(&term);
                    let _ = content_tx.send(new_content);
                    
                    // 通知 Entity 更新
                    if let Some(entity) = weak_content.upgrade() {
                        entity.update(cx, |_, cx| {
                            cx.emit(TerminalEvent::Wakeup);
                        }).ok();
                    }
                }
            }
        }
    }
});
```

## 4. UI 层接口

### 4.1 TerminalView

```rust
pub struct TerminalView {
    terminal: Entity<Terminal>,
    focus_handle: FocusHandle,
    _content_observer: Subscription,
}

impl TerminalView {
    pub fn new(terminal: Entity<Terminal>, cx: &mut Context<Self>) -> Self;
    
    fn handle_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>);
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement;
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle;
}
```

### 4.2 TerminalElement

```rust
pub struct TerminalElement {
    content: Entity<TerminalContent>,
    focus_handle: FocusHandle,
}

impl TerminalElement {
    pub fn new(content: Entity<TerminalContent>, focus_handle: FocusHandle) -> Self;
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
terminal.input(data) → input_tx.send(TerminalInput::Write(data))
    │
    ▼
[跨线程]
    │
    ▼
后台任务 select! 收到 TerminalInput::Write
    │
    ▼
pty.write(data)  // 直接写入
```

### 5.2 终端输出

```
PTY 有输出
    │
    ▼
Pty::start_reader() 内部线程
    │
    ▼
阻塞读取 → reader_tx.send(data)
    │
    ▼
[跨线程]
    │
    ▼
后台任务 select! 收到 TerminalInput::PtyData
    │
    ▼
parser.advance(&mut term, data)
    │
    ▼
content_tx.send(new_content)
    │
    ▼
TerminalContent Entity 被更新
    │
    ▼
cx.emit(TerminalEvent::Wakeup)
    │
    ▼
UI 重渲染
```

## 6. 文件结构

```
src/terminal/
├── mod.rs              # 模块导出
├── terminal.rs         # Terminal 结构体（Entity + 协调器）
├── content.rs          # TerminalContent（Entity 状态）
├── input.rs            # TerminalInput 枚举
├── pty.rs              # Pty trait + TerminalSize
├── local_pty.rs        # LocalPty 实现
├── ssh_pty.rs          # SshPty 实现
├── view.rs             # TerminalView
├── element.rs          # TerminalElement
└── mappings/
    ├── keys.rs         # 按键映射
    └── colors.rs       # 颜色映射
```

## 7. 使用示例

### 7.1 创建本地终端

```rust
let terminal = cx.new(|cx| Terminal::new(cx));

terminal.update(cx, |term, cx| {
    let pty = LocalPty::new(
        TerminalSize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 },
        "/bin/bash",
    ).unwrap();
    
    term.attach_pty(Box::new(pty), cx);
});
```

### 7.2 创建 SSH 终端

```rust
let terminal = cx.new(|cx| Terminal::new(cx));

// 在后台创建 SSH 连接
cx.spawn(async move |this, cx| {
    let pty = cx.background_spawn(async move {
        SshPty::new("host:22", "user", SshAuth::Agent, size)
    }).await?;
    
    this.update(cx, |term, cx| {
        term.attach_pty(Box::new(pty), cx);
    })?;
    
    Ok(())
}).detach();
```

### 7.3 观察内容变化

```rust
// 在 TerminalView 中
let _observer = cx.observe(&terminal.read(cx).content, |this, _, cx| {
    cx.notify(); // TerminalContent 变化时重渲染
});
```

## 8. 设计要点

1. **TerminalContent 是独立 Entity**
   - 可被单独观察，解耦渲染和数据更新
   - watch channel 保证总能获取最新值

2. **Terminal 作为协调器**
   - 管理后台任务生命周期
   - 持有 input_tx 供外部发送命令
   - 持有 content_rx 供读取最新内容

3. **TerminalInput 统一输入**
   - 枚举类型包含所有可能的输入来源
   - 单通道简化后台任务处理

4. **Pty::start_reader() 模式**
   - PTY 各自管理读取线程
   - 通过 mpsc channel 将读取结果汇入 TerminalInput::PtyData

5. **write 直接调用**
   - portable-pty 和 ssh2 的 write 都是线程安全的
   - 无需通过 channel 转发，减少延迟
