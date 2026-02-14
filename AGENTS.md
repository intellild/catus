# Catus - 文件资源管理器

## 项目简介

Catus 是一个基于 Rust 和 GPUI 框架开发的文件资源管理器桌面应用，提供直观的文件浏览和操作界面。

## 技术栈

- **语言**: Rust (Edition 2024)
- **UI 框架**: GPUI 0.2.2 (Zed 编辑器同款框架)
- **组件库**: gpui-component 0.5.1
- **系统目录**: dirs 6
- **异步/错误处理**: anyhow

## 项目结构

```
src/
├── main.rs              # 应用入口，窗口初始化
├── explorer_view.rs     # 文件浏览器主视图
└── explorer_view_item.rs # 文件项数据结构
```

## 编码规范

### 1. Render  trait 实现

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
```

## 参考实现

本项目参考 Zed 编辑器源码中的 GPUI 使用模式。Zed 源码位于 `/Volumes/code/zed`。

### 如何参考 Zed 源码

1. **项目面板实现**：参考 `crates/project_panel/src/project_panel.rs` 了解文件浏览器的完整实现
2. **文件图标处理**：参考 `crates/file_icons/src/file_icons.rs` 了解图标获取逻辑
3. **GPUI 核心 API**：参考 `crates/gpui/src/` 了解框架底层实现

### 参考原则

- **借鉴模式**：学习 Zed 中 GPUI 组件的组织方式、事件处理、状态管理
- **保持简洁**：Zed 功能复杂，本项目只取所需，不必完全照搬
- **遵循风格**：代码风格保持与现有 Catus 代码一致

详细参考指南见 `SKILLS/zed-reference/SKILL.md`。

## 依赖更新注意事项

GPUI 和 gpui-component 的 API 可能随版本变化，升级后建议：

1. 运行 `cargo doc` 查看最新 API
2. 检查 `Theme`、`Window` 等核心类型的变更
3. 验证主题设置方式是否有变化
