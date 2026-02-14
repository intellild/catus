mod explorer_view;
mod explorer_view_item;
mod tab;
mod terminal_view;
mod workspace;

use gpui::*;
use gpui_component::*;
use workspace::Workspace;

fn main() {
    let app = Application::new().with_assets(gpui_component_assets::Assets);

    app.run(move |cx| {
        // This must be called before using any GPUI Component features.
        gpui_component::init(cx);

        // 注册键盘快捷键
        cx.bind_keys([
            KeyBinding::new("ctrl-t", workspace::workspace::NewTerminal, Some("Workspace")),
            KeyBinding::new("ctrl-shift-e", workspace::workspace::NewFileExplorer, Some("Workspace")),
            KeyBinding::new("ctrl-w", workspace::workspace::CloseActiveTab, Some("Workspace")),
            KeyBinding::new("ctrl-tab", workspace::workspace::NextTab, Some("Workspace")),
            KeyBinding::new("ctrl-shift-tab", workspace::workspace::PrevTab, Some("Workspace")),
        ]);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1200.0), px(800.0)),
                    cx,
                ))),
                focus: true,
                show: true,
                ..WindowOptions::default()
            },
            |window, cx| {
                let view = cx.new(|cx| Workspace::new(window, cx));
                // This first level on the window, should be a Root.
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .ok();
    });
}
