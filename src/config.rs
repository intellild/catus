//! Workspace 列表的 TOML 配置。
//!
//! 配置文件默认位于 `~/.config/catus/config.toml`。每个 workspace 对应一个
//! `[[workspaces]]` 条目，`command` 为启动命令：空字符串表示系统默认 shell，
//! 非空时按空白拆分为程序 + 参数（例如 `ssh user@host`）。
//! 启动时按条目顺序创建 workspace 并激活第一个；新增/关闭 workspace 时整体写回。

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 单个 workspace 的配置条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceConfig {
  /// 启动命令。空字符串表示系统默认 shell。
  pub command: String,
}

/// 应用配置：workspace 启动命令列表。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
  #[serde(default)]
  pub workspaces: Vec<WorkspaceConfig>,
}

impl Default for AppConfig {
  /// 默认配置：一个使用系统默认 shell 的本地 workspace。
  fn default() -> Self {
    Self {
      workspaces: vec![WorkspaceConfig {
        command: String::new(),
      }],
    }
  }
}

impl AppConfig {
  /// 从 TOML 文件读取配置。
  ///
  /// 文件不存在时写入默认配置并返回；读取或解析失败时返回默认配置，
  /// 但不覆盖已有文件内容，便于人工修复。
  pub fn load(path: &Path) -> Self {
    match fs::read_to_string(path) {
      Ok(content) => match toml::from_str(&content) {
        Ok(config) => config,
        Err(e) => {
          tracing::warn!(
            target: "catus",
            "failed to parse config {}: {} — using defaults",
            path.display(),
            e
          );
          Self::default()
        }
      },
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
        let config = Self::default();
        if let Err(e) = config.save(path) {
          tracing::warn!(
            target: "catus",
            "failed to write default config {}: {}",
            path.display(),
            e
          );
        }
        config
      }
      Err(e) => {
        tracing::warn!(
          target: "catus",
          "failed to read config {}: {} — using defaults",
          path.display(),
          e
        );
        Self::default()
      }
    }
  }

  /// 以 TOML 写回配置文件，父目录不存在时自动创建。
  pub fn save(&self, path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(self)
      .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    fs::write(path, content)
  }
}

/// 默认配置文件路径：`~/.config/catus/config.toml`。
pub fn default_config_path() -> PathBuf {
  let home = std::env::var("HOME")
    .map(PathBuf::from)
    .unwrap_or_else(|_| PathBuf::from("."));
  home.join(".config").join("catus").join("config.toml")
}

#[cfg(test)]
mod tests {
  use super::*;

  /// 每个测试独立的临时配置路径，避免并行测试互相干扰。
  fn temp_config_path(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("catus-config-test-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir.join("config.toml")
  }

  #[test]
  fn default_config_has_single_empty_command() {
    let config = AppConfig::default();
    assert_eq!(config.workspaces.len(), 1);
    assert_eq!(config.workspaces[0].command, "");
  }

  #[test]
  fn save_and_load_round_trips() {
    let path = temp_config_path("round-trip");
    let config = AppConfig {
      workspaces: vec![
        WorkspaceConfig {
          command: String::new(),
        },
        WorkspaceConfig {
          command: "ssh user@host".into(),
        },
        WorkspaceConfig {
          command: "/bin/zsh -l".into(),
        },
      ],
    };
    config.save(&path).unwrap();
    assert_eq!(AppConfig::load(&path), config);
    let _ = fs::remove_dir_all(path.parent().unwrap());
  }

  #[test]
  fn load_missing_file_writes_default() {
    let path = temp_config_path("bootstrap");
    assert!(!path.exists());
    let loaded = AppConfig::load(&path);
    assert_eq!(loaded, AppConfig::default());
    assert!(path.exists());
    // 再次加载应读到刚写入的默认文件
    assert_eq!(AppConfig::load(&path), AppConfig::default());
    let _ = fs::remove_dir_all(path.parent().unwrap());
  }

  #[test]
  fn load_invalid_toml_keeps_file_and_returns_default() {
    let path = temp_config_path("invalid");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "workspaces = 42").unwrap();
    let loaded = AppConfig::load(&path);
    assert_eq!(loaded, AppConfig::default());
    // 解析失败不覆盖原文件
    assert_eq!(fs::read_to_string(&path).unwrap(), "workspaces = 42");
    let _ = fs::remove_dir_all(path.parent().unwrap());
  }

  #[test]
  fn load_empty_file_yields_empty_workspaces() {
    let path = temp_config_path("empty");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "").unwrap();
    let loaded = AppConfig::load(&path);
    assert!(loaded.workspaces.is_empty());
    let _ = fs::remove_dir_all(path.parent().unwrap());
  }

  #[test]
  fn saved_toml_uses_workspaces_array_entries() {
    let path = temp_config_path("format");
    AppConfig::default().save(&path).unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert_eq!(content, "[[workspaces]]\ncommand = \"\"\n");
    let _ = fs::remove_dir_all(path.parent().unwrap());
  }
}
