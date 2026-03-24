## Why

当前终端渲染架构中，`TerminalContent` 与 `Term`（alacritty 终端状态）分离存储，导致内容同步复杂且容易出错。收到 `Wakeup` 事件时，需要确保从 `Mutex<Term>` 正确提取最新内容并更新 `TerminalContent`。同时，为了优化性能，需要减少锁持有时间，避免在持有锁的同时进行内容复制操作。

## What Changes

- **新增** `Terminal::refresh_content()` 方法：从 `Mutex<Term>` 提取可渲染内容到 `TerminalContent`，在独立函数内完成，最小化锁持有时间
- **修改** `Wakeup` 事件处理：收到 `Wakeup` 事件时调用 `refresh_content()` 更新内容，然后触发重绘
- **修改** `TerminalElement::prepaint()`：不再直接读取 content，依赖 Terminal 内部的状态管理
- **重构** `TerminalContent` 的更新逻辑：确保所有渲染状态都是从 `Term` 派生，避免状态不一致

## Capabilities

### New Capabilities
- `terminal-content-refresh`: 从 alacritty `Term` 提取可渲染内容到 `TerminalContent`，优化锁持有时间

### Modified Capabilities
- *(无 spec-level 行为变化，主要是内部实现重构)*

## Impact

- **src/terminal/terminal.rs**: 新增 `refresh_content()` 方法，修改 `Wakeup` 事件处理
- **src/terminal/content.rs**: 可能添加辅助方法用于从 `Term` 提取内容
- **src/terminal/terminal_element.rs**: 调整 prepaint 中的内容获取逻辑
- **性能**: 减少 `Mutex<Term>` 锁持有时间，提升渲染响应性
