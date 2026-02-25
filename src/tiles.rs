use crate::placeholder::PlaceholderView;
use gpui::*;

#[derive(Clone)]
pub enum Tile {
  EvenHorizontal(Entity<Tile>, Entity<Tile>),
  EvenVertical(Entity<Tile>, Entity<Tile>),
  Content(AnyView),
}

pub struct TileView {
  tile: Entity<Tile>,
  parent: Option<Entity<Tile>>,
  focus_handle: FocusHandle,
}

impl TileView {
  pub fn new(
    tile: Entity<Tile>,
    parent: Option<Entity<Tile>>,
    cx: &mut Context<TileView>,
  ) -> TileView {
    let focus_handle = cx.focus_handle();

    Self {
      tile,
      parent,
      focus_handle,
    }
  }

  fn handle_keydown(&self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
    let Some(child) = (match self.tile.read(cx) {
      Tile::Content(child) => Some(child),
      _ => None,
    }) else {
      return;
    };
    if event.keystroke.key == "d" && event.keystroke.modifiers.control {
      let next = match self.parent.as_ref().map(|parent| parent.read(cx)) {
        Some(Tile::EvenHorizontal(_, _)) => {
          let top = child.clone();
          let bottom = cx.new(|_| PlaceholderView);

          Tile::EvenVertical(
            cx.new(|cx| Tile::Content(top)),
            cx.new(|cx| Tile::Content(bottom.into())),
          )
        }
        _ => {
          let left = child.clone();
          let right = cx.new(|_| PlaceholderView);

          Tile::EvenHorizontal(
            cx.new(|cx| Tile::Content(left)),
            cx.new(|cx| Tile::Content(right.into())),
          )
        }
      };
      self.tile.write(cx, next);
    }
  }

  fn render_child(
    tile: Entity<Tile>,
    parent: Entity<Tile>,
    cx: &mut Context<Self>,
  ) -> impl IntoElement {
    cx.new(|cx| Self::new(tile, Some(parent), cx))
  }

  fn render_container(&self, cx: &mut Context<Self>) -> Div {
    div()
      .relative()
      .flex()
      .w_full()
      .h_full()
      .flex_1()
      .on_mouse_up(
        MouseButton::Left,
        cx.listener(|this, _, window, cx| {
          if !this.focus_handle.contains_focused(window, cx) {
            this.focus_handle.focus(window);
          }
        }),
      )
      .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
        this.handle_keydown(event, window, cx);
      }))
  }
}

impl Focusable for TileView {
  fn focus_handle(&self, _cx: &App) -> FocusHandle {
    self.focus_handle.clone()
  }
}

impl Render for TileView {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let tile = self.tile.read(cx).clone();

    match tile {
      Tile::EvenVertical(top, bottom) => self
        .render_container(cx)
        .flex_col()
        .child(Self::render_child(top.clone(), self.tile.clone(), cx))
        .child(Self::render_child(bottom.clone(), self.tile.clone(), cx)),
      Tile::EvenHorizontal(left, right) => self
        .render_container(cx)
        .flex_row()
        .child(Self::render_child(left.clone(), self.tile.clone(), cx))
        .child(Self::render_child(right.clone(), self.tile.clone(), cx)),
      Tile::Content(child) => self.render_container(cx).child(child.clone().into_any()),
    }
  }
}
