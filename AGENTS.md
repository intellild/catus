# Catus

## 项目定位

Catus 是一个基于 Rust 和 GPUI 的本地终端客户端。当前代码已实现：

- 多 Workspace：左侧侧边栏列出所有 Workspace，可切换/关闭/新增。
- Workspace 可以是本地的（系统默认 shell）或 SSH（启动 `ssh` 等本地进程作为命令）。
- 每个 Workspace 内多 Tab、Pane 水平/垂直分割。
- 本地 PTY 终端会话。
- 终端输入、滚动、选择、复制和粘贴。

当前仓库没有 SFTP 实现，也没有 Tokio 运行时依赖；不要在文档或实现里假设这些能力已经存在。SSH workspace 复用 `LocalPty`，只是把启动命令换成 `ssh`，不是远程 PTY。

## 技术栈

- UI: `gpui`, `gpui-component`
- 终端仿真: `alacritty_terminal`
- PTY: `portable-pty`
- 异步与通信: GPUI task API, `async-channel`, `async-lock`, `blocking`, `async-trait`

## 关键目录

- `src/main.rs`: 应用初始化、主题设置、全局 key binding。
- `src/app.rs`: 应用级状态，持有多个 `Workspace` 与激活索引。
- `src/workspace_kind.rs`: `WorkspaceKind`（`Local` / `Ssh`），决定 workspace 的图标、展示名和 PTY 启动命令。
- `src/workspace.rs`: 单个 `Workspace`：Tab 管理和终端实体创建，按 `kind.command()` 创建 PTY。
- `src/sidebar/mod.rs`: 左侧 `WorkspaceSidebar`，列出 workspace 列表、切换/关闭/新增。
- `src/add_workspace_dialog.rs`: 「添加 Workspace」对话框，选择类型并填写命令。
- `src/main_view.rs`: 主视图，组合侧边栏与当前 workspace 的 title bar + pane 区，render 时从 `App` 解析激活的 workspace。
- `src/pane/`: Pane tree、分割和关闭逻辑。
- `src/terminal/terminal.rs`: `Terminal` 协调器，连接 PTY、alacritty 状态和渲染状态。
- `src/terminal/local_pty.rs`: 本地 PTY 实现，使用独立 reader/writer 线程处理阻塞 I/O。
- `src/terminal/view.rs`: 终端 GPUI view、键盘输入、滚轮、复制粘贴。
- `src/terminal/terminal_element.rs`: 低层 GPUI `Element`，负责 prepaint 同步和 paint 渲染。
- `src/terminal/content.rs`: 终端渲染用的 plain state 和颜色转换。

## 终端架构约定

- `Terminal` 是协调器，不直接绘制 UI。它持有 `TerminalContent`、`Arc<async_lock::Mutex<Term>>`、`Arc<dyn Pty>`、当前尺寸、标题、滚动状态和选择状态。
- `Term` 是 `alacritty_terminal::Term<EventProxy>` 与 VTE `Processor` 的内部适配层。PTY 输出到达后只做 VTE 解析和 `cx.notify()`。
- `TerminalElement::prepaint()` 是渲染数据的消费点：先 `sync_size()`，再 `refresh_content()`，最后 paint 使用快照后的 `TerminalContent`。
- 保持“生产/消费分离”：后台 PTY reader 只推进 alacritty 状态；可见 UI 帧才提取可渲染数据。
- `LocalPty` 使用 `std::thread::spawn` 处理阻塞读写，通过 `async_channel` 与 UI/任务侧通信。
- `Pty` trait 使用 `&self` 的 async 方法，内部可变性由具体实现负责。
- `Workspace::create_terminal_entity` 是创建终端的唯一入口，用 `kind.command()` 作为 `LocalPty::new` 的命令；`None` 表示系统默认 shell，`Some("ssh ...")` 用于 SSH workspace。
- `MainView` 在 render 时从 `App::active_workspace()` 解析当前 workspace，因此切换 workspace 不需要重建 title bar / pane。

## GPUI 约定

- 修改实体状态时使用 `Entity::update` / `cx.notify()` 触发 UI 更新。
- 无特殊要求时，后台异步工作优先使用 GPUI 提供的 task API；阻塞 I/O 放到专门线程或 `blocking` 中。
- 终端渲染使用低层 `Element` API；涉及 layout/prepaint/paint 时先参考 `src/terminal/terminal_element.rs` 的现有生命周期。
- 新增 GPUI action 或 key binding 时，让 action 定义、`.key_context(...)`、`.on_action(...)` 和 `cx.bind_keys(...)` 保持在同一语义层级。

## 键盘事件注意事项

项目使用 `gpui_component::Root`。`gpui_component::init(cx)` 会注册 Root 层的全局键绑定，例如 `tab` 和 `shift-tab` 的焦点导航。

终端需要接收这些按键时，必须在更深的 `"Terminal"` context 中覆盖绑定：

- `TerminalView` 根元素保留 `.key_context("Terminal")`。
- `TerminalView` 注册对应 `.on_action(...)`。
- `main.rs` 在 `gpui_component::init(cx)` 之后调用 `cx.bind_keys(...)` 注册 `"Terminal"` context 的绑定。

同类被上层 keymap 拦截的特殊按键，也按这个 context 覆盖方式处理。

## 开发要求

- 代码变更后运行 `cargo fmt`。
- Rust 行为变更后至少运行 `cargo check`；涉及终端输入、PTY、pane 或 tab 的变更应补充更具体的验证。
- 保持文档和源码一致。不要在 `AGENTS.md` 中保留字段级代码块、长流程图或尚未实现的能力说明，除非它们会被持续维护。
