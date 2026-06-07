use gpui::*;
use gpui_component::{Root, Theme, ThemeMode};

mod app;
mod id;
mod main_view;
mod pane;
mod terminal;
mod title_bar;
mod workspace;

use app::App as CatusApp;
use main_view::MainView;
use pane::{ClosePane, SplitDown, SplitRight};
use terminal::view::{CopySelection, PasteFromClipboard, Tab, TabPrev};

fn main() {
  let app = Application::new().with_assets(gpui_component_assets::Assets);

  app.run(move |cx| {
    gpui_component::init(cx);
    Theme::change(ThemeMode::Dark, None, cx);

    cx.bind_keys([
      KeyBinding::new("tab", Tab, Some("Terminal")),
      KeyBinding::new("shift-tab", TabPrev, Some("Terminal")),
      KeyBinding::new("cmd-c", CopySelection, Some("Terminal")),
      KeyBinding::new("cmd-v", PasteFromClipboard, Some("Terminal")),
    ]);

    cx.bind_keys([
      KeyBinding::new("cmd-d", SplitRight, Some("Pane")),
      KeyBinding::new("cmd-shift-d", SplitDown, Some("Pane")),
      KeyBinding::new("cmd-w", ClosePane, Some("Pane")),
    ]);

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

        let view = cx.new(|cx| MainView::new(workspace, cx));
        cx.new(|cx| Root::new(view, window, cx))
      },
    )
    .ok();
  });
}
