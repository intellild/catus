use crate::terminal::provider::{RenderableContentStatic, TerminalProvider};
use gpui::*;

/// 终端元素布局状态
pub struct LayoutState {
  bounds: Bounds<Pixels>,
  content: RenderableContentStatic,
  char_width: Pixels,
  char_height: Pixels,
  background_color: Hsla,
  cursor_visible: bool,
}

/// 自定义 Terminal Element，使用 paint 方式渲染终端内容
pub struct TerminalElement {
  provider: Entity<TerminalProvider>,
  content: RenderableContentStatic,
  char_width: Pixels,
  char_height: Pixels,
  focus_handle: FocusHandle,
}

impl TerminalElement {
  /// 创建新的 TerminalElement
  pub fn new(
    provider: Entity<TerminalProvider>,
    content: RenderableContentStatic,
    focus_handle: FocusHandle,
  ) -> Self {
    Self {
      provider,
      content,
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

  /// 将 RGB 数组转换为 Hsla
  fn rgb_to_hsla(rgb: [u8; 3]) -> Hsla {
    gpui::rgb((rgb[0] as u32) << 16 | (rgb[1] as u32) << 8 | rgb[2] as u32).into()
  }

  /// 创建文本运行
  fn create_text_run(len: usize, font: &Font, color: Hsla) -> TextRun {
    TextRun {
      len,
      font: font.clone(),
      color,
      background_color: None,
      underline: None,
      strikethrough: None,
    }
  }

  /// 绘制文本
  fn paint_text(
    window: &mut Window,
    text: impl Into<SharedString>,
    font_size: Pixels,
    font: &Font,
    fg: [u8; 3],
    char_width: Pixels,
    pos: Point<Pixels>,
    char_height: Pixels,
    cx: &mut App,
  ) {
    let shared_text: SharedString = text.into();
    let fg_color = Self::rgb_to_hsla(fg);
    let text_run = Self::create_text_run(shared_text.len(), font, fg_color);

    let _ = window
      .text_system()
      .shape_line(shared_text, font_size, &[text_run], Some(char_width))
      .paint(pos, char_height, window, cx);
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
    let bg_color = Self::rgb_to_hsla(bg);
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
    let cursor_run =
      Self::create_text_run(cursor_char.len_utf8(), font, gpui::rgb(0x000000).into());

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

  /// 刷新批量绘制的文本
  fn flush_text_batch(
    window: &mut Window,
    current_text: &str,
    current_fg: [u8; 3],
    font: &Font,
    font_size: Pixels,
    char_width: Pixels,
    current_pos: Point<Pixels>,
    char_height: Pixels,
    cx: &mut App,
  ) {
    if current_text.is_empty() {
      return;
    }
    Self::paint_text(
      window,
      current_text.to_string(),
      font_size,
      font,
      current_fg,
      char_width,
      current_pos,
      char_height,
      cx,
    );
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

    // 从 provider 获取最新内容
    let content = self.provider.read(cx).get_update().content.clone();
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

    // 批量绘制相同颜色的文本以提高性能
    let mut current_text = String::new();
    let mut current_pos = Point::default();
    let mut current_fg = [212u8, 212, 212];
    let mut last_row = 0usize;
    let mut last_col = 0usize;

    for (row, col, c, fg, bg, _bold) in &content.cells {
      let row = *row;
      let col = *col;
      let fg = *fg;

      // 绘制单元格背景
      Self::paint_cell_background(window, origin, row, col, *bg, char_width, char_height);

      // 如果颜色变化或位置不连续，先绘制之前的文本
      if fg != current_fg
        || row != last_row
        || (row == last_row && col != last_col + 1 && !current_text.is_empty())
      {
        Self::flush_text_batch(
          window,
          &current_text,
          current_fg,
          &font,
          font_size,
          char_width,
          current_pos,
          char_height,
          cx,
        );

        current_text.clear();
        current_fg = fg;
        current_pos = Point::new(
          origin.x + col as f32 * char_width,
          origin.y + row as f32 * char_height,
        );
      }

      current_text.push(*c);
      last_row = row;
      last_col = col;
    }

    // 绘制剩余的文本
    Self::flush_text_batch(
      window,
      &current_text,
      current_fg,
      &font,
      font_size,
      char_width,
      current_pos,
      char_height,
      cx,
    );

    // 绘制光标
    if content.cursor_visible && layout.cursor_visible {
      // 查找光标位置的字符
      let cursor_char = content
        .cells
        .iter()
        .find(|(r, c, _, _, _, _)| *r == content.cursor_row && *c == content.cursor_col)
        .map(|(_, _, c, _, _, _)| *c)
        .unwrap_or(' ');

      Self::paint_cursor(
        window,
        origin,
        content.cursor_row,
        content.cursor_col,
        cursor_char,
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
