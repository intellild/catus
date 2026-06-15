# Catus

## 项目定位

Catus 是一个基于 Rust 和 GPUI 的本地终端客户端。当前代码已实现：

- 多 Tab 工作区。
- Pane 水平/垂直分割。
- 本地 PTY 终端会话。
- 终端输入、滚动、选择、复制和粘贴。

当前仓库没有 SFTP 实现，也没有 Tokio 运行时依赖；不要在文档或实现里假设这些能力已经存在。

## 技术栈

- UI: `gpui`, `gpui-component`
- 终端仿真: `alacritty_terminal`
- PTY: `portable-pty`
- 异步与通信: GPUI task API, `async-channel`, `async-lock`, `blocking`, `async-trait`

## 关键目录

- `src/main.rs`: 应用初始化、主题设置、全局 key binding。
- `src/app.rs`: 应用级状态，目前持有一个 `Workspace`。
- `src/workspace.rs`: Tab 管理和终端实体创建。
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
