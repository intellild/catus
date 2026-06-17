use std::cell::Cell;
use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariant, ButtonVariants};
use gpui_component::dialog::DialogButtonProps;
use gpui_component::input::{Input, InputState};
use gpui_component::{ActiveTheme, Icon, IconName, Sizable, WindowExt};

use crate::app::App as CatusApp;
use crate::workspace_kind::WorkspaceKind;

/// 对话框中可选的 workspace 类型。
#[derive(Clone, Copy, PartialEq, Eq)]
enum DialogKind {
  Local,
  Ssh,
}

impl DialogKind {
  fn label(&self) -> &'static str {
    match self {
      DialogKind::Local => "Local",
      DialogKind::Ssh => "SSH",
    }
  }

  fn icon(&self) -> IconName {
    match self {
      DialogKind::Local => IconName::SquareTerminal,
      DialogKind::Ssh => IconName::Globe,
    }
  }

  /// 切换到该类型时命令输入框的默认值。
  fn default_command(&self) -> &'static str {
    match self {
      DialogKind::Local => "",
      DialogKind::Ssh => "ssh",
    }
  }
}

/// 根据类型与命令构造 `WorkspaceKind`。
fn build_kind(kind: DialogKind, value: &str) -> WorkspaceKind {
  let trimmed = value.trim();
  match kind {
    DialogKind::Local => {
      if trimmed.is_empty() {
        WorkspaceKind::Local
      } else {
        // Local 下填了自定义命令，统一走启动本地进程的变体。
        WorkspaceKind::Ssh(trimmed.to_string())
      }
    }
    DialogKind::Ssh => WorkspaceKind::Ssh(if trimmed.is_empty() {
      "ssh".to_string()
    } else {
      trimmed.to_string()
    }),
  }
}

/// 打开「添加 Workspace」对话框。
///
/// 确认后会调用 `App::add_workspace`，失败时弹出通知。
pub fn open_add_workspace_dialog(app: Entity<CatusApp>, window: &mut Window, cx: &mut gpui::App) {
  // 共享的当前类型，供类型按钮与 on_ok 读取。
  let kind = Rc::new(Cell::new(DialogKind::Ssh));
  let command: Entity<InputState> = cx.new(|cx| {
    InputState::new(window, cx)
      .placeholder("ssh user@host")
      .default_value("ssh")
  });

  window.open_dialog(cx, {
    let kind = kind.clone();
    let command = command.clone();
    let app = app.clone();
    move |dialog, _window, cx| {
      let theme = cx.theme();
      let kind_for_buttons = kind.clone();
      let command_for_buttons = command.clone();
      let kind_for_input = kind.clone();
      let command_for_input = command.clone();

      let kind_for_ok = kind.clone();
      let command_for_ok = command.clone();
      let app_for_ok = app.clone();

      dialog
        .title("Add Workspace")
        .w(px(460.))
        .child(
          div()
            .flex()
            .flex_col()
            .gap(px(12.))
            // 类型选择
            .child(
              div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                  div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("Type"),
                )
                .child(
                  div()
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .child(render_kind_button(
                      DialogKind::Local,
                      kind_for_buttons.clone(),
                      command_for_buttons.clone(),
                      cx,
                    ))
                    .child(render_kind_button(
                      DialogKind::Ssh,
                      kind_for_buttons.clone(),
                      command_for_buttons.clone(),
                      cx,
                    )),
                ),
            )
            // 命令输入框
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
                    .prefix(Icon::new(kind_for_input.get().icon()).with_size(px(14.))),
                ),
            ),
        )
        .button_props(
          DialogButtonProps::default()
            .ok_text("Add")
            .ok_variant(ButtonVariant::Primary),
        )
        .confirm()
        .on_ok(move |_, window, cx| {
          let kind = kind_for_ok.get();
          let value = command_for_ok.read(cx).value().to_string();
          let new_kind = build_kind(kind, &value);
          match app_for_ok.update(cx, |app, cx| app.add_workspace(new_kind, cx)) {
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

/// 渲染类型选择按钮。点击后更新共享的 `kind` 并重置命令输入框。
fn render_kind_button(
  target: DialogKind,
  kind: Rc<Cell<DialogKind>>,
  command: Entity<InputState>,
  _cx: &gpui::App,
) -> impl IntoElement {
  let selected = kind.get() == target;
  let theme = _cx.theme();

  Button::new(target.label())
    .ghost()
    .small()
    .gap(px(6.))
    .when(selected, |this| this.with_variant(ButtonVariant::Primary))
    .child(Icon::new(target.icon()).with_size(px(14.)))
    .child(target.label())
    .when(!selected, |this| {
      this.hover(|style| style.bg(theme.secondary_hover))
    })
    .on_click(move |_, window, cx| {
      if kind.get() != target {
        kind.set(target);
        let default = target.default_command().to_string();
        command.update(cx, |state, cx| {
          state.set_value(&default, window, cx);
        });
      }
    })
}
