use tokio::sync::mpsc;

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
pub trait Pty: Send + Sync {
  /// 写入数据
  fn write(&self, data: &[u8]) -> anyhow::Result<()>;

  /// 调整大小
  fn resize(&self, size: TerminalSize) -> anyhow::Result<()>;

  /// 启动读取循环，返回数据接收器
  /// 在内部创建独立线程进行阻塞读取
  /// 注意：此方法只能调用一次
  fn start_reader(&self) -> mpsc::Receiver<Vec<u8>>;

  /// 关闭 PTY
  fn close(&self) -> anyhow::Result<()>;

  /// 获取进程 ID（本地 PTY 有效）
  fn process_id(&self) -> Option<u32>;
}
