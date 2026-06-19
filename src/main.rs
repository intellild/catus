use gpui::*;
use gpui_component::{Root, Theme, ThemeMode};
use tracing::info;

mod add_workspace_dialog;
mod app;
mod logging;
mod main_view;
mod pane;
mod sidebar;
mod terminal;
mod title_bar;
mod workspace;
mod workspace_kind;

use app::App as CatusApp;
use main_view::MainView;
use pane::{ClosePane, SplitDown, SplitRight};
use terminal::view::{CopySelection, PasteFromClipboard, Tab, TabPrev};

fn main() {
  logging::init();
  info!(target: "catus", "starting catus");

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

    let catus_app = cx.new(CatusApp::new);

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

        let view = cx.new(|cx| MainView::new(catus_app.clone(), cx));
        cx.new(|cx| Root::new(view, window, cx))
      },
    )
    .ok();
  });
}
