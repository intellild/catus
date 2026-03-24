## Context

当前终端渲染架构中，`Terminal` 结构体持有：
- `content: TerminalContent` - 可渲染内容（GPUI 线程访问）
- `term: Arc<Mutex<Term>>` - alacritty 终端状态（后台任务和 GPUI 线程共享）

当 PTY 有数据时，后台任务获取 `Mutex<Term>` 锁，调用 `advance()` 解析数据，然后发送 `Wakeup` 事件。渲染时，`TerminalElement::prepaint()` 需要从 `Term` 提取内容到 `TerminalContent`。

当前问题：
1. 内容提取逻辑分散，没有统一的刷新函数
2. 锁持有时间过长可能导致渲染卡顿

## Goals / Non-Goals

**Goals:**
- 提供统一的 `refresh_content()` 方法从 `Term` 提取内容到 `TerminalContent`
- 最小化 `Mutex<Term>` 的锁持有时间（获取数据后立即释放锁）
- 收到 `Wakeup` 事件时自动触发内容刷新

**Non-Goals:**
- 不修改 PTY 读取和事件循环架构
- 不修改 `TerminalContent` 的数据结构
- 不引入额外的缓存层或异步内容计算

## Decisions

### Decision: 在独立函数中提取内容

**选择**: 创建 `Terminal::refresh_content()` 方法，在独立作用域内获取锁、提取数据、立即释放锁。

**理由**:
- Rust 的锁在作用域结束时自动释放
- 将内容提取逻辑封装在函数内，避免在多处重复
- 便于单元测试和性能分析

**替代方案**:
- 使用 `MutexGuard` 的显式 drop：代码更冗长，容易出错
- 在后台任务中预计算内容：增加复杂性，需要额外的 channel 通信

### Decision: Wakeup 事件触发刷新

**选择**: 收到 `Wakeup` 事件时，在 `Terminal` 内部调用 `refresh_content()` 更新 `content` 字段。

**理由**:
- 内容与事件同步，确保渲染时看到的是最新状态
- `TerminalElement::prepaint()` 只需读取已更新的 `content`，无需再次访问 `Mutex<Term>`

**数据流**:
```
PTY Data → Term.advance() → Wakeup Event → refresh_content() → content updated → UI render
```

## Risks / Trade-offs

**风险**: 高频 Wakeup 事件可能导致频繁的内容刷新
- **缓解**: 内容提取操作是内存复制，时间复杂度 O(屏幕单元格数)，在现代硬件上足够快

**风险**: 如果 `refresh_content()` 失败可能导致内容不一致
- **缓解**: 使用 `Result` 返回类型，错误时保持上一帧内容

## Migration Plan

无需迁移步骤。此重构是内部实现变更，不影响外部 API。

## Open Questions

无。
