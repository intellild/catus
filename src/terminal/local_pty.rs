use crate::terminal::Pty;
use crate::terminal::pty::TerminalSize;
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
    let cmd = build_command(command);
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

/// 将命令字符串按空白拆分为程序名 + 参数。
///
/// `portable_pty::CommandBuilder::new` 只接受单个程序路径，不接受
/// shell 命令行，因此 `"ssh user@host"` 必须拆分为 `ssh` + `user@host`。
fn build_command(command: Option<&str>) -> CommandBuilder {
  if let Some(cmd_str) = command {
    let trimmed = cmd_str.trim();
    if !trimmed.is_empty() {
      let mut parts = trimmed.split_whitespace();
      let program = parts.next().expect("non-empty after trim");
      let mut cmd = CommandBuilder::new(program);
      for arg in parts {
        cmd.arg(arg);
      }
      return cmd;
    }
  }

  // 系统默认 shell
  #[cfg(target_os = "windows")]
  {
    CommandBuilder::new("cmd.exe")
  }
  #[cfg(not(target_os = "windows"))]
  {
    std::env::var("SHELL")
      .map(|shell| CommandBuilder::new(&shell))
      .unwrap_or_else(|_| CommandBuilder::new("/bin/sh"))
  }
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
    // 同步 kill 子进程。drop writer_tx / reader_rx 会关闭通道，
    // reader/writer 线程检测到通道关闭后自行退出。
    let mut killer = self.child.clone_killer();
    let _ = killer.kill();
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
    let cmd = build_command(None);
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
    let cmd = build_command(Some(""));
    let cmd2 = build_command(Some("   "));
    assert!(!argv_strings(&cmd).is_empty());
    assert!(!argv_strings(&cmd2).is_empty());
  }

  #[test]
  fn build_command_single_program_no_args() {
    let cmd = build_command(Some("ssh"));
    let argv = argv_strings(&cmd);
    assert_eq!(argv, vec!["ssh".to_string()]);
  }

  #[test]
  fn build_command_splits_on_whitespace() {
    let cmd = build_command(Some("ssh user@host -p 2222"));
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
    let cmd = build_command(Some("  ssh user@host  "));
    let argv = argv_strings(&cmd);
    assert_eq!(argv, vec!["ssh".to_string(), "user@host".to_string()]);
  }
}
