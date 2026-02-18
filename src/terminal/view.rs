use crate::terminal::content::TerminalContent;
use crate::terminal::terminal::Terminal;
use crate::terminal::terminal_element::TerminalElement;
use gpui::*;
use std::sync::Arc;

/// Terminal view component using GPUI
pub struct TerminalView {
  terminal: Arc<Terminal>,
  focus_handle: FocusHandle,
  _content_observer: Subscription,
  last_content: TerminalContent,
}

impl TerminalView {
  /// 创建新的 TerminalView，使用已存在的 Terminal Entity
  pub fn new(terminal: Arc<Terminal>, cx: &mut Context<Self>) -> Self {
    // 获取内容实体
    let content_entity = terminal.content.clone();

    // 获取初始内容
    let initial_content = terminal.current_content(cx);

    // 观察 TerminalContent 变化
    let content_observer = cx.observe(&content_entity, |this, _content, cx| {
      // 内容变化时更新本地缓存并重绘
      this.sync(cx);
      cx.notify();
    });

    Self {
      terminal,
      focus_handle: cx.focus_handle(),
      _content_observer: content_observer,
      last_content: initial_content,
    }
  }

  /// 处理按键事件
  fn handle_key_down(
    &mut self,
    event: &KeyDownEvent,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let keystroke = event.keystroke.clone();

    let data = encode_keystroke(&keystroke);
    self.terminal.input(data);

    cx.notify();
  }

  /// 同步终端状态
  pub fn sync(&mut self, cx: &mut Context<Self>) {
    self.terminal.sync();

    // 更新本地缓存
    let content = self.terminal.current_content(cx);
    self.last_content = content;
  }

  /// 获取关联的 Terminal Entity
  pub fn terminal(&self) -> &Terminal {
    &self.terminal
  }
}

impl Render for TerminalView {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    // 获取当前内容
    let content = self.terminal.current_content(cx);
    self.last_content = content.clone();

    // 获取内容实体
    let content_entity = self.terminal.content.clone();

    div()
      .id("terminal-view")
      .size_full()
      .bg(gpui::rgb(0x1e1e1e))
      .cursor_text()
      .child(TerminalElement::new(
        content_entity,
        content,
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
