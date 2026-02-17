use crate::terminal::pty::TerminalSize;

/// 终端输入事件（UI → Background）
#[derive(Clone, Debug)]
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
