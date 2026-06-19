//! 只支持 echo 的假 PTY，用于测试。
//!
//! `FakePty` 不启动任何子进程，只把写入的数据原样回送到读取通道，
//! 模拟处于 echo 模式的终端。此外提供 `push_output` / `push_bytes` 等
//! 测试辅助方法，用于在不依赖真实进程的情况下向终端注入数据
//! （例如模拟程序输出、OSC 标题序列、子进程退出等）。
//!
//! 所有写入和 resize 调用都会被记录，便于在测试中验证终端发出的数据。

use crate::terminal::Pty;
use crate::terminal::pty::TerminalSize;
use anyhow::Result;
use async_channel::{Receiver, Sender, unbounded};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

/// 控制写入到 `FakePty` 的数据如何回显。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EchoMode {
  /// 不回显任何内容。测试需要完全控制输出时使用。
  None,
  /// 原样回显写入的字节。
  Echo,
}

/// 记录一次 resize 调用。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordedResize {
  pub rows: u16,
  pub cols: u16,
  pub pixel_width: u16,
  pub pixel_height: u16,
}

impl From<TerminalSize> for RecordedResize {
  fn from(size: TerminalSize) -> Self {
    Self {
      rows: size.rows,
      cols: size.cols,
      pixel_width: size.pixel_width,
      pixel_height: size.pixel_height,
    }
  }
}

/// 假 PTY，仅支持 echo。
///
/// 设计要点：
/// - `write` 会把数据追加到 `writes` 记录，并按 `echo_mode` 决定是否回显。
/// - `resize` 会更新 `last_size`，并追加到 `resizes` 记录。
/// - `reader` 返回的通道接收回显数据以及 `push_output` 注入的数据。
/// - 通过 `Arc<Mutex<...>>` 持有可变状态以满足 `&self` 的 trait 约定。
pub struct FakePty {
  reader_tx: Arc<Mutex<Option<Sender<Vec<u8>>>>>,
  reader_rx: Receiver<Vec<u8>>,
  echo_mode: Arc<Mutex<EchoMode>>,
  writes: Arc<Mutex<Vec<Vec<u8>>>>,
  resizes: Arc<Mutex<Vec<RecordedResize>>>,
  last_size: Arc<Mutex<Option<TerminalSize>>>,
}

impl FakePty {
  /// 创建一个新的 `FakePty`，默认 `EchoMode::Echo`。
  pub fn new() -> Self {
    Self::with_echo_mode(EchoMode::Echo)
  }

  /// 用指定的 echo 模式创建 `FakePty`。
  pub fn with_echo_mode(mode: EchoMode) -> Self {
    let (reader_tx, reader_rx) = unbounded::<Vec<u8>>();
    Self {
      reader_tx: Arc::new(Mutex::new(Some(reader_tx))),
      reader_rx,
      echo_mode: Arc::new(Mutex::new(mode)),
      writes: Arc::new(Mutex::new(Vec::new())),
      resizes: Arc::new(Mutex::new(Vec::new())),
      last_size: Arc::new(Mutex::new(None)),
    }
  }

  /// 设置 echo 模式。可在测试运行中动态切换。
  pub fn set_echo_mode(&self, mode: EchoMode) {
    *self.echo_mode.lock().unwrap() = mode;
  }

  /// 直接向读取通道注入数据，模拟子进程输出。
  ///
  /// 与 `write` 不同，`push_output` 不会记录到 `writes`，也不会受
  /// `echo_mode` 影响。用于在测试中模拟程序主动打印的内容。
  pub fn push_output(&self, data: impl Into<Vec<u8>>) -> Result<()> {
    let tx = self.reader_tx.lock().unwrap();
    if let Some(tx) = tx.as_ref() {
      tx.send_blocking(data.into())?;
    }
    Ok(())
  }

  /// 便捷方法：注入字符串输出。
  pub fn push_bytes(&self, data: &str) -> Result<()> {
    self.push_output(data.as_bytes().to_vec())
  }

  /// 关闭读取端：丢弃内部 sender，使 `reader()` 的接收者收到 EOF
  /// （`recv()` 返回 `Err`）。用于模拟子进程退出后 PTY 关闭的场景。
  ///
  /// 关闭后 `write` 仍会记录数据但不再回显，`push_output` 变为空操作。
  pub fn close_reader(&self) {
    *self.reader_tx.lock().unwrap() = None;
  }

  /// 读取端是否已被 `close_reader` 关闭。
  pub fn is_reader_closed(&self) -> bool {
    self.reader_tx.lock().unwrap().is_none()
  }

  /// 获取所有通过 `write` 写入的数据快照。
  pub fn writes(&self) -> Vec<Vec<u8>> {
    self.writes.lock().unwrap().clone()
  }

  /// 获取写入数据的拼接字符串（用于断言），非法 UTF-8 会被替换。
  pub fn writes_string(&self) -> String {
    String::from_utf8_lossy(&self.writes().into_iter().flatten().collect::<Vec<u8>>()).into_owned()
  }

  /// 获取所有 resize 调用的记录快照。
  pub fn resizes(&self) -> Vec<RecordedResize> {
    self.resizes.lock().unwrap().clone()
  }

  /// 获取最近一次 resize 设置的尺寸。
  pub fn last_size(&self) -> Option<TerminalSize> {
    *self.last_size.lock().unwrap()
  }

  /// 当前挂起、尚未被 `reader` 消费的数据块数量。
  pub fn pending_output_count(&self) -> usize {
    self.reader_rx.len()
  }
}

impl Default for FakePty {
  fn default() -> Self {
    Self::new()
  }
}

#[async_trait]
impl Pty for FakePty {
  async fn write(&self, data: Vec<u8>) -> Result<()> {
    self.writes.lock().unwrap().push(data.clone());

    let mode = *self.echo_mode.lock().unwrap();
    if mode == EchoMode::Echo {
      // 先克隆出 sender 再 await，避免跨 await 持有 MutexGuard（要求 Send）
      let tx = self.reader_tx.lock().unwrap().clone();
      if let Some(tx) = tx {
        tx.send(data).await?;
      }
    }
    Ok(())
  }

  async fn resize(&self, size: TerminalSize) -> Result<()> {
    self.resizes.lock().unwrap().push(size.into());
    *self.last_size.lock().unwrap() = Some(size);
    Ok(())
  }

  fn reader(&self) -> Receiver<Vec<u8>> {
    self.reader_rx.clone()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[gpui::test]
  async fn write_echoes_in_echo_mode() {
    let pty = FakePty::new();
    pty.write(b"hello".to_vec()).await.unwrap();

    let rx = pty.reader();
    let data = rx.recv().await.unwrap();
    assert_eq!(data, b"hello".to_vec());
    assert_eq!(pty.writes_string(), "hello");
  }

  #[gpui::test]
  async fn write_does_not_echo_in_none_mode() {
    let pty = FakePty::with_echo_mode(EchoMode::None);
    pty.write(b"silent".to_vec()).await.unwrap();
    assert_eq!(pty.pending_output_count(), 0);
    assert_eq!(pty.writes_string(), "silent");
  }

  #[gpui::test]
  async fn push_output_injects_without_recording() {
    let pty = FakePty::new();
    pty.push_bytes("injected").unwrap();
    // push_output 不算作 write
    assert_eq!(pty.writes_string(), "");
    // 但能从 reader 读到
    let rx = pty.reader();
    let data = rx.recv().await.unwrap();
    assert_eq!(data, b"injected".to_vec());
  }

  #[gpui::test]
  async fn resize_records_history_and_last_size() {
    let pty = FakePty::new();
    let size = TerminalSize::new(30, 100, 1, 2);
    pty.resize(size).await.unwrap();

    assert_eq!(pty.last_size(), Some(size));
    let resizes = pty.resizes();
    assert_eq!(resizes.len(), 1);
    assert_eq!(resizes[0], RecordedResize::from(size));
  }

  #[gpui::test]
  async fn switching_echo_mode_at_runtime() {
    let pty = FakePty::new();
    pty.write(b"a".to_vec()).await.unwrap(); // echoed
    assert_eq!(pty.pending_output_count(), 1);

    pty.set_echo_mode(EchoMode::None);
    pty.write(b"b".to_vec()).await.unwrap(); // not echoed
    assert_eq!(pty.pending_output_count(), 1); // still 1
    assert_eq!(pty.writes_string(), "ab");
  }

  #[test]
  fn recorded_resize_from_terminal_size() {
    let size = TerminalSize::new(24, 80, 5, 7);
    let rec = RecordedResize::from(size);
    assert_eq!(rec.rows, 24);
    assert_eq!(rec.cols, 80);
    assert_eq!(rec.pixel_width, 5);
    assert_eq!(rec.pixel_height, 7);
  }

  #[gpui::test]
  async fn close_reader_makes_recv_return_err() {
    let pty = FakePty::new();
    let rx = pty.reader();
    assert!(!pty.is_reader_closed());

    pty.close_reader();
    assert!(pty.is_reader_closed());

    // 接收端应收到 EOF（recv 返回 Err）
    assert!(rx.recv().await.is_err());
  }

  #[gpui::test]
  async fn close_reader_makes_write_no_longer_echo() {
    let pty = FakePty::new();
    pty.write(b"before".to_vec()).await.unwrap();
    assert_eq!(pty.pending_output_count(), 1);

    pty.close_reader();
    // 关闭后 write 仍记录，但不再回显
    pty.write(b"after".to_vec()).await.unwrap();
    assert_eq!(pty.pending_output_count(), 1); // 仍是 1
    assert_eq!(pty.writes_string(), "beforeafter");
    // push_output 关闭后变为空操作
    assert!(pty.push_bytes("ignored").is_ok());
    assert_eq!(pty.pending_output_count(), 1);
  }
}
