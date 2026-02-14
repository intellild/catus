use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use gpui::*;

/// 全局Tab ID生成器
static TAB_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// 生成新的唯一Tab ID
pub fn generate_tab_id() -> TabId {
    TabId(TAB_ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Tab唯一标识符
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

impl std::fmt::Display for TabId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Tab类型
#[derive(Clone, Debug, PartialEq)]
pub enum TabKind {
    /// 文件资源管理器
    Explorer { path: PathBuf },
    /// 终端（本地）
    Terminal { working_dir: PathBuf },
    /// SSH远程终端（预留接口）
    SshTerminal {
        host: String,
        port: u16,
        username: String,
        working_dir: PathBuf,
    },
    /// SFTP文件管理器（预留接口）
    SftpExplorer {
        host: String,
        port: u16,
        username: String,
        remote_path: PathBuf,
    },
}

impl TabKind {
    /// 获取Tab的默认标题
    pub fn default_title(&self) -> String {
        match self {
            TabKind::Explorer { path } => {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Explorer");
                format!("📁 {}", name)
            }
            TabKind::Terminal { working_dir } => {
                let name = working_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Terminal");
                format!("⌨️ {}", name)
            }
            TabKind::SshTerminal { host, username, .. } => {
                format!("🔌 {}@{}:SSH", username, host)
            }
            TabKind::SftpExplorer { host, username, .. } => {
                format!("🌐 {}@{}:SFTP", username, host)
            }
        }
    }

    /// 获取Tab的图标标识
    pub fn icon_name(&self) -> &'static str {
        match self {
            TabKind::Explorer { .. } => "folder",
            TabKind::Terminal { .. } => "terminal",
            TabKind::SshTerminal { .. } => "ssh",
            TabKind::SftpExplorer { .. } => "sftp",
        }
    }

    /// 是否为远程连接
    pub fn is_remote(&self) -> bool {
        matches!(
            self,
            TabKind::SshTerminal { .. } | TabKind::SftpExplorer { .. }
        )
    }
}

/// Tab状态
#[derive(Clone, Debug)]
pub struct Tab {
    pub id: TabId,
    pub kind: TabKind,
    pub title: String,
    pub is_active: bool,
    pub is_modified: bool,
    pub is_loading: bool,
}

impl Tab {
    pub fn new(kind: TabKind) -> Self {
        let title = kind.default_title();
        Self {
            id: generate_tab_id(),
            kind,
            title,
            is_active: false,
            is_modified: false,
            is_loading: false,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn set_active(&mut self, active: bool) {
        self.is_active = active;
    }

    pub fn set_modified(&mut self, modified: bool) {
        self.is_modified = modified;
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.is_loading = loading;
    }
}

/// Tab管理器 - 管理所有Tab的状态
pub struct TabManager {
    tabs: Vec<Tab>,
    active_tab_id: Option<TabId>,
}

impl TabManager {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active_tab_id: None,
        }
    }

    /// 创建默认的Tab配置
    pub fn with_defaults() -> Self {
        let mut manager = Self::new();
        // 添加一个默认的文件管理器Tab
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        manager.add_tab(TabKind::Explorer { path: home_dir });
        manager
    }

    /// 添加新Tab
    pub fn add_tab(&mut self, kind: TabKind) -> TabId {
        let tab = Tab::new(kind);
        let id = tab.id;

        // 如果这是第一个Tab，自动激活
        if self.tabs.is_empty() {
            self.active_tab_id = Some(id);
        }

        self.tabs.push(tab);
        id
    }

    /// 关闭指定Tab
    pub fn close_tab(&mut self, id: TabId) -> bool {
        let index = self.tabs.iter().position(|t| t.id == id);

        if let Some(index) = index {
            self.tabs.remove(index);

            // 如果关闭的是当前激活的Tab，切换到相邻的Tab
            if self.active_tab_id == Some(id) {
                if self.tabs.is_empty() {
                    self.active_tab_id = None;
                } else {
                    // 优先选择右侧，如果没有则选择左侧
                    let new_index = index.min(self.tabs.len() - 1);
                    self.active_tab_id = Some(self.tabs[new_index].id);
                }
            }
            true
        } else {
            false
        }
    }

    /// 激活指定Tab
    pub fn activate_tab(&mut self, id: TabId) -> bool {
        if self.tabs.iter().any(|t| t.id == id) {
            // 取消之前的激活状态
            if let Some(active_id) = self.active_tab_id {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == active_id) {
                    tab.set_active(false);
                }
            }

            // 设置新的激活状态
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
                tab.set_active(true);
            }

            self.active_tab_id = Some(id);
            true
        } else {
            false
        }
    }

    /// 切换到下一个Tab
    pub fn next_tab(&mut self) {
        if let Some(current_id) = self.active_tab_id {
            if let Some(current_index) = self.tabs.iter().position(|t| t.id == current_id) {
                let next_index = (current_index + 1) % self.tabs.len();
                self.activate_tab(self.tabs[next_index].id);
            }
        }
    }

    /// 切换到上一个Tab
    pub fn prev_tab(&mut self) {
        if let Some(current_id) = self.active_tab_id {
            if let Some(current_index) = self.tabs.iter().position(|t| t.id == current_id) {
                let prev_index = if current_index == 0 {
                    self.tabs.len() - 1
                } else {
                    current_index - 1
                };
                self.activate_tab(self.tabs[prev_index].id);
            }
        }
    }

    /// 获取当前激活的Tab
    pub fn active_tab(&self) -> Option<&Tab> {
        self.active_tab_id
            .and_then(|id| self.tabs.iter().find(|t| t.id == id))
    }

    /// 获取当前激活的Tab（可变）
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.active_tab_id
            .and_then(|id| self.tabs.iter_mut().find(|t| t.id == id))
    }

    /// 获取指定Tab
    pub fn get_tab(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    /// 获取所有Tab
    pub fn all_tabs(&self) -> &[Tab] {
        &self.tabs
    }

    /// 获取所有Tab（可变）
    pub fn all_tabs_mut(&mut self) -> &mut Vec<Tab> {
        &mut self.tabs
    }

    /// Tab数量
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// 获取激活的Tab ID
    pub fn active_tab_id(&self) -> Option<TabId> {
        self.active_tab_id
    }

    /// 更新Tab标题
    pub fn update_tab_title(&mut self, id: TabId, title: impl Into<String>) -> bool {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
            tab.title = title.into();
            true
        } else {
            false
        }
    }
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_id_generation() {
        let id1 = generate_tab_id();
        let id2 = generate_tab_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_tab_kind_default_title() {
        let kind = TabKind::Explorer {
            path: PathBuf::from("/home/user/projects"),
        };
        assert!(kind.default_title().contains("projects"));

        let kind = TabKind::Terminal {
            working_dir: PathBuf::from("/home/user"),
        };
        assert!(kind.default_title().contains("user"));

        let kind = TabKind::SshTerminal {
            host: "example.com".to_string(),
            port: 22,
            username: "admin".to_string(),
            working_dir: PathBuf::from("/home/admin"),
        };
        let title = kind.default_title();
        assert!(title.contains("example.com"));
        assert!(title.contains("admin"));
    }

    #[test]
    fn test_tab_manager_basic() {
        let mut manager = TabManager::new();
        assert_eq!(manager.tab_count(), 0);

        let id1 = manager.add_tab(TabKind::Explorer {
            path: PathBuf::from("/tmp"),
        });
        assert_eq!(manager.tab_count(), 1);
        assert_eq!(manager.active_tab_id(), Some(id1));

        let id2 = manager.add_tab(TabKind::Terminal {
            working_dir: PathBuf::from("/home"),
        });
        assert_eq!(manager.tab_count(), 2);

        manager.activate_tab(id2);
        assert_eq!(manager.active_tab_id(), Some(id2));

        manager.close_tab(id1);
        assert_eq!(manager.tab_count(), 1);
        assert_eq!(manager.active_tab_id(), Some(id2));
    }
}
