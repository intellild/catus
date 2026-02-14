use std::path::PathBuf;

use gpui::prelude::FluentBuilder;
use gpui::prelude::InteractiveElement;
use gpui::*;
use gpui_component::Selectable;
use gpui_component::Sizable;
use gpui_component::button::ButtonVariants;
use gpui_component::{
    ActiveTheme, Icon, IconName,
    button::Button,
    h_flex,
    label::Label,
    tab::{Tab, TabBar},
    v_flex,
};

use crate::explorer_view::ExplorerView;
use crate::tab::{TabId, TabKind, TabManager};
use crate::terminal_view::TerminalView;

/// 工作区 - 管理多个Tab的主视图
pub struct Workspace {
    /// Tab管理器
    tab_manager: TabManager,
    /// 子视图实体（ExplorerView, TerminalView等）
    active_views: hashbrown::HashMap<TabId, ActiveView>,
}

/// 活跃视图枚举
#[derive(Clone)]
enum ActiveView {
    Explorer(Entity<ExplorerView>),
    Terminal(Entity<TerminalView>),
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tab_manager = TabManager::with_defaults();

        // 为默认的Explorer Tab创建视图
        let mut active_views = hashbrown::HashMap::new();

        if let Some(tab) = tab_manager.active_tab() {
            if let TabKind::Explorer { path } = &tab.kind {
                let explorer = cx.new(|cx| ExplorerView::new_with_path(path.clone(), window, cx));
                active_views.insert(tab.id, ActiveView::Explorer(explorer));
            }
        }

        Self {
            tab_manager,
            active_views,
        }
    }

    /// 创建新的Explorer Tab
    pub fn new_explorer_tab(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let id = self
            .tab_manager
            .add_tab(TabKind::Explorer { path: path.clone() });

        let explorer = cx.new(|cx| ExplorerView::new_with_path(path, window, cx));
        self.active_views.insert(id, ActiveView::Explorer(explorer));

        self.tab_manager.activate_tab(id);
        cx.notify();
    }

    /// 创建新的Terminal Tab
    pub fn new_terminal_tab(
        &mut self,
        working_dir: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.tab_manager.add_tab(TabKind::Terminal {
            working_dir: working_dir.clone(),
        });

        let terminal = cx.new(|cx| TerminalView::new(working_dir, window, cx));
        self.active_views.insert(id, ActiveView::Terminal(terminal));

        self.tab_manager.activate_tab(id);
        cx.notify();
    }

    /// 关闭指定Tab
    pub fn close_tab(&mut self, id: TabId, cx: &mut Context<Self>) {
        // 清理视图
        self.active_views.remove(&id);

        // 关闭Tab
        self.tab_manager.close_tab(id);
        cx.notify();
    }

    /// 激活指定Tab
    pub fn activate_tab(&mut self, id: TabId, cx: &mut Context<Self>) {
        self.tab_manager.activate_tab(id);
        cx.notify();
    }

    /// 切换到下一个Tab
    pub fn next_tab(&mut self, cx: &mut Context<Self>) {
        self.tab_manager.next_tab();
        cx.notify();
    }

    /// 切换到上一个Tab
    pub fn prev_tab(&mut self, cx: &mut Context<Self>) {
        self.tab_manager.prev_tab();
        cx.notify();
    }

    /// 获取当前激活的视图
    fn active_view(&self) -> Option<&ActiveView> {
        self.tab_manager
            .active_tab_id()
            .and_then(|id| self.active_views.get(&id))
    }

    /// 渲染Tab栏
    fn render_tab_bar(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs: Vec<_> = self.tab_manager.all_tabs().iter().cloned().collect();
        let active_id = self.tab_manager.active_tab_id();
        let view = cx.entity();

        // 找到当前激活的Tab索引
        let selected_index =
            active_id.and_then(|active_id| tabs.iter().position(|t| t.id == active_id));

        // 为on_click闭包克隆数据
        let tabs_for_click = tabs.clone();
        let view_for_click = view.clone();

        TabBar::new("workspace-tab-bar")
            .h(px(36.0))
            .when_some(selected_index, |this, idx| this.selected_index(idx))
            .on_click(move |index: &usize, _window, cx| {
                let tab_id = tabs_for_click[*index].id;
                view_for_click.update(cx, |this, cx| {
                    this.activate_tab(tab_id, cx);
                });
            })
            .prefix(
                h_flex().h_full().items_center().px_1().child(
                    Button::new("new-tab")
                        .icon(IconName::Plus)
                        .xsmall()
                        .ghost()
                        .on_click(cx.listener(|this, _, _window, cx| {
                            let working_dir =
                                dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                            this.new_terminal_tab(working_dir, _window, cx);
                        })),
                ),
            )
            .suffix(
                div().w_2(), // 右侧留白
            )
            .children(tabs.into_iter().enumerate().map(move |(_index, tab)| {
                let tab_id = tab.id;
                let view_for_close = view.clone();
                let is_active = active_id == Some(tab_id);

                // 根据Tab类型选择图标
                let icon = match &tab.kind {
                    TabKind::Explorer { .. } => IconName::Folder,
                    TabKind::Terminal { .. } => IconName::SquareTerminal,
                    TabKind::SshTerminal { .. } => IconName::CircleUser,
                    TabKind::SftpExplorer { .. } => IconName::Globe,
                };

                Tab::new()
                    .icon(icon)
                    .label(tab.title.clone())
                    .selected(is_active)
                    .suffix(
                        Button::new(("close", tab_id.0))
                            .icon(IconName::Close)
                            .ghost()
                            .xsmall()
                            .on_click(move |_e, _window, cx| {
                                view_for_close.update(cx, |this, cx| {
                                    this.close_tab(tab_id, cx);
                                });
                            }),
                    )
            }))
    }

    /// 渲染工具栏
    fn render_toolbar(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .h(px(40.0))
            .px_3()
            .gap_2()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            // 新建文件管理器按钮
            .child(
                Button::new("new-explorer")
                    .icon(IconName::Folder)
                    .label("Explorer")
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| {
                        let path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                        this.new_explorer_tab(path, window, cx);
                    })),
            )
            // 新建终端按钮
            .child(
                Button::new("new-terminal")
                    .icon(IconName::SquareTerminal)
                    .label("Terminal")
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| {
                        let working_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                        this.new_terminal_tab(working_dir, window, cx);
                    })),
            )
            .child(div().flex_1())
            // 窗口控制提示
            .child(
                h_flex()
                    .gap_3()
                    .text_color(cx.theme().muted_foreground)
                    .child(Label::new("Ctrl+Tab: Next Tab").text_xs())
                    .child(Label::new("Ctrl+W: Close Tab").text_xs()),
            )
    }

    /// 渲染内容区域
    fn render_content(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 确保激活的Tab有视图
        if let Some(tab) = self.tab_manager.active_tab().cloned() {
            if !self.active_views.contains_key(&tab.id) {
                // 在这里创建视图
                match &tab.kind {
                    TabKind::Explorer { path } => {
                        let explorer =
                            cx.new(|cx| ExplorerView::new_with_path(path.clone(), _window, cx));
                        self.active_views
                            .insert(tab.id, ActiveView::Explorer(explorer));
                    }
                    TabKind::Terminal { working_dir } => {
                        let terminal =
                            cx.new(|cx| TerminalView::new(working_dir.clone(), _window, cx));
                        self.active_views
                            .insert(tab.id, ActiveView::Terminal(terminal));
                    }
                    _ => {
                        // 远程连接类型暂未实现
                        return div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                v_flex()
                                    .gap_4()
                                    .items_center()
                                    .child(Icon::new(IconName::SquareTerminal).large())
                                    .child(Label::new("Remote connection not yet implemented")),
                            )
                            .into_any_element();
                    }
                }
            }
        }

        // 渲染当前激活的视图
        if let Some(view) = self.active_view() {
            match view {
                ActiveView::Explorer(entity) => {
                    return div()
                        .flex_1()
                        .overflow_hidden()
                        .child(entity.clone())
                        .into_any_element();
                }
                ActiveView::Terminal(entity) => {
                    return div()
                        .flex_1()
                        .overflow_hidden()
                        .child(entity.clone())
                        .into_any_element();
                }
            }
        }

        // 没有激活的视图
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .child(Label::new("No tab selected"))
            .into_any_element()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(self.render_toolbar(window, cx))
            .child(self.render_tab_bar(window, cx))
            .child(self.render_content(window, cx))
            .key_context("Workspace")
            .on_action(
                cx.listener(|this, _: &workspace::NewFileExplorer, window, cx| {
                    let path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                    this.new_explorer_tab(path, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &workspace::NewTerminal, window, cx| {
                let working_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                this.new_terminal_tab(working_dir, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &workspace::CloseActiveTab, _window, cx| {
                    if let Some(id) = this.tab_manager.active_tab_id() {
                        this.close_tab(id, cx);
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &workspace::NextTab, _window, cx| {
                this.next_tab(cx);
            }))
            .on_action(cx.listener(|this, _: &workspace::PrevTab, _window, cx| {
                this.prev_tab(cx);
            }))
    }
}

/// Workspace相关的Action
pub mod workspace {
    use gpui::actions;

    actions!(
        workspace,
        [
            NewFileExplorer,
            NewTerminal,
            CloseActiveTab,
            NextTab,
            PrevTab
        ]
    );
}

/// 为ExplorerView添加辅助方法
impl ExplorerView {
    /// 从指定路径创建ExplorerView
    pub fn new_with_path(root_path: PathBuf, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        use std::collections::HashMap;

        let mut view = Self {
            file_nodes: HashMap::new(),
            root_path,
            flat_items: Vec::new(),
            scroll_handle: gpui_component::VirtualListScrollHandle::new(),
            selected_index: None,
        };

        view.load_directory(&view.root_path.clone(), cx);
        view
    }
}
