use crate::terminal::provider::{Modifiers, RenderableContentStatic, TerminalProvider};
use gpui::prelude::FluentBuilder;
use gpui::*;

/// Terminal view component using GPUI
pub struct TerminalView {
  provider: Entity<TerminalProvider>,
  content: RenderableContentStatic,
  char_width: Pixels,
  char_height: Pixels,
}

impl TerminalView {
  /// 创建新的 TerminalView，使用已存在的 TerminalProvider
  pub fn new(provider: Entity<TerminalProvider>, cx: &mut Context<Self>) -> Self {
    // 获取初始内容
    let content = provider.read(cx).get_update().content.clone();

    // 设置定期刷新以获取终端更新
    cx.spawn(async move |this, cx| {
      loop {
        // 每 50ms 刷新一次
        cx.background_executor().timer(std::time::Duration::from_millis(50)).await;
        
        // 更新实体
        if let Some(this) = this.upgrade() {
          let result: Result<()> = cx.update_entity(
            &this,
            |this: &mut TerminalView, cx: &mut Context<TerminalView>| {
              // 从 provider 获取最新内容
              let new_content = this.provider.read(cx).get_update().content.clone();
              this.content = new_content;
              cx.notify();
            },
          );
          if result.is_err() {
            break;
          }
        } else {
          break;
        }
      }
    }).detach();

    Self {
      provider,
      content,
      char_width: px(8.),
      char_height: px(16.),
    }
  }

  /// 处理按键事件
  fn handle_key_down(
    &mut self,
    event: &KeyDownEvent,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let _modifiers = Modifiers {
      ctrl: event.keystroke.modifiers.control,
      alt: event.keystroke.modifiers.alt,
      shift: event.keystroke.modifiers.shift,
      meta: event.keystroke.modifiers.platform,
    };

    let _key = gpui_key_to_provider_key(&event.keystroke.key);

    // TODO: 实现按键发送
    // 由于 send_key 是异步的，我们需要使用 cx.spawn

    cx.notify();
  }

  /// 渲染终端内容
  fn render_terminal_content(&self, _cx: &mut Context<Self>) -> impl IntoElement {
    let content = self.content.clone();
    let char_width = self.char_width;
    let char_height = self.char_height;
    let cursor_row = content.cursor_row;
    let cursor_col = content.cursor_col;
    let cursor_visible = content.cursor_visible;

    // 收集所有单元格
    let cells = content.cells.clone();

    div()
      .id("terminal-content")
      .relative()
      .size_full()
      .children(
        // 渲染所有单元格
        cells
          .into_iter()
          .map(move |(row, col, c, fg, bg, _bold)| {
            let x = char_width * col as f32;
            let y = char_height * row as f32;
            let fg_color = rgb((fg[0] as u32) << 16 | (fg[1] as u32) << 8 | (fg[2] as u32));
            let bg_color = rgb((bg[0] as u32) << 16 | (bg[1] as u32) << 8 | (bg[2] as u32));

            let has_bg = bg != [30, 30, 30];

            div()
              .id(("cell", row * 1000 + col))
              .absolute()
              .left(x)
              .top(y)
              .when(has_bg, |this| this.bg(bg_color))
              .text_color(fg_color)
              .font_family("Monaco")
              .text_size(px(14.))
              .child(c.to_string())
          })
          .collect::<Vec<_>>(),
      )
      .when(cursor_visible, |this| {
        // 渲染光标
        let cursor_x = char_width * cursor_col as f32;
        let cursor_y = char_height * cursor_row as f32;

        // 查找光标位置的字符
        let cursor_char = content
          .cells
          .iter()
          .find(|(r, c, _, _, _, _)| *r == cursor_row && *c == cursor_col)
          .map(|(_, _, c, _, _, _)| *c)
          .unwrap_or(' ');

        this.child(
          div()
            .id("cursor")
            .absolute()
            .left(cursor_x)
            .top(cursor_y)
            .w(char_width)
            .h(char_height)
            .bg(rgba(0x80ffffff))
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(0x000000))
            .font_family("Monaco")
            .text_size(px(14.))
            .child(cursor_char.to_string()),
        )
      })
  }
}

impl Render for TerminalView {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    div()
      .id("terminal-view")
      .size_full()
      .bg(rgb(0x1e1e1e))
      .cursor_text()
      .child(self.render_terminal_content(cx))
      .on_key_down(cx.listener(|this, event, window, cx| {
        this.handle_key_down(event, window, cx);
      }))
      .track_focus(&cx.focus_handle())
  }
}

impl Focusable for TerminalView {
  fn focus_handle(&self, _cx: &App) -> FocusHandle {
    _cx.focus_handle()
  }
}

/// 将 GPUI 的 Keystroke key 转换为 provider 的 Key
fn gpui_key_to_provider_key(key: &str) -> crate::terminal::provider::Key {
  use crate::terminal::provider::Key;

  match key {
    "enter" => Key::Enter,
    "escape" | "esc" => Key::Escape,
    "tab" => Key::Tab,
    "backspace" => Key::Backspace,
    "delete" | "del" => Key::Delete,
    "insert" | "ins" => Key::Insert,
    "up" => Key::ArrowUp,
    "down" => Key::ArrowDown,
    "left" => Key::ArrowLeft,
    "right" => Key::ArrowRight,
    "home" => Key::Home,
    "end" => Key::End,
    "pageup" | "page up" => Key::PageUp,
    "pagedown" | "page down" => Key::PageDown,
    "f1" => Key::F1,
    "f2" => Key::F2,
    "f3" => Key::F3,
    "f4" => Key::F4,
    "f5" => Key::F5,
    "f6" => Key::F6,
    "f7" => Key::F7,
    "f8" => Key::F8,
    "f9" => Key::F9,
    "f10" => Key::F10,
    "f11" => Key::F11,
    "f12" => Key::F12,
    k if k.len() == 1 => Key::Character(k.to_string()),
    _ => Key::Unidentified,
  }
}
