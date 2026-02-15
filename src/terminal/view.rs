use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use super::provider::{Modifiers, TerminalProvider, TerminalUpdate};

/// 终端视图组件 - 使用 TerminalProvider (watch channel 版本)
#[component]
pub fn TerminalView() -> Element {
    // 默认终端尺寸
    let default_rows = 30;
    let default_cols = 100;

    // 使用 Rc<RefCell> 包装 TerminalProvider 以便在闭包中共享
    let provider = use_hook(|| {
        Rc::new(RefCell::new(TerminalProvider::new(
            default_rows,
            default_cols,
        )))
    });

    // 终端内容状态 - 使用 Signal 存储
    let mut terminal_content = use_signal::<Option<TerminalUpdate>>(|| None);

    // 跟踪是否已聚焦
    let mut is_focused = use_signal(|| false);

    // 后台任务：监听事件和内容更新 (watch channel 版本)
    let provider_for_coro = provider.clone();
    use_coroutine(move |_rx: UnboundedReceiver<()>| {
        let provider = provider_for_coro.clone();
        async move {
            loop {
                {
                    let mut p = provider.borrow_mut();

                    // 等待事件变化
                    match p.wait_for_event().await {
                        Ok(event) => {
                            match event {
                                alacritty_terminal::event::Event::Wakeup => {
                                    // 获取最新内容
                                    let update = p.get_update();
                                    terminal_content.set(Some(update));
                                }
                                _ => {
                                    // 其他事件也触发内容刷新
                                    let update = p.get_update();
                                    terminal_content.set(Some(update));
                                }
                            }
                        }
                        Err(_) => {
                            // channel 关闭，退出循环
                            break;
                        }
                    }
                }

                // 短暂休眠避免 CPU 占用过高
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
    });

    // 获取当前内容
    let content = terminal_content.read();

    // 提取渲染数据
    let (_rows, _cols, grid_data, cursor_row, cursor_col) = content
        .as_ref()
        .map(|update| {
            let rows = update.rows;
            let cols = update.cols;
            let cursor_row = update.content.cursor_row;
            let cursor_col = update.content.cursor_col;

            // 构建网格数据
            let mut grid: Vec<Vec<(char, [u8; 3], [u8; 3], bool)>> = vec![vec![]; rows];
            for row in &mut grid {
                row.resize(cols, (' ', [212, 212, 212], [30, 30, 30], false));
            }

            // 填充单元格数据
            for (row, col, c, fg, bg, bold) in &update.content.cells {
                if *row < rows && *col < cols {
                    grid[*row][*col] = (*c, *fg, *bg, *bold);
                }
            }

            (rows, cols, grid, cursor_row, cursor_col)
        })
        .unwrap_or_else(|| {
            // 默认空内容
            let rows = default_rows;
            let cols = default_cols;
            let grid: Vec<Vec<(char, [u8; 3], [u8; 3], bool)>> =
                vec![vec![(' ', [212, 212, 212], [30, 30, 30], false); cols]; rows];
            (rows, cols, grid, 0, 0)
        });

    // 键盘事件处理器
    let provider_for_key = provider.clone();
    let onkeydown = move |event: Event<KeyboardData>| {
        let key = event.key();
        let modifiers = Modifiers {
            ctrl: event.modifiers().ctrl(),
            alt: event.modifiers().alt(),
            shift: event.modifiers().shift(),
            meta: event.modifiers().meta(),
        };

        // 转换 key 为字符串
        let key_name = match key {
            Key::Character(c) => c.to_string(),
            Key::Enter => "Enter".to_string(),
            Key::Escape => "Escape".to_string(),
            Key::Tab => "Tab".to_string(),
            Key::Backspace => "Backspace".to_string(),
            Key::Delete => "Delete".to_string(),
            Key::Insert => "Insert".to_string(),
            Key::Home => "Home".to_string(),
            Key::End => "End".to_string(),
            Key::PageUp => "PageUp".to_string(),
            Key::PageDown => "PageDown".to_string(),
            Key::ArrowUp => "ArrowUp".to_string(),
            Key::ArrowDown => "ArrowDown".to_string(),
            Key::ArrowLeft => "ArrowLeft".to_string(),
            Key::ArrowRight => "ArrowRight".to_string(),
            Key::F1 => "F1".to_string(),
            Key::F2 => "F2".to_string(),
            Key::F3 => "F3".to_string(),
            Key::F4 => "F4".to_string(),
            Key::F5 => "F5".to_string(),
            Key::F6 => "F6".to_string(),
            Key::F7 => "F7".to_string(),
            Key::F8 => "F8".to_string(),
            Key::F9 => "F9".to_string(),
            Key::F10 => "F10".to_string(),
            Key::F11 => "F11".to_string(),
            Key::F12 => "F12".to_string(),
            _ => {
                let key_str = format!("{:?}", key);
                if key_str == "Space" || key_str == " " {
                    " ".to_string()
                } else {
                    key_str
                }
            }
        };

        // 同步发送按键
        let p = provider_for_key.borrow();
        if let Err(e) = p.try_send_key(&key_name, modifiers) {
            eprintln!("Failed to send key: {}", e);
        }
    };

    // 聚焦处理器
    let onfocus = move |_: Event<FocusData>| {
        is_focused.set(true);
    };

    let onblur = move |_: Event<FocusData>| {
        is_focused.set(false);
    };

    let onclick = move |_: Event<MouseData>| {
        // 点击时聚焦
    };

    let focused = is_focused();
    let border_color = if focused { "#007acc" } else { "#333" };

    rsx! {
        div {
            class: "terminal-view",
            style: "background: #1e1e1e; color: #d4d4d4; font-family: 'Monaco', 'Menlo', 'Ubuntu Mono', monospace; font-size: 14px; line-height: 1.4; padding: 8px; overflow: auto; height: 100%; white-space: pre; outline: none; border: 2px solid {border_color}; box-sizing: border-box;",

            tabindex: "0",
            onkeydown: onkeydown,
            onclick: onclick,
            onfocus: onfocus,
            onblur: onblur,
            autofocus: "true",

            {grid_data.iter().enumerate().map(|(row_idx, row)| {
                rsx! {
                    div {
                        key: "{row_idx}",
                        class: "terminal-line",
                        style: "display: flex; height: 20px;",

                        {row.iter().enumerate().map(|(col_idx, (c, fg, bg, bold))| {
                            let is_cursor = row_idx == cursor_row && col_idx == cursor_col;
                            let bg_str = if is_cursor {
                                "#007acc".to_string()
                            } else {
                                format!("rgb({}, {}, {})", bg[0], bg[1], bg[2])
                            };
                            let fg_str = format!("rgb({}, {}, {})", fg[0], fg[1], fg[2]);
                            let font_weight = if *bold { "bold" } else { "normal" };

                            rsx! {
                                span {
                                    key: "{col_idx}",
                                    style: "background: {bg_str}; color: {fg_str}; font-weight: {font_weight}; width: 9px; height: 20px; display: inline-flex; align-items: center; justify-content: center; overflow: hidden;",
                                    "{c}"
                                }
                            }
                        })}
                    }
                }
            })}
        }
    }
}
