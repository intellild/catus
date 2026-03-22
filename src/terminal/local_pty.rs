use crate::terminal::Pty;
use crate::terminal::pty::TerminalSize;
use anyhow::{Context, Result};
use async_channel::{Receiver, Sender, unbounded};
use async_trait::async_trait;
use blocking::unblock;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::thread;
use std::thread::JoinHandle;

/// 写入命令枚举
enum WriteCommand {
  Write(Vec<u8>),
  Resize(PtySize),
}

/// 本地 PTY 实现
///
/// 使用 `Arc<Mutex<_>>` 实现内部可变性，支持 `&self` 方法（类似 Zed 的设计）
pub struct LocalPty {
  process_id: Option<u32>,
  child: Box<dyn Child + Send + Sync>,
  reader_handle: JoinHandle<Result<()>>,
  reader_rx: Receiver<Vec<u8>>,
  writer_handle: JoinHandle<Result<()>>,
  writer_tx: Sender<WriteCommand>,
}

impl LocalPty {
  /// 创建本地 PTY
  ///
  /// # Arguments
  /// * `size` - 终端尺寸
  /// * `command` - 可选的命令，如果为 None 则启动系统默认 shell
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

    // 获取要执行的命令，如果没有提供则使用系统默认 shell
    let cmd = if let Some(cmd) = command {
      CommandBuilder::new(cmd)
    } else {
      // 使用系统默认 shell
      #[cfg(target_os = "windows")]
      {
        CommandBuilder::new("cmd.exe")
      }
      #[cfg(not(target_os = "windows"))]
      {
        // 优先使用用户配置的 shell，否则使用 /bin/sh
        std::env::var("SHELL")
          .map(|shell| CommandBuilder::new(&shell))
          .unwrap_or_else(|_| CommandBuilder::new("/bin/sh"))
      }
    };

    let child = pty_pair
      .slave
      .spawn_command(cmd)
      .with_context(|| "Failed to spawn command in PTY")?;
    let process_id = child.process_id();

    let master = pty_pair.master;

    // 获取 writer 和 reader
    let writer = master
      .take_writer()
      .with_context(|| "Failed to get PTY writer")?;
    let reader = master
      .try_clone_reader()
      .with_context(|| "Failed to get PTY reader")?;

    // 创建读取通道
    let (reader_tx, reader_rx) = unbounded::<Vec<u8>>();
    let (writer_tx, writer_rx) = unbounded::<WriteCommand>();

    let reader_handle = run_reader(reader, reader_tx);
    let writer_handle = run_writer(master, writer_rx)?;

    Ok(Self {
      process_id,
      child,
      reader_handle,
      reader_rx,
      writer_handle,
      writer_tx,
    })
  }
}

#[async_trait]
impl Pty for LocalPty {
  /// 写入数据到 PTY
  async fn write(&self, data: Vec<u8>) -> Result<()> {
    self.writer_tx.send(WriteCommand::Write(data)).await?;

    Ok(())
  }

  /// 调整 PTY 大小
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

  /// 关闭 PTY
  async fn close(&mut self) -> Result<()> {
    let _ = self.writer_tx;

    let mut killer = self.child.clone_killer();
    unblock(move || killer.kill()).await?;

    Ok(())
  }

  /// 获取进程 ID
  fn process_id(&self) -> Option<u32> {
    self.process_id
  }
}

impl Drop for LocalPty {
  fn drop(&mut self) {
    // 确保关闭 PTY
    let _ = self.close();
  }
}

fn run_reader(mut reader: Box<dyn Read + Send>, tx: Sender<Vec<u8>>) -> JoinHandle<Result<()>> {
  // 启动读取线程
  thread::spawn(move || -> Result<()> {
    loop {
      let mut buf = vec![0u8; 4096];
      match reader.read(&mut buf) {
        Ok(0) => {
          // EOF - PTY 关闭
          break;
        }
        Ok(size) => {
          buf.resize(size, 0u8);
          if tx.send_blocking(buf).is_err() {
            // 接收端关闭
            break;
          }
        }
        Err(e) => {
          eprintln!("PTY read error: {}", e);
          break;
        }
      }
    }
    Ok(())
  })
}

fn run_writer(
  master: Box<dyn MasterPty + Send>,
  rx: Receiver<WriteCommand>,
) -> Result<JoinHandle<Result<()>>> {
  let mut writer = master
    .take_writer()
    .with_context(|| "Failed to get PTY writer")?;

  let write_handle = thread::spawn(move || -> Result<()> {
    loop {
      let cmd = rx.recv_blocking()?;
      match cmd {
        WriteCommand::Write(data) => {
          writer.write_all(&data)?;
          writer.flush()?;
        }
        WriteCommand::Resize(size) => {
          master.resize(size)?;
        }
      }
    }
    Ok(())
  });

  Ok(write_handle)
}
