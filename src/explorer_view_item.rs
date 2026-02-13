use std::path::PathBuf;

/// 文件树中的单个项目
#[derive(Clone, Debug)]
pub struct ExplorerViewItem {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
    pub is_expanded: bool,
    pub selected: bool,
}

impl ExplorerViewItem {
    pub fn new(
        path: PathBuf,
        name: String,
        is_dir: bool,
        depth: usize,
        is_expanded: bool,
    ) -> Self {
        Self {
            path,
            name,
            is_dir,
            depth,
            is_expanded,
            selected: false,
        }
    }
}
