//! 全局日志初始化。
//!
//! 基于 `tracing` + `tracing-subscriber`。输出目标由 `CATUS_LOG_DIR` 决定：
//!
//! - 设置 `CATUS_LOG_DIR` 时，日志追加写入 `<dir>/catus.log`，方便集中排查
//!   （例如 e2e 视觉测试时与编排日志放同一目录）。
//! - 未设置时，日志按级别输出到标准流：`WARN`/`ERROR` 到 stderr，
//!   `INFO`/`DEBUG`/`TRACE` 到 stdout。
//!
//! 日志级别由 `RUST_LOG` 控制；未设置时使用默认过滤器：
//! `CATUS_LOG_DIR` 启用时为 `catus=debug,warn`，否则为 `catus=info,warn`。
use std::fs::{OpenOptions, create_dir_all};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Mutex;

use tracing::Level;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// 控制日志输出目录的环境变量名。
pub const LOG_DIR_ENV: &str = "CATUS_LOG_DIR";

/// 初始化全局日志订阅者。仅应在程序入口调用一次。
///
/// 即使文件日志初始化失败也会回退到标准流，保证不会因日志问题阻断启动。
pub fn init() {
  let log_dir = std::env::var(LOG_DIR_ENV)
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty());

  let default_filter = if log_dir.is_some() {
    "catus=debug,warn"
  } else {
    "catus=info,warn"
  };
  let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

  let registry = tracing_subscriber::registry().with(filter);

  // 优先尝试打开文件日志；失败时回退到标准流。
  let using_file = match log_dir.as_ref().and_then(|dir| {
    let dir_path = PathBuf::from(dir);
    // 先确保目录存在，再打开文件（OpenOptions::create 只创建文件，不创建父目录）。
    let path = dir_path.join("catus.log");
    match create_dir_all(&dir_path)
      .and_then(|()| OpenOptions::new().create(true).append(true).open(&path))
    {
      Ok(file) => Some(file),
      Err(e) => {
        eprintln!(
          "Failed to open log file {}: {} — falling back to stdout/stderr",
          path.display(),
          e
        );
        None
      }
    }
  }) {
    Some(file) => {
      // 文件不需要 ANSI 颜色码。
      registry
        .with(fmt::layer().with_writer(Mutex::new(file)).with_ansi(false))
        .init();
      true
    }
    None => {
      registry.with(fmt::layer().with_writer(StdioWriter)).init();
      false
    }
  };

  tracing::info!(
    target: "catus",
    "logging initialized (output: {})",
    if using_file { "file" } else { "stdout/stderr" }
  );
}

/// 按日志级别路由到标准流的 `MakeWriter`：
/// `WARN`/`ERROR` → stderr，其余 → stdout。
struct StdioWriter;

impl<'a> MakeWriter<'a> for StdioWriter {
  type Writer = StdioStream<'a>;

  fn make_writer(&'a self) -> Self::Writer {
    StdioStream::Stdout(io::stdout().lock())
  }

  fn make_writer_for(&'a self, meta: &tracing::Metadata<'_>) -> Self::Writer {
    if meta.level() <= &Level::WARN {
      StdioStream::Stderr(io::stderr().lock())
    } else {
      StdioStream::Stdout(io::stdout().lock())
    }
  }
}

enum StdioStream<'a> {
  Stdout(io::StdoutLock<'a>),
  Stderr(io::StderrLock<'a>),
}

impl<'a> Write for StdioStream<'a> {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    match self {
      StdioStream::Stdout(w) => w.write(buf),
      StdioStream::Stderr(w) => w.write(buf),
    }
  }

  fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
    match self {
      StdioStream::Stdout(w) => w.write_all(buf),
      StdioStream::Stderr(w) => w.write_all(buf),
    }
  }

  fn flush(&mut self) -> io::Result<()> {
    match self {
      StdioStream::Stdout(w) => w.flush(),
      StdioStream::Stderr(w) => w.flush(),
    }
  }
}
