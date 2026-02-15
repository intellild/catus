use dioxus::prelude::*;
use dioxus::prelude::UnboundedReceiver;
use std::time::Duration;

use super::state::{run_terminal, TerminalCell};

/// 终端视图组件
#[component]
pub fn TerminalView() -> Element {
    let rows = 24;
    let cols = 80;
    
    // 终端内容信号
    let mut grid = use_signal(|| vec![vec![TerminalCell::default(); cols]; rows]);
    let mut cursor = use_signal(|| (0usize, 0usize));
    
    // 使用 use_coroutine 来处理后台任务
    use_coroutine(move |_rx: UnboundedReceiver<()>| async move {
        let (mut data_rx, child) = run_terminal(rows, cols);

        // 主循环：接收数据并更新信号
        loop {
            tokio::select! {
                Some((new_grid, new_cursor)) = data_rx.recv() => {
                    grid.set(new_grid);
                    cursor.set(new_cursor);
                }
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    // 检查子进程是否还在运行
                    match child.process_id() {
                        Some(_) => {},
                        None => break,
                    }
                }
            }
        }
    });

    // 渲染
    render_terminal(grid(), cursor())
}

/// 渲染终端
fn render_terminal(grid: Vec<Vec<TerminalCell>>, cursor: (usize, usize)) -> Element {
    let (cursor_row, cursor_col) = cursor;

    rsx! {
        div {
            class: "terminal-view",
            style: "background: #1e1e1e; color: #d4d4d4; font-family: 'Monaco', 'Menlo', 'Ubuntu Mono', monospace; font-size: 14px; line-height: 1.4; padding: 8px; overflow: auto; height: 100%; white-space: pre;",
            
            tabindex: "0",
            
            {grid.iter().enumerate().map(|(row_idx, row)| {
                rsx! {
                    div {
                        key: "{row_idx}",
                        class: "terminal-line",
                        style: "display: flex; height: 20px;",
                        
                        {row.iter().enumerate().map(|(col_idx, cell)| {
                            let is_cursor = row_idx == cursor_row && col_idx == cursor_col;
                            let bg = if is_cursor { 
                                "#007acc" 
                            } else { 
                                &format!("rgb({}, {}, {})", cell.bg[0], cell.bg[1], cell.bg[2]) 
                            };
                            let fg = format!("rgb({}, {}, {})", cell.fg[0], cell.fg[1], cell.fg[2]);
                            let font_weight = if cell.bold { "bold" } else { "normal" };
                            
                            rsx! {
                                span {
                                    key: "{col_idx}",
                                    style: "background: {bg}; color: {fg}; font-weight: {font_weight}; min-width: 9px; height: 20px; display: flex; align-items: center; justify-content: center;",
                                    "{cell.c}"
                                }
                            }
                        })}
                    }
                }
            })}
        }
    }
}
