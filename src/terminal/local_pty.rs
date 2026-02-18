use crate::terminal::pty::{Pty, TerminalSize};
use anyhow::Context;
use portable_pty::{Child, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// 本地 PTY 实现
pub struct LocalPty {
  writer: Arc<Mutex<Box<dyn Write + Send>>>,
  process_id: Option<u32>,
  _reader_thread: Option<std::thread::JoinHandle<()>>,
  _child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
  /// PTY 输出接收器（只能被 take 一次）
  reader_rx: Option<mpsc::Receiver<Vec<u8>>>,
}

impl LocalPty {
  /// 创建本地 PTY
  ///
  /// # Arguments
  /// * `size` - 终端尺寸
  /// * `command` - 可选的命令，如果为 None 则启动系统默认 shell
  pub fn new(size: TerminalSize, command: Option<&str>) -> anyhow::Result<Self> {
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

    // 获取 writer 和 reader
    let writer = pty_pair
      .master
      .take_writer()
      .with_context(|| "Failed to get PTY writer")?;
    let mut reader = pty_pair
      .master
      .try_clone_reader()
      .with_context(|| "Failed to get PTY reader")?;

    // 创建 channel 用于接收数据
    let (tx, rx) = mpsc::channel::<Vec<u8>>(1024);

    // 启动读取线程并保存 handle
    let reader_thread = std::thread::spawn(move || {
      let mut buffer = [0u8; 4096];
      loop {
        match reader.read(&mut buffer) {
          Ok(0) => break,
          Ok(n) => {
            let data = buffer[..n].to_vec();
            if tx.blocking_send(data).is_err() {
              break;
            }
          }
          Err(_) => break,
        }
      }
    });

    // 关闭 slave 端
    drop(pty_pair.slave);

    // 保存子进程和线程 handle
    let child: Box<dyn Child + Send + Sync> = child;
    let reader_thread = Some(reader_thread);

    let pty = Self {
      writer: Arc::new(Mutex::new(writer)),
      process_id,
      _reader_thread: reader_thread,
      _child: Arc::new(Mutex::new(child)),
      reader_rx: Some(rx),
    };

    Ok(pty)
  }
}

impl Pty for LocalPty {
  fn write(&self, data: &[u8]) -> anyhow::Result<()> {
    let mut writer = self.writer.lock().unwrap();
    writer
      .write_all(data)
      .with_context(|| "Failed to write to PTY")?;
    writer.flush().with_context(|| "Failed to flush PTY")?;
    Ok(())
  }

  fn resize(&self, _size: TerminalSize) -> anyhow::Result<()> {
    // 由于我们已经移除了 PtyPair，无法调整大小
    // 需要在设计中加入这个功能
    Ok(())
  }

  fn start_reader(&mut self) -> mpsc::Receiver<Vec<u8>> {
    // 返回存储的 receiver，只能 take 一次
    self
      .reader_rx
      .take()
      .expect("start_reader can only be called once")
  }

  fn close(&self) -> anyhow::Result<()> {
    // writer 会在 drop 时关闭
    Ok(())
  }

  fn process_id(&self) -> Option<u32> {
    self.process_id
  }
}
