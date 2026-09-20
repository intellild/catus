use crate::terminal::Pty;
use crate::terminal::pty::TerminalSize;
use crate::terminal::title::{DEFAULT_TERMINAL_TITLE, normalize_title};
use anyhow::{Context, Result};
use async_channel::{Receiver, Sender, unbounded};
use async_trait::async_trait;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::thread;
use tracing::{debug, info, warn};

/// 写入命令枚举
enum WriteCommand {
  Write(Vec<u8>),
  Resize(PtySize),
}

/// Command launched inside a local PTY.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PtyCommand {
  /// Launch the user's default shell.
  DefaultShell,
  /// Launch a concrete program with explicit arguments.
  Program { program: String, args: Vec<String> },
}

impl PtyCommand {
  pub fn program(
    program: impl Into<String>,
    args: impl IntoIterator<Item = impl Into<String>>,
  ) -> Self {
    Self::Program {
      program: program.into(),
      args: args.into_iter().map(Into::into).collect(),
    }
  }

  pub fn from_command_line(command: Option<&str>) -> Self {
    if let Some(cmd_str) = command {
      let trimmed = cmd_str.trim();
      if !trimmed.is_empty() {
        let mut parts = trimmed.split_whitespace();
        let program = parts.next().expect("non-empty after trim").to_string();
        return Self::Program {
          program,
          args: parts.map(ToString::to_string).collect(),
        };
      }
    }

    Self::DefaultShell
  }

  /// kitty/WezTerm 风格的初始标题：使用实际启动程序的 basename。
  /// 应用后续发送的 OSC 标题会覆盖它，标题重置后则回退到这里。
  pub fn default_title(&self) -> String {
    let program = match self {
      Self::DefaultShell => default_shell_program(),
      Self::Program { program, .. } => program.clone(),
    };
    executable_name(&program)
  }
}

/// 本地 PTY 实现
///
/// 使用独立 reader/writer 线程处理阻塞 I/O，通过 `async_channel` 与
/// UI/任务侧通信。子进程在 `Drop` 时被同步 kill。
pub struct LocalPty {
  child: Box<dyn Child + Send + Sync>,
  reader_rx: Receiver<Vec<u8>>,
  writer_tx: Sender<WriteCommand>,
}

impl LocalPty {
  /// 创建本地 PTY
  ///
  /// # Arguments
  /// * `size` - 终端尺寸
  /// * `command` - 可选的命令字符串。`None` 启动系统默认 shell；
  ///   `Some("ssh user@host")` 等会被按空白拆分为程序 + 参数。
  pub fn new(size: TerminalSize, command: Option<&str>) -> Result<Self> {
    Self::new_with_command(size, PtyCommand::from_command_line(command))
  }

  /// 使用显式命令创建本地 PTY。
  pub fn new_with_command(size: TerminalSize, command: PtyCommand) -> Result<Self> {
    let pty_system = portable_pty::native_pty_system();

    let pty_size = PtySize {
      rows: size.rows,
      cols: size.cols,
      pixel_width: size.pixel_width,
      pixel_height: size.pixel_height,
    };

    let pty_pair = pty_system
      .openpty(pty_size)
      .with_context(|| "Failed to open PTY")?;

    // 构造要执行的命令
    let cmd = build_command(&command);
    debug!(target: "catus", "spawning PTY command: {:?}", cmd.get_argv());

    let child = pty_pair
      .slave
      .spawn_command(cmd)
      .with_context(|| "Failed to spawn command in PTY")?;

    // 丢弃 slave 句柄，确保子进程退出后 master 能收到 EOF
    drop(pty_pair.slave);

    let master = pty_pair.master;

    let reader = master
      .try_clone_reader()
      .with_context(|| "Failed to get PTY reader")?;

    // 先 take_writer，失败时可以在 drop 中 kill 子进程
    let writer = master
      .take_writer()
      .with_context(|| "Failed to get PTY writer")?;

    let (reader_tx, reader_rx) = unbounded::<Vec<u8>>();
    let (writer_tx, writer_rx) = unbounded::<WriteCommand>();

    run_reader(reader, reader_tx);
    run_writer(master, writer, writer_rx);

    info!(target: "catus", "local PTY created");

    Ok(Self {
      child,
      reader_rx,
      writer_tx,
    })
  }
}

/// 构造 portable-pty 命令。
///
/// `portable_pty::CommandBuilder::new` 只接受单个程序路径，不接受
/// shell 命令行，因此 `"ssh user@host"` 必须拆分为 `ssh` + `user@host`。
fn build_command(command: &PtyCommand) -> CommandBuilder {
  match command {
    PtyCommand::Program { program, args } => {
      let mut cmd = CommandBuilder::new(program);
      for arg in args {
        cmd.arg(arg);
      }
      cmd
    }
    PtyCommand::DefaultShell => {
      // 系统默认 shell
      CommandBuilder::new(default_shell_program())
    }
  }
}

/// 当前用户默认 shell 的程序路径（`$SHELL`，回退 `/bin/sh`）。
pub(crate) fn default_shell_program() -> String {
  #[cfg(target_os = "windows")]
  {
    "cmd.exe".to_string()
  }
  #[cfg(not(target_os = "windows"))]
  {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
  }
}

fn executable_name(program: &str) -> String {
  program
    .rsplit(['/', '\\'])
    .find(|part| !part.is_empty())
    .and_then(normalize_title)
    .unwrap_or_else(|| DEFAULT_TERMINAL_TITLE.to_string())
}

#[async_trait]
impl Pty for LocalPty {
  async fn write(&self, data: Vec<u8>) -> Result<()> {
    self.writer_tx.send(WriteCommand::Write(data)).await?;
    Ok(())
  }

  async fn resize(&self, size: TerminalSize) -> Result<()> {
    let pty_size = PtySize {
      rows: size.rows,
      cols: size.cols,
      pixel_width: size.pixel_width,
      pixel_height: size.pixel_height,
    };
    self.writer_tx.send(WriteCommand::Resize(pty_size)).await?;
    Ok(())
  }

  fn reader(&self) -> Receiver<Vec<u8>> {
    self.reader_rx.clone()
  }
}

impl Drop for LocalPty {
  fn drop(&mut self) {
    // 直接通过 Child 终止并回收子进程。portable-pty 的 Unix
    // Child::kill 会先发 SIGHUP，超时后再发 SIGKILL；wait 避免僵尸进程。
    match self.child.try_wait() {
      Ok(Some(_)) => {}
      Ok(None) => {
        if let Err(error) = self.child.kill() {
          warn!(target: "catus", "failed to terminate PTY child: {}", error);
        }
        if let Err(error) = self.child.wait() {
          warn!(target: "catus", "failed to reap PTY child: {}", error);
        }
      }
      Err(error) => {
        warn!(target: "catus", "failed to query PTY child status: {}", error);
        let _ = self.child.kill();
        let _ = self.child.wait();
      }
    }
  }
}

fn run_reader(mut reader: Box<dyn Read + Send>, tx: Sender<Vec<u8>>) {
  thread::spawn(move || {
    loop {
      let mut buf = vec![0u8; 4096];
      match reader.read(&mut buf) {
        Ok(0) => break, // EOF - PTY 关闭
        Ok(size) => {
          buf.resize(size, 0u8);
          if tx.send_blocking(buf).is_err() {
            break; // 接收端关闭
          }
        }
        Err(e) => {
          warn!(target: "catus", "PTY read error: {}", e);
          break;
        }
      }
    }
  });
}

fn run_writer(
  master: Box<dyn MasterPty + Send>,
  mut writer: Box<dyn Write + Send>,
  rx: Receiver<WriteCommand>,
) {
  thread::spawn(move || {
    while let Ok(cmd) = rx.recv_blocking() {
      match cmd {
        WriteCommand::Write(data) => {
          if writer.write_all(&data).is_err() || writer.flush().is_err() {
            break;
          }
        }
        WriteCommand::Resize(size) => {
          let _ = master.resize(size);
        }
      }
    }
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  /// 将 CommandBuilder 的 argv 转为字符串列表，便于断言。
  fn argv_strings(cmd: &CommandBuilder) -> Vec<String> {
    cmd
      .get_argv()
      .iter()
      .map(|s| s.to_string_lossy().into_owned())
      .collect()
  }

  #[test]
  fn build_command_none_uses_default_shell() {
    let cmd = build_command(&PtyCommand::from_command_line(None));
    // 非默认程序：argv 第一个元素为 shell 路径
    let argv = argv_strings(&cmd);
    assert!(!argv.is_empty(), "default shell should have a program");
    // 与 SHELL 环境变量或 /bin/sh 一致
    let expected = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    assert_eq!(argv[0], expected);
    assert_eq!(argv.len(), 1, "default shell has no extra args");
  }

  #[test]
  fn build_command_empty_string_falls_back_to_default_shell() {
    let cmd = build_command(&PtyCommand::from_command_line(Some("")));
    let cmd2 = build_command(&PtyCommand::from_command_line(Some("   ")));
    assert!(!argv_strings(&cmd).is_empty());
    assert!(!argv_strings(&cmd2).is_empty());
  }

  #[test]
  fn build_command_single_program_no_args() {
    let cmd = build_command(&PtyCommand::from_command_line(Some("ssh")));
    let argv = argv_strings(&cmd);
    assert_eq!(argv, vec!["ssh".to_string()]);
  }

  #[test]
  fn build_command_splits_on_whitespace() {
    let cmd = build_command(&PtyCommand::from_command_line(Some(
      "ssh user@host -p 2222",
    )));
    let argv = argv_strings(&cmd);
    assert_eq!(
      argv,
      vec![
        "ssh".to_string(),
        "user@host".to_string(),
        "-p".to_string(),
        "2222".to_string()
      ]
    );
  }

  #[test]
  fn build_command_trims_surrounding_whitespace() {
    let cmd = build_command(&PtyCommand::from_command_line(Some("  ssh user@host  ")));
    let argv = argv_strings(&cmd);
    assert_eq!(argv, vec!["ssh".to_string(), "user@host".to_string()]);
  }

  #[test]
  fn explicit_program_preserves_arguments() {
    let cmd = build_command(&PtyCommand::program("node", ["scripts/echo-pty.js"]));
    let argv = argv_strings(&cmd);
    assert_eq!(
      argv,
      vec!["node".to_string(), "scripts/echo-pty.js".to_string()]
    );
  }

  #[test]
  fn explicit_program_title_uses_executable_basename() {
    assert_eq!(
      PtyCommand::program("/usr/local/bin/node", ["script.js"]).default_title(),
      "node"
    );
    assert_eq!(
      PtyCommand::program(
        r"C:\Program Files\PowerShell\pwsh.exe",
        std::iter::empty::<&str>()
      )
      .default_title(),
      "pwsh.exe"
    );
  }

  #[test]
  fn default_shell_title_uses_shell_basename() {
    let expected = executable_name(&default_shell_program());
    assert_eq!(PtyCommand::DefaultShell.default_title(), expected);
  }
}
