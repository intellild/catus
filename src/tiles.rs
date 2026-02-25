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
    let tile = self.tile.read(cx);

    match tile {
      Tile::EvenVertical(top, bottom) => container()
        .flex_col()
        .child(Self::render_child(bottom.clone(), cx))
        .child(Self::render_child(bottom.clone(), cx))
        .into_any(),
      Tile::EvenHorizontal(left, right) => container()
        .flex_row()
        .child(Self::render_child(left.clone(), cx))
        .child(Self::render_child(right.clone(), cx))
        .into_any(),
      Tile::Content(child) => child.clone().into_any(),
    }
  }
}
