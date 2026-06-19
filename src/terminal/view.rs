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
