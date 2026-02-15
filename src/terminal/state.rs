use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::{
    io::Read,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;
use portable_pty::Child;

/// 终端单元格
#[derive(Clone)]
pub struct TerminalCell {
    pub c: char,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
    pub bold: bool,
}

impl Default for TerminalCell {
    fn default() -> Self {
        Self {
            c: ' ',
            fg: [212, 212, 212], // #d4d4d4
            bg: [30, 30, 30],    // #1e1e1e
            bold: false,
        }
    }
}

/// 终端状态
pub struct TerminalState {
    rows: usize,
    cols: usize,
    grid: Vec<Vec<TerminalCell>>,
    cursor_row: usize,
    cursor_col: usize,
}

impl TerminalState {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            grid: vec![vec![TerminalCell::default(); cols]; rows],
            cursor_row: 0,
            cursor_col: 0,
        }
    }

    pub fn grid(&self) -> &[Vec<TerminalCell>] {
        &self.grid
    }

    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_col)
    }

    #[allow(dead_code)]
    pub fn rows(&self) -> usize {
        self.rows
    }

    #[allow(dead_code)]
    pub fn cols(&self) -> usize {
        self.cols
    }

    fn clear(&mut self) {
        for row in &mut self.grid {
            for cell in row {
                *cell = TerminalCell::default();
            }
        }
    }

    fn scroll_up(&mut self) {
        for i in 0..self.rows - 1 {
            self.grid[i] = self.grid[i + 1].clone();
        }
        self.grid[self.rows - 1] = vec![TerminalCell::default(); self.cols];
    }

    pub fn write_char(&mut self, c: char) {
        match c {
            '\n' => {
                self.cursor_row += 1;
                if self.cursor_row >= self.rows {
                    self.scroll_up();
                    self.cursor_row = self.rows - 1;
                }
            }
            '\r' => {
                self.cursor_col = 0;
            }
            '\t' => {
                let next_tab = (self.cursor_col / 8 + 1) * 8;
                self.cursor_col = next_tab.min(self.cols - 1);
            }
            '\x08' => {
                // Backspace
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                }
            }
            c if c.is_ascii() || !c.is_control() => {
                if self.cursor_col < self.cols && self.cursor_row < self.rows {
                    self.grid[self.cursor_row][self.cursor_col] = TerminalCell {
                        c,
                        ..TerminalCell::default()
                    };
                    self.cursor_col += 1;
                    if self.cursor_col >= self.cols {
                        self.cursor_col = 0;
                        self.cursor_row += 1;
                        if self.cursor_row >= self.rows {
                            self.scroll_up();
                            self.cursor_row = self.rows - 1;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub fn handle_escape_sequence(&mut self, seq: &str) {
        // 简化的转义序列处理
        if seq.starts_with('[') {
            let params: Vec<u32> = seq[1..]
                .split(';')
                .filter_map(|s| s.parse().ok())
                .collect();
            
            if let Some(cmd) = seq.chars().last() {
                match cmd {
                    'H' | 'f' => {
                        // 移动光标
                        if params.len() >= 2 {
                            self.cursor_row = (params[0] as usize).saturating_sub(1).min(self.rows - 1);
                            self.cursor_col = (params[1] as usize).saturating_sub(1).min(self.cols - 1);
                        }
                    }
                    'J' => {
                        // 清屏
                        let mode = params.first().copied().unwrap_or(0);
                        match mode {
                            2 => self.clear(),
                            _ => {}
                        }
                    }
                    'K' => {
                        // 清除行
                        if self.cursor_row < self.rows {
                            for col in self.cursor_col..self.cols {
                                self.grid[self.cursor_row][col] = TerminalCell::default();
                            }
                        }
                    }
                    'A' => {
                        // 光标上移
                        let n = params.first().copied().unwrap_or(1) as usize;
                        self.cursor_row = self.cursor_row.saturating_sub(n);
                    }
                    'B' => {
                        // 光标下移
                        let n = params.first().copied().unwrap_or(1) as usize;
                        self.cursor_row = (self.cursor_row + n).min(self.rows - 1);
                    }
                    'C' => {
                        // 光标右移
                        let n = params.first().copied().unwrap_or(1) as usize;
                        self.cursor_col = (self.cursor_col + n).min(self.cols - 1);
                    }
                    'D' => {
                        // 光标左移
                        let n = params.first().copied().unwrap_or(1) as usize;
                        self.cursor_col = self.cursor_col.saturating_sub(n);
                    }
                    _ => {}
                }
            }
        }
    }
}

/// 运行终端，返回数据通道接收端和子进程
pub fn run_terminal(
    rows: usize, 
    cols: usize
) -> (mpsc::Receiver<(Vec<Vec<TerminalCell>>, (usize, usize))>, Box<dyn Child + Send + Sync>) {
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

    // 启动 shell
    let mut cmd = CommandBuilder::new("/bin/sh");
    cmd.arg("-l");
    let child = pair.slave.spawn_command(cmd).expect("Failed to spawn shell");

    // 获取主 PTY 的 reader
    let mut reader = pair.master.try_clone_reader().expect("Failed to clone reader");
    
    // 创建终端状态
    let term_state = Arc::new(Mutex::new(TerminalState::new(rows, cols)));
    
    // 创建数据通道
    let (data_tx, data_rx) = mpsc::channel::<(Vec<Vec<TerminalCell>>, (usize, usize))>(10);
    
    // 启动读取循环
    let term_state_for_thread = Arc::clone(&term_state);
    std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        let mut escape_buf = String::new();
        let mut in_escape = false;
        let mut last_update = std::time::Instant::now();
        
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let mut state = term_state_for_thread.lock().unwrap();
                    for &byte in &buf[..n] {
                        let c = byte as char;
                        
                        if in_escape {
                            escape_buf.push(c);
                            if c.is_ascii_alphabetic() || c == '~' {
                                state.handle_escape_sequence(&escape_buf);
                                escape_buf.clear();
                                in_escape = false;
                            }
                        } else if c == '\x1b' {
                            in_escape = true;
                            escape_buf.push('[');
                        } else {
                            state.write_char(c);
                        }
                    }
                    
                    // 定期发送更新
                    if last_update.elapsed() > Duration::from_millis(50) {
                        let grid_data = state.grid().to_vec();
                        let cursor_pos = state.cursor();
                        let _ = data_tx.try_send((grid_data, cursor_pos));
                        last_update = std::time::Instant::now();
                    }
                }
                Err(e) => {
                    eprintln!("PTY read error: {}", e);
                    break;
                }
            }
        }
    });

    (data_rx, child)
}
