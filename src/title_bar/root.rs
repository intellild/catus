use gpui::*;
use gpui_component::ActiveTheme;

pub struct TitleBarRoot {
  content: AnyView,
}

impl TitleBarRoot {
  pub fn new(content: AnyView) -> Self {
    Self { content }
  }
}

impl Render for TitleBarRoot {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let height = px(34.);

    // Left padding for macOS traffic lights or general spacing
    #[cfg(target_os = "macos")]
    let left_padding = px(80.);
    #[cfg(not(target_os = "macos"))]
    let left_padding = px(12.);

    div()
      .flex()
      .flex_row()
      .items_center()
      .h(height)
      .bg(cx.theme().title_bar)
      .border_b_1()
      .border_color(cx.theme().title_bar_border)
      .pl(left_padding)
      .child(
        div()
          .h_full()
          .w(px(60.))
          .flex_none()
          // Extra drag area width
          .window_control_area(WindowControlArea::Drag),
      )
      .child(self.content.clone())
  }
}
