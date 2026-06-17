use gpui::SharedString;
use gpui_component::IconName;

/// A workspace 的类型，决定它启动什么样的终端命令。
///
/// - `Local`：使用系统默认 shell（不传命令给 PTY）。
/// - `Ssh`：以用户提供的命令（通常形如 `ssh user@host`）启动本地 ssh 进程，
///   复用 `LocalPty`，不引入额外依赖。
#[derive(Clone, Debug)]
pub enum WorkspaceKind {
  Local,
  Ssh(String),
}

impl WorkspaceKind {
  /// 侧边栏展示用的图标。
  pub fn icon(&self) -> IconName {
    match self {
      WorkspaceKind::Local => IconName::SquareTerminal,
      WorkspaceKind::Ssh(_) => IconName::Globe,
    }
  }

  /// 传给 `LocalPty::new` 的命令：`None` 表示使用系统默认 shell。
  pub fn command(&self) -> Option<&str> {
    match self {
      WorkspaceKind::Local => None,
      WorkspaceKind::Ssh(cmd) => Some(cmd.as_str()),
    }
  }

  /// 侧边栏展示用的名称。
  pub fn display_name(&self) -> SharedString {
    match self {
      WorkspaceKind::Local => "Local".into(),
      WorkspaceKind::Ssh(cmd) => {
        // 去掉首尾空白后展示命令本身（例如 "ssh user@host"），
        // 若用户只填了 "ssh" 则退化为 "SSH"。
        let trimmed = cmd.trim();
        if trimmed.is_empty() {
          "SSH".into()
        } else {
          trimmed.to_string().into()
        }
      }
    }
  }
}
