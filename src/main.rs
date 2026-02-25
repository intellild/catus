use gpui::*;
use gpui_component::Root;

mod app;
mod main_view;
mod terminal;
mod tiles;
mod workspace;

use app::App as CatusApp;
use main_view::MainView;

fn main() {
  let app = Application::new().with_assets(gpui_component_assets::Assets);

  app.run(move |cx| {
    // Initialize GPUI Component
    gpui_component::init(cx);

    // 创建 App，包含一个默认的 Workspace
    let catus_app = cx.new(|cx| CatusApp::new(cx));
    let workspace = catus_app.read(cx).workspace().clone();

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

        let view = cx.new(|_| MainView::new(workspace));
        cx.new(|cx| Root::new(view, window, cx))
      },
    )
    .ok();
  });
}
