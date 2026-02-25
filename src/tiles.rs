use gpui::*;

pub enum Tile {
  EvenHorizontal(Entity<Tile>, Entity<Tile>),
  EvenVertical(Entity<Tile>, Entity<Tile>),
  Content(AnyView),
}

pub struct TileView {
  tile: Entity<Tile>,
  focus_handle: FocusHandle,
}

fn container() -> Div {
  div().relative().flex().w_full().h_full().flex_1()
}

impl TileView {
  pub fn new(tile: Entity<Tile>, cx: &mut Context<TileView>) -> TileView {
    let focus_handle = cx.focus_handle();

    Self { tile, focus_handle }
  }

  fn render_child(tile: Entity<Tile>, cx: &mut Context<Self>) -> impl IntoElement {
    cx.new(|cx| Self::new(tile, cx))
  }
}

impl Render for TileView {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let children = match self.tile.read(cx) {
      Tile::EvenVertical(top, bottom) => Some((top.clone(), bottom.clone(), true)),
      Tile::EvenHorizontal(left, right) => Some((left.clone(), right.clone(), false)),
      Tile::Content(child) => return child.clone().into_any(),
    };

    if let Some((first, second, is_vertical)) = children {
      let container = container();
      if is_vertical {
        container
          .flex_col()
          .child(Self::render_child(first, cx))
          .child(Self::render_child(second, cx))
          .into_any()
      } else {
        container
          .flex_row()
          .child(Self::render_child(first, cx))
          .child(Self::render_child(second, cx))
          .into_any()
      }
    } else {
      div().into_any()
    }
  }
}
