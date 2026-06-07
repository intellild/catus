use alacritty_terminal::{
  index::{Column, Line},
  term::{TermMode, cell::Cell},
  vte::ansi::Color as AnsiColor,
};
use gpui::*;

/// 终端事件
#[derive(Clone, Debug)]
pub enum TerminalEvent {
  /// 标题变化
  TitleChanged,
  /// 终端铃声
  Bell,
  /// 关闭终端
  Closed,
}

/// 带位置的单元格
#[derive(Clone, Debug)]
pub struct IndexedCell {
  pub point: TerminalPoint,
  pub cell: Cell,
}

/// 终端位置（行和列）
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalPoint {
  pub line: Line,
  pub column: Column,
}

/// 选择范围
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectionRange {
  pub start: TerminalPoint,
  pub end: TerminalPoint,
}

impl SelectionRange {
  /// 规范化选择范围，确保 start <= end
  pub fn normalized(self) -> (TerminalPoint, TerminalPoint) {
    let mut start = self.start;
    let mut end = self.end;

    if start.line.0 > end.line.0 || (start.line.0 == end.line.0 && start.column.0 > end.column.0) {
      std::mem::swap(&mut start, &mut end);
    }

    (start, end)
  }

  /// 检查指定点是否在选择范围内
  pub fn contains(self, point: TerminalPoint) -> bool {
    let (start, end) = self.normalized();

    if point.line.0 < start.line.0 || point.line.0 > end.line.0 {
      return false;
    }

    if point.line.0 == start.line.0 && point.line.0 == end.line.0 {
      return point.column.0 >= start.column.0 && point.column.0 <= end.column.0;
    }

    if point.line.0 == start.line.0 {
      return point.column.0 >= start.column.0;
    }

    if point.line.0 == end.line.0 {
      return point.column.0 <= end.column.0;
    }

    true
  }
}

/// 可渲染的光标状态
#[derive(Clone, Debug)]
pub struct CursorState {
  pub point: TerminalPoint,
  pub shape: alacritty_terminal::vte::ansi::CursorShape,
}

impl Default for CursorState {
  fn default() -> Self {
    Self {
      point: TerminalPoint::default(),
      shape: alacritty_terminal::vte::ansi::CursorShape::Block,
    }
  }
}

/// 终端内容实体 - 纯渲染状态
#[derive(Clone)]
pub struct TerminalContent {
  pub cells: Vec<IndexedCell>,
  pub mode: TermMode,
  pub display_offset: usize,
  pub cursor: CursorState,
  pub cursor_char: char,
  pub scrolled_to_bottom: bool,
  pub selection: Option<SelectionRange>,
}

impl TerminalContent {
  /// 创建空的终端内容
  pub fn new() -> Self {
    Self {
      cells: Vec::new(),
      mode: TermMode::default(),
      display_offset: 0,
      cursor: CursorState::default(),
      cursor_char: ' ',
      scrolled_to_bottom: true,
      selection: None,
    }
  }
}

impl Default for TerminalContent {
  fn default() -> Self {
    Self::new()
  }
}

/// 将 ANSI 颜色转换为 RGB
pub fn ansi_color_to_rgb(color: &AnsiColor) -> [u8; 3] {
  use alacritty_terminal::vte::ansi::NamedColor;

  match color {
    AnsiColor::Named(name) => match name {
      NamedColor::Black => [0, 0, 0],
      NamedColor::Red => [255, 0, 0],
      NamedColor::Green => [0, 255, 0],
      NamedColor::Yellow => [255, 255, 0],
      NamedColor::Blue => [0, 0, 255],
      NamedColor::Magenta => [255, 0, 255],
      NamedColor::Cyan => [0, 255, 255],
      NamedColor::White => [255, 255, 255],
      NamedColor::BrightBlack => [64, 64, 64],
      NamedColor::BrightRed => [255, 64, 64],
      NamedColor::BrightGreen => [64, 255, 64],
      NamedColor::BrightYellow => [255, 255, 64],
      NamedColor::BrightBlue => [64, 64, 255],
      NamedColor::BrightMagenta => [255, 64, 255],
      NamedColor::BrightCyan => [64, 255, 255],
      NamedColor::BrightWhite => [255, 255, 255],
      NamedColor::Foreground => [212, 212, 212],
      NamedColor::Background => [30, 30, 30],
      _ => [212, 212, 212],
    },
    AnsiColor::Spec(rgb) => [rgb.r, rgb.g, rgb.b],
    AnsiColor::Indexed(idx) => {
      // ANSI 256 色表简化处理
      match idx {
        0 => [0, 0, 0],
        1 => [255, 0, 0],
        2 => [0, 255, 0],
        3 => [255, 255, 0],
        4 => [0, 0, 255],
        5 => [255, 0, 255],
        6 => [0, 255, 255],
        7 => [255, 255, 255],
        _ => [212, 212, 212],
      }
    }
  }
}

/// 将 RGB 转换为 Hsla
pub fn rgb_to_hsla(rgb: [u8; 3]) -> Hsla {
  gpui::rgb((rgb[0] as u32) << 16 | (rgb[1] as u32) << 8 | rgb[2] as u32).into()
}
