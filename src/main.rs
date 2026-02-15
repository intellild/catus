mod terminal;

use dioxus::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use terminal::TerminalView;

// Tab 类型
#[derive(Clone, PartialEq)]
enum TabKind {
    Shell,
    Sftp,
}

impl TabKind {
    #[allow(dead_code)]
    fn name(&self) -> &'static str {
        match self {
            TabKind::Shell => "Shell",
            TabKind::Sftp => "SFTP",
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            TabKind::Shell => "🖥️",
            TabKind::Sftp => "📁",
        }
    }
}

// Tab 数据结构
#[derive(Clone, PartialEq)]
struct Tab {
    id: usize,
    kind: TabKind,
    title: String,
}

impl Tab {
    fn new_shell(id: usize) -> Self {
        Self {
            id,
            kind: TabKind::Shell,
            title: format!("Shell {}", id + 1),
        }
    }

    fn new_sftp(id: usize) -> Self {
        Self {
            id,
            kind: TabKind::Sftp,
            title: format!("SFTP {}", id + 1),
        }
    }
}

// 全局 Tab ID 计数器
static TAB_ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

fn main() {
    dioxus::launch(app);
}

fn app() -> Element {
    // Tab 状态
    let mut tabs = use_signal(|| vec![Tab::new_shell(0)]);
    let mut active_tab_id = use_signal(|| 0usize);

    // 添加新 Tab
    let mut add_shell_tab = move || {
        let id = TAB_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        tabs.with_mut(|tabs| tabs.push(Tab::new_shell(id)));
        active_tab_id.set(id);
    };

    let mut add_sftp_tab = move || {
        let id = TAB_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        tabs.with_mut(|tabs| tabs.push(Tab::new_sftp(id)));
        active_tab_id.set(id);
    };

    // 关闭 Tab
    let mut close_tab = move |id: usize| {
        tabs.with_mut(|tabs| {
            if let Some(pos) = tabs.iter().position(|t| t.id == id) {
                tabs.remove(pos);
                // 如果关闭的是当前激活的 Tab，切换到前一个
                let current_active = active_tab_id();
                if current_active == id {
                    let new_active = if pos > 0 {
                        tabs.get(pos - 1).map(|t| t.id)
                    } else {
                        tabs.first().map(|t| t.id)
                    };
                    if let Some(new_id) = new_active {
                        active_tab_id.set(new_id);
                    }
                }
            }
        });
    };

    // 激活 Tab
    let mut activate_tab = move |id: usize| {
        active_tab_id.set(id);
    };

    // 获取当前激活的 tab
    let current_tabs = tabs();
    let active_tab = current_tabs
        .iter()
        .find(|t| t.id == active_tab_id())
        .cloned();

    rsx! {
        div {
            class: "app-container",
            style: "display: flex; flex-direction: column; height: 100vh; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;",

            // Tab 栏
            div {
                class: "tab-bar",
                style: "display: flex; background: #1e1e1e; border-bottom: 1px solid #333; overflow-x: auto;",

                // Tab 列表
                {current_tabs.iter().map(|tab| {
                    let tab_id = tab.id;
                    let is_active = active_tab_id() == tab_id;
                    let bg_color = if is_active { "#2d2d2d" } else { "transparent" };
                    let border_bottom = if is_active { "2px solid #007acc" } else { "2px solid transparent" };

                    rsx! {
                        div {
                            key: "{tab.id}",
                            class: "tab",
                            style: "display: flex; align-items: center; padding: 10px 16px; cursor: pointer; background: {bg_color}; border-bottom: {border_bottom}; color: #ccc; font-size: 13px; min-width: 120px; max-width: 200px; user-select: none; transition: background 0.2s;",
                            onclick: move |_| activate_tab(tab_id),

                            // 图标
                            span {
                                style: "margin-right: 6px;",
                                "{tab.kind.icon()}"
                            }

                            // 标题
                            span {
                                style: "flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                "{tab.title}"
                            }

                            // 关闭按钮
                            button {
                                class: "close-btn",
                                style: "margin-left: 8px; background: none; border: none; color: #888; cursor: pointer; font-size: 14px; padding: 0 4px; border-radius: 3px;",
                                onclick: move |e: Event<MouseData>| {
                                    e.stop_propagation();
                                    close_tab(tab_id);
                                },
                                "×"
                            }
                        }
                    }
                })}

                // 添加 Tab 按钮
                div {
                    class: "add-tab-menu",
                    style: "display: flex; align-items: center; padding: 0 8px;",

                    button {
                        class: "add-shell-btn",
                        style: "background: #2d2d2d; border: 1px solid #444; color: #ccc; padding: 4px 10px; margin-right: 8px; cursor: pointer; border-radius: 4px; font-size: 12px;",
                        onclick: move |_| add_shell_tab(),
                        "+ 🖥️ Shell"
                    }

                    button {
                        class: "add-sftp-btn",
                        style: "background: #2d2d2d; border: 1px solid #444; color: #ccc; padding: 4px 10px; cursor: pointer; border-radius: 4px; font-size: 12px;",
                        onclick: move |_| add_sftp_tab(),
                        "+ 📁 SFTP"
                    }
                }
            }

            // Tab 内容区域
            div {
                class: "tab-content",
                style: "flex: 1; background: #1e1e1e; overflow: hidden;",

                {active_tab.map(|tab| {
                    rsx! {
                        TabContent {
                            key: "{tab.id}",
                            tab: tab
                        }
                    }
                })}
            }
        }
    }
}

// Tab 内容组件
#[component]
fn TabContent(tab: Tab) -> Element {
    match tab.kind {
        TabKind::Shell => {
            rsx! {
                TerminalView {}
            }
        }
        TabKind::Sftp => {
            rsx! {
                SftpPlaceholder { tab: tab }
            }
        }
    }
}

// SFTP 占位组件
#[component]
fn SftpPlaceholder(tab: Tab) -> Element {
    rsx! {
        div {
            class: "sftp-placeholder",
            style: "padding: 20px; height: 100%; box-sizing: border-box;",

            div {
                style: "display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; color: #666;",

                div {
                    style: "font-size: 64px; margin-bottom: 20px;",
                    "📁"
                }

                h2 {
                    style: "color: #ccc; margin: 0 0 10px 0; font-weight: normal;",
                    "SFTP"
                }

                p {
                    style: "color: #888; margin: 0;",
                    "Tab ID: {tab.id} | Title: {tab.title}"
                }

                div {
                    style: "margin-top: 30px; padding: 20px; background: #252526; border-radius: 8px; border: 1px solid #333; max-width: 400px; text-align: center;",

                    p {
                        style: "color: #666; font-size: 14px; margin: 0;",
                        "This is a placeholder for the SFTP content."
                    }

                    p {
                        style: "color: #555; font-size: 12px; margin-top: 10px;",
                        "Implementation coming soon..."
                    }
                }
            }
        }
    }
}
