use gpui::SharedString;
use gpui_component::IconName;
use serde::{Deserialize, Serialize};

use crate::terminal::PtyCommand;

/// Workspace backend, independent of its startup command.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceMode {
  #[default]
  Regular,
  Tmux,
}

/// Every workspace has a backend and a command. SSH is simply a command, not
/// a separate backend. Structured commands preserve argument boundaries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSpec {
  pub mode: WorkspaceMode,
  pub command: PtyCommand,
}

impl Default for WorkspaceSpec {
  fn default() -> Self {
    Self {
      mode: WorkspaceMode::Regular,
      command: PtyCommand::DefaultShell,
    }
  }
}

impl WorkspaceSpec {
  pub fn new(mode: WorkspaceMode, command: &str) -> Self {
    let command = if command.trim().is_empty() && mode == WorkspaceMode::Regular {
      default_local_command()
    } else {
      PtyCommand::from_command_line(Some(command))
    };
    Self { mode, command }
  }

  #[cfg(test)]
  pub fn local_program(
    program: impl Into<String>,
    args: impl IntoIterator<Item = impl Into<String>>,
  ) -> Self {
    Self {
      mode: WorkspaceMode::Regular,
      command: PtyCommand::program(program, args),
    }
  }

  pub fn to_command_line(&self) -> String {
    match &self.command {
      PtyCommand::DefaultShell => String::new(),
      PtyCommand::Program { program, args } => std::iter::once(program.as_str())
        .chain(args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" "),
    }
  }

  pub fn icon(&self) -> IconName {
    match &self.command {
      PtyCommand::Program { program, .. }
        if std::path::Path::new(program)
          .file_name()
          .is_some_and(|name| name == "ssh") =>
      {
        IconName::Globe
      }
      _ => IconName::SquareTerminal,
    }
  }

  pub fn default_terminal_title(&self) -> String {
    self.command.default_title()
  }

  pub fn display_name(&self) -> SharedString {
    match &self.command {
      PtyCommand::DefaultShell => "Local".into(),
      _ => self.to_command_line().into(),
    }
  }
}

/// Test/e2e overrides only affect an empty regular workspace command.
fn default_local_command() -> PtyCommand {
  let Ok(program) = std::env::var("CATUS_LOCAL_PTY_PROGRAM") else {
    return PtyCommand::DefaultShell;
  };
  let program = program.trim();
  if program.is_empty() {
    return PtyCommand::DefaultShell;
  }
  let args_json = std::env::var("CATUS_LOCAL_PTY_ARGS").ok();
  let args = parse_local_pty_args(args_json.as_deref());
  PtyCommand::program(program, args)
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
  fn backend_is_independent_of_command() {
    for command in [
      "tmux -CC new-session",
      "ssh host tmux -CC attach",
      "/usr/local/bin/my-tmux-wrapper",
      "ssh -C host",
    ] {
      let regular = WorkspaceSpec::new(WorkspaceMode::Regular, command);
      let tmux = WorkspaceSpec::new(WorkspaceMode::Tmux, command);
      assert_eq!(regular.mode, WorkspaceMode::Regular);
      assert_eq!(tmux.mode, WorkspaceMode::Tmux);
      assert_eq!(regular.command, tmux.command);
      assert_eq!(tmux.to_command_line(), command);
    }
  }

  #[test]
  fn default_workspace_uses_default_shell() {
    let spec = WorkspaceSpec::default();
    assert_eq!(spec.mode, WorkspaceMode::Regular);
    assert_eq!(spec.command, PtyCommand::DefaultShell);
    assert_eq!(spec.display_name().as_ref(), "Local");
  }

  #[test]
  fn explicit_program_preserves_argument_boundaries() {
    let spec = WorkspaceSpec::local_program(
      "/usr/local/bin/node",
      ["/path with spaces/script.js", "--label=two words"],
    );
    assert_eq!(
      spec.command,
      PtyCommand::program(
        "/usr/local/bin/node",
        ["/path with spaces/script.js", "--label=two words"]
      )
    );
    assert_eq!(spec.default_terminal_title(), "node");
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
        WorkspaceSpec::new(WorkspaceMode::Regular, command).to_command_line(),
        command
      );
    }
  }

  #[test]
  fn ssh_command_controls_icon_and_display_for_either_backend() {
    for mode in [WorkspaceMode::Regular, WorkspaceMode::Tmux] {
      let spec = WorkspaceSpec::new(mode, "  ssh user@host  ");
      assert!(matches!(spec.icon(), IconName::Globe));
      assert_eq!(spec.display_name().as_ref(), "ssh user@host");
      assert_eq!(spec.default_terminal_title(), "ssh");
    }
    assert!(matches!(
      WorkspaceSpec::new(WorkspaceMode::Regular, "ssh-keygen").icon(),
      IconName::SquareTerminal
    ));
  }

  #[test]
  fn local_pty_args_json_preserves_argument_boundaries() {
    assert_eq!(
      parse_local_pty_args(Some(r#"["/path with spaces/echo.js","--label=two words"]"#)),
      vec!["/path with spaces/echo.js", "--label=two words"]
    );
    assert!(parse_local_pty_args(Some("not json")).is_empty());
  }
}
