use crate::terminal::provider::TerminalProvider;
use crate::terminal::terminal_element::TerminalElement;
use gpui::*;

/// Terminal view component using GPUI
pub struct TerminalView {
  provider: Entity<TerminalProvider>,
  focus_handle: FocusHandle,
}

impl TerminalView {
  /// 创建新的 TerminalView，使用已存在的 TerminalProvider
  pub fn new(provider: Entity<TerminalProvider>, cx: &mut Context<Self>) -> Self {
    // 设置定期刷新以获取终端更新
    cx.spawn(async move |this, cx| {
      loop {
        // 每 50ms 刷新一次
        cx.background_executor()
          .timer(std::time::Duration::from_millis(50))
          .await;

        // 更新实体
        if let Some(this) = this.upgrade() {
          let result: Result<()> = cx.update_entity(
            &this,
            |_this: &mut TerminalView, cx: &mut Context<TerminalView>| {
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
    })
    .detach();

    Self {
      provider,
      focus_handle: cx.focus_handle(),
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
    let command_tx = self.provider.read(cx).command_tx.clone();

    cx.spawn(async move |this, cx| {
      // 直接通过 command_tx 发送按键
      let cmd = crate::terminal::provider::ProviderCommand::SendKey(keystroke);
      let _ = command_tx.send(cmd).await;

      // 通知刷新
      let _ = this.update(cx, |_, cx| {
        cx.notify();
      });
    })
    .detach();
  }
}

impl Render for TerminalView {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    // 获取当前内容
    let content = self.provider.read(cx).get_update().content.clone();

    div()
      .id("terminal-view")
      .size_full()
      .bg(gpui::rgb(0x1e1e1e))
      .cursor_text()
      .child(TerminalElement::new(
        self.provider.clone(),
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
