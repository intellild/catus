use crate::terminal::pty::TerminalSize;
use std::fmt;

/// 终端输入事件（UI → Background）
pub enum TerminalInput {
  /// PTY 输出数据（来自 read thread）
  PtyData(Vec<u8>),

  /// 用户输入数据
  Write(Vec<u8>),

  /// 调整终端大小
  Resize(TerminalSize),

  /// 获取当前内容（强制刷新）
  Sync,

  /// 关闭终端
  Shutdown,
}

impl fmt::Debug for TerminalInput {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      TerminalInput::PtyData(data) => f.debug_tuple("PtyData").field(&data.len()).finish(),
      TerminalInput::Write(data) => f.debug_tuple("Write").field(&data.len()).finish(),
      TerminalInput::Resize(size) => f.debug_tuple("Resize").field(size).finish(),
      TerminalInput::Sync => write!(f, "Sync"),
      TerminalInput::Shutdown => write!(f, "Shutdown"),
    }
  }
}
