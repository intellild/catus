pub mod provider;
pub mod state;
pub mod view;

// 导出 alacritty_terminal 的 Event 类型
#[allow(unused_imports)]
pub use alacritty_terminal::event::Event;
// 导出 Dioxus Key 类型
#[allow(unused_imports)]
pub use dioxus::prelude::Key;

#[allow(unused_imports)]
pub use provider::{
    ChannelEventListener, Modifiers, ProviderCommand, RenderableContentStatic, TerminalProvider,
    TerminalUpdate, encode_key,
};
#[allow(unused_imports)]
pub use state::{
    Modifiers as StateModifiers, TerminalHandle, TerminalWriter, cell_to_ui_cell, run_terminal,
};
pub use view::TerminalView;
