use dioxus::prelude::UnboundedReceiver;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use super::state::{Modifiers, cell_to_ui_cell, run_terminal};

/// 终端视图组件
#[component]
pub fn TerminalView() -> Element {
    // 默认终端尺寸
    let default_rows = 30;
    let default_cols = 100;

    // 使用 RefCell 包装 TerminalHandle 以便在闭包中可变借用
    let handle = use_hook(|| Rc::new(RefCell::new(run_terminal(default_rows, default_cols))));

    // 触发重绘的信号
    let mut refresh = use_signal(|| 0u64);

    // 跟踪是否已聚焦
    let mut is_focused = use_signal(|| false);

    // 后台任务：监听事件并触发重绘
    let handle_for_coro = handle.clone();
    use_coroutine(move |_rx: UnboundedReceiver<()>| {
        let handle = handle_for_coro.clone();
        async move {
            loop {
                // 短暂睡眠避免 CPU 占用过高
                tokio::time::sleep(Duration::from_millis(16)).await;

                // 检查是否有新事件
                let has_event = {
                    let mut h = handle.borrow_mut();
                    h.try_recv_event().is_some()
                };

                if has_event {
                    // 触发重绘
                    refresh.with_mut(|v| *v = v.wrapping_add(1));
                }
            }
        }
    });

    // 读取当前 refresh 值来触发重新渲染
    let _ = refresh();

    // 获取当前尺寸用于渲染
    let (rows, cols) = {
        let h = handle.borrow();
        h.size()
    };

    // 收集所有单元格数据用于渲染
    let mut grid_data: Vec<Vec<(char, [u8; 3], [u8; 3], bool)>> = vec![vec![]; rows];
    let mut cursor_pos = (0, 0);

    {
        let h = handle.borrow();
        h.with_renderable_content(|content| {
            // 获取光标位置
            cursor_pos = (
                content.cursor.point.line.0 as usize,
                content.cursor.point.column.0 as usize,
            );

            // 初始化每行
            for row in &mut grid_data {
                row.resize(cols, (' ', [212, 212, 212], [30, 30, 30], false));
            }

            // 遍历所有单元格
            for indexed in content.display_iter {
                let line = indexed.point.line.0 as usize;
                let column = indexed.point.column.0 as usize;

                if line < rows && column < cols {
                    let (c, fg, bg, bold) = cell_to_ui_cell(indexed.cell);
                    grid_data[line][column] = (c, fg, bg, bold);
                }
            }
        });
    }

    // 键盘事件处理器
    let handle_for_key = handle.clone();
    let onkeydown = move |event: Event<KeyboardData>| {
        let key = event.key();
        let modifiers = Modifiers {
            ctrl: event.modifiers().ctrl(),
            alt: event.modifiers().alt(),
            shift: event.modifiers().shift(),
            meta: event.modifiers().meta(),
        };

        let mut h = handle_for_key.borrow_mut();
        let key_str = format!("{:?}", key);

        // 转换 key 为字符串进行处理
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
            _ if key_str == "Space" || key_str == " " => " ".to_string(),
            _ => key_str,
        };

        // 发送按键到终端
        if let Err(e) = h.writer.send_key(&key_name, modifiers) {
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
                            let is_cursor = row_idx == cursor_pos.0 && col_idx == cursor_pos.1;
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
