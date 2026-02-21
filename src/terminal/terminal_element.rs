use crate::terminal::content::{ansi_color_to_rgb, rgb_to_hsla, TerminalContent};
use crate::terminal::terminal::Terminal;
use alacritty_terminal::term::cell::Flags;
use gpui::*;
use std::mem;

/// 终端元素布局状态
pub struct LayoutState {
  bounds: Bounds<Pixels>,
  content: TerminalContent,
  char_width: Pixels,
  char_height: Pixels,
  background_color: Hsla,
  cursor_visible: bool,
}

/// 批处理的文本运行（类似 Zed 的 BatchedTextRun）
#[derive(Debug)]
pub struct BatchedTextRun {
  pub start_row: usize,
  pub start_col: usize,
  pub text: String,
  pub cell_count: usize,
  pub fg: [u8; 3],
  pub bg: [u8; 3],
  pub bold: bool,
}

impl BatchedTextRun {
  fn new(start_row: usize, start_col: usize, fg: [u8; 3], bg: [u8; 3], bold: bool) -> Self {
    Self {
      start_row,
      start_col,
      text: String::with_capacity(100),
      cell_count: 0,
      fg,
      bg,
      bold,
    }
  }

  fn can_append(&self, fg: [u8; 3], bg: [u8; 3], bold: bool) -> bool {
    self.fg == fg && self.bg == bg && self.bold == bold
  }

  fn append_char(&mut self, c: char) {
    self.text.push(c);
    self.cell_count += 1;
  }
}

/// 自定义 Terminal Element，使用 paint 方式渲染终端内容
pub struct TerminalElement {
  terminal: Entity<Terminal>,
  content: TerminalContent,
  char_width: Pixels,
  char_height: Pixels,
  focus_handle: FocusHandle,
}

impl TerminalElement {
  /// 创建新的 TerminalElement
  pub fn new(terminal: Entity<Terminal>, focus_handle: FocusHandle) -> Self {
    // 初始化时使用空内容，prepaint 时会从 Terminal 读取
    let initial_content = TerminalContent::new();

    Self {
      terminal,
      content: initial_content,
      char_width: px(8.),
      char_height: px(16.),
      focus_handle,
    }
  }

  /// 创建终端字体
  fn create_font() -> Font {
    Font {
      family: "Monaco".into(),
      features: FontFeatures::default(),
      fallbacks: None,
      weight: FontWeight::NORMAL,
      style: FontStyle::Normal,
    }
  }

  /// 计算并更新字符尺寸
  fn calculate_char_dimensions(&mut self, window: &mut Window) {
    let font = Self::create_font();
    let font_id = window.text_system().resolve_font(&font);
    if let Ok(advance) = window.text_system().advance(font_id, px(14.), 'm') {
      self.char_width = advance.width;
    }
    // 行高通常是字体大小的 1.2 倍左右
    self.char_height = px(14. * 1.2);
  }

  /// 创建文本运行
  fn create_text_run(len: usize, font: &Font, color: Hsla, bold: bool) -> TextRun {
    TextRun {
      len,
      font: Font {
        weight: if bold {
          FontWeight::BOLD
        } else {
          FontWeight::NORMAL
        },
        ..font.clone()
      },
      color,
      background_color: None,
      underline: None,
      strikethrough: None,
    }
  }

  /// 绘制单元格背景
  fn paint_cell_background(
    window: &mut Window,
    origin: Point<Pixels>,
    row: usize,
    col: usize,
    bg: [u8; 3],
    char_width: Pixels,
    char_height: Pixels,
  ) {
    if bg == [30, 30, 30] {
      return;
    }
    let bg_color = rgb_to_hsla(bg);
    let bg_bounds = Bounds {
      origin: Point::new(
        origin.x + col as f32 * char_width,
        origin.y + row as f32 * char_height,
      ),
      size: Size::new(char_width, char_height),
    };
    window.paint_quad(fill(bg_bounds, bg_color));
  }

  /// 绘制光标
  fn paint_cursor(
    window: &mut Window,
    origin: Point<Pixels>,
    cursor_row: usize,
    cursor_col: usize,
    cursor_char: char,
    font: &Font,
    font_size: Pixels,
    char_width: Pixels,
    char_height: Pixels,
    cx: &mut App,
  ) {
    let cursor_x = origin.x + cursor_col as f32 * char_width;
    let cursor_y = origin.y + cursor_row as f32 * char_height;

    let cursor_bounds = Bounds {
      origin: Point::new(cursor_x, cursor_y),
      size: Size::new(char_width, char_height),
    };

    // 绘制光标背景
    window.paint_quad(fill(cursor_bounds, gpui::rgba(0x80ffffff)));

    // 绘制光标处的字符（反色）
    let cursor_run = Self::create_text_run(
      cursor_char.len_utf8(),
      font,
      gpui::rgb(0x000000).into(),
      false,
    );

    let _ = window
      .text_system()
      .shape_line(
        cursor_char.to_string().into(),
        font_size,
        &[cursor_run],
        Some(char_width),
      )
      .paint(Point::new(cursor_x, cursor_y), char_height, window, cx);
  }

  /// 布局网格 - 将单元格批处理（类似 Zed 的 layout_grid）
  fn layout_grid(content: &TerminalContent) -> Vec<BatchedTextRun> {
    let mut batched_runs: Vec<BatchedTextRun> = Vec::new();
    let mut current_batch: Option<BatchedTextRun> = None;

    let mut last_row: usize = 0;
    let mut last_col: usize = 0;

    for indexed in &content.cells {
      let row = indexed.point.line.0 as usize;
      let col = indexed.point.column.0 as usize;
      let cell = &indexed.cell;

      // 跳过宽字符的 spacer
      if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
        continue;
      }

      let mut fg = ansi_color_to_rgb(&cell.fg);
      let mut bg = ansi_color_to_rgb(&cell.bg);

      // 处理反色（inverse）标志
      if cell.flags.contains(Flags::INVERSE) {
        mem::swap(&mut fg, &mut bg);
      }

      let bold = cell.flags.intersects(Flags::BOLD);
      let c = cell.c;

      // 跳过空白字符但保留背景
      if c == ' '
        && bg == [30, 30, 30]
        && !cell.flags.intersects(Flags::UNDERLINE | Flags::STRIKEOUT)
      {
        if let Some(batch) = current_batch.take() {
          batched_runs.push(batch);
        }
        last_row = row;
        last_col = col;
        continue;
      }

      // 检查是否可以追加到当前批次
      let can_append = if let Some(ref batch) = current_batch {
        batch.can_append(fg, bg, bold) && row == last_row && col == last_col + 1
      } else {
        false
      };

      if can_append {
        if let Some(ref mut batch) = current_batch {
          batch.append_char(c);
        }
      } else {
        // 保存当前批次
        if let Some(batch) = current_batch.take() {
          batched_runs.push(batch);
        }
        // 创建新批次
        let mut new_batch = BatchedTextRun::new(row as usize, col as usize, fg, bg, bold);
        new_batch.append_char(c);
        current_batch = Some(new_batch);
      }

      last_row = row;
      last_col = col;
    }

    // 保存最后一个批次
    if let Some(batch) = current_batch {
      batched_runs.push(batch);
    }

    batched_runs
  }
}

impl Element for TerminalElement {
  type RequestLayoutState = ();
  type PrepaintState = LayoutState;

  fn id(&self) -> Option<ElementId> {
    Some(ElementId::Name("terminal-element".into()))
  }

  fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
    None
  }

  fn request_layout(
    &mut self,
    _global_id: Option<&GlobalElementId>,
    _inspector_id: Option<&InspectorElementId>,
    window: &mut Window,
    _cx: &mut App,
  ) -> (LayoutId, Self::RequestLayoutState) {
    let mut style = Style::default();
    style.size.width = relative(1.).into();
    style.size.height = relative(1.).into();

    let layout_id = window.request_layout(style, None, _cx);
    (layout_id, ())
  }

  fn prepaint(
    &mut self,
    _global_id: Option<&GlobalElementId>,
    _inspector_id: Option<&InspectorElementId>,
    bounds: Bounds<Pixels>,
    _request_layout: &mut Self::RequestLayoutState,
    window: &mut Window,
    cx: &mut App,
  ) -> Self::PrepaintState {
    self.calculate_char_dimensions(window);

    // 从 Terminal 实体获取最新内容
    let content = self.terminal.read(cx).content().clone();
    self.content = content.clone();

    LayoutState {
      bounds,
      content,
      char_width: self.char_width,
      char_height: self.char_height,
      background_color: gpui::rgb(0x1e1e1e).into(),
      cursor_visible: true,
    }
  }

  fn paint(
    &mut self,
    _global_id: Option<&GlobalElementId>,
    _inspector_id: Option<&InspectorElementId>,
    _bounds: Bounds<Pixels>,
    _request_layout: &mut Self::RequestLayoutState,
    layout: &mut Self::PrepaintState,
    window: &mut Window,
    cx: &mut App,
  ) {
    let origin = layout.bounds.origin;
    let content = &layout.content;
    let char_width = layout.char_width;
    let char_height = layout.char_height;

    // 绘制背景
    window.paint_quad(fill(layout.bounds, layout.background_color));

    // 准备字体
    let font_size = px(14.);
    let font = Self::create_font();

    // 先绘制所有单元格背景
    for indexed in &content.cells {
      let row = indexed.point.line.0 as usize;
      let col = indexed.point.column.0 as usize;
      let cell = &indexed.cell;

      let mut bg = ansi_color_to_rgb(&cell.bg);

      // 处理反色（inverse）标志
      if cell.flags.contains(Flags::INVERSE) {
        bg = ansi_color_to_rgb(&cell.fg);
      }

      Self::paint_cell_background(window, origin, row, col, bg, char_width, char_height);
    }

    // 批处理绘制文本
    let batched_runs = Self::layout_grid(content);

    for batch in &batched_runs {
      if batch.text.is_empty() {
        continue;
      }

      let fg_color = rgb_to_hsla(batch.fg);
      let pos = Point::new(
        origin.x + batch.start_col as f32 * char_width,
        origin.y + batch.start_row as f32 * char_height,
      );

      let text_run = Self::create_text_run(batch.text.len(), &font, fg_color, batch.bold);

      let _ = window
        .text_system()
        .shape_line(
          batch.text.clone().into(),
          font_size,
          &[text_run],
          Some(char_width),
        )
        .paint(pos, char_height, window, cx);
    }

    // 绘制光标
    let cursor = &content.cursor;
    let cursor_row = cursor.point.line.0 as usize;
    let cursor_col = cursor.point.column.0 as usize;

    // 检查光标是否可见（根据光标形状）
    let cursor_visible = layout.cursor_visible
      && !matches!(
        cursor.shape,
        alacritty_terminal::vte::ansi::CursorShape::Hidden
      );

    if cursor_visible {
      Self::paint_cursor(
        window,
        origin,
        cursor_row,
        cursor_col,
        content.cursor_char,
        &font,
        font_size,
        char_width,
        char_height,
        cx,
      );
    }
  }
}

impl IntoElement for TerminalElement {
  type Element = Self;

  fn into_element(self) -> Self::Element {
    self
  }
}
