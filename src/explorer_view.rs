use std::collections::HashMap;
use std::path::PathBuf;

use gpui::*;
use gpui_component::{
    button::Button,
    h_flex,
    label::Label,
    list::ListItem,
    scroll::ScrollableElement,
    v_flex,
    ActiveTheme, Icon, IconName, Sizable,
    tree::{TreeItem, TreeState, tree},
};

/// 文件树查看器视图
pub struct ExplorerView {
    tree_state: Entity<TreeState>,
    /// 存储每个路径对应的文件节点信息
    file_nodes: HashMap<PathBuf, FileNode>,
    /// 当前根目录
    root_path: PathBuf,
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

        // 创建树状态
        let tree_state = cx.new(|cx| TreeState::new(cx));

        let mut view = Self {
            tree_state,
            file_nodes: HashMap::new(),
            root_path: home_dir.clone(),
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
                    a_name.cmp(b_name)
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

        // 更新树状态
        self.rebuild_tree(cx);
    }

    /// 切换目录的展开/折叠状态
    fn toggle_expanded(&mut self, path: &PathBuf, cx: &mut Context<Self>) {
        if let Some(node) = self.file_nodes.get_mut(path) {
            if node.is_dir {
                node.expanded = !node.expanded;
                
                // 如果是展开且未加载，则加载子节点
                if node.expanded && !node.loaded {
                    self.load_directory(path, cx);
                } else {
                    // 只需重建树
                    self.rebuild_tree(cx);
                }
            }
        }
    }

    /// 根据 file_nodes 重建 TreeState
    fn rebuild_tree(&mut self, cx: &mut Context<Self>) {
        let root_path = self.root_path.clone();
        let tree_items = self.build_tree_items(&root_path);
        
        self.tree_state.update(cx, |state, cx| {
            state.set_items(tree_items, cx);
        });
    }

    /// 递归构建 TreeItem
    fn build_tree_items(&self, path: &PathBuf) -> Vec<TreeItem> {
        let mut items = Vec::new();
        
        if let Some(node) = self.file_nodes.get(path) {
            for child_path in &node.children {
                if let Some(child_node) = self.file_nodes.get(child_path) {
                    let mut item = TreeItem::new(
                        child_path.to_string_lossy().to_string(),
                        child_node.name.clone(),
                    );

                    if child_node.is_dir {
                        // 如果是展开的目录，递归构建子节点
                        if child_node.expanded {
                            let children = self.build_tree_items(child_path);
                            item = item.children(children).expanded(true);
                        } else {
                            // 未展开的目录，添加空 children 以显示展开箭头
                            item = item.children(vec![]);
                        }
                    }

                    items.push(item);
                }
            }
        }
        
        items
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

    /// 根据文件类型返回对应的图标
    fn get_file_icon(path: &PathBuf) -> IconName {
        if path.is_dir() {
            return IconName::Folder;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            "rs" | "js" | "ts" | "jsx" | "tsx" | "py" | "java" | "cpp" | "c" | "h" | "hpp" | "go" | "rb" | "php" => IconName::File,
            "json" | "yaml" | "yml" | "toml" => IconName::File,
            "md" | "txt" => IconName::File,
            _ => IconName::File,
        }
    }
}

impl Render for ExplorerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root_path = self.root_path.clone();
        let tree_state = self.tree_state.clone();

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
                        div()
                            .flex_1()
                            .child(
                                Label::new(
                                    root_path.to_string_lossy().to_string(),
                                )
                                .text_sm()
                                .text_color(cx.theme().muted_foreground),
                            ),
                    ),
            )
            .child(
                // 文件树
                div()
                    .flex_1()
                    .overflow_y_scrollbar()
                    .child(
                        tree(&tree_state, {
                            let view = cx.entity();
                            move |ix, entry, selected, _window, _cx| {
                                let path = PathBuf::from(entry.item().id.as_ref());
                                let is_dir = entry.is_folder();
                                let name = entry.item().label.to_string();
                                let depth = entry.depth();
                                let is_expanded = entry.is_expanded();

                                // 选择图标
                                let icon = if is_dir {
                                    if is_expanded {
                                        IconName::FolderOpen
                                    } else {
                                        IconName::Folder
                                    }
                                } else {
                                    Self::get_file_icon(&path)
                                };

                                ListItem::new(ix)
                                    .selected(selected)
                                    .py_0()
                                    .px_1()
                                    .pl(px(12. + depth as f32 * 16.))
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .items_center()
                                            .child(Icon::new(icon).xsmall())
                                            .child(
                                                Label::new(name).text_sm(),
                                            ),
                                    )
                                    .on_click({
                                        let view = view.clone();
                                        let path = path.clone();
                                        move |_e, _window, cx| {
                                            view.update(cx, |this, cx| {
                                                this.toggle_expanded(&path, cx);
                                            });
                                            cx.stop_propagation();
                                        }
                                    })
                            }
                        }),
                    ),
            )
    }
}
