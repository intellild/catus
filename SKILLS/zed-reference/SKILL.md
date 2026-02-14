# Zed 源码参考指南

本技能提供 Zed 编辑器源码中 GPUI 框架使用模式的参考，供 Catus 项目开发时借鉴。

## 概述

Zed 是 GPUI 框架的创造者，其源码是学习和参考 GPUI 最佳实践的权威来源。Catus 项目位于 `/Volumes/code/catus`，Zed 源码位于 `/Volumes/code/zed`。

## 相关 Crate 目录

| Crate | 路径 | 说明 |
|-------|------|------|
| `gpui` | `crates/gpui/src/` | GPUI 框架核心 |
| `project_panel` | `crates/project_panel/src/` | 文件浏览器面板实现 |
| `file_icons` | `crates/file_icons/src/` | 文件图标处理 |
| `ui` | `crates/ui/src/` | UI 组件库 |

## GPUI 核心使用模式

### 1. Render Trait 实现

```rust
// 标准模式：返回 impl IntoElement
impl Render for MyView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().child("Hello")
    }
}
```

参考文件：`/Volumes/code/zed/crates/project_panel/src/project_panel.rs:6190`

### 2. 视图结构定义

```rust
pub struct ProjectPanel {
    project: Entity<Project>,
    focus_handle: FocusHandle,
    scroll_handle: UniformListScrollHandle,
    fs: Arc<dyn Fs>,
    // ... 其他状态字段
}
```

### 3. 列表渲染（uniform_list）

```rust
// 使用 uniform_list 渲染大量条目
uniform_list("entries", item_count, {
    cx.processor(|this, range: Range<usize>, window, cx| {
        let mut items = Vec::with_capacity(range.end - range.start);
        // 根据 range 渲染可见条目
        this.for_each_visible_entry(range, window, cx, |id, details, window, cx| {
            items.push(this.render_entry(id, details, window, cx));
        });
        items
    })
})
```

参考文件：`/Volumes/code/zed/crates/project_panel/src/project_panel.rs:6359`

### 4. ListItem 组件使用

```rust
ListItem::new(id)
    .indent_level(depth)
    .indent_step_size(px(settings.indent_size))
    .spacing(match settings.entry_spacing {
        ProjectPanelEntrySpacing::Comfortable => ListItemSpacing::Dense,
        ProjectPanelEntrySpacing::Standard => ListItemSpacing::ExtraDense,
    })
    .selectable(false)
    .when_some(canonical_path, |this, path| {
        this.end_slot::<AnyElement>(/* ... */)
    })
    .child(/* 内容 */)
```

参考文件：`/Volumes/code/zed/crates/project_panel/src/project_panel.rs:5464`

### 5. Action 定义

```rust
actions!(
    project_panel,
    [
        /// 展开选中条目
        ExpandSelectedEntry,
        /// 折叠选中条目
        CollapseSelectedEntry,
        /// 新建目录
        NewDirectory,
        /// 新建文件
        NewFile,
        /// 复制
        Copy,
        /// 粘贴
        Paste,
        /// 重命名
        Rename,
        /// 删除
        Delete,
        // ... 更多 action
    ]
);
```

参考文件：`/Volumes/code/zed/crates/project_panel/src/project_panel.rs:296`

### 6. EventEmitter 和 Focusable 实现

```rust
impl EventEmitter<Event> for ProjectPanel {}
impl EventEmitter<PanelEvent> for ProjectPanel {}

impl Focusable for ProjectPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
```

参考文件：`/Volumes/code/zed/crates/project_panel/src/project_panel.rs:6841`

### 7. 文件图标获取

```rust
// 使用 FileIcons 获取文件图标
use file_icons::FileIcons;

// 获取文件图标
let icon = FileIcons::get_icon(Path::new(&filename), cx);

// 获取文件夹图标
let folder_icon = FileIcons::get_folder_icon(is_expanded, entry.path.as_std_path(), cx);

// 获取折叠展开图标
let chevron_icon = FileIcons::get_chevron_icon(is_expanded, cx);
```

参考文件：`/Volumes/code/zed/crates/project_panel/src/project_panel.rs:5028`

### 8. 事件处理

```rust
.on_click(cx.listener(move |project_panel, event: &ClickEvent, window, cx| {
    if event.button == MouseButton::Right {
        // 右键菜单
        project_panel.deploy_context_menu(entry_id, event.up.clone(), window, cx);
    } else if event.click_count == 2 {
        // 双击打开
        project_panel.open_entry(entry_id, true, false, cx);
    }
}))
```

### 9. 拖拽支持

```rust
.on_drag_move(cx.listener(handle_drag_move::<ExternalPaths>))
```

### 10. 状态更新与通知

```rust
// 更新状态并触发重绘
view.update(cx, |this, cx| {
    this.some_state = new_value;
    cx.notify();
});
```

## 常用 GPUI 类型对照

| 类型 | 用途 |
|------|------|
| `Entity<T>` | GPUI 实体类型，相当于智能指针 |
| `Window` | 窗口上下文 |
| `Context<Self>` | 组件上下文 |
| `App` | 应用上下文 |
| `FocusHandle` | 焦点控制句柄 |
| `UniformListScrollHandle` | 列表滚动控制 |
| `SharedString` | 共享字符串类型 |
| `Pixels` / `px()` | 像素尺寸 |

## 参考路径速查

```bash
# GPUI 核心
/Volumes/code/zed/crates/gpui/src/

# 项目面板（文件浏览器）
/Volumes/code/zed/crates/project_panel/src/project_panel.rs

# 文件图标
/Volumes/code/zed/crates/file_icons/src/file_icons.rs

# UI 组件
/Volumes/code/zed/crates/ui/src/
```

## 使用建议

1. **阅读源码时**：重点关注 `project_panel` crate，它实现了完整的文件树浏览功能
2. **遇到 API 疑问时**：查看 `gpui` crate 的源码，了解底层实现
3. **需要复杂交互时**：参考 Zed 中事件处理、拖拽、右键菜单的实现方式
4. **保持简洁**：Zed 功能复杂，Catus 只需参考其核心模式，不必完全照搬
