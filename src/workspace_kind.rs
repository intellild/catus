use gpui::SharedString;
use gpui_component::IconName;

use crate::terminal::PtyCommand;

/// A workspace 的类型，决定它启动什么样的终端命令。
///
/// - `Local`：使用系统默认 shell（不传命令给 PTY）。
/// - `LocalProgram`：以本地自定义程序作为 Local workspace 的终端进程。
/// - `Ssh`：以用户提供的命令（通常形如 `ssh user@host`）启动本地 ssh 进程，
///   复用 `LocalPty`，不引入额外依赖。
#[derive(Clone, Debug)]
pub enum WorkspaceKind {
  Local,
  LocalProgram {
    program: String,
    args: Vec<String>,
  },
  Ssh(String),
  /// Explicit tmux control mode command, locally or through ssh.
  Tmux(String),
}

impl WorkspaceKind {
  pub fn local_program(
    program: impl Into<String>,
    args: impl IntoIterator<Item = impl Into<String>>,
  ) -> Self {
    Self::LocalProgram {
      program: program.into(),
      args: args.into_iter().map(Into::into).collect(),
    }
  }

  /// 从启动命令字符串解析 workspace 类型，供 TOML 配置与「添加 Workspace」对话框共用。
  ///
  /// - 空白 → 默认本地 workspace（系统默认 shell，受 `CATUS_LOCAL_PTY_PROGRAM` 覆盖）。
  /// - 首个词是 `ssh` → SSH workspace。
  /// - 其他 → 启动指定本地程序的 workspace。
  pub fn from_command_line(command: &str) -> Self {
    let trimmed = command.trim();
    if trimmed.is_empty() {
      return default_local_kind();
    }
    let mut parts = trimmed.split_whitespace();
    let program = parts.next().expect("non-empty after trim");
    let words: Vec<_> = trimmed.split_whitespace().collect();
    let tmux = words.iter().position(|word| {
      std::path::Path::new(word)
        .file_name()
        .is_some_and(|name| name == "tmux")
    });
    if (program == "ssh" || tmux == Some(0))
      && tmux.is_some_and(|i| {
        words[i + 1..]
          .iter()
          .any(|word| matches!(*word, "-C" | "-CC"))
      })
    {
      // -CC disables PTY echo. Preserve the user's persisted spelling otherwise.
      return Self::Tmux(trimmed.to_string());
    }
    if program == "ssh" {
      Self::Ssh(trimmed.to_string())
    } else {
      Self::local_program(program, parts)
    }
  }

  /// 序列化为启动命令字符串，与 [`WorkspaceKind::from_command_line`] 互逆，
  /// 用于写回 TOML 配置。
  pub fn to_command_line(&self) -> String {
    match self {
      Self::Local => String::new(),
      Self::LocalProgram { program, args } => std::iter::once(program.as_str())
        .chain(args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" "),
      Self::Ssh(cmd) | Self::Tmux(cmd) => cmd.trim().to_string(),
    }
  }

  /// 侧边栏展示用的图标。
  pub fn icon(&self) -> IconName {
    match self {
      WorkspaceKind::Local | WorkspaceKind::LocalProgram { .. } => IconName::SquareTerminal,
      WorkspaceKind::Ssh(_) => IconName::Globe,
      WorkspaceKind::Tmux(_) => IconName::SquareTerminal,
    }
  }

  /// 传给 `LocalPty::new` 的命令：`None` 表示使用系统默认 shell。
  pub fn command(&self) -> Option<&str> {
    match self {
      WorkspaceKind::Local => None,
      WorkspaceKind::LocalProgram { .. } => None,
      WorkspaceKind::Ssh(cmd) | WorkspaceKind::Tmux(cmd) => Some(cmd.as_str()),
    }
  }

  /// 传给 `LocalPty` 的启动命令。
  pub fn pty_command(&self) -> PtyCommand {
    match self {
      WorkspaceKind::Local => PtyCommand::DefaultShell,
      WorkspaceKind::LocalProgram { program, args } => {
        PtyCommand::program(program.clone(), args.clone())
      }
      WorkspaceKind::Ssh(cmd) | WorkspaceKind::Tmux(cmd) => {
        PtyCommand::from_command_line(Some(cmd.as_str()))
      }
    }
  }

  /// 新 pane 在应用尚未发送 OSC 标题时使用的标题。
  pub fn default_terminal_title(&self) -> String {
    self.pty_command().default_title()
  }

  /// 侧边栏展示用的名称。
  pub fn display_name(&self) -> SharedString {
    match self {
      WorkspaceKind::Local => "Local".into(),
      // 自定义程序 workspace 展示完整命令行。
      WorkspaceKind::LocalProgram { .. } => self.to_command_line().into(),
      WorkspaceKind::Tmux(cmd) => cmd.clone().into(),
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

/// 空命令时的默认本地 workspace 类型。
///
/// 默认启动系统默认 shell；`CATUS_LOCAL_PTY_PROGRAM`（参数用
/// `CATUS_LOCAL_PTY_ARGS` 传 JSON 数组）可覆盖为指定程序，供 e2e 测试使用。
fn default_local_kind() -> WorkspaceKind {
  let Ok(program) = std::env::var("CATUS_LOCAL_PTY_PROGRAM") else {
    return WorkspaceKind::Local;
  };

  let program = program.trim().to_string();
  if program.is_empty() {
    return WorkspaceKind::Local;
  }

  let args_json = std::env::var("CATUS_LOCAL_PTY_ARGS").ok();
  let args = parse_local_pty_args(args_json.as_deref());

  WorkspaceKind::local_program(program, args)
}

fn parse_local_pty_args(args_json: Option<&str>) -> Vec<String> {
  args_json
    .and_then(|args| serde_json::from_str(args).ok())
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn explicit_control_commands_round_trip_without_changing_normal_tmux() {
    for command in [
      "tmux -CC new-session -A -s work",
      "tmux -L test -C attach",
      "ssh host tmux -CC attach",
      "/opt/homebrew/bin/tmux -CC",
    ] {
      let kind = WorkspaceKind::from_command_line(command);
      assert!(matches!(kind, WorkspaceKind::Tmux(_)));
      assert_eq!(kind.to_command_line(), command);
    }
    assert!(matches!(
      WorkspaceKind::from_command_line("tmux attach"),
      WorkspaceKind::LocalProgram { .. }
    ));
    assert!(matches!(
      WorkspaceKind::from_command_line("ssh -C host"),
      WorkspaceKind::Ssh(_)
    ));
  }

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
  fn local_program_builds_explicit_pty_command() {
    let kind = WorkspaceKind::local_program("node", ["scripts/echo-pty.js"]);
    assert_eq!(
      kind.pty_command(),
      PtyCommand::program("node", ["scripts/echo-pty.js"])
    );
    assert_eq!(kind.command(), None);
  }

  #[test]
  fn terminal_title_uses_launched_program_basename() {
    let local = WorkspaceKind::local_program("/usr/local/bin/node", ["script.js"]);
    let ssh = WorkspaceKind::Ssh("ssh user@host".to_string());
    assert_eq!(local.default_terminal_title(), "node");
    assert_eq!(ssh.default_terminal_title(), "ssh");
  }

  #[test]
  fn local_icon_is_square_terminal() {
    assert!(matches!(
      WorkspaceKind::Local.icon(),
      IconName::SquareTerminal
    ));
  }

  #[test]
  fn local_program_icon_is_square_terminal() {
    assert!(matches!(
      WorkspaceKind::local_program("node", ["script.js"]).icon(),
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
  fn local_program_display_name_shows_command_line() {
    assert_eq!(
      WorkspaceKind::local_program("node", ["script.js"])
        .display_name()
        .as_ref(),
      "node script.js"
    );
  }

  #[test]
  fn empty_command_yields_local() {
    assert!(matches!(
      WorkspaceKind::from_command_line(""),
      WorkspaceKind::Local
    ));
    assert!(matches!(
      WorkspaceKind::from_command_line("   "),
      WorkspaceKind::Local
    ));
  }

  #[test]
  fn ssh_first_word_maps_to_ssh_kind() {
    match WorkspaceKind::from_command_line("ssh user@host") {
      WorkspaceKind::Ssh(cmd) => assert_eq!(cmd, "ssh user@host"),
      other => panic!("expected Ssh variant, got {:?}", other),
    }
  }

  #[test]
  fn ssh_only_command_maps_to_ssh_kind() {
    assert!(matches!(
      WorkspaceKind::from_command_line("ssh"),
      WorkspaceKind::Ssh(_)
    ));
  }

  #[test]
  fn ssh_prefixed_program_is_not_ssh() {
    // "ssh-keygen" 是普通程序，不应识别为 SSH workspace。
    match WorkspaceKind::from_command_line("ssh-keygen -l key.pub") {
      WorkspaceKind::LocalProgram { program, args } => {
        assert_eq!(program, "ssh-keygen");
        assert_eq!(args, vec!["-l".to_string(), "key.pub".to_string()]);
      }
      other => panic!("expected LocalProgram variant, got {:?}", other),
    }
  }

  #[test]
  fn command_with_args_maps_to_local_program() {
    match WorkspaceKind::from_command_line("  /bin/zsh -l  ") {
      WorkspaceKind::LocalProgram { program, args } => {
        assert_eq!(program, "/bin/zsh");
        assert_eq!(args, vec!["-l".to_string()]);
      }
      other => panic!("expected LocalProgram variant, got {:?}", other),
    }
  }

  #[test]
  fn command_line_round_trips() {
    for command in [
      "",
      "ssh",
      "ssh user@host",
      "/bin/zsh -l",
      "node script.js --flag two words",
    ] {
      assert_eq!(
        WorkspaceKind::from_command_line(command).to_command_line(),
        command,
        "round-trip failed for {command:?}"
      );
    }
  }

  #[test]
  fn local_pty_args_json_preserves_argument_boundaries() {
    let args = parse_local_pty_args(Some(r#"["/path with spaces/echo.js","--label=two words"]"#));
    assert_eq!(
      args,
      vec![
        "/path with spaces/echo.js".to_string(),
        "--label=two words".to_string()
      ]
    );
  }

  #[test]
  fn invalid_local_pty_args_json_falls_back_to_no_arguments() {
    assert!(parse_local_pty_args(Some("not json")).is_empty());
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
