pub mod constants;
pub mod content;
pub mod local_pty;
pub mod model;
pub mod pty;
pub mod terminal_element;
pub mod view;

// 重导出主要类型
pub use local_pty::LocalPty;
pub use model::Terminal;
pub use pty::{Pty, TerminalSize};
pub use view::{TerminalView, TerminalViewEvent};

// 测试用具：仅测试构建中可用，避免生产二进制携带未使用代码。
#[cfg(test)]
pub mod fake_pty;
#[cfg(test)]
pub use fake_pty::FakePty;
