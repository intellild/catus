use alacritty_terminal::{
    grid::Dimensions,
    term::{Config, RenderableContent, Term as AlacrittyTerm},
    vte::ansi::Processor,
};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::{
    io::Read,
    sync::{Arc, Mutex},
    thread,
};
use tokio::sync::mpsc::{self, Receiver, Sender};

/// 发送给 TerminalProvider 的命令
pub enum ProviderCommand {
    /// 向 PTY 写入数据
    WriteData(Vec<u8>),
    /// 调整终端尺寸
    Resize { rows: usize, cols: usize },
    /// 关闭终端
    Shutdown,
}

/// TerminalProvider 发送给 UI 的更新
pub struct TerminalUpdate {
    /// 可渲染的终端内容
    pub content: RenderableContentStatic,
    /// 终端行数
    pub rows: usize,
    /// 终端列数
    pub cols: usize,
}

/// 静态化的 RenderableContent 数据，可以跨线程发送
pub struct RenderableContentStatic {
    /// 单元格数据: (行, 列, 字符, 前景色RGB, 背景色RGB, 是否粗体)
    pub cells: Vec<(usize, usize, char, [u8; 3], [u8; 3], bool)>,
    /// 光标行位置
    pub cursor_row: usize,
    /// 光标列位置
    pub cursor_col: usize,
    /// 光标是否可见
    pub cursor_visible: bool,
    /// 显示的行数
    pub display_lines: usize,
    /// 显示的列数
    pub display_cols: usize,
}

/// 将 alacritty Terminal 的 RenderableContent 转换为静态数据
fn convert_content_to_static(
    content: RenderableContent,
    rows: usize,
    cols: usize,
) -> RenderableContentStatic {
    let mut cells = Vec::new();

    for indexed in content.display_iter {
        let row = indexed.point.line.0 as usize;
        let col = indexed.point.column.0 as usize;
        let cell = &indexed.cell;

        let c = cell.c;

        // 获取前景色
        let fg = match cell.fg {
            alacritty_terminal::vte::ansi::Color::Named(name) => match name {
                alacritty_terminal::vte::ansi::NamedColor::Black => [0, 0, 0],
                alacritty_terminal::vte::ansi::NamedColor::Red => [255, 0, 0],
                alacritty_terminal::vte::ansi::NamedColor::Green => [0, 255, 0],
                alacritty_terminal::vte::ansi::NamedColor::Yellow => [255, 255, 0],
                alacritty_terminal::vte::ansi::NamedColor::Blue => [0, 0, 255],
                alacritty_terminal::vte::ansi::NamedColor::Magenta => [255, 0, 255],
                alacritty_terminal::vte::ansi::NamedColor::Cyan => [0, 255, 255],
                alacritty_terminal::vte::ansi::NamedColor::White => [255, 255, 255],
                alacritty_terminal::vte::ansi::NamedColor::BrightBlack => [64, 64, 64],
                alacritty_terminal::vte::ansi::NamedColor::BrightRed => [255, 64, 64],
                alacritty_terminal::vte::ansi::NamedColor::BrightGreen => [64, 255, 64],
                alacritty_terminal::vte::ansi::NamedColor::BrightYellow => [255, 255, 64],
                alacritty_terminal::vte::ansi::NamedColor::BrightBlue => [64, 64, 255],
                alacritty_terminal::vte::ansi::NamedColor::BrightMagenta => [255, 64, 255],
                alacritty_terminal::vte::ansi::NamedColor::BrightCyan => [64, 255, 255],
                alacritty_terminal::vte::ansi::NamedColor::BrightWhite => [255, 255, 255],
                alacritty_terminal::vte::ansi::NamedColor::Foreground => [212, 212, 212],
                alacritty_terminal::vte::ansi::NamedColor::Background => [30, 30, 30],
                _ => [212, 212, 212],
            },
            alacritty_terminal::vte::ansi::Color::Spec(rgb) => [rgb.r, rgb.g, rgb.b],
            _ => [212, 212, 212],
        };

        // 获取背景色
        let bg = match cell.bg {
            alacritty_terminal::vte::ansi::Color::Named(name) => match name {
                alacritty_terminal::vte::ansi::NamedColor::Black => [0, 0, 0],
                alacritty_terminal::vte::ansi::NamedColor::Red => [255, 0, 0],
                alacritty_terminal::vte::ansi::NamedColor::Green => [0, 255, 0],
                alacritty_terminal::vte::ansi::NamedColor::Yellow => [255, 255, 0],
                alacritty_terminal::vte::ansi::NamedColor::Blue => [0, 0, 255],
                alacritty_terminal::vte::ansi::NamedColor::Magenta => [255, 0, 255],
                alacritty_terminal::vte::ansi::NamedColor::Cyan => [0, 255, 255],
                alacritty_terminal::vte::ansi::NamedColor::White => [255, 255, 255],
                alacritty_terminal::vte::ansi::NamedColor::BrightBlack => [64, 64, 64],
                alacritty_terminal::vte::ansi::NamedColor::BrightRed => [255, 64, 64],
                alacritty_terminal::vte::ansi::NamedColor::BrightGreen => [64, 255, 64],
                alacritty_terminal::vte::ansi::NamedColor::BrightYellow => [255, 255, 64],
                alacritty_terminal::vte::ansi::NamedColor::BrightBlue => [64, 64, 255],
                alacritty_terminal::vte::ansi::NamedColor::BrightMagenta => [255, 64, 255],
                alacritty_terminal::vte::ansi::NamedColor::BrightCyan => [64, 255, 255],
                alacritty_terminal::vte::ansi::NamedColor::BrightWhite => [255, 255, 255],
                alacritty_terminal::vte::ansi::NamedColor::Foreground => [30, 30, 30],
                alacritty_terminal::vte::ansi::NamedColor::Background => [30, 30, 30],
                _ => [30, 30, 30],
            },
            alacritty_terminal::vte::ansi::Color::Spec(rgb) => [rgb.r, rgb.g, rgb.b],
            _ => [30, 30, 30],
        };

        let bold = cell
            .flags
            .intersects(alacritty_terminal::term::cell::Flags::BOLD);

        cells.push((row, col, c, fg, bg, bold));
    }

    // 获取光标位置（RenderableCursor 直接使用）
    let cursor = &content.cursor;
    let cursor_row = cursor.point.line.0 as usize;
    let cursor_col = cursor.point.column.0 as usize;
    // 光标可见性由 shape 控制，这里简化处理为始终可见
    let cursor_visible = true;

    let display_lines = rows;
    let display_cols = cols;

    RenderableContentStatic {
        cells,
        cursor_row,
        cursor_col,
        cursor_visible,
        display_lines,
        display_cols,
    }
}

/// Channel 事件监听器 - 将 alacritty 事件转发到 channel
pub struct ChannelEventListener {
    tx: Sender<alacritty_terminal::event::Event>,
}

impl ChannelEventListener {
    pub fn new(tx: Sender<alacritty_terminal::event::Event>) -> Self {
        Self { tx }
    }
}

impl alacritty_terminal::event::EventListener for ChannelEventListener {
    fn send_event(&self, event: alacritty_terminal::event::Event) {
        // 通知有事件发生，UI 需要刷新
        let _ = self.tx.try_send(event);
    }
}

/// 终端尺寸实现
struct TermDimensions {
    rows: usize,
    cols: usize,
}

impl Dimensions for TermDimensions {
    fn columns(&self) -> usize {
        self.cols
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn total_lines(&self) -> usize {
        self.rows
    }
}

/// TerminalProvider - 在独立线程中管理终端
///
/// 架构：
/// 1. 启动新线程，在线程中创建 alacritty Terminal 和 portable-pty
/// 2. 持续读取 PTY 输出，写入 Terminal，然后将 renderable_content 通过 channel 发送给 UI
/// 3. UI 通过 command channel 发送输入和命令
/// 4. 通过 event_rx 接收 alacritty 事件（如 Wakeup、Bell 等）
#[derive(Clone)]
pub struct TerminalProvider {
    /// 向 Provider 发送命令的通道
    pub command_tx: Sender<ProviderCommand>,
    /// 从 Provider 接收更新内容的通道（使用 Arc<Mutex> 支持 Clone）
    pub update_rx: Arc<Mutex<Receiver<TerminalUpdate>>>,
    /// 从 Provider 接收 alacritty 事件的通道（使用 Arc<Mutex> 支持 Clone）
    pub event_rx: Arc<Mutex<Receiver<alacritty_terminal::event::Event>>>,
}

impl TerminalProvider {
    /// 创建新的 TerminalProvider
    ///
    /// # Arguments
    /// * `rows` - 终端行数
    /// * `cols` - 终端列数
    pub fn new(rows: usize, cols: usize) -> Self {
        let (command_tx, command_rx) = mpsc::channel::<ProviderCommand>(100);
        let (update_tx, update_rx) = mpsc::channel::<TerminalUpdate>(10);
        let (event_tx, event_rx) = mpsc::channel::<alacritty_terminal::event::Event>(100);

        let worker_handle = thread::spawn(move || {
            run_terminal_worker(rows, cols, command_rx, update_tx, event_tx);
        });

        // 忽略 worker_handle，让它在后台运行
        let _ = worker_handle;

        Self {
            command_tx,
            update_rx: Arc::new(Mutex::new(update_rx)),
            event_rx: Arc::new(Mutex::new(event_rx)),
        }
    }

    /// 发送按键输入
    pub async fn send_key(&self, key: &str, modifiers: Modifiers) -> Result<(), String> {
        let data = encode_key(key, modifiers);
        self.command_tx
            .send(ProviderCommand::WriteData(data))
            .await
            .map_err(|e| format!("Failed to send key: {}", e))
    }

    /// 发送原始字节数据
    pub async fn write_data(&self, data: Vec<u8>) -> Result<(), String> {
        self.command_tx
            .send(ProviderCommand::WriteData(data))
            .await
            .map_err(|e| format!("Failed to write data: {}", e))
    }

    /// 调整终端尺寸
    pub async fn resize(&self, rows: usize, cols: usize) -> Result<(), String> {
        self.command_tx
            .send(ProviderCommand::Resize { rows, cols })
            .await
            .map_err(|e| format!("Failed to resize: {}", e))
    }

    /// 关闭终端
    pub async fn shutdown(&self) -> Result<(), String> {
        self.command_tx
            .send(ProviderCommand::Shutdown)
            .await
            .map_err(|e| format!("Failed to shutdown: {}", e))
    }

    /// 尝试接收更新（非阻塞）
    pub fn try_recv_update(&self) -> Option<TerminalUpdate> {
        self.update_rx.lock().ok()?.try_recv().ok()
    }

    /// 异步接收更新
    pub async fn recv_update(&self) -> Option<TerminalUpdate> {
        // 由于 MutexGuard 不能跨 await，需要特殊处理
        // 这里简化处理，使用阻塞接收
        tokio::task::block_in_place(|| self.update_rx.lock().ok()?.blocking_recv())
    }

    /// 尝试接收事件（非阻塞）
    pub fn try_recv_event(&self) -> Option<alacritty_terminal::event::Event> {
        self.event_rx.lock().ok()?.try_recv().ok()
    }

    /// 异步接收事件
    pub async fn recv_event(&self) -> Option<alacritty_terminal::event::Event> {
        tokio::task::block_in_place(|| self.event_rx.lock().ok()?.blocking_recv())
    }
}

/// 在独立线程中运行终端
fn run_terminal_worker(
    rows: usize,
    cols: usize,
    mut command_rx: Receiver<ProviderCommand>,
    update_tx: Sender<TerminalUpdate>,
    event_tx: Sender<alacritty_terminal::event::Event>,
) {
    // 创建 PTY
    let pty_system = NativePtySystem::default();
    let pair = match pty_system.openpty(PtySize {
        rows: rows as u16,
        cols: cols as u16,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to open PTY: {}", e);
            return;
        }
    };

    // 检测可用的 shell
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

    // 启动 shell
    let mut cmd = CommandBuilder::new(&shell);
    cmd.arg("-l");
    let _child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to spawn shell: {}", e);
            return;
        }
    };

    // 获取主 PTY 的 reader 和 writer
    let mut reader = match pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to clone reader: {}", e);
            return;
        }
    };

    let mut writer = match pair.master.take_writer() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Failed to take writer: {}", e);
            return;
        }
    };

    // 创建 alacritty Terminal
    let config = Config::default();
    let dimensions = TermDimensions { rows, cols };
    let event_listener = ChannelEventListener::new(event_tx);
    let mut term: AlacrittyTerm<ChannelEventListener> =
        AlacrittyTerm::new(config, &dimensions, event_listener);

    // 创建 ANSI 处理器
    let mut parser: Processor = Processor::new();

    // 缓冲区
    let mut buf = [0u8; 4096];
    let mut running = true;

    // 主循环：读取 PTY 数据和处理命令
    while running {
        // 非阻塞读取 PTY
        match reader.read(&mut buf) {
            Ok(0) => {
                // EOF - shell 退出
                running = false;
            }
            Ok(n) => {
                // 将数据写入 Terminal
                parser.advance(&mut term, &buf[..n]);

                // 发送更新给 UI
                let rows = term.screen_lines();
                let cols = term.columns();
                let content = term.renderable_content();
                let static_content = convert_content_to_static(content, rows, cols);
                let update = TerminalUpdate {
                    content: static_content,
                    rows,
                    cols,
                };

                if update_tx.try_send(update).is_err() {
                    // 如果发送失败（channel 满或关闭），继续运行
                }
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::WouldBlock {
                    eprintln!("Error reading PTY: {}", e);
                    running = false;
                }
            }
        }

        // 处理命令（非阻塞）
        while let Ok(cmd) = command_rx.try_recv() {
            match cmd {
                ProviderCommand::WriteData(data) => {
                    if writer.write_all(&data).is_err() || writer.flush().is_err() {
                        running = false;
                        break;
                    }
                }
                ProviderCommand::Resize { rows, cols } => {
                    // 调整 PTY 尺寸
                    let _ = pair.master.resize(PtySize {
                        rows: rows as u16,
                        cols: cols as u16,
                        pixel_width: 0,
                        pixel_height: 0,
                    });
                    // 调整 Terminal 尺寸
                    term.resize(TermDimensions { rows, cols });
                }
                ProviderCommand::Shutdown => {
                    running = false;
                }
            }
        }

        // 短暂休眠避免 CPU 占用过高
        thread::sleep(std::time::Duration::from_millis(1));
    }

    // 发送最后一次更新
    let rows = term.screen_lines();
    let cols = term.columns();
    let content = term.renderable_content();
    let static_content = convert_content_to_static(content, rows, cols);
    let _ = update_tx.try_send(TerminalUpdate {
        content: static_content,
        rows,
        cols,
    });
}

/// 修饰键状态
#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

/// 编码按键为字节序列
fn encode_key(key: &str, modifiers: Modifiers) -> Vec<u8> {
    match key {
        // 单字符输入（字母、数字、符号）
        c if c.len() == 1 => {
            let ch = c.chars().next().unwrap();

            // 处理 Ctrl 修饰符
            if modifiers.ctrl && ch.is_ascii_alphabetic() {
                let byte = ch.to_ascii_lowercase() as u8;
                vec![byte - b'a' + 1] // Ctrl+A = 0x01
            } else {
                c.as_bytes().to_vec()
            }
        }

        // 功能键
        "Enter" => vec![b'\r'],
        "Escape" => vec![0x1b],
        "Tab" => vec![b'\t'],
        "Backspace" => vec![0x08],
        "Delete" => vec![0x1b, b'[', b'3', b'~'],
        "Insert" => vec![0x1b, b'[', b'2', b'~'],

        // 方向键
        "ArrowUp" => vec![0x1b, b'[', b'A'],
        "ArrowDown" => vec![0x1b, b'[', b'B'],
        "ArrowRight" => vec![0x1b, b'[', b'C'],
        "ArrowLeft" => vec![0x1b, b'[', b'D'],

        // Home/End
        "Home" => vec![0x1b, b'[', b'H'],
        "End" => vec![0x1b, b'[', b'F'],

        // Page Up/Down
        "PageUp" => vec![0x1b, b'[', b'5', b'~'],
        "PageDown" => vec![0x1b, b'[', b'6', b'~'],

        // 功能键 F1-F12
        "F1" => vec![0x1b, b'[', b'1', b'1', b'~'],
        "F2" => vec![0x1b, b'[', b'1', b'2', b'~'],
        "F3" => vec![0x1b, b'[', b'1', b'3', b'~'],
        "F4" => vec![0x1b, b'[', b'1', b'4', b'~'],
        "F5" => vec![0x1b, b'[', b'1', b'5', b'~'],
        "F6" => vec![0x1b, b'[', b'1', b'7', b'~'],
        "F7" => vec![0x1b, b'[', b'1', b'8', b'~'],
        "F8" => vec![0x1b, b'[', b'1', b'9', b'~'],
        "F9" => vec![0x1b, b'[', b'2', b'0', b'~'],
        "F10" => vec![0x1b, b'[', b'2', b'1', b'~'],
        "F11" => vec![0x1b, b'[', b'2', b'3', b'~'],
        "F12" => vec![0x1b, b'[', b'2', b'4', b'~'],

        // 空格
        " " => vec![b' '],

        _ => key.as_bytes().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_key_basic() {
        assert_eq!(encode_key("a", Modifiers::default()), vec![b'a']);
        assert_eq!(encode_key("Enter", Modifiers::default()), vec![b'\r']);
        assert_eq!(encode_key("Tab", Modifiers::default()), vec![b'\t']);
    }

    #[test]
    fn test_encode_key_ctrl() {
        let mut mods = Modifiers::default();
        mods.ctrl = true;
        assert_eq!(encode_key("a", mods), vec![0x01]); // Ctrl+A
        assert_eq!(encode_key("c", mods), vec![0x03]); // Ctrl+C
    }

    #[test]
    fn test_encode_key_arrows() {
        assert_eq!(
            encode_key("ArrowUp", Modifiers::default()),
            vec![0x1b, b'[', b'A']
        );
        assert_eq!(
            encode_key("ArrowDown", Modifiers::default()),
            vec![0x1b, b'[', b'B']
        );
    }
}
