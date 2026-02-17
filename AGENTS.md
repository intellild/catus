# Catus - 终端与 SFTP 客户端

## 项目简介

Catus 是一个基于 Rust 和 GPUI 框架构建的终端与 SFTP 客户端应用程序。它采用多 Tab 工作区设计，支持同时管理多个终端会话和文件传输任务。

## 技术栈

- **UI 框架**: GPUI
- **终端仿真**: alacritty_terminal
- **PTY 实现**: portable-pty
- **异步运行时**: tokio

## 关键技术细节

### 异步与并发

- 使用 `tokio` 运行时处理异步任务
- 终端 Worker 运行在独立线程
- GPUI 提供单线程 UI 更新机制

### 进程间通信

- `tokio::sync::mpsc` - Provider 命令通道
- `tokio::sync::watch` - 终端更新广播

### 终端仿真

- `alacritty_terminal` - VTE 解析和终端状态管理
- `portable-pty` - 跨平台 PTY 实现

## 编码规范

### 后台任务处理

- 无特殊要求时，使用 `cx.background_spawn()` 处理后台异步任务

### 代码格式化

- 每次修改代码后，必须运行 `rustfmt` 格式化代码

## 参考实现

### Zed 编辑器

项目根目录下的 `zed` 目录包含 Zed 编辑器的源代码。在实现 terminal 和 SSH 相关功能时，应参考 Zed 编辑器的实现方式，借鉴其设计思路和代码组织方式。

详见 skill: `zed-terminal`
