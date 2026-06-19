use crate::terminal::content::TerminalEvent;
use crate::terminal::model::Terminal;
use crate::terminal::terminal_element::TerminalElement;
use gpui::*;
use gpui_component::ActiveTheme;

actions!(
  terminal,
  [
    Tab,
    TabPrev,
    ScrollToBottom,
    CopySelection,
    PasteFromClipboard
  ]
);

/// `TerminalView` 向外发射的事件，供 `PaneGroup` / `Workspace` 订阅。
#[derive(Clone, Debug)]
pub enum TerminalViewEvent {
  /// 终端标题变更（OSC 序列）。
  TitleChanged,
  /// 子进程退出。
  Closed,
}

/// Terminal view component using GPUI
pub struct TerminalView {
  terminal: Entity<Terminal>,
  focus_handle: FocusHandle,
  closed: bool,
}

impl TerminalView {
  /// 创建新的 TerminalView，使用已存在的 Terminal Entity
  pub fn new(terminal: Entity<Terminal>, cx: &mut Context<Self>) -> Self {
    // Terminal notify → 重新渲染
    cx.observe(&terminal, |_, _, cx| {
      cx.notify();
    })
    .detach();

    // Terminal event → 转发为 TerminalViewEvent
    cx.subscribe(
      &terminal,
      |this, terminal, event: &TerminalEvent, cx| match event {
        TerminalEvent::TitleChanged => {
          cx.emit(TerminalViewEvent::TitleChanged);
        }
        TerminalEvent::Closed => {
          this.closed = terminal.read(cx).is_closed();
          cx.emit(TerminalViewEvent::Closed);
          cx.notify();
        }
        TerminalEvent::Bell => {}
      },
    )
    .detach();

    Self {
      terminal,
      focus_handle: cx.focus_handle(),
      closed: false,
    }
  }

  fn on_action_tab(&mut self, _: &Tab, _: &mut Window, cx: &mut Context<Self>) {
    self.terminal.update(cx, |terminal, cx| {
      terminal.input(cx, vec![b'\t']);
    });
  }

  fn on_action_tab_prev(&mut self, _: &TabPrev, _: &mut Window, cx: &mut Context<Self>) {
    self.terminal.update(cx, |terminal, cx| {
      terminal.input(cx, vec![0x1b, b'[', b'Z']);
    });
  }

  fn on_action_copy(&mut self, _: &CopySelection, _window: &mut Window, cx: &mut Context<Self>) {
    let text = self.terminal.read(cx).selected_text();
    if !text.is_empty() {
      cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
  }

  fn on_action_paste(
    &mut self,
    _: &PasteFromClipboard,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
      return;
    };

    self.terminal.update(cx, |terminal, cx| {
      terminal.paste(cx, text);
    });
  }

  fn handle_scroll_wheel(
    &mut self,
    event: &ScrollWheelEvent,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    use crate::terminal::constants::{SCROLL_LINES_PER_WHEEL_TICK, SCROLL_PIXEL_THRESHOLD};

    self.terminal.update(cx, |terminal, cx| match event.delta {
      ScrollDelta::Lines(lines) => {
        let delta = lines.y as i32 * SCROLL_LINES_PER_WHEEL_TICK;
        if delta != 0 {
          terminal.scroll_lines(delta, true, cx);
        }
      }
      ScrollDelta::Pixels(pixels) => {
        let delta: f32 = pixels.y.into();
        let ticks = (delta / SCROLL_PIXEL_THRESHOLD) as i32;
        if ticks != 0 {
          terminal.scroll_lines(ticks, true, cx);
        }
      }
    });
  }

  /// 处理按键事件
  fn handle_key_down(
    &mut self,
    event: &KeyDownEvent,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    if event.keystroke.modifiers.platform {
      return;
    }

    // Plain Tab 和 Shift+Tab 由 Tab/TabPrev action 处理，避免双重输入。
    // Ctrl+Tab / Ctrl+Shift+Tab 仍然走 encode_keystroke。
    if event.keystroke.key == "tab"
      && !event.keystroke.modifiers.control
      && !event.keystroke.modifiers.alt
    {
      return;
    }

    let data = encode_keystroke(&event.keystroke);
    if data.is_empty() {
      return;
    }

    self.terminal.update(cx, |terminal, cx| {
      terminal.input(cx, data);
    });
    cx.stop_propagation();
  }

  /// 获取终端标题
  pub fn title(&self, cx: &App) -> String {
    self.terminal.read(cx).title().to_string()
  }
}

impl EventEmitter<TerminalViewEvent> for TerminalView {}

impl Render for TerminalView {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    if self.closed {
      return div()
        .id("terminal-view")
        .key_context("Terminal")
        .size_full()
        .bg(cx.theme().background)
        .flex()
        .items_center()
        .justify_center()
        .text_color(cx.theme().muted_foreground)
        .text_sm()
        .child("Process exited")
        .into_any_element();
    }

    let terminal = self.terminal.clone();
    let show_scroll_button =
      self.terminal.read(cx).user_has_scrolled() && !self.terminal.read(cx).scrolled_to_bottom();

    div()
      .id("terminal-view")
      .key_context("Terminal")
      .size_full()
      .bg(cx.theme().background)
      .cursor_text()
      .relative()
      .child(TerminalElement::new(
        terminal.clone(),
        self.focus_handle.clone(),
      ))
      .on_action(cx.listener(Self::on_action_tab))
      .on_action(cx.listener(Self::on_action_tab_prev))
      .on_action(cx.listener(Self::on_action_copy))
      .on_action(cx.listener(Self::on_action_paste))
      .on_key_down(cx.listener(|this, event, window, cx| {
        this.handle_key_down(event, window, cx);
      }))
      .on_scroll_wheel(cx.listener(Self::handle_scroll_wheel))
      .track_focus(&self.focus_handle)
      .children(show_scroll_button.then(|| {
        div()
          .id("scroll-to-bottom-btn")
          .absolute()
          .right_2()
          .bottom_2()
          .w_8()
          .h_8()
          .rounded_full()
          .bg(gpui::rgba(0x40404080))
          .hover(|style| style.bg(gpui::rgba(0x606060ff)))
          .flex()
          .items_center()
          .justify_center()
          .cursor_pointer()
          .text_color(gpui::rgb(0xffffff))
          .text_sm()
          .child("↓")
          .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
              this.terminal.update(cx, |terminal, cx| {
                terminal.scroll_to_bottom(false, cx);
              });
            }),
          )
      }))
      .into_any_element()
  }
}

impl Focusable for TerminalView {
  fn focus_handle(&self, _cx: &App) -> FocusHandle {
    self.focus_handle.clone()
  }
}

/// 将 GPUI Keystroke 编码为字节序列
fn encode_keystroke(keystroke: &Keystroke) -> Vec<u8> {
  let modifiers = &keystroke.modifiers;
  let key = keystroke.key.as_str();

  if modifiers.control
    && !modifiers.alt
    && let Some(ctrl_byte) = encode_ctrl_key(key)
  {
    return ctrl_byte;
  }

  if modifiers.alt {
    let base = encode_base_key(keystroke, key, modifiers);
    if base.is_empty() {
      return base;
    }
    let mut result = vec![0x1b];
    result.extend(base);
    return result;
  }

  encode_base_key(keystroke, key, modifiers)
}

fn encode_ctrl_key(key: &str) -> Option<Vec<u8>> {
  match key {
    "space" => return Some(vec![0x00]),
    _ if key.len() == 1 => {
      let ch = key.chars().next().unwrap();
      match ch {
        'a'..='z' => return Some(vec![ch as u8 - b'a' + 1]),
        'A'..='Z' => return Some(vec![ch.to_ascii_lowercase() as u8 - b'a' + 1]),
        '[' | '{' => return Some(vec![0x1b]),
        ']' | '}' => return Some(vec![0x1d]),
        '\\' | '|' => return Some(vec![0x1c]),
        '^' | '~' => return Some(vec![0x1e]),
        '_' | '-' => return Some(vec![0x1f]),
        '@' | '`' | '2' | '0' => return Some(vec![0x00]),
        '/' | '?' => return Some(vec![0x1f]),
        _ => {}
      }
    }
    _ => {}
  }
  None
}

fn encode_base_key(keystroke: &Keystroke, key: &str, modifiers: &Modifiers) -> Vec<u8> {
  let result = match key {
    "enter" | "return" => {
      if modifiers.control {
        return vec![b'\n'];
      }
      if modifiers.shift && !modifiers.control && !modifiers.alt {
        return vec![0x1b, b'[', b'1', b'3', b';', b'2', b'u'];
      }
      return vec![b'\r'];
    }
    "escape" | "esc" => return vec![0x1b],
    "tab" | "\t" => {
      if modifiers.control && modifiers.shift {
        return vec![0x1b, b'[', b'1', b';', b'6', b'u'];
      }
      return vec![b'\t'];
    }
    "backspace" => {
      if modifiers.control {
        return vec![0x08];
      }
      return vec![0x7f];
    }
    "delete" | "del" => vec![0x1b, b'[', b'3', b'~'],
    "insert" | "ins" => vec![0x1b, b'[', b'2', b'~'],
    "up" => {
      if modifiers.control {
        vec![0x1b, b'[', b'1', b';', b'5', b'A']
      } else if modifiers.shift {
        vec![0x1b, b'[', b'1', b';', b'2', b'A']
      } else {
        vec![0x1b, b'[', b'A']
      }
    }
    "down" => {
      if modifiers.control {
        vec![0x1b, b'[', b'1', b';', b'5', b'B']
      } else if modifiers.shift {
        vec![0x1b, b'[', b'1', b';', b'2', b'B']
      } else {
        vec![0x1b, b'[', b'B']
      }
    }
    "right" => {
      if modifiers.control {
        vec![0x1b, b'[', b'1', b';', b'5', b'C']
      } else if modifiers.shift {
        vec![0x1b, b'[', b'1', b';', b'2', b'C']
      } else {
        vec![0x1b, b'[', b'C']
      }
    }
    "left" => {
      if modifiers.control {
        vec![0x1b, b'[', b'1', b';', b'5', b'D']
      } else if modifiers.shift {
        vec![0x1b, b'[', b'1', b';', b'2', b'D']
      } else {
        vec![0x1b, b'[', b'D']
      }
    }
    "home" => {
      if modifiers.control {
        vec![0x1b, b'[', b'1', b';', b'5', b'H']
      } else if modifiers.shift {
        vec![0x1b, b'[', b'1', b';', b'2', b'H']
      } else {
        vec![0x1b, b'[', b'H']
      }
    }
    "end" => {
      if modifiers.control {
        vec![0x1b, b'[', b'1', b';', b'5', b'F']
      } else if modifiers.shift {
        vec![0x1b, b'[', b'1', b';', b'2', b'F']
      } else {
        vec![0x1b, b'[', b'F']
      }
    }
    "pageup" | "page up" => {
      if modifiers.control {
        vec![0x1b, b'[', b'5', b';', b'5', b'~']
      } else if modifiers.shift {
        vec![0x1b, b'[', b'5', b';', b'2', b'~']
      } else {
        vec![0x1b, b'[', b'5', b'~']
      }
    }
    "pagedown" | "page down" => {
      if modifiers.control {
        vec![0x1b, b'[', b'6', b';', b'5', b'~']
      } else if modifiers.shift {
        vec![0x1b, b'[', b'6', b';', b'2', b'~']
      } else {
        vec![0x1b, b'[', b'6', b'~']
      }
    }
    _ if key.starts_with('f') || key.starts_with('F') => encode_function_key(key, modifiers),
    "space" => return vec![b' '],
    _ => vec![],
  };

  if !result.is_empty() {
    return result;
  }

  if let Some(key_char) = &keystroke.key_char
    && !key_char.is_empty()
  {
    return key_char.as_bytes().to_vec();
  }
  if key.len() == 1 {
    return key.as_bytes().to_vec();
  }
  vec![]
}

fn encode_function_key(key: &str, modifiers: &Modifiers) -> Vec<u8> {
  let num: Option<u8> = key.trim_start_matches(['f', 'F']).parse().ok();
  let Some(n) = num else { return vec![] };

  let modifier_param = if modifiers.control && modifiers.shift {
    "1;6"
  } else if modifiers.control {
    "1;5"
  } else if modifiers.shift {
    "1;2"
  } else {
    ""
  };

  match n {
    1 => return vec![0x1b, b'O', b'P'],
    2 => return vec![0x1b, b'O', b'Q'],
    3 => return vec![0x1b, b'O', b'R'],
    4 => return vec![0x1b, b'O', b'S'],
    5..=12 => {
      let code = match n {
        5 => &[b'1', b'5'][..],
        6 => &[b'1', b'7'][..],
        7 => &[b'1', b'8'][..],
        8 => &[b'1', b'9'][..],
        9 => &[b'2', b'0'][..],
        10 => &[b'2', b'1'][..],
        11 => &[b'2', b'3'][..],
        12 => &[b'2', b'4'][..],
        _ => unreachable!(),
      };
      if modifier_param.is_empty() {
        let mut v = vec![0x1b, b'['];
        v.extend_from_slice(code);
        v.push(b'~');
        return v;
      } else {
        let mut v = vec![0x1b, b'['];
        v.extend_from_slice(code);
        v.push(b';');
        v.extend_from_slice(modifier_param.as_bytes());
        v.push(b'~');
        return v;
      }
    }
    13..=19 => {
      let code = (n - 10).to_string();
      if modifier_param.is_empty() {
        let mut v = vec![0x1b, b'[', b'2'];
        v.extend_from_slice(code.as_bytes());
        v.push(b'~');
        return v;
      } else {
        let mut v = vec![0x1b, b'[', b'2'];
        v.extend_from_slice(code.as_bytes());
        v.push(b';');
        v.extend_from_slice(modifier_param.as_bytes());
        v.push(b'~');
        return v;
      }
    }
    _ => {}
  }
  vec![]
}

#[cfg(test)]
mod tests {
  use super::{encode_ctrl_key, encode_function_key, encode_keystroke};
  use gpui::{Keystroke, Modifiers};

  fn ks(key: &str, mods: Modifiers) -> Keystroke {
    Keystroke {
      modifiers: mods,
      key: key.to_string(),
      key_char: None,
    }
  }

  fn ks_with_char(key: &str, mods: Modifiers, key_char: &str) -> Keystroke {
    Keystroke {
      modifiers: mods,
      key: key.to_string(),
      key_char: Some(key_char.to_string()),
    }
  }

  fn mods(ctrl: bool, alt: bool, shift: bool) -> Modifiers {
    Modifiers {
      control: ctrl,
      alt,
      shift,
      platform: false,
      function: false,
    }
  }

  #[test]
  fn enter_encodes_to_cr() {
    assert_eq!(
      encode_keystroke(&ks("enter", mods(false, false, false))),
      b"\r"
    );
  }

  #[test]
  fn ctrl_enter_encodes_to_lf() {
    assert_eq!(
      encode_keystroke(&ks("enter", mods(true, false, false))),
      b"\n"
    );
  }

  #[test]
  fn shift_enter_encodes_to_csi() {
    assert_eq!(
      encode_keystroke(&ks("enter", mods(false, false, true))),
      b"\x1b[13;2u"
    );
  }

  #[test]
  fn escape_encodes_to_esc() {
    assert_eq!(
      encode_keystroke(&ks("escape", mods(false, false, false))),
      b"\x1b"
    );
  }

  #[test]
  fn tab_encodes_to_tab_byte() {
    assert_eq!(
      encode_keystroke(&ks("tab", mods(false, false, false))),
      b"\t"
    );
  }

  #[test]
  fn ctrl_shift_tab_encodes_csi() {
    assert_eq!(
      encode_keystroke(&ks("tab", mods(true, false, true))),
      b"\x1b[1;6u"
    );
  }

  #[test]
  fn backspace_default_is_del() {
    assert_eq!(
      encode_keystroke(&ks("backspace", mods(false, false, false))),
      b"\x7f"
    );
  }

  #[test]
  fn ctrl_backspace_is_bs() {
    assert_eq!(
      encode_keystroke(&ks("backspace", mods(true, false, false))),
      b"\x08"
    );
  }

  #[test]
  fn delete_and_insert_csi() {
    assert_eq!(
      encode_keystroke(&ks("delete", mods(false, false, false))),
      b"\x1b[3~"
    );
    assert_eq!(
      encode_keystroke(&ks("insert", mods(false, false, false))),
      b"\x1b[2~"
    );
  }

  #[test]
  fn arrow_keys_with_modifiers() {
    // 普通方向键
    assert_eq!(
      encode_keystroke(&ks("up", mods(false, false, false))),
      b"\x1b[A"
    );
    assert_eq!(
      encode_keystroke(&ks("down", mods(false, false, false))),
      b"\x1b[B"
    );
    assert_eq!(
      encode_keystroke(&ks("right", mods(false, false, false))),
      b"\x1b[C"
    );
    assert_eq!(
      encode_keystroke(&ks("left", mods(false, false, false))),
      b"\x1b[D"
    );
    // Ctrl 方向键
    assert_eq!(
      encode_keystroke(&ks("up", mods(true, false, false))),
      b"\x1b[1;5A"
    );
    // Shift 方向键
    assert_eq!(
      encode_keystroke(&ks("left", mods(false, false, true))),
      b"\x1b[1;2D"
    );
  }

  #[test]
  fn home_end_pageup_pagedown() {
    assert_eq!(
      encode_keystroke(&ks("home", mods(false, false, false))),
      b"\x1b[H"
    );
    assert_eq!(
      encode_keystroke(&ks("home", mods(true, false, false))),
      b"\x1b[1;5H"
    );
    assert_eq!(
      encode_keystroke(&ks("end", mods(false, false, false))),
      b"\x1b[F"
    );
    assert_eq!(
      encode_keystroke(&ks("pageup", mods(false, false, false))),
      b"\x1b[5~"
    );
    assert_eq!(
      encode_keystroke(&ks("pagedown", mods(false, false, false))),
      b"\x1b[6~"
    );
    assert_eq!(
      encode_keystroke(&ks("pageup", mods(false, false, true))),
      b"\x1b[5;2~"
    );
  }

  #[test]
  fn ctrl_letter_encodes_control_code() {
    assert_eq!(
      encode_keystroke(&ks("c", mods(true, false, false))),
      b"\x03"
    );
    assert_eq!(
      encode_keystroke(&ks("a", mods(true, false, false))),
      b"\x01"
    );
    assert_eq!(
      encode_keystroke(&ks("z", mods(true, false, false))),
      b"\x1a"
    );
    // 大写字母也应映射为对应控制码
    assert_eq!(
      encode_keystroke(&ks("C", mods(true, false, false))),
      b"\x03"
    );
  }

  #[test]
  fn ctrl_space_is_null() {
    assert_eq!(
      encode_keystroke(&ks("space", mods(true, false, false))),
      b"\x00"
    );
  }

  #[test]
  fn ctrl_bracket_encodes_to_esc() {
    assert_eq!(
      encode_keystroke(&ks("[", mods(true, false, false))),
      b"\x1b"
    );
    assert_eq!(
      encode_keystroke(&ks("]", mods(true, false, false))),
      b"\x1d"
    );
    assert_eq!(
      encode_keystroke(&ks("\\", mods(true, false, false))),
      b"\x1c"
    );
  }

  #[test]
  fn alt_prefixes_with_esc() {
    // Alt+a => ESC a
    let result = encode_keystroke(&ks_with_char("a", mods(false, true, false), "a"));
    assert_eq!(result, b"\x1ba");
  }

  #[test]
  fn space_encodes_to_space() {
    assert_eq!(
      encode_keystroke(&ks("space", mods(false, false, false))),
      b" "
    );
  }

  #[test]
  fn plain_letter_uses_key_char() {
    // 普通字母：无修饰符，走 key_char 分支
    assert_eq!(
      encode_keystroke(&ks_with_char("a", mods(false, false, false), "a")),
      b"a"
    );
  }

  #[test]
  fn function_keys_f1_to_f4_ss3() {
    assert_eq!(
      encode_keystroke(&ks("f1", mods(false, false, false))),
      b"\x1bOP"
    );
    assert_eq!(
      encode_keystroke(&ks("f2", mods(false, false, false))),
      b"\x1bOQ"
    );
    assert_eq!(
      encode_keystroke(&ks("f3", mods(false, false, false))),
      b"\x1bOR"
    );
    assert_eq!(
      encode_keystroke(&ks("f4", mods(false, false, false))),
      b"\x1bOS"
    );
  }

  #[test]
  fn function_keys_f5_to_f12_csi() {
    // F5 = ESC[15~
    assert_eq!(
      encode_keystroke(&ks("f5", mods(false, false, false))),
      b"\x1b[15~"
    );
    // F12 = ESC[24~
    assert_eq!(
      encode_keystroke(&ks("f12", mods(false, false, false))),
      b"\x1b[24~"
    );
  }

  #[test]
  fn function_key_with_ctrl_modifier() {
    // Ctrl+F5 => ESC[15;1;5~（code "15" + modifier_param "1;5"）
    assert_eq!(
      encode_keystroke(&ks("f5", mods(true, false, false))),
      b"\x1b[15;1;5~"
    );
    // Shift+F5 => ESC[15;1;2~
    assert_eq!(
      encode_keystroke(&ks("f5", mods(false, false, true))),
      b"\x1b[15;1;2~"
    );
  }

  #[test]
  fn function_keys_f13_to_f19_supported() {
    // F13 不在 1..=12，但落入 13..=19 分支 => ESC[23~
    assert_eq!(
      encode_keystroke(&ks("f13", mods(false, false, false))),
      b"\x1b[23~"
    );
  }

  #[test]
  fn unknown_function_key_returns_empty() {
    // F20+ 超出所有范围
    assert_eq!(encode_keystroke(&ks("f20", mods(false, false, false))), b"");
    assert_eq!(encode_function_key("f99", &mods(false, false, false)), b"");
  }

  #[test]
  fn ctrl_key_helper_directly() {
    assert_eq!(encode_ctrl_key("c"), Some(b"\x03".to_vec()));
    assert_eq!(encode_ctrl_key("space"), Some(b"\x00".to_vec()));
    assert_eq!(encode_ctrl_key("2"), Some(b"\x00".to_vec()));
    // '1' 不在控制码映射中 → None
    assert_eq!(encode_ctrl_key("1"), None);
    // 非控制键返回 None
    assert_eq!(encode_ctrl_key("f5"), None);
    assert_eq!(encode_ctrl_key("enter"), None);
  }

  #[test]
  fn empty_for_unrecognized_key_without_char() {
    // 无 key_char 且 key 长度>1 且不匹配任何分支
    assert_eq!(
      encode_keystroke(&ks("nomatch", mods(false, false, false))),
      b""
    );
  }

  // ===== copy / paste action 测试 =====
  // 这些测试需要 Window（剪贴板与 action handler 签名要求），因此用 VisualTestContext。

  use super::{CopySelection, PasteFromClipboard, TerminalView};
  use crate::terminal::Terminal;
  use crate::terminal::content::TerminalPoint;
  use crate::terminal::fake_pty::{EchoMode, FakePty};
  use alacritty_terminal::index::{Column, Line};
  use gpui::{AppContext as _, ClipboardItem, TestAppContext, VisualTestContext};
  use std::sync::Arc;

  /// 构造一个带选中文本的 TerminalView（用于 copy 测试）。
  /// 通过 FakePty 注入 "hi" 输出并 refresh_content 填充 content，再用公开方法设置选区。
  fn view_with_selection(cx: &mut TestAppContext) -> gpui::Entity<TerminalView> {
    let fake = Arc::new(FakePty::with_echo_mode(EchoMode::None));
    let pty_dyn: Arc<dyn crate::terminal::Pty> = fake.clone();
    let terminal = cx.new(|cx| Terminal::new(pty_dyn, cx).expect("terminal"));
    // 注入输出 "hi" 并提取到 content
    fake.push_bytes("hi").unwrap();
    cx.run_until_parked();
    terminal.update(cx, |t, cx| t.refresh_content(cx));
    // 设置选区覆盖 "hi"（col 0..=1）
    terminal.update(cx, |t, cx| {
      t.set_selection_start(
        TerminalPoint {
          line: Line(0),
          column: Column(0),
        },
        cx,
      );
      t.set_selection_end(
        TerminalPoint {
          line: Line(0),
          column: Column(1),
        },
        cx,
      );
    });
    cx.new(|cx| TerminalView::new(terminal, cx))
  }

  /// 构造一个保留 FakePty 引用的 TerminalView（用于 paste 测试）。
  fn view_with_fake_pty(cx: &mut TestAppContext) -> (gpui::Entity<TerminalView>, Arc<FakePty>) {
    let fake = Arc::new(FakePty::with_echo_mode(EchoMode::None));
    let pty_dyn: Arc<dyn crate::terminal::Pty> = fake.clone();
    let terminal = cx.new(|cx| Terminal::new(pty_dyn, cx).expect("terminal"));
    let view = cx.new(|cx| TerminalView::new(terminal, cx));
    (view, fake)
  }

  #[gpui::test]
  fn copy_action_writes_selection_to_clipboard(cx: &mut TestAppContext) {
    let view = view_with_selection(cx);
    let mut cx: VisualTestContext = cx.add_empty_window().clone();

    cx.update(|window, app_cx| {
      view.update(app_cx, |v, vcx| {
        v.on_action_copy(&CopySelection, window, vcx)
      });
    });

    let clip = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(clip.as_deref(), Some("hi"));
  }

  #[gpui::test]
  fn copy_action_with_no_selection_writes_nothing(cx: &mut TestAppContext) {
    let (view, _fake) = view_with_fake_pty(cx);
    let mut cx: VisualTestContext = cx.add_empty_window().clone();

    cx.update(|window, app_cx| {
      view.update(app_cx, |v, vcx| {
        v.on_action_copy(&CopySelection, window, vcx)
      });
    });

    // 无选区时不应写入剪贴板
    assert!(cx.read_from_clipboard().is_none());
  }

  #[gpui::test]
  fn paste_action_sends_clipboard_to_terminal(cx: &mut TestAppContext) {
    let (view, fake) = view_with_fake_pty(cx);
    let mut cx: VisualTestContext = cx.add_empty_window().clone();

    // 预置剪贴板内容
    cx.write_to_clipboard(ClipboardItem::new_string("pasted text".to_string()));

    cx.update(|window, app_cx| {
      view.update(app_cx, |v, vcx| {
        v.on_action_paste(&PasteFromClipboard, window, vcx)
      });
    });
    cx.run_until_parked();

    // 默认（非 bracketed paste）模式，文本原样写入 PTY
    assert_eq!(fake.writes_string(), "pasted text");
  }

  #[gpui::test]
  fn paste_action_normalizes_crlf(cx: &mut TestAppContext) {
    let (view, fake) = view_with_fake_pty(cx);
    let mut cx: VisualTestContext = cx.add_empty_window().clone();

    cx.write_to_clipboard(ClipboardItem::new_string("a\r\nb\rc".to_string()));
    cx.update(|window, app_cx| {
      view.update(app_cx, |v, vcx| {
        v.on_action_paste(&PasteFromClipboard, window, vcx)
      });
    });
    cx.run_until_parked();

    assert_eq!(fake.writes_string(), "a\nb\nc");
  }
}
