use gpui::*;

pub struct PlaceholderView;

impl Render for PlaceholderView {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    div().child("Placeholder")
  }
}
