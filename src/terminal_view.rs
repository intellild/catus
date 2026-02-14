use std::path::PathBuf;

use gpui::*;
use gpui_component::{ActiveTheme, Icon, IconName, Sizable, h_flex, label::Label, v_flex};

/// 终端视图 - 基于alacritty_terminal的本地终端实现
///
/// 设计说明：
/// 1. 当前版本为基础框架，预留alacritty_terminal集成接口
/// 2. 渲染使用GPUI的Canvas进行自定义绘制
/// 3. 输入处理支持键盘和鼠标事件
pub struct TerminalView {
    /// 工作目录
    pub working_dir: PathBuf,
    /// 终端标题（通常是当前shell提示）
    pub title: String,
    /// 是否正在运行
    pub is_running: bool,
    /// 光标位置
    pub cursor_position: (usize, usize),
    /// 终端尺寸（行列数）
    pub terminal_size: (usize, usize),
    /// 滚动位置
    pub scroll_offset: usize,
    /// 选中的文本
    pub selection: Option<String>,
    /// 内容版本号（用于触发重绘）
    pub content_version: u64,
}

impl TerminalView {
    pub fn new(working_dir: PathBuf, _window: &mut Window, _cx: &mut Context<Self>) -> Self {
        let title = format!("Terminal - {}", working_dir.display());

        Self {
            working_dir,
            title,
            is_running: true,
            cursor_position: (0, 0),
            terminal_size: (80, 24),
            scroll_offset: 0,
            selection: None,
            content_version: 0,
        }
    }

    /// 获取工作目录
    pub fn working_dir(&self) -> &PathBuf {
        &self.working_dir
    }

    /// 设置终端标题
    pub fn set_title(&mut self, title: impl Into<String>, cx: &mut Context<Self>) {
        self.title = title.into();
        cx.notify();
    }

    /// 向上滚动
    fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
        self.content_version += 1;
    }

    /// 向下滚动
    fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        self.content_version += 1;
    }

    /// 获取终端标题
    pub fn title(&self) -> &str {
        &self.title
    }
}

impl Render for TerminalView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        v_flex()
            .size_full()
            .bg(theme.background)
            .child(
                // 终端标题栏
                h_flex()
                    .h(px(32.0))
                    .px_3()
                    .gap_2()
                    .items_center()
                    .border_b_1()
                    .border_color(theme.border)
                    .bg(theme.muted)
                    .child(
                        Icon::new(IconName::SquareTerminal)
                            .xsmall()
                            .text_color(theme.muted_foreground)
                    )
                    .child(
                        Label::new(self.title.clone())
                            .text_sm()
                            .text_color(theme.foreground),
                    )
                    .child(
                        div()
                            .flex_1()
                            .child(Label::new(self.working_dir.display().to_string())
                                .text_sm()
                                .text_color(theme.muted_foreground))
                    ),
            )
            .child(
                // 终端内容区域
                div()
                    .flex_1()
                    .p_4()
                    .child(
                        Label::new("Terminal ready - alacritty_terminal integration pending\n\nPress Ctrl+T to open a new Terminal tab")
                            .text_color(theme.foreground)
                    )
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                match event.keystroke.key.as_str() {
                    "up" => this.scroll_up(1),
                    "down" => this.scroll_down(1),
                    "pageup" => this.scroll_up(this.terminal_size.1 / 2),
                    "pagedown" => this.scroll_down(this.terminal_size.1 / 2),
                    _ => {}
                }
                cx.notify();
            }))
    }
}

/// SSH终端配置（预留接口）
#[derive(Clone, Debug)]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: SshAuthMethod,
    pub working_dir: PathBuf,
}

/// SSH认证方式
#[derive(Clone, Debug)]
pub enum SshAuthMethod {
    /// 密码认证
    Password(String),
    /// 私钥认证
    PrivateKey {
        key_path: PathBuf,
        passphrase: Option<String>,
    },
    /// 代理认证
    Agent,
}

/// 远程终端视图（预留接口）
///
/// 将来用于SSH远程终端
pub struct RemoteTerminalView {
    pub config: SshConfig,
    pub connection_state: ConnectionState,
    // 其他字段与TerminalView类似
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

impl RemoteTerminalView {
    pub fn new(config: SshConfig, _window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            config,
            connection_state: ConnectionState::Disconnected,
        }
    }

    /// 连接到远程服务器（预留接口）
    pub fn connect(&mut self, _cx: &mut Context<Self>) {
        // TODO: 使用russh库实现SSH连接
        self.connection_state = ConnectionState::Connecting;
    }

    /// 断开连接（预留接口）
    pub fn disconnect(&mut self, _cx: &mut Context<Self>) {
        // TODO: 断开SSH连接
        self.connection_state = ConnectionState::Disconnected;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssh_config() {
        let config = SshConfig {
            host: "example.com".to_string(),
            port: 22,
            username: "test".to_string(),
            auth_method: SshAuthMethod::Password("secret".to_string()),
            working_dir: PathBuf::from("/home/test"),
        };

        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 22);
    }

    #[test]
    fn test_connection_state() {
        let state = ConnectionState::Connected;
        assert_eq!(state, ConnectionState::Connected);
    }
}
