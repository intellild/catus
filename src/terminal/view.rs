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

  match key {
    // 单字符输入（字母、数字、符号）
    k if k.len() == 1 => {
      let ch = k.chars().next().unwrap_or('\0');

      // 处理 Ctrl 修饰符
      if modifiers.control && ch.is_ascii_alphabetic() {
        let byte = ch.to_ascii_lowercase() as u8;
        vec![byte - b'a' + 1] // Ctrl+A = 0x01
      } else {
        k.as_bytes().to_vec()
      }
    }

    // 功能键
    "enter" | "return" => vec![b'\r'],
    "escape" | "esc" => vec![0x1b],
    "tab" => vec![b'\t'],
    "backspace" => vec![0x08],
    "delete" | "del" => vec![0x1b, b'[', b'3', b'~'],
    "insert" | "ins" => vec![0x1b, b'[', b'2', b'~'],

    // 方向键
    "up" => vec![0x1b, b'[', b'A'],
    "down" => vec![0x1b, b'[', b'B'],
    "right" => vec![0x1b, b'[', b'C'],
    "left" => vec![0x1b, b'[', b'D'],

    // Home/End
    "home" => vec![0x1b, b'[', b'H'],
    "end" => vec![0x1b, b'[', b'F'],

    // Page Up/Down
    "pageup" | "page up" => vec![0x1b, b'[', b'5', b'~'],
    "pagedown" | "page down" => vec![0x1b, b'[', b'6', b'~'],

    // 功能键 F1-F12
    "f1" => vec![0x1b, b'[', b'1', b'1', b'~'],
    "f2" => vec![0x1b, b'[', b'1', b'2', b'~'],
    "f3" => vec![0x1b, b'[', b'1', b'3', b'~'],
    "f4" => vec![0x1b, b'[', b'1', b'4', b'~'],
    "f5" => vec![0x1b, b'[', b'1', b'5', b'~'],
    "f6" => vec![0x1b, b'[', b'1', b'7', b'~'],
    "f7" => vec![0x1b, b'[', b'1', b'8', b'~'],
    "f8" => vec![0x1b, b'[', b'1', b'9', b'~'],
    "f9" => vec![0x1b, b'[', b'2', b'0', b'~'],
    "f10" => vec![0x1b, b'[', b'2', b'1', b'~'],
    "f11" => vec![0x1b, b'[', b'2', b'3', b'~'],
    "f12" => vec![0x1b, b'[', b'2', b'4', b'~'],

    // 其他未处理的键
    _ => vec![],
  }
}
