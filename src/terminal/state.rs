use alacritty_terminal::{
    event::{Event, EventListener},
    grid::{Dimensions, GridCell},
    sync::FairMutex,
    term::{Config, RenderableContent, Term as AlacrittyTerm, cell::Cell},
    vte::ansi::Processor,
};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::{
    io::{Read, Write},
    sync::Arc,
};
use tokio::sync::mpsc;

/// 事件监听器 - 将 alacritty 事件转发到 channel
pub struct ChannelEventListener {
    tx: mpsc::Sender<Event>,
}

impl ChannelEventListener {
    pub fn new(tx: mpsc::Sender<Event>) -> Self {
        Self { tx }
    }
}

impl EventListener for ChannelEventListener {
    fn send_event(&self, event: Event) {
        // 通知有事件发生，UI 需要刷新
        let _ = self.tx.try_send(event);
    }
}

/// 终端尺寸信息
pub struct TermDimensions {
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

/// 终端写入器 - 用于向 PTY 发送输入
pub struct TerminalWriter {
    writer: Box<dyn Write + Send>,
}

impl TerminalWriter {
    pub fn new(writer: Box<dyn Write + Send>) -> Self {
        Self { writer }
    }

    /// 写入原始字节数据
    pub fn write(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()
    }

    /// 写入字符串
    #[allow(dead_code)]
    pub fn write_str(&mut self, s: &str) -> std::io::Result<()> {
        self.write(s.as_bytes())
    }

    /// 发送按键输入（处理特殊按键）
    pub fn send_key(&mut self, key: &str, modifiers: Modifiers) -> std::io::Result<()> {
        let data = match key {
            // 单字符输入（字母、数字、符号）
            c if c.len() == 1 => {
                let ch = c.chars().next().unwrap();
                let mut data = vec![];

                // 处理 Ctrl 修饰符
                if modifiers.ctrl && ch.is_ascii_alphabetic() {
                    let byte = ch.to_ascii_lowercase() as u8;
                    data.push(byte - b'a' + 1); // Ctrl+A = 0x01
                } else {
                    data.extend_from_slice(c.as_bytes());
                }
                data
            }

            // 功能键
            "Enter" => vec![b'\r'],
            "Escape" => vec![0x1b],
            "Tab" => vec![b'\t'],
            "Backspace" => vec![0x08], // 退格键
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
        };

        self.write(&data)
    }
}

/// 修饰键状态
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

/// 终端状态句柄 - 包含 Term 和通知通道
pub struct TerminalHandle {
    pub term: Arc<FairMutex<AlacrittyTerm<ChannelEventListener>>>,
    pub event_rx: mpsc::Receiver<Event>,
    pub writer: TerminalWriter,
    _child: Box<dyn portable_pty::Child + Send + Sync>,
}

impl TerminalHandle {
    /// 在持有锁的情况下执行渲染
    pub fn with_renderable_content<F, R>(&self, f: F) -> R
    where
        F: FnOnce(RenderableContent<'_>) -> R,
    {
        let term = self.term.lock();
        let content = term.renderable_content();
        f(content)
    }

    /// 检查是否有新事件（非阻塞）
    pub fn try_recv_event(&mut self) -> Option<Event> {
        self.event_rx.try_recv().ok()
    }

    /// 获取终端尺寸
    pub fn size(&self) -> (usize, usize) {
        let term = self.term.lock();
        (term.screen_lines(), term.columns())
    }

    /// 调整终端尺寸
    #[allow(dead_code)]
    pub fn resize(&self, _rows: usize, _cols: usize) {
        // 暂不实现
    }

    /// 初始化终端 - 发送一个换行触发 shell 显示提示符
    pub fn init(&mut self) {
        // 发送一个空命令来触发 shell 输出提示符
        let _ = self.writer.write(b"\r");
    }
}

/// 将 alacritty Cell 转换为 UI 可渲染的简单格式
pub fn cell_to_ui_cell(cell: &Cell) -> (char, [u8; 3], [u8; 3], bool) {
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
            alacritty_terminal::vte::ansi::NamedColor::Foreground => [212, 212, 212],
            alacritty_terminal::vte::ansi::NamedColor::Background => [30, 30, 30],
            _ => [30, 30, 30],
        },
        alacritty_terminal::vte::ansi::Color::Spec(rgb) => [rgb.r, rgb.g, rgb.b],
        _ => [30, 30, 30],
    };

    let bold = cell
        .flags()
        .intersects(alacritty_terminal::term::cell::Flags::BOLD);

    (c, fg, bg, bold)
}

/// 运行终端，返回终端句柄
pub fn run_terminal(rows: usize, cols: usize) -> TerminalHandle {
    // 创建 PTY
    let pty_system = NativePtySystem::default();
    let pair = pty_system
        .openpty(PtySize {
            rows: rows as u16,
            cols: cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("Failed to open PTY");

    // 检测可用的 shell
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

    // 启动 shell
    let mut cmd = CommandBuilder::new(&shell);
    cmd.arg("-l");
    let child = pair
        .slave
        .spawn_command(cmd)
        .expect("Failed to spawn shell");

    // 获取主 PTY 的 reader 和 writer
    let mut reader = pair
        .master
        .try_clone_reader()
        .expect("Failed to clone reader");

    let writer = pair.master.take_writer().expect("Failed to take writer");

    // 创建通道 - 用于接收 alacritty 事件
    let (event_tx, event_rx) = mpsc::channel::<Event>(100);

    // 创建 alacritty 配置
    let config = Config::default();

    // 创建事件监听器
    let event_listener = ChannelEventListener::new(event_tx.clone());

    // 创建 Term 实例
    let dimensions = TermDimensions { rows, cols };
    let term = Arc::new(FairMutex::new(AlacrittyTerm::new(
        config,
        &dimensions,
        event_listener,
    )));

    // 创建 ANSI 处理器
    let mut parser: Processor = Processor::new();

    // 启动读取循环线程
    let term_for_thread = Arc::clone(&term);
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];

        loop {
            match reader.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    // 锁定终端并处理输入
                    let mut terminal = term_for_thread.lock();
                    parser.advance(&mut *terminal, &buf[..n]);
                    drop(terminal);
                    // 发送事件通知 UI 更新
                    let _ = event_tx.try_send(Event::Wakeup);
                }
                Err(_) => break,
            }
        }
    });

    let mut handle = TerminalHandle {
        term,
        event_rx,
        writer: TerminalWriter::new(writer),
        _child: child,
    };

    // 初始化终端
    handle.init();

    handle
}
