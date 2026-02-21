use crate::terminal::terminal::Terminal;
use crate::terminal::terminal_element::TerminalElement;
use gpui::*;

/// Terminal view component using GPUI
pub struct TerminalView {
  terminal: Entity<Terminal>,
  focus_handle: FocusHandle,
  _terminal_observer: Subscription,
}

impl TerminalView {
  /// 创建新的 TerminalView，使用已存在的 Terminal Entity
  pub fn new(terminal: Entity<Terminal>, cx: &mut Context<Self>) -> Self {
    // 观察 Terminal 实体变化（包含 content 更新）
    let terminal_observer = cx.observe(&terminal, |this, _terminal, cx| {
      // Terminal 内容变化时同步并重绘
      this.sync(cx);
      cx.notify();
    });

    Self {
      terminal,
      focus_handle: cx.focus_handle(),
      _terminal_observer: terminal_observer,
    }
  }

  /// 处理按键事件
  fn handle_key_down(
    &mut self,
    event: &KeyDownEvent,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let data = encode_keystroke(&event.keystroke);
    self.terminal.update(cx, |terminal, _cx| {
      let _ = terminal.input(data);
    });
  }

  /// 处理粘贴事件
  fn handle_paste(&mut self, text: &str, cx: &mut Context<Self>) {
    self.terminal.update(cx, |terminal, _cx| {
      terminal.paste(text);
    });
  }

  /// 同步终端状态
  pub fn sync(&mut self, cx: &mut Context<Self>) {
    // 更新终端状态
    self.terminal.update(cx, |terminal, _cx| {
      terminal.sync(_cx);
    });

    cx.notify();
  }

  /// 获取关联的 Terminal Entity
  pub fn terminal(&self) -> &Entity<Terminal> {
    &self.terminal
  }

  /// 向上滚动一行
  pub fn scroll_line_up(&mut self, cx: &mut Context<Self>) {
    self.terminal.update(cx, |terminal, _cx| {
      terminal.scroll_line_up();
    });
  }

  /// 向下滚动一行
  pub fn scroll_line_down(&mut self, cx: &mut Context<Self>) {
    self.terminal.update(cx, |terminal, _cx| {
      terminal.scroll_line_down();
    });
  }

  /// 向上滚动一页
  pub fn scroll_page_up(&mut self, cx: &mut Context<Self>) {
    self.terminal.update(cx, |terminal, _cx| {
      terminal.scroll_page_up();
    });
  }

  /// 向下滚动一页
  pub fn scroll_page_down(&mut self, cx: &mut Context<Self>) {
    self.terminal.update(cx, |terminal, _cx| {
      terminal.scroll_page_down();
    });
  }

  /// 滚动到顶部
  pub fn scroll_to_top(&mut self, cx: &mut Context<Self>) {
    self.terminal.update(cx, |terminal, _cx| {
      terminal.scroll_to_top();
    });
  }

  /// 滚动到底部
  pub fn scroll_to_bottom(&mut self, cx: &mut Context<Self>) {
    self.terminal.update(cx, |terminal, _cx| {
      terminal.scroll_to_bottom();
    });
  }

  /// 清除屏幕
  pub fn clear(&mut self, cx: &mut Context<Self>) {
    self.terminal.update(cx, |terminal, _cx| {
      terminal.clear();
    });
  }

  /// 复制选区
  pub fn copy(&mut self, cx: &mut Context<Self>) {
    self.terminal.update(cx, |terminal, _cx| {
      terminal.copy();
    });
  }
}

impl Render for TerminalView {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    // 获取当前内容（直接从 Terminal 读取）
    let content = self.terminal.read(cx).content().clone();

    // 创建一个临时内容实体，用于 TerminalElement
    // 由于 TerminalElement 期望 Entity<TerminalContent>，我们需要创建一个
    let content_entity = cx.new(|_cx| content);

    div()
      .id("terminal-view")
      .size_full()
      .bg(gpui::rgb(0x1e1e1e))
      .cursor_text()
      .child(TerminalElement::new(
        content_entity,
        self.focus_handle.clone(),
      ))
      .on_key_down(cx.listener(|this, event, window, cx| {
        this.handle_key_down(event, window, cx);
      }))
      .track_focus(&self.focus_handle)
  }
}

impl Focusable for TerminalView {
  fn focus_handle(&self, _cx: &App) -> FocusHandle {
    self.focus_handle.clone()
  }
}

/// 将 GPUI Keystroke 编码为字节序列
fn encode_keystroke(keystroke: &Keystroke) -> Vec<u8> {
  let key = keystroke.key.as_str();
  let modifiers = &keystroke.modifiers;

  // 处理 Ctrl 修饰符
  if modifiers.control && key.len() == 1 {
    let ch = key.chars().next().unwrap_or('\0');
    if ch.is_ascii_alphabetic() {
      return vec![ch.to_ascii_lowercase() as u8 - b'a' + 1]; // Ctrl+A = 0x01
    }
  }

  // 功能键和特殊键使用 key 字段
  match key {
    "enter" | "return" => return vec![b'\r'],
    "escape" | "esc" => return vec![0x1b],
    "tab" => return vec![b'\t'],
    "backspace" => return vec![0x7f],
    "delete" | "del" => return vec![0x1b, b'[', b'3', b'~'],
    "insert" | "ins" => return vec![0x1b, b'[', b'2', b'~'],
    "up" => return vec![0x1b, b'[', b'A'],
    "down" => return vec![0x1b, b'[', b'B'],
    "right" => return vec![0x1b, b'[', b'C'],
    "left" => return vec![0x1b, b'[', b'D'],
    "home" => return vec![0x1b, b'[', b'H'],
    "end" => return vec![0x1b, b'[', b'F'],
    "pageup" | "page up" => return vec![0x1b, b'[', b'5', b'~'],
    "pagedown" | "page down" => return vec![0x1b, b'[', b'6', b'~'],
    "f1" => return vec![0x1b, b'O', b'P'],
    "f2" => return vec![0x1b, b'O', b'Q'],
    "f3" => return vec![0x1b, b'O', b'R'],
    "f4" => return vec![0x1b, b'O', b'S'],
    "f5" => return vec![0x1b, b'[', b'1', b'5', b'~'],
    "f6" => return vec![0x1b, b'[', b'1', b'7', b'~'],
    "f7" => return vec![0x1b, b'[', b'1', b'8', b'~'],
    "f8" => return vec![0x1b, b'[', b'1', b'9', b'~'],
    "f9" => return vec![0x1b, b'[', b'2', b'0', b'~'],
    "f10" => return vec![0x1b, b'[', b'2', b'1', b'~'],
    "f11" => return vec![0x1b, b'[', b'2', b'3', b'~'],
    "f12" => return vec![0x1b, b'[', b'2', b'4', b'~'],
    "space" => return vec![b' '],
    _ => {}
  }

  // 对于普通字符输入，优先使用 key_char（包含实际输入的字符，处理了 IME 和 shift 等）
  if let Some(key_char) = &keystroke.key_char {
    if !key_char.is_empty() {
      return key_char.as_bytes().to_vec();
    }
  }

  // 回退到 key 字段
  if key.len() == 1 {
    return key.as_bytes().to_vec();
  }

  vec![]
}
