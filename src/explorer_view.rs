use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable, VirtualListScrollHandle, button::Button, h_flex,
    label::Label, v_flex, v_virtual_list,
};

use crate::explorer_view_item::ExplorerViewItem;

/// 文件树查看器视图
pub struct ExplorerView {
    /// 存储每个路径对应的文件节点信息
    file_nodes: HashMap<PathBuf, FileNode>,
    /// 当前根目录
    root_path: PathBuf,
    /// 扁平化的项目列表（用于 VirtualList）
    flat_items: Vec<ExplorerViewItem>,
    /// 滚动句柄
    scroll_handle: VirtualListScrollHandle,
    /// 选中项的索引
    selected_index: Option<usize>,
}

/// 表示文件或目录的元数据
#[derive(Clone, Debug)]
pub struct FileNode {
    #[allow(dead_code)]
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    /// 子节点路径（仅目录）
    pub children: Vec<PathBuf>,
    /// 是否已经加载过子节点
    pub loaded: bool,
    /// 是否展开
    pub expanded: bool,
}

impl FileNode {
    pub fn new(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();
        let is_dir = path.is_dir();
        Self {
            path: path.clone(),
            name,
            is_dir,
            children: Vec::new(),
            loaded: false,
            expanded: false,
        }
    }
}

impl ExplorerView {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        // 获取 home 目录
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

        let mut view = Self {
            file_nodes: HashMap::new(),
            root_path: home_dir.clone(),
            flat_items: Vec::new(),
            scroll_handle: VirtualListScrollHandle::new(),
            selected_index: None,
        };

        // 初始加载 home 目录
        view.load_directory(&home_dir, cx);

        view
    }

    /// 加载指定目录的内容
    fn load_directory(&mut self, path: &PathBuf, cx: &mut Context<Self>) {
        if self.file_nodes.get(path).map(|n| n.loaded).unwrap_or(false) {
            return;
        }

        // 读取目录内容
        let mut entries = Vec::new();
        if let Ok(dir_entries) = std::fs::read_dir(path) {
            for entry in dir_entries.filter_map(|e| e.ok()) {
                let entry_path = entry.path();
                let node = FileNode::new(entry_path.clone());

                // 保存节点信息
                self.file_nodes.insert(entry_path.clone(), node);
                entries.push(entry_path);
            }
        }

        // 排序：目录在前，文件在后，各自按名称排序
        entries.sort_by(|a, b| {
            let a_is_dir = a.is_dir();
            let b_is_dir = b.is_dir();
            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    let a_name = a.file_name().unwrap_or_default();
                    let b_name = b.file_name().unwrap_or_default();
                    a_name.cmp(&b_name)
                }
            }
        });

        // 更新根节点或父节点的子节点列表
        if let Some(node) = self.file_nodes.get_mut(path) {
            node.children = entries;
            node.loaded = true;
        } else {
            // 这是根目录
            let mut root_node = FileNode::new(path.clone());
            root_node.children = entries;
            root_node.loaded = true;
            root_node.expanded = true; // 根目录默认展开
            self.file_nodes.insert(path.clone(), root_node);
        }

        // 重建扁平化列表
        self.rebuild_flat_items();
        cx.notify();
    }

    /// 重建扁平化的项目列表
    fn rebuild_flat_items(&mut self) {
        self.flat_items.clear();
        let root_path = self.root_path.clone();
        self.build_flat_items_recursive(&root_path, 0);
    }

    /// 递归构建扁平化列表
    fn build_flat_items_recursive(&mut self, path: &PathBuf, depth: usize) {
        let node = self.file_nodes.get(path).cloned();

        if let Some(node) = node {
            // 添加当前节点（根目录除外，除非你想显示它）
            if depth > 0 || path != &self.root_path {
                self.flat_items.push(ExplorerViewItem::new(
                    path.clone(),
                    node.name.clone(),
                    node.is_dir,
                    depth,
                    node.expanded,
                ));
            }

            // 如果是展开的目录，递归添加子节点
            if node.expanded {
                for child_path in &node.children {
                    self.build_flat_items_recursive(child_path, depth + 1);
                }
            }
        }
    }

    /// 刷新当前目录
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.file_nodes.clear();
        self.load_directory(&self.root_path.clone(), cx);
    }

    /// 返回 home 目录
    fn go_home(&mut self, cx: &mut Context<Self>) {
        if let Some(home) = dirs::home_dir() {
            self.root_path = home.clone();
            self.file_nodes.clear();
            self.load_directory(&home, cx);
        }
    }

    /// 获取项目大小列表（用于 VirtualList）
    fn get_item_sizes(&self) -> Rc<Vec<Size<Pixels>>> {
        // 假设每个项目高度为 28px
        let sizes: Vec<Size<Pixels>> = self
            .flat_items
            .iter()
            .map(|_| Size::new(px(200.0), px(28.0)))
            .collect();
        Rc::new(sizes)
    }

    /// 获取文件图标
    fn get_file_icon(path: &PathBuf, is_expanded: bool) -> IconName {
        if path.is_dir() {
            if is_expanded {
                IconName::FolderOpen
            } else {
                IconName::Folder
            }
        } else {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            match ext.as_str() {
                "rs" | "js" | "ts" | "jsx" | "tsx" | "py" | "java" | "cpp" | "c" | "h" | "hpp"
                | "go" | "rb" | "php" => IconName::File,
                "json" | "yaml" | "yml" | "toml" => IconName::File,
                "md" | "txt" => IconName::File,
                _ => IconName::File,
            }
        }
    }
}

impl Render for ExplorerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root_path = self.root_path.clone();
        let item_sizes = self.get_item_sizes();
        let items_count = self.flat_items.len();
        let view = cx.entity();

        v_flex()
            .size_full()
            .gap_2()
            .child(
                // 工具栏
                h_flex()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("refresh")
                            .icon(IconName::ArrowUp)
                            .xsmall()
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.refresh(cx);
                            })),
                    )
                    .child(
                        Button::new("home")
                            .icon(IconName::ArrowLeft)
                            .xsmall()
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.go_home(cx);
                            })),
                    )
                    .child(
                        div().flex_1().child(
                            Label::new(root_path.to_string_lossy().to_string())
                                .text_sm()
                                .text_color(cx.theme().muted_foreground),
                        ),
                    ),
            )
            .child(
                // 使用 VirtualList 渲染文件树
                div().flex_1().overflow_hidden().child(
                    v_virtual_list(
                        view.clone(),
                        "explorer-list",
                        item_sizes,
                        move |this, range, _window, cx| {
                            let mut elements: Vec<Stateful<Div>> = Vec::new();

                            for ix in range {
                                if ix >= items_count {
                                    break;
                                }

                                let item = &this.flat_items[ix];
                                let icon = Self::get_file_icon(&item.path, item.is_expanded);
                                let depth = item.depth;
                                let is_selected = item.selected;
                                let path = item.path.clone();
                                let is_dir = item.is_dir;

                                // 使用 view.clone() 来避免移动问题
                                let view_for_click = view.clone();

                                let element = h_flex()
                                    .id(SharedString::from(format!("explorer-item-{}", ix)))
                                    .w_full()
                                    .h(px(28.0))
                                    .py_0()
                                    .px_1()
                                    .pl(px(12.0 + depth as f32 * 16.0))
                                    .gap_2()
                                    .items_center()
                                    .when(is_selected, |this| this.bg(cx.theme().accent))
                                    .when(!is_selected, |this| {
                                        this.hover(|style| style.bg(cx.theme().accent.opacity(0.5)))
                                    })
                                    .child(Icon::new(icon).xsmall())
                                    .child(Label::new(item.name.clone()).text_sm())
                                    .cursor_pointer()
                                    .on_click(move |_e, _window, cx| {
                                        view_for_click.update(cx, |this, cx| {
                                            // 清除之前的选中状态
                                            if let Some(prev_index) = this.selected_index {
                                                if let Some(item) =
                                                    this.flat_items.get_mut(prev_index)
                                                {
                                                    item.selected = false;
                                                }
                                            }

                                            // 设置新的选中状态
                                            if let Some(item) = this.flat_items.get_mut(ix) {
                                                item.selected = true;
                                                this.selected_index = Some(ix);

                                                // 如果是目录，切换展开状态
                                                if is_dir {
                                                    let path = path.clone();
                                                    if let Some(node) =
                                                        this.file_nodes.get_mut(&path)
                                                    {
                                                        node.expanded = !node.expanded;

                                                        // 如果是展开且未加载，则加载子节点
                                                        if node.expanded && !node.loaded {
                                                            this.load_directory(&path, cx);
                                                            return;
                                                        }
                                                    }
                                                    // 重建扁平化列表
                                                    this.rebuild_flat_items();
                                                }
                                            }

                                            cx.notify();
                                        });
                                        cx.stop_propagation();
                                    });

                                elements.push(element);
                            }

                            elements
                        },
                    )
                    .track_scroll(&self.scroll_handle),
                ),
            )
    }
}
