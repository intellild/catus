use alacritty_terminal::{
  index::{Column, Line},
  term::{RenderableCursor, TermMode, cell::Cell},
  vte::ansi::Color as AnsiColor,
};
use gpui::*;

/// 终端事件
#[derive(Clone, Debug)]
pub enum TerminalEvent {
  /// 终端内容变化，需要重绘
  Wakeup,
  /// 标题变化
  TitleChanged(String),
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

/// 选区范围
#[derive(Clone, Debug)]
pub struct SelectionRange {
  pub start: TerminalPoint,
  pub end: TerminalPoint,
}

/// 终端边界信息
#[derive(Clone, Copy, Debug)]
pub struct TerminalBounds {
  pub cell_width: Pixels,
  pub line_height: Pixels,
  pub bounds: Bounds<Pixels>,
  pub rows: usize,
  pub cols: usize,
}

impl TerminalBounds {
  pub fn new(
    cell_width: Pixels,
    line_height: Pixels,
    bounds: Bounds<Pixels>,
    rows: usize,
    cols: usize,
  ) -> Self {
    Self {
      cell_width,
      line_height,
      bounds,
      rows,
      cols,
    }
  }

  /// 计算行数
  pub fn num_lines(&self) -> usize {
    self.rows
  }

  /// 计算列数
  pub fn num_columns(&self) -> usize {
    self.cols
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
  pub selection: Option<SelectionRange>,
  pub cursor: CursorState,
  pub cursor_char: char,
  pub terminal_bounds: TerminalBounds,
  pub scrolled_to_top: bool,
  pub scrolled_to_bottom: bool,
  pub title: String,
}

impl TerminalContent {
  /// 创建空的终端内容
  pub fn new() -> Self {
    Self {
      cells: Vec::new(),
      mode: TermMode::default(),
      display_offset: 0,
      selection: None,
      cursor: CursorState::default(),
      cursor_char: ' ',
      terminal_bounds: TerminalBounds::new(px(8.), px(16.), Bounds::default(), 24, 80),
      scrolled_to_top: true,
      scrolled_to_bottom: true,
      title: "Terminal".to_string(),
    }
  }

  /// 更新终端内容
  pub fn update_from_cells(
    &mut self,
    cells: Vec<IndexedCell>,
    cursor: CursorState,
    cursor_char: char,
  ) {
    self.cells = cells;
    self.cursor = cursor;
    self.cursor_char = cursor_char;
  }

  /// 设置终端标题
  pub fn set_title(&mut self, title: String) {
    self.title = title;
  }

  /// 设置边界
  pub fn set_bounds(&mut self, bounds: TerminalBounds) {
    self.terminal_bounds = bounds;
  }
}

impl Default for TerminalContent {
  fn default() -> Self {
    Self::new()
  }
}

impl EventEmitter<TerminalEvent> for TerminalContent {}

/// 将 alacritty 的 RenderableCursor 转换为 CursorState
pub fn renderable_cursor_to_state(cursor: &RenderableCursor) -> CursorState {
  CursorState {
    point: TerminalPoint {
      line: cursor.point.line,
      column: cursor.point.column,
    },
    shape: cursor.shape,
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
