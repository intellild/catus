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
  #[allow(dead_code)]
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
      // 标准色
      NamedColor::Black => [0, 0, 0],
      NamedColor::Red => [205, 49, 49],
      NamedColor::Green => [13, 188, 121],
      NamedColor::Yellow => [229, 229, 16],
      NamedColor::Blue => [36, 114, 200],
      NamedColor::Magenta => [188, 63, 188],
      NamedColor::Cyan => [17, 168, 205],
      NamedColor::White => [229, 229, 229],
      // 亮色
      NamedColor::BrightBlack => [128, 128, 128],
      NamedColor::BrightRed => [255, 85, 85],
      NamedColor::BrightGreen => [85, 255, 85],
      NamedColor::BrightYellow => [255, 255, 85],
      NamedColor::BrightBlue => [85, 85, 255],
      NamedColor::BrightMagenta => [255, 85, 255],
      NamedColor::BrightCyan => [85, 255, 255],
      NamedColor::BrightWhite => [255, 255, 255],
      // 暗淡色
      NamedColor::DimBlack => [64, 64, 64],
      NamedColor::DimRed => [103, 25, 25],
      NamedColor::DimGreen => [7, 94, 61],
      NamedColor::DimYellow => [115, 115, 8],
      NamedColor::DimBlue => [18, 57, 100],
      NamedColor::DimMagenta => [94, 32, 94],
      NamedColor::DimCyan => [9, 84, 103],
      NamedColor::DimWhite => [115, 115, 115],
      // 特殊色
      NamedColor::Foreground => [212, 212, 212],
      NamedColor::Background => [30, 30, 30],
      NamedColor::Cursor => [255, 255, 255],
      NamedColor::BrightForeground => [255, 255, 255],
      NamedColor::DimForeground => [128, 128, 128],
    },
    AnsiColor::Spec(rgb) => [rgb.r, rgb.g, rgb.b],
    AnsiColor::Indexed(idx) => {
      // ANSI 256 色表完整实现
      match idx {
        // 标准色 0-7
        0 => [0, 0, 0],
        1 => [205, 49, 49],
        2 => [13, 188, 121],
        3 => [229, 229, 16],
        4 => [36, 114, 200],
        5 => [188, 63, 188],
        6 => [17, 168, 205],
        7 => [229, 229, 229],
        // 亮色 8-15
        8 => [128, 128, 128],
        9 => [255, 85, 85],
        10 => [85, 255, 85],
        11 => [255, 255, 85],
        12 => [85, 85, 255],
        13 => [255, 85, 255],
        14 => [85, 255, 255],
        15 => [255, 255, 255],
        // 216 色立方 16-231
        16..=231 => {
          let i = *idx - 16;
          let r = (i / 36) as usize;
          let g = ((i % 36) / 6) as usize;
          let b = (i % 6) as usize;
          let values = [0u8, 95, 135, 175, 215, 255];
          [values[r], values[g], values[b]]
        }
        // 灰度 232-255
        232..=255 => {
          let gray = 8 + (*idx - 232) * 10;
          [gray, gray, gray]
        }
      }
    }
  }
}

/// 将 RGB 转换为 Hsla
pub fn rgb_to_hsla(rgb: [u8; 3]) -> Hsla {
  gpui::rgb((rgb[0] as u32) << 16 | (rgb[1] as u32) << 8 | rgb[2] as u32).into()
}
