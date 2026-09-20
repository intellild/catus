use gpui::*;
use gpui_component::button::ButtonVariant;
use gpui_component::dialog::DialogButtonProps;
use gpui_component::input::{Input, InputState};
use gpui_component::radio::{Radio, RadioGroup};
use gpui_component::{ActiveTheme, Icon, IconName, Sizable, WindowExt};

use crate::app::App as CatusApp;
use crate::workspace_spec::{WorkspaceMode, WorkspaceSpec};

/// 打开「添加 Workspace」对话框。
///
/// 选择常规 / tmux 类型并填写启动命令；切换类型不会修改命令。提交后调用
/// `App::add_workspace`（成功时会持久化到 TOML 配置文件），
/// 失败时弹出通知。
pub fn open_add_workspace_dialog(app: Entity<CatusApp>, window: &mut Window, cx: &mut gpui::App) {
  let command = cx.new(|cx| InputState::new(window, cx).placeholder("Enter a command"));
  let mode = cx.new(|_| WorkspaceMode::Regular);

  window.open_dialog(cx, {
    let command = command.clone();
    let app = app.clone();
    move |dialog, _window, cx| {
      let selected_mode = *mode.read(cx);
      let mode_for_change = mode.clone();
      let mode_for_ok = mode.clone();
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
                .child("Workspace Type"),
            )
            .child(
              RadioGroup::horizontal("workspace-mode")
                .selected_index(Some(if selected_mode == WorkspaceMode::Regular {
                  0
                } else {
                  1
                }))
                .child(Radio::new("regular").label("Regular"))
                .child(Radio::new("tmux").label("tmux"))
                .on_click(move |index, window, cx| {
                  mode_for_change.update(cx, |mode, cx| {
                    *mode = if *index == 0 {
                      WorkspaceMode::Regular
                    } else {
                      WorkspaceMode::Tmux
                    };
                    cx.notify();
                  });
                  window.refresh();
                }),
            )
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
                .child(match selected_mode {
                  WorkspaceMode::Regular => "Empty command starts the default shell.",
                  WorkspaceMode::Tmux => {
                    "Enter a tmux control mode command, e.g. tmux -CC new-session -A -s work."
                  }
                }),
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
          let mode = *mode_for_ok.read(cx);
          if mode == WorkspaceMode::Tmux && value.trim().is_empty() {
            window.push_notification(
              gpui_component::notification::Notification::error(
                "Enter a tmux control mode command.",
              ),
              cx,
            );
            return false;
          }
          let spec = WorkspaceSpec::new(mode, &value);
          match app_for_ok.update(cx, |app, cx| app.add_workspace(spec, cx)) {
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
