use gpui::*;
use gpui_component::Root;

mod app_state;
mod main_view;
mod terminal;
mod workspace;

use main_view::MainView;

fn main() {
  let app = Application::new().with_assets(gpui_component_assets::Assets);

  app.run(move |cx| {
    // Initialize GPUI Component
    gpui_component::init(cx);

    cx.spawn(async move |cx| {
      cx.open_window(
        WindowOptions {
          titlebar: Some(TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(gpui::point(px(9.0), px(9.0))),
          }),
          ..WindowOptions::default()
        },
        |window, cx| {
          cx.activate(true);

          let view = cx.new(|_| MainView::new());
          cx.new(|cx| Root::new(view, window, cx))
        },
      )?;

      Ok::<_, anyhow::Error>(())
    })
    .detach();
  });
}
