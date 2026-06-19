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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn local_command_is_none() {
    assert_eq!(WorkspaceKind::Local.command(), None);
  }

  #[test]
  fn ssh_command_returns_provided_string() {
    let kind = WorkspaceKind::Ssh("ssh user@host".to_string());
    assert_eq!(kind.command(), Some("ssh user@host"));
  }

  #[test]
  fn local_icon_is_square_terminal() {
    assert!(matches!(
      WorkspaceKind::Local.icon(),
      IconName::SquareTerminal
    ));
  }

  #[test]
  fn ssh_icon_is_globe() {
    let kind = WorkspaceKind::Ssh("ssh user@host".to_string());
    assert!(matches!(kind.icon(), IconName::Globe));
  }

  #[test]
  fn local_display_name_is_local() {
    assert_eq!(WorkspaceKind::Local.display_name().as_ref(), "Local");
  }

  #[test]
  fn ssh_display_name_shows_command() {
    let kind = WorkspaceKind::Ssh("ssh user@host".to_string());
    assert_eq!(kind.display_name().as_ref(), "ssh user@host");
  }

  #[test]
  fn ssh_display_name_trims_surrounding_whitespace() {
    let kind = WorkspaceKind::Ssh("  ssh user@host  ".to_string());
    assert_eq!(kind.display_name().as_ref(), "ssh user@host");
  }

  #[test]
  fn ssh_display_name_falls_back_to_ssh_when_empty() {
    let kind = WorkspaceKind::Ssh("   ".to_string());
    assert_eq!(kind.display_name().as_ref(), "SSH");
  }
}
