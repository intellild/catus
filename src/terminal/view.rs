use dioxus::prelude::*;
use std::time::Duration;

use super::provider::{Modifiers, ProviderCommand, TerminalProvider, TerminalUpdate, encode_key};

/// 终端视图组件 - 使用 TerminalProvider (watch channel 版本)
#[component]
pub fn TerminalView() -> Element {
    // 默认终端尺寸
    let default_rows = 30;
    let default_cols = 100;

    // 创建 TerminalProvider，然后解构出各个 channel
    let provider = use_hook(|| TerminalProvider::new(default_rows, default_cols));

    // 分别 clone 需要的部分给不同用途
    // 后台任务需要 event_rx 和 update_rx
    let event_rx = provider.event_rx.clone();
    let update_rx = provider.update_rx.clone();

    // 键盘处理器需要 command_tx
    let command_tx = provider.command_tx.clone();

    // 终端内容状态 - 使用 Signal 存储
    let mut terminal_content = use_signal::<Option<TerminalUpdate>>(|| None);

    // 跟踪是否已聚焦
    let mut is_focused = use_signal(|| false);

    // 后台任务：监听事件和内容更新 (只 clone receiver，不 clone 整个 provider)
    use_coroutine(move |_rx: UnboundedReceiver<()>| {
        let mut event_rx = event_rx.clone();
        let update_rx = update_rx.clone();
        async move {
            loop {
                // 等待事件变化
                match event_rx.changed().await {
                    Ok(_) => {
                        let event = event_rx.borrow().clone();
                        match event {
                            alacritty_terminal::event::Event::Wakeup => {
                                // 获取最新内容
                                let update = update_rx.borrow().clone();
                                terminal_content.set(Some(update));
                            }
                            _ => {
                                // 其他事件也触发内容刷新
                                let update = update_rx.borrow().clone();
                                terminal_content.set(Some(update));
                            }
                        }
                    }
                    Err(_) => {
                        // channel 关闭，退出循环
                        break;
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

    // 键盘事件处理器 - 直接使用 Key enum
    let onkeydown = move |event: Event<KeyboardData>| {
        let key = event.key();
        let modifiers = Modifiers {
            ctrl: event.modifiers().ctrl(),
            alt: event.modifiers().alt(),
            shift: event.modifiers().shift(),
            meta: event.modifiers().meta(),
        };

        // 直接编码 Key enum 并发送
        let data = encode_key(&key, modifiers);
        if !data.is_empty() {
            if let Err(e) = command_tx.try_send(ProviderCommand::WriteData(data)) {
                eprintln!("Failed to send key: {}", e);
            }
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
