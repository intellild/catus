pub mod provider;
pub mod state;
pub mod view;

#[allow(unused_imports)]
pub use provider::{
    ChannelEventListener, Modifiers, ProviderCommand, RenderableContentStatic, TerminalProvider,
    TerminalUpdate,
};

// 导出 alacritty_terminal 的 Event 类型
#[allow(unused_imports)]
pub use alacritty_terminal::event::Event;
#[allow(unused_imports)]
pub use state::{
    Modifiers as StateModifiers, TerminalHandle, TerminalWriter, cell_to_ui_cell, run_terminal,
};
pub use view::TerminalView;
