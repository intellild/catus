use gpui::*;
use gpui_component::button::ButtonVariant;
use gpui_component::dialog::DialogButtonProps;
use gpui_component::input::{Input, InputState};
use gpui_component::{ActiveTheme, Icon, IconName, Sizable, WindowExt};

use crate::app::App as CatusApp;
use crate::terminal::default_shell_program;
use crate::workspace_kind::WorkspaceKind;

/// 打开「添加 Workspace」对话框。
///
/// 编辑启动命令，输入框默认填入当前用户默认 shell；提交后调用
/// `App::add_workspace`（成功时会持久化到 TOML 配置文件），
/// 失败时弹出通知。
pub fn open_add_workspace_dialog(app: Entity<CatusApp>, window: &mut Window, cx: &mut gpui::App) {
  let command: Entity<InputState> = cx.new(|cx| {
    InputState::new(window, cx)
      .placeholder("ssh user@host")
      .default_value(default_shell_program())
  });

  window.open_dialog(cx, {
    let command = command.clone();
    let app = app.clone();
    move |dialog, _window, cx| {
      let theme = cx.theme();
      let command_for_input = command.clone();
      let command_for_ok = command.clone();
      let app_for_ok = app.clone();

      dialog
        .title("Add Workspace")
        .w(px(460.))
        .child(
          div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
              div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("Command"),
            )
            .child(
              Input::new(&command_for_input)
                .prefix(Icon::new(IconName::SquareTerminal).with_size(px(14.))),
            )
            .child(
              div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Empty command starts the default shell."),
            ),
        )
        .button_props(
          DialogButtonProps::default()
            .ok_text("Add")
            .ok_variant(ButtonVariant::Primary),
        )
        .confirm()
        .on_ok(move |_, window, cx| {
          let value = command_for_ok.read(cx).value().to_string();
          let kind = WorkspaceKind::from_command_line(&value);
          match app_for_ok.update(cx, |app, cx| app.add_workspace(kind, cx)) {
            Ok(_) => true,
            Err(e) => {
              window.push_notification(gpui_component::notification::Notification::error(e), cx);
              false
            }
          }
        })
    }
  });
}
