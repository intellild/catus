# Catus - 文件资源管理器

## 项目简介

Catus 是一个基于 Rust 和 Dioxus 框架开发的文件资源管理器桌面应用，提供直观的文件浏览和操作界面。

## 技术栈

- **语言**: Rust (Edition 2024)
- **UI 框架**: Dioxus 0.7 (跨平台 Rust UI 框架)
- **异步运行时**: Tokio
- **终端仿真**: alacritty_terminal 0.25.1
- **PTY 操作**: portable-pty 0.9
- **序列化**: serde

## 项目结构

```
src/
├── main.rs              # 应用入口，Tab 系统，UI 布局
└── terminal/
    ├── mod.rs           # 模块导出
    ├── state.rs         # Terminal 状态管理（alacritty Term 封装）
    └── view.rs          # Terminal 视图组件
```

## 编码规范

### 1. 代码格式化

**⚠️ Coding Agent 必须遵循**: 每次修改完代码后，必须运行 `cargo fmt` 进行格式化，确保代码风格一致。

```bash
# 格式化所有代码（修改后必须执行）
cargo fmt

# 检查代码格式是否符合规范（CI 场景）
cargo fmt -- --check
```

**要求**：
- 在完成任何代码修改后，必须执行 `cargo fmt` 格式化代码
- 确保代码符合 Rust 官方代码风格规范
- 提交前确认无格式化错误

### 2. 文档查看

使用 `cargo doc` 生成并查看 API 文档：

```bash
# 生成文档
cargo doc

# 生成并在浏览器中打开
cargo doc --open

# 包含私有项的文档
cargo doc --document-private-items
```

## 终端系统架构

### 核心组件

1. **TerminalHandle**: 终端状态句柄
   - 封装 `Arc<FairMutex<AlacrittyTerm<ChannelEventListener>>>`
   - 提供 `with_renderable_content()` 方法用于渲染
   - 通过 channel 接收 alacritty 事件

2. **ChannelEventListener**: 事件监听器
   - 实现 `EventListener` trait
   - 将 alacritty 的 `Event::Wakeup` 等事件转发到 channel

3. **渲染流程**:
   ```
   PTY 读取 → vte::Processor::advance() → AlacrittyTerm
                                       ↓
                                    Event::Wakeup
                                       ↓
                              dioxus signal 更新
                                       ↓
                    TerminalHandle::with_renderable_content()
                                       ↓
                         遍历 display_iter 渲染
   ```

### 关键类型

```rust
// 终端句柄
pub struct TerminalHandle {
    pub term: Arc<FairMutex<AlacrittyTerm<ChannelEventListener>>>,
    pub event_rx: mpsc::Receiver<Event>,
    ...
}

// 渲染时使用
handle.with_renderable_content(|content| {
    for indexed in content.display_iter {
        // indexed.point - 单元格位置
        // indexed.cell - 单元格数据
    }
});
```

### Cell 转换

使用 `cell_to_ui_cell()` 将 alacritty 的 Cell 转换为 UI 可渲染格式：

```rust
pub fn cell_to_ui_cell(cell: &Cell) -> (char, [u8; 3], [u8; 3], bool) {
    // 返回: (字符, 前景色RGB, 背景色RGB, 是否粗体)
}
```

## Tab 系统

当前实现为简单的 Dioxus Signal 管理：

```rust
// Tab 类型
#[derive(Clone, PartialEq)]
enum TabKind {
    Shell,
    Sftp,
}

// Tab 数据结构
#[derive(Clone, PartialEq)]
struct Tab {
    id: usize,
    kind: TabKind,
    title: String,
}
```

使用 `use_signal()` 管理 Tab 列表和激活状态。

## 构建运行

```bash
# 开发模式运行
cargo run

# 发布构建
cargo build --release

# 检查代码
cargo check

# 查看文档
cargo doc --open

# 运行测试
cargo test
```

## 参考资源

- [Dioxus 文档](https://dioxuslabs.com/docs/)
- [alacritty_terminal 文档](https://docs.rs/alacritty_terminal)
- [tokio 文档](https://docs.rs/tokio)
- [portable-pty 文档](https://docs.rs/portable-pty)
