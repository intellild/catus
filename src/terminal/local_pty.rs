use crate::terminal::Pty;
use crate::terminal::pty::TerminalSize;
use anyhow::{Context, Result};
use async_channel::{Receiver, Sender, unbounded};
use portable_pty::{Child, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
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
  child: Arc<Box<dyn Child + Send + Sync>>,
  writer: Arc<Mutex<Box<dyn Write + Send>>>,
  master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
  write_tx: Sender<WriteCommand>,
  _write_handle: Arc<Mutex<Option<JoinHandle<Result<()>>>>>,
  _read_handle: Arc<Mutex<Option<JoinHandle<Result<()>>>>>,
  read_rx: Mutex<Option<Receiver<Vec<u8>>>>,
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

    // 创建写入通道
    let (write_tx, write_rx) = unbounded::<WriteCommand>();

    // 启动写入线程
    let writer_clone = Arc::new(Mutex::new(writer));
    let writer_for_thread = writer_clone.clone();
    let master_for_resize = Arc::new(Mutex::new(master));
    let master_for_thread = master_for_resize.clone();

    let write_handle = thread::spawn(move || -> Result<()> {
      while let Ok(cmd) = write_rx.recv_blocking() {
        match cmd {
          WriteCommand::Write(data) => {
            if let Ok(mut w) = writer_for_thread.lock() {
              w.write_all(&data)?;
              w.flush()?;
            }
          }
          WriteCommand::Resize(size) => {
            if let Ok(m) = master_for_thread.lock() {
              let _ = m.resize(size);
            }
          }
        }
      }
      Ok(())
    });

    // 创建读取通道
    let (read_tx, read_rx) = unbounded::<Vec<u8>>();

    // 启动读取线程
    let mut reader_for_thread = reader;
    let read_handle = thread::spawn(move || -> Result<()> {
      loop {
        let mut buf = vec![0u8; 4096];
        match reader_for_thread.read(&mut buf) {
          Ok(0) => {
            // EOF - PTY 关闭
            break;
          }
          Ok(size) => {
            buf.resize(size, 0u8);
            if read_tx.send_blocking(buf).is_err() {
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
    });

    Ok(Self {
      process_id,
      child: Arc::new(child),
      writer: writer_clone,
      master: master_for_resize,
      write_tx,
      _write_handle: Arc::new(Mutex::new(Some(write_handle))),
      _read_handle: Arc::new(Mutex::new(Some(read_handle))),
      read_rx: Mutex::new(Some(read_rx)),
    })
  }
}

impl Pty for LocalPty {
  /// 写入数据到 PTY
  ///
  /// 使用 `&self` 而非 `&mut self`，内部使用 Arc<Mutex<_>> 实现可变性
  fn write(&self, data: &[u8]) -> Result<()> {
    self
      .write_tx
      .send_blocking(WriteCommand::Write(data.to_vec()))
      .map_err(|e| anyhow::anyhow!("Failed to send write command: {}", e))
  }

  /// 调整 PTY 大小
  ///
  /// 使用 `&self` 而非 `&mut self`，内部使用 Arc<Mutex<_>> 实现可变性
  fn resize(&self, size: TerminalSize) -> Result<()> {
    let pty_size = PtySize {
      rows: size.rows,
      cols: size.cols,
      pixel_width: size.pixel_width,
      pixel_height: size.pixel_height,
    };

    self
      .write_tx
      .send_blocking(WriteCommand::Resize(pty_size))
      .map_err(|e| anyhow::anyhow!("Failed to send resize command: {}", e))
  }

  /// 启动读取循环，返回数据接收器
  ///
  /// 注意：只能调用一次，第二次调用会 panic
  fn start_reader(&self) -> Receiver<Vec<u8>> {
    self
      .read_rx
      .lock()
      .unwrap()
      .take()
      .expect("start_reader() can only be called once")
  }

  /// 关闭 PTY
  fn close(&self) -> Result<()> {
    // 关闭写入通道，这会终止写入线程
    drop(&self.write_tx);

    // 尝试杀死子进程
    if let Ok(mut child) = Arc::try_unwrap(Arc::clone(&self.child)) {
      let _ = child.kill();
    }

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
