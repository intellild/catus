# Catus - 文件资源管理器

## 项目简介

Catus 是一个基于 Rust 和 GPUI 框架开发的文件资源管理器桌面应用，提供直观的文件浏览和操作界面。

## 技术栈

- **语言**: Rust (Edition 2024)
- **UI 框架**: GPUI 0.2.2 (Zed 编辑器同款框架)
- **组件库**: gpui-component 0.5.1
- **系统目录**: dirs 6
- **异步/错误处理**: anyhow
- **终端仿真**: alacritty_terminal 0.25.1 (预留)
- **SSH连接**: russh 0.57.0, russh-sftp 2.1.1 (预留)

## 项目结构

```
src/
├── main.rs              # 应用入口，窗口初始化
├── explorer_view.rs     # 文件浏览器视图
├── explorer_view_item.rs # 文件项数据结构
├── tab.rs               # Tab系统（Tab, TabKind, TabManager）
├── terminal_view.rs     # 终端视图（本地+SSH预留）
└── workspace.rs         # 工作区主视图，管理多个Tab
```

## 编码规范

### 1. Render trait 实现

所有视图组件实现 `Render` trait 时，返回类型使用 `impl IntoElement`：

```rust
impl Render for MyView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 返回 Element
    }
}
```

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

GPUI 和 gpui-component 的 API 文档可以通过此方式本地查看，建议开发时保持开启。

### 3. 代码格式化

使用 `rustfmt` 格式化代码，保持代码风格一致：

```bash
# 格式化所有代码
cargo fmt

# 检查代码格式是否符合规范（CI 场景）
cargo fmt -- --check
```

提交代码前建议先运行 `cargo fmt` 确保代码格式正确。

## 开发提示

### 获取主题颜色

在渲染上下文中通过 `cx.theme()` 获取当前主题：

```rust
.border_color(cx.theme().border)
.text_color(cx.theme().muted_foreground)
.bg(cx.theme().accent)
```

### 常用 GPUI Component 组件

- **布局**: `h_flex()`, `v_flex()`, `div()`
- **按钮**: `Button::new("id").icon(IconName::XXX)`
- **图标**: `Icon::new(IconName::Folder)`
- **文本**: `Label::new("text")`
- **列表**: `v_virtual_list()` - 虚拟列表，适合大量数据

### VirtualList 使用模式

```rust
v_virtual_list(
    view.clone(),
    "list-id",
    item_sizes,  // Rc<Vec<Size<Pixels>>>
    move | this, range, _window, cx| {
        // range: Range<usize>，需要渲染的索引范围
        // 返回 Vec<impl IntoElement>
    },
)
.track_scroll( & self .scroll_handle)
```

### 状态更新

通过 `view.update(cx, |this, cx| { ... })` 更新视图状态，修改后调用 `cx.notify()` 触发重绘：

```rust
view_for_click.update(cx, | this, cx| {
    this.some_state = new_value;
    cx.notify();
});
```

## Tab 系统架构

### 核心组件

1. **TabId**: 全局唯一的 Tab 标识符
2. **TabKind**: Tab 类型枚举
   - `Explorer` - 本地文件浏览器
   - `Terminal` - 本地终端
   - `SshTerminal` - SSH远程终端（预留）
   - `SftpExplorer` - SFTP文件管理器（预留）
3. **Tab**: 单个Tab的状态（标题、激活状态、修改标记等）
4. **TabManager**: 管理所有Tab的生命周期

### 使用示例

```rust
// 创建TabManager
let mut tab_manager = TabManager::with_defaults();

// 添加新Tab
let id = tab_manager.add_tab(TabKind::Explorer { path: PathBuf::from("/home") });

// 切换Tab
tab_manager.activate_tab(id);

// 关闭Tab
tab_manager.close_tab(id);
```

### TabBar 渲染要点

- 使用 `Button` 组件实现可点击的Tab项
- 使用 `Selectable` trait 设置选中状态
- 使用 `ButtonVariants` trait 设置ghost样式
- 每个Tab包含独立的关闭按钮

```rust
Button::new(("tab", tab_id.0))
    .icon(icon)
    .label(title)
    .xsmall()
    .when(is_active, |this| this.selected(true))
    .when(!is_active, |this| this.ghost())
    .on_click(move |_e, _window, cx| {
        // 激活Tab逻辑
    })
```

## 终端视图（Terminal View）

### 架构设计

1. **TerminalView**: 本地终端视图
   - 基于 `alacritty_terminal` 预留集成接口
   - 支持键盘输入、滚动、选择

2. **RemoteTerminalView**: SSH远程终端（预留）
   - 配置结构：`SshConfig`
   - 认证方式：`SshAuthMethod`（密码/私钥/代理）
   - 连接状态：`ConnectionState`

### 预留接口

```rust
// SSH配置
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: SshAuthMethod,
    pub working_dir: PathBuf,
}

// 认证方式
pub enum SshAuthMethod {
    Password(String),
    PrivateKey { key_path: PathBuf, passphrase: Option<String> },
    Agent,
}
```

## 键盘快捷键

在 `main.rs` 中使用 `cx.bind_keys()` 注册全局快捷键：

```rust
cx.bind_keys([
    KeyBinding::new("ctrl-t", workspace::NewTerminal, Some("Workspace")),
    KeyBinding::new("ctrl-shift-e", workspace::NewFileExplorer, Some("Workspace")),
    KeyBinding::new("ctrl-w", workspace::CloseActiveTab, Some("Workspace")),
    KeyBinding::new("ctrl-tab", workspace::NextTab, Some("Workspace")),
    KeyBinding::new("ctrl-shift-tab", workspace::PrevTab, Some("Workspace")),
]);
```

使用 `on_action` 处理Action：

```rust
.on_action(cx.listener(|this, _: &workspace::NewTerminal, window, cx| {
    let working_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    this.new_terminal_tab(working_dir, window, cx);
}))
```

## 常见问题

### 1. Icon 使用

`IconName` 枚举可用的图标名称需要查阅 `gpui-component` 源码：

```rust
// 查看完整列表
~/.cargo/registry/src/*/gpui-component-*/src/icon.rs

// 常用图标
IconName::Folder          // 文件夹
IconName::SquareTerminal  // 终端
IconName::Close           // 关闭
IconName::Plus            // 添加
IconName::Globe           // 远程/网络
IconName::CircleUser      // 用户/SSH
```

### 2. 尺寸设置

`Icon` 使用 `Sizable` trait：

```rust
Icon::new(IconName::Folder).xsmall()  // 超小
Icon::new(IconName::Folder).small()   // 小
Icon::new(IconName::Folder).large()   // 大
```

### 3. 必需导入的 Trait

```rust
use gpui::prelude::FluentBuilder;      // .when() 方法
use gpui::prelude::InteractiveElement; // 交互方法
use gpui_component::Sizable;           // 尺寸方法
use gpui_component::Selectable;        // 选中状态
use gpui_component::button::ButtonVariants; // 按钮样式
```

### 4. Borrow Checker 注意事项

在遍历集合同时修改自身时，注意借用规则：

```rust
// 错误示例
if let Some(tab) = self.tab_manager.get_tab(id) {
    self.modify_something(); // 错误：同时存在不可变和可变借用
}

// 正确做法：克隆数据
if let Some(tab) = self.tab_manager.get_tab(id).cloned() {
    self.modify_something(); // OK
}
```

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

## 依赖更新注意事项

GPUI 和 gpui-component 的 API 可能随版本变化，升级后建议：

1. 运行 `cargo doc` 查看最新 API
2. 检查 `Theme`、`Window` 等核心类型的变更
3. 验证主题设置方式是否有变化

## 参考资源

- [GPUI 文档](https://docs.rs/gpui)
- [gpui-component 文档](https://docs.rs/gpui-component)
- [Zed 编辑器源代码](https://github.com/zed-industries/zed)
- [alacritty_terminal 文档](https://docs.rs/alacritty_terminal)
- [russh 文档](https://docs.rs/russh)
