use crate::terminal::pty::{Pty, TerminalSize};
use portable_pty::{CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// 本地 PTY 实现
pub struct LocalPty {
  writer: Arc<Mutex<Box<dyn Write + Send>>>,
  process_id: Option<u32>,
}

impl LocalPty {
  /// 创建本地 PTY
  pub fn new(size: TerminalSize, shell: &str) -> anyhow::Result<(Self, mpsc::Receiver<Vec<u8>>)> {
    let pty_system = portable_pty::native_pty_system();

    let pty_size = PtySize {
      rows: size.rows,
      cols: size.cols,
      pixel_width: size.pixel_width,
      pixel_height: size.pixel_height,
    };

    let pty_pair = pty_system.openpty(pty_size)?;

    // 启动 shell
    let cmd = CommandBuilder::new(shell);
    let child = pty_pair.slave.spawn_command(cmd)?;
    let process_id = child.process_id();

    // 获取 writer 和 reader
    let writer = pty_pair.master.take_writer()?;
    let mut reader = pty_pair.master.try_clone_reader()?;

    // 创建 channel 用于接收数据
    let (tx, rx) = mpsc::channel::<Vec<u8>>(1024);

    // 启动读取线程
    std::thread::spawn(move || {
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

    let pty = Self {
      writer: Arc::new(Mutex::new(writer)),
      process_id,
    };

    Ok((pty, rx))
  }
}

impl Pty for LocalPty {
  fn write(&self, data: &[u8]) -> anyhow::Result<()> {
    let mut writer = self.writer.lock().unwrap();
    writer.write_all(data)?;
    writer.flush()?;
    Ok(())
  }

  fn resize(&self, _size: TerminalSize) -> anyhow::Result<()> {
    // 由于我们已经移除了 PtyPair，无法调整大小
    // 需要在设计中加入这个功能
    Ok(())
  }

  fn start_reader(self: Box<Self>) -> mpsc::Receiver<Vec<u8>> {
    // 这个实现已经在 new 中返回了 reader
    // 这里返回一个空的 channel，实际应用中需要重新设计
    let (_, rx) = mpsc::channel::<Vec<u8>>(1);
    rx
  }

  fn close(&self) -> anyhow::Result<()> {
    // writer 会在 drop 时关闭
    Ok(())
  }

  fn process_id(&self) -> Option<u32> {
    self.process_id
  }
}
