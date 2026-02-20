pub mod content;
pub mod input;
pub mod local_pty;
pub mod pty;
pub mod terminal;
pub mod terminal_element;
pub mod view;

// 重导出主要类型
pub use content::{
  CursorState, IndexedCell, SelectionRange, SelectionType, TerminalBounds, TerminalContent,
  TerminalEvent, TerminalPoint,
};
pub use local_pty::LocalPty;
pub use pty::{Pty, TerminalSize};
pub use terminal::Terminal;
pub use terminal_element::TerminalElement;
pub use view::TerminalView;
