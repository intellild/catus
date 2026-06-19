use crate::terminal::content::{TerminalContent, ansi_color_to_rgb, rgb_to_hsla};
use crate::terminal::model::Terminal;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};
use gpui::*;
use gpui_component::ActiveTheme;
use std::mem;

/// 终端元素布局状态
pub struct LayoutState {
  bounds: Bounds<Pixels>,
  content: TerminalContent,
  char_width: Pixels,
  char_height: Pixels,
  background_color: Hsla,
  cursor_visible: bool,
  hitbox: Hitbox,
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

/// 终端文本绘制布局参数
struct PaintLayout<'a> {
  font: &'a Font,
  font_size: Pixels,
  char_width: Pixels,
  char_height: Pixels,
}

/// 终端渲染元素
///
/// 数据流的「消费」侧：prepaint 阶段调用 Terminal::refresh_content()
/// 从 alacritty Term 提取最新内容，然后 paint 阶段绘制。
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

  /// 计算字符尺寸。
  ///
  /// `TerminalElement` 每帧由 `TerminalView::render` 重新创建，
  /// 因此这里每帧都会执行一次计算。字体 advance 查询开销很小，可以接受。
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

  /// 判断单元格背景是否为终端默认背景色
  fn is_default_bg(color: &AnsiColor) -> bool {
    matches!(color, AnsiColor::Named(NamedColor::Background))
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
    layout: &PaintLayout,
    cx: &mut App,
  ) {
    let cursor_x = origin.x + cursor_col as f32 * layout.char_width;
    let cursor_y = origin.y + cursor_row as f32 * layout.char_height;

    let cursor_bounds = Bounds {
      origin: Point::new(cursor_x, cursor_y),
      size: Size::new(layout.char_width, layout.char_height),
    };

    // 绘制光标背景
    window.paint_quad(fill(cursor_bounds, gpui::rgba(0x80ffffff)));

    // 绘制光标处的字符（反色）
    let cursor_run = Self::create_text_run(
      cursor_char.len_utf8(),
      layout.font,
      gpui::rgb(0x000000).into(),
      false,
    );

    let _ = window
      .text_system()
      .shape_line(
        cursor_char.to_string().into(),
        layout.font_size,
        &[cursor_run],
        Some(layout.char_width),
      )
      .paint(
        Point::new(cursor_x, cursor_y),
        layout.char_height,
        window,
        cx,
      );
  }

  /// 布局网格 - 将单元格批处理（类似 Zed 的 layout_grid）
  fn layout_grid(content: &TerminalContent) -> Vec<BatchedTextRun> {
    let mut batched_runs: Vec<BatchedTextRun> = Vec::new();
    let mut current_batch: Option<BatchedTextRun> = None;

    let mut last_row: usize = 0;
    let mut last_col: usize = 0;

    for indexed in &content.cells {
      let row = indexed.point.line.0 as usize;
      let col = indexed.point.column.0;
      let cell = &indexed.cell;

      // 跳过宽字符的 spacer
      if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
        continue;
      }

      let mut fg = ansi_color_to_rgb(&cell.fg);
      let mut bg = ansi_color_to_rgb(&cell.bg);

      // 处理暗淡（dim）标志
      if cell.flags.contains(Flags::DIM) && !cell.flags.contains(Flags::BOLD) {
        fg = [fg[0] / 2, fg[1] / 2, fg[2] / 2];
      }

      // 处理反色（inverse）标志
      if cell.flags.contains(Flags::INVERSE) {
        mem::swap(&mut fg, &mut bg);
      }

      let bold = cell.flags.intersects(Flags::BOLD);
      let c = cell.c;

      // 跳过空白字符但保留背景
      if c == ' '
        && Self::is_default_bg(&cell.bg)
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
        let mut new_batch = BatchedTextRun::new(row, col, fg, bg, bold);
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

    self.terminal.update(cx, |terminal, cx| {
      terminal.sync_size(bounds, self.char_width, self.char_height, cx);
      terminal.refresh_content(cx);
    });
    let content = self.terminal.read(cx).content().clone();
    self.content = content.clone();

    let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);

    LayoutState {
      bounds,
      content,
      char_width: self.char_width,
      char_height: self.char_height,
      background_color: cx.theme().background,
      cursor_visible: true,
      hitbox,
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
      let col = indexed.point.column.0;
      let cell = &indexed.cell;

      let mut fg = ansi_color_to_rgb(&cell.fg);
      let mut bg = ansi_color_to_rgb(&cell.bg);

      // 处理暗淡（dim）标志
      if cell.flags.contains(Flags::DIM) && !cell.flags.contains(Flags::BOLD) {
        fg = [fg[0] / 2, fg[1] / 2, fg[2] / 2];
      }

      // 处理反色（inverse）标志
      if cell.flags.contains(Flags::INVERSE) {
        bg = fg;
      }

      // 默认背景不需要单独绘制（已由整体背景覆盖）
      if !Self::is_default_bg(&cell.bg) {
        Self::paint_cell_background(window, origin, row, col, bg, char_width, char_height);
      }
    }

    // 绘制选择高亮背景
    if let Some(selection) = content.selection {
      for indexed in &content.cells {
        if selection.contains(indexed.point) {
          let row = indexed.point.line.0 as usize;
          let col = indexed.point.column.0;
          let selection_bounds = Bounds {
            origin: Point::new(
              origin.x + col as f32 * char_width,
              origin.y + row as f32 * char_height,
            ),
            size: Size::new(char_width, char_height),
          };
          window.paint_quad(fill(selection_bounds, gpui::rgba(0x3b82f680)));
        }
      }
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
    let cursor_col = cursor.point.column.0;

    // 检查光标是否可见（根据光标形状）
    let cursor_visible = layout.cursor_visible
      && !matches!(
        cursor.shape,
        alacritty_terminal::vte::ansi::CursorShape::Hidden
      );

    if cursor_visible {
      let text_layout = PaintLayout {
        font: &font,
        font_size,
        char_width,
        char_height,
      };
      Self::paint_cursor(
        window,
        origin,
        cursor_row,
        cursor_col,
        content.cursor_char,
        &text_layout,
        cx,
      );
    }

    // 鼠标事件处理
    let terminal = self.terminal.clone();
    let bounds = layout.bounds;
    let char_width = layout.char_width;
    let char_height = layout.char_height;
    let hitbox = layout.hitbox.clone();
    let hitbox_for_move = hitbox.clone();

    // 鼠标按下：开始选择
    let focus_handle = self.focus_handle.clone();
    window.on_mouse_event({
      let terminal = terminal.clone();
      move |event: &MouseDownEvent, phase, window, cx| {
        if phase.bubble() && event.button == MouseButton::Left && hitbox.is_hovered(window) {
          window.focus(&focus_handle);
          let point =
            terminal
              .read(cx)
              .point_from_pixel(event.position, bounds, char_width, char_height);
          terminal.update(cx, |terminal, cx| {
            if event.click_count == 2 {
              terminal.select_word_at(point, cx);
            } else {
              terminal.set_selection_start(point, cx);
            }
          });
          cx.stop_propagation();
        }
      }
    });

    // 鼠标移动：更新选择（仅在终端区域内拖动时）
    window.on_mouse_event({
      let terminal = terminal.clone();
      move |event: &MouseMoveEvent, phase, window, cx| {
        if phase.bubble()
          && event.pressed_button == Some(MouseButton::Left)
          && hitbox_for_move.is_hovered(window)
        {
          let point =
            terminal
              .read(cx)
              .point_from_pixel(event.position, bounds, char_width, char_height);
          terminal.update(cx, |terminal, cx| {
            terminal.set_selection_end(point, cx);
          });
          cx.stop_propagation();
        }
      }
    });

    // 鼠标释放：完成选择
    window.on_mouse_event({
      let _terminal = terminal.clone();
      move |event: &MouseUpEvent, phase, _window, cx| {
        if phase.bubble() && event.button == MouseButton::Left {
          cx.stop_propagation();
        }
      }
    });
  }
}

impl IntoElement for TerminalElement {
  type Element = Self;

  fn into_element(self) -> Self::Element {
    self
  }
}

#[cfg(test)]
mod tests {
  use super::{BatchedTextRun, TerminalElement};
  use crate::terminal::content::{IndexedCell, TerminalContent, TerminalPoint};
  use alacritty_terminal::index::{Column, Line};
  use alacritty_terminal::term::cell::{Cell, Flags};
  use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};

  fn point(line: i32, col: usize) -> TerminalPoint {
    TerminalPoint {
      line: Line(line),
      column: Column(col),
    }
  }

  /// 构造单元格：默认前景/背景，无 flags。
  fn cell(c: char) -> Cell {
    Cell {
      c,
      ..Default::default()
    }
  }

  fn cell_with_colors(c: char, fg: AnsiColor, bg: AnsiColor) -> Cell {
    Cell {
      c,
      fg,
      bg,
      ..Default::default()
    }
  }

  fn cell_with_flags(c: char, flags: Flags) -> Cell {
    Cell {
      c,
      flags,
      ..Default::default()
    }
  }

  fn indexed(line: i32, col: usize, cell: Cell) -> IndexedCell {
    IndexedCell {
      point: point(line, col),
      cell,
    }
  }

  fn content(cells: Vec<IndexedCell>) -> TerminalContent {
    let mut c = TerminalContent::new();
    c.cells = cells;
    c
  }

  #[test]
  fn is_default_bg_recognizes_background() {
    assert!(TerminalElement::is_default_bg(&AnsiColor::Named(
      NamedColor::Background
    )));
    assert!(!TerminalElement::is_default_bg(&AnsiColor::Named(
      NamedColor::Foreground
    )));
    assert!(!TerminalElement::is_default_bg(&AnsiColor::Spec(
      alacritty_terminal::vte::ansi::Rgb { r: 0, g: 0, b: 0 }
    )));
  }

  #[test]
  fn layout_grid_merges_contiguous_same_style() {
    // 同行、同色、连续 3 个字符 → 一个 run
    let c = content(vec![
      indexed(0, 0, cell('a')),
      indexed(0, 1, cell('b')),
      indexed(0, 2, cell('c')),
    ]);
    let runs = TerminalElement::layout_grid(&c);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "abc");
    assert_eq!(runs[0].cell_count, 3);
    assert_eq!(runs[0].start_row, 0);
    assert_eq!(runs[0].start_col, 0);
  }

  #[test]
  fn layout_grid_breaks_on_new_row() {
    let c = content(vec![indexed(0, 0, cell('a')), indexed(1, 0, cell('b'))]);
    let runs = TerminalElement::layout_grid(&c);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, "a");
    assert_eq!(runs[1].text, "b");
    assert_eq!(runs[1].start_row, 1);
  }

  #[test]
  fn layout_grid_breaks_on_color_change() {
    let red = AnsiColor::Named(NamedColor::Red);
    let green = AnsiColor::Named(NamedColor::Green);
    let c = content(vec![
      indexed(
        0,
        0,
        cell_with_colors('a', red, AnsiColor::Named(NamedColor::Background)),
      ),
      indexed(
        0,
        1,
        cell_with_colors('b', green, AnsiColor::Named(NamedColor::Background)),
      ),
    ]);
    let runs = TerminalElement::layout_grid(&c);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, "a");
    assert_eq!(runs[1].text, "b");
    // 颜色不同不应合并
    assert_ne!(runs[0].fg, runs[1].fg);
  }

  #[test]
  fn layout_grid_breaks_on_gap() {
    // col 不连续（0 然后 2，跳过 1）
    let c = content(vec![indexed(0, 0, cell('a')), indexed(0, 2, cell('b'))]);
    let runs = TerminalElement::layout_grid(&c);
    assert_eq!(runs.len(), 2);
  }

  #[test]
  fn layout_grid_space_with_default_bg_breaks_run() {
    // 默认背景的空格会断开当前 run，但不产生新 run
    let c = content(vec![
      indexed(0, 0, cell('a')),
      indexed(0, 1, cell(' ')),
      indexed(0, 2, cell('b')),
    ]);
    let runs = TerminalElement::layout_grid(&c);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, "a");
    assert_eq!(runs[1].text, "b");
  }

  #[test]
  fn layout_grid_inverse_swaps_fg_bg() {
    let bg = AnsiColor::Named(NamedColor::Background);
    let c = content(vec![indexed(0, 0, cell_with_flags('a', Flags::INVERSE))]);
    // 默认 cell 的 fg=Foreground, bg=Background；inverse 后应交换
    let runs = TerminalElement::layout_grid(&c);
    assert_eq!(runs.len(), 1);
    // 交换后 fg 应为 Background 的 RGB，bg 应为 Foreground 的 RGB
    let expected_fg = crate::terminal::content::ansi_color_to_rgb(&bg);
    let expected_bg =
      crate::terminal::content::ansi_color_to_rgb(&AnsiColor::Named(NamedColor::Foreground));
    assert_eq!(runs[0].fg, expected_fg);
    assert_eq!(runs[0].bg, expected_bg);
  }

  #[test]
  fn layout_grid_dim_halves_fg() {
    let c = content(vec![indexed(0, 0, cell_with_flags('a', Flags::DIM))]);
    let runs = TerminalElement::layout_grid(&c);
    assert_eq!(runs.len(), 1);
    let full_fg =
      crate::terminal::content::ansi_color_to_rgb(&AnsiColor::Named(NamedColor::Foreground));
    assert_eq!(runs[0].fg, [full_fg[0] / 2, full_fg[1] / 2, full_fg[2] / 2]);
  }

  #[test]
  fn layout_grid_bold_sets_bold_flag() {
    let c = content(vec![indexed(0, 0, cell_with_flags('a', Flags::BOLD))]);
    let runs = TerminalElement::layout_grid(&c);
    assert_eq!(runs.len(), 1);
    assert!(runs[0].bold);
  }

  #[test]
  fn layout_grid_skips_wide_char_spacer() {
    // WIDE_CHAR_SPACER 单元格应被跳过
    let c = content(vec![
      indexed(0, 0, cell('a')),
      indexed(0, 1, cell_with_flags(' ', Flags::WIDE_CHAR_SPACER)),
      indexed(0, 2, cell('b')),
    ]);
    let runs = TerminalElement::layout_grid(&c);
    // 跳过 spacer 后，col 0 和 col 2 不连续 → 两个 run
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, "a");
    assert_eq!(runs[1].text, "b");
  }

  #[test]
  fn layout_grid_empty_content_returns_no_runs() {
    let c = content(vec![]);
    let runs = TerminalElement::layout_grid(&c);
    assert!(runs.is_empty());
  }

  #[test]
  fn batched_text_run_can_append_logic() {
    let mut run = BatchedTextRun::new(0, 0, [1, 2, 3], [4, 5, 6], false);
    assert!(run.can_append([1, 2, 3], [4, 5, 6], false));
    assert!(!run.can_append([9, 9, 9], [4, 5, 6], false));
    assert!(!run.can_append([1, 2, 3], [4, 5, 6], true));
    run.append_char('x');
    assert_eq!(run.text, "x");
    assert_eq!(run.cell_count, 1);
  }
}
