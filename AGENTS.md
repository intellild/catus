# Catus - 终端与 SFTP 客户端

## 项目简介

Catus 是一个基于 Rust 和 GPUI 框架构建的终端与 SFTP 客户端应用程序。它采用多 Tab 工作区设计，支持同时管理多个终端会话和文件传输任务。

## 核心架构

### 整体结构

```
App (应用根)
  └── Workspace (工作区，可扩展为多个)
        └── TabItem[] (Tab 集合)
            ├── TabType::Terminal
            │       └── TerminalProvider (终端实现)
            └── TabType::SFTP (预留)
```

### 模块职责

#### 1. `main.rs` - 应用入口

- 初始化 GPUI 框架和组件库
- 创建 `App` 实例
- 打开主窗口并挂载 `MainView`

#### 2. `workspace.rs` - 工作区管理

```rust
pub struct Workspace {
    pub tabs: Vec<TabItem>,           // 直接管理所有 Tab
    pub active_tab_id: Option<TabId>, // 当前激活的 Tab
}

pub struct TabItem {
    pub id: TabId,
    pub state: Entity<TabState>,      // 标题、图标等状态
    pub tab_type: TabType,            // Tab 类型（Terminal/SFTP）
}

pub struct TabState {
    pub title: SharedString,
    pub icon: IconName,
}

pub enum TabType {
    Terminal(Entity<TerminalProvider>),
    Sftp,  // TODO: 待实现
}

pub struct App {
    pub workspace: Entity<Workspace>,  // 当前简化：只支持一个 Workspace
}
```

- 直接管理 Workspace 中的 tabs
- 提供添加/关闭/激活 Tab 的接口
- 生成唯一 Tab ID
- 支持多 Workspace（当前简化实现）

#### 3. `main_view.rs` - 主界面视图

- 渲染自定义标题栏（Tab 栏）
- 处理 Tab 点击、关闭、新建事件
- 根据激活 Tab 类型渲染对应内容
- 集成 TerminalView 显示终端

#### 4. `terminal/` - 终端模块

##### `provider.rs` - 终端后端

- 基于 `alacritty_terminal` 实现终端仿真
- 使用 `portable-pty` 创建 PTY 和 Shell 进程
- 通过 Channel 与 UI 线程通信
- 支持键盘输入、尺寸调整、关闭等操作

##### `view.rs` - 终端视图

- GPUI 组件，渲染终端内容
- 将 RenderableContent 转换为 GPUI 元素
- 处理键盘事件并发送给 Provider
- 定时刷新显示终端输出

## 数据流

```
用户输入 (TerminalView)
    ↓
输入事件 → ProviderCommand
    ↓
TerminalProvider (Channel 发送)
    ↓
后台 Worker 线程 → PTY → Shell
    ↓
Shell 输出 → PTY
    ↓
alacritty Terminal (解析 ANSI)
    ↓
watch::Receiver<TerminalUpdate> (Channel)
    ↓
TerminalView 刷新显示
```

## 关键技术

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
