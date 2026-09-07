pub(crate) const DEFAULT_TERMINAL_TITLE: &str = "Terminal";
pub(crate) const MAX_TAB_TITLE_CHARS: usize = 32;

/// 清理来自终端应用或用户输入的标题。
///
/// 标题保留可见字符和普通空格；控制字符替换为空格，并移除首尾空白。
/// 空标题表示清除覆盖值，调用方应回退到下一层标题。
pub(crate) fn normalize_title(title: &str) -> Option<String> {
  let sanitized: String = title
    .chars()
    .map(|character| {
      if character.is_control() {
        ' '
      } else {
        character
      }
    })
    .collect();
  let trimmed = sanitized.trim();
  (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// 限制 tab 中展示的标题长度，底层模型仍保留完整标题。
pub(crate) fn truncate_title(title: &str, max_chars: usize) -> String {
  let mut characters = title.chars();
  let truncated: String = characters.by_ref().take(max_chars).collect();
  if characters.next().is_some() {
    format!("{truncated}…")
  } else {
    truncated
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn normalize_title_trims_and_replaces_control_characters() {
    assert_eq!(
      normalize_title("  project\nserver\t ").as_deref(),
      Some("project server")
    );
  }

  #[test]
  fn normalize_title_rejects_empty_values() {
    assert_eq!(normalize_title(" \r\n\t "), None);
  }

  #[test]
  fn truncate_title_preserves_short_and_unicode_titles() {
    assert_eq!(truncate_title("构建服务", 4), "构建服务");
    assert_eq!(truncate_title("构建服务日志", 4), "构建服务…");
  }
}
