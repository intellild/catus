use anyhow::Result;
use async_channel::Receiver;

/// 终端尺寸
#[derive(Clone, Copy, Debug)]
pub struct TerminalSize {
  pub rows: u16,
  pub cols: u16,
  pub pixel_width: u16,
  pub pixel_height: u16,
}

impl TerminalSize {
  /// 创建新的终端尺寸
  pub fn new(rows: u16, cols: u16, pixel_width: u16, pixel_height: u16) -> Self {
    Self {
      rows,
      cols,
      pixel_width,
      pixel_height,
    }
  }

  /// 创建默认的终端尺寸 (24x80)
  pub fn default_size() -> Self {
    Self {
      rows: 24,
      cols: 80,
      pixel_width: 0,
      pixel_height: 0,
    }
  }
}

/// PTY 抽象
///
/// 参考 Zed 的设计，所有方法使用 `&self` 而非 `&mut self`，
/// 内部通过 `Arc<Mutex<_>>` 实现可变性。
///
/// 需要 `Send + Sync` bound 以支持多线程访问。
pub trait Pty: Send + Sync {
  /// 写入数据到 PTY
  ///
  /// # Arguments
  /// * `data` - 要写入的字节数据
  ///
  /// # Errors
  /// 如果写入失败则返回错误
  fn write(&self, data: &[u8]) -> Result<()>;

  /// 调整 PTY 大小
  ///
  /// # Arguments
  /// * `size` - 新的终端尺寸
  ///
  /// # Errors
  /// 如果调整大小失败则返回错误
  fn resize(&self, size: TerminalSize) -> Result<()>;

  /// 启动读取循环，返回数据接收器
  ///
  /// 注意：只能调用一次，第二次调用会 panic。
  /// 这是因为接收器只能有一个所有者。
  ///
  /// # Returns
  /// 返回一个异步通道接收器，用于接收 PTY 输出的数据
  fn start_reader(&self) -> Receiver<Vec<u8>>;

  /// 关闭 PTY
  ///
  /// 清理资源，终止子进程。
  ///
  /// # Errors
  /// 如果关闭失败则返回错误
  fn close(&self) -> Result<()>;

  /// 获取进程 ID（本地 PTY 有效）
  ///
  /// # Returns
  /// 如果可用，返回进程 ID，否则返回 None
  fn process_id(&self) -> Option<u32>;
}
