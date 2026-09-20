use anyhow::{Result, ensure};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
  pub width: u16,
  pub height: u16,
  pub kind: LayoutKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutKind {
  Pane(u64),
  Split {
    horizontal: bool,
    children: Vec<Layout>,
  },
}
impl Layout {
  pub fn parse(value: &str) -> Result<Self> {
    let (_, value) = value
      .split_once(',')
      .ok_or_else(|| anyhow::anyhow!("missing layout checksum"))?;
    let mut input = value.as_bytes();
    let layout = Self::node(&mut input, 0)?;
    ensure!(input.is_empty(), "trailing layout data");
    Ok(layout)
  }
  fn node(input: &mut &[u8], depth: usize) -> Result<Self> {
    ensure!(depth < 64, "tmux layout too deeply nested");
    let width = number(input)?.try_into()?;
    take(input, b'x')?;
    let height = number(input)?.try_into()?;
    ensure!(width > 0 && height > 0, "empty tmux pane");
    take(input, b',')?;
    number(input)?;
    take(input, b',')?;
    number(input)?;
    let delimiter = *input
      .first()
      .ok_or_else(|| anyhow::anyhow!("incomplete layout"))?;
    *input = &input[1..];
    let kind = match delimiter {
      b',' => LayoutKind::Pane(number(input)?),
      b'{' | b'[' => {
        let mut children = vec![];
        loop {
          children.push(Self::node(input, depth + 1)?);
          if input.first() == Some(&b',') {
            *input = &input[1..];
          } else {
            break;
          }
        }
        take(input, if delimiter == b'{' { b'}' } else { b']' })?;
        ensure!(children.len() >= 2, "invalid split");
        LayoutKind::Split {
          horizontal: delimiter == b'{',
          children,
        }
      }
      _ => anyhow::bail!("unsupported tmux layout"),
    };
    Ok(Self {
      width,
      height,
      kind,
    })
  }
  pub fn panes(&self, output: &mut Vec<(u64, u16, u16)>) {
    match &self.kind {
      LayoutKind::Pane(id) => output.push((*id, self.width, self.height)),
      LayoutKind::Split { children, .. } => {
        for child in children {
          child.panes(output);
        }
      }
    }
  }
}
fn number(input: &mut &[u8]) -> Result<u64> {
  let end = input
    .iter()
    .position(|b| !b.is_ascii_digit())
    .unwrap_or(input.len());
  let value = std::str::from_utf8(&input[..end])?.parse()?;
  *input = &input[end..];
  Ok(value)
}
fn take(input: &mut &[u8], byte: u8) -> Result<()> {
  ensure!(input.first() == Some(&byte), "invalid layout delimiter");
  *input = &input[1..];
  Ok(())
}
#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn nested_layout_preserves_ids_and_geometry() {
    let l =
      Layout::parse("abcd,120x40,0,0{60x40,0,0,0,59x40,61,0[59x20,61,0,4,59x19,61,21,8]}").unwrap();
    let mut panes = vec![];
    l.panes(&mut panes);
    assert_eq!(panes, vec![(0, 60, 40), (4, 59, 20), (8, 59, 19)]);
    assert!(Layout::parse("abcd,80x24,0,0{80x24,0,0,1}").is_err());
    assert!(Layout::parse("abcd,80x24,0,0,1garbage").is_err());
  }
}
