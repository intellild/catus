//! Byte-oriented control protocol framing. Response bodies are opaque: a captured
//! screen may itself contain lines starting with `%output` or `%end`.
use anyhow::{Result, bail, ensure};

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
  Response { failed: bool, lines: Vec<Vec<u8>> },
  Output { pane: u64, bytes: Vec<u8> },
  Notification(String),
}

#[derive(Default)]
pub struct Parser {
  line: Vec<u8>,
  block: Option<(Vec<u8>, Vec<Vec<u8>>)>,
  block_bytes: usize,
}

impl Parser {
  pub fn advance(&mut self, bytes: &[u8]) -> Result<Vec<Event>> {
    let mut events = Vec::new();
    for &byte in bytes {
      if byte != b'\n' {
        ensure!(
          self.line.len() < 4 * 1024 * 1024,
          "tmux protocol line too long"
        );
        self.line.push(byte);
        continue;
      }
      let mut line = std::mem::take(&mut self.line);
      if line.last() == Some(&b'\r') {
        line.pop();
      }
      let line = line.strip_prefix(b"\x1bP1000p").unwrap_or(&line);
      if let Some((guard, body)) = &mut self.block {
        let end = line
          .strip_prefix(b"%end ")
          .map(|g| (false, g))
          .or_else(|| line.strip_prefix(b"%error ").map(|g| (true, g)));
        if let Some((failed, end)) = end
          && end == guard
        {
          let (_, lines) = self.block.take().unwrap();
          events.push(Event::Response { failed, lines });
        } else {
          ensure!(
            self.block_bytes + line.len() + 1 < 16 * 1024 * 1024,
            "tmux response too large"
          );
          self.block_bytes += line.len() + 1;
          body.push(line.to_vec());
        }
      } else if let Some(guard) = line.strip_prefix(b"%begin ") {
        let fields: Vec<_> = guard.split(|b| *b == b' ').collect();
        ensure!(
          fields.len() == 3
            && fields
              .iter()
              .all(|f| !f.is_empty() && f.iter().all(u8::is_ascii_digit)),
          "invalid tmux response guard"
        );
        self.block_bytes = 0;
        self.block = Some((guard.to_vec(), Vec::new()));
      } else if let Some(output) = line.strip_prefix(b"%output ") {
        let (pane, bytes) =
          split_once(output, b' ').ok_or_else(|| anyhow::anyhow!("invalid output"))?;
        events.push(Event::Output {
          pane: parse_id(pane, b'%')?,
          bytes: unescape(bytes)?,
        });
      } else if let Some(output) = line.strip_prefix(b"%extended-output ") {
        let (pane, rest) =
          split_once(output, b' ').ok_or_else(|| anyhow::anyhow!("invalid extended output"))?;
        let offset = rest
          .windows(3)
          .position(|w| w == b" : ")
          .ok_or_else(|| anyhow::anyhow!("invalid extended output separator"))?;
        events.push(Event::Output {
          pane: parse_id(pane, b'%')?,
          bytes: unescape(&rest[offset + 3..])?,
        });
      } else if line.starts_with(b"%") {
        events.push(Event::Notification(
          String::from_utf8_lossy(line).into_owned(),
        ));
      }
    }
    Ok(events)
  }
}

fn split_once(bytes: &[u8], delimiter: u8) -> Option<(&[u8], &[u8])> {
  let i = bytes.iter().position(|b| *b == delimiter)?;
  Some((&bytes[..i], &bytes[i + 1..]))
}

pub fn parse_id(bytes: &[u8], prefix: u8) -> Result<u64> {
  ensure!(bytes.first() == Some(&prefix), "invalid tmux id");
  Ok(std::str::from_utf8(&bytes[1..])?.parse()?)
}

pub fn unescape(bytes: &[u8]) -> Result<Vec<u8>> {
  let mut result = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] == b'\\' {
      // capture-pane -C uses vis(3)'s doubled backslash, while %output
      // uses three octal digits. Do not recursively decode the result.
      if bytes.get(i + 1) == Some(&b'\\') {
        result.push(b'\\');
        i += 2;
        continue;
      }
      let digits = bytes
        .get(i + 1..i + 4)
        .ok_or_else(|| anyhow::anyhow!("truncated tmux escape"))?;
      ensure!(
        digits[0] <= b'3' && digits.iter().all(|b| (b'0'..=b'7').contains(b)),
        "invalid tmux escape"
      );
      result.push((digits[0] - b'0') * 64 + (digits[1] - b'0') * 8 + (digits[2] - b'0'));
      i += 4;
    } else {
      result.push(bytes[i]);
      i += 1;
    }
  }
  Ok(result)
}

/// Quote a single tmux command argument, never permitting a second command.
pub fn quote(value: &str) -> Result<String> {
  if value.contains(['\n', '\r', '\0']) {
    bail!("tmux argument contains a line break or NUL");
  }
  Ok(format!(
    "\"{}\"",
    value
      .replace('\\', "\\\\")
      .replace('"', "\\\"")
      .replace('$', "\\$")
      .replace('`', "\\`")
  ))
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn capture_backslashes_are_decoded_once() {
    assert_eq!(
      unescape(br"\\033\\path\033[0m").unwrap(),
      b"\\033\\path\x1b[0m"
    );
  }
  #[test]
  fn octal_decoding_covers_every_byte() {
    let encoded = (0..=255u8)
      .map(|b| format!("\\{b:03o}"))
      .collect::<String>();
    assert_eq!(
      unescape(encoded.as_bytes()).unwrap(),
      (0..=255).collect::<Vec<u8>>()
    );
  }
  #[test]
  fn fragmented_binary_and_guarded_percent_lines() {
    let input = b"\x1bP1000p%begin 1 7 0\r\n%output %1 literal\r\n%end 2 8 1\r\n%end 1 7 0\r\n%output %3 a\\000\\377\\134\\012\n";
    let mut parser = Parser::default();
    let mut events = vec![];
    for byte in input {
      events.extend(parser.advance(&[*byte]).unwrap());
    }
    assert_eq!(
      events,
      vec![
        Event::Response {
          failed: false,
          lines: vec![b"%output %1 literal".to_vec(), b"%end 2 8 1".to_vec()]
        },
        Event::Output {
          pane: 3,
          bytes: b"a\0\xff\\\n".to_vec()
        }
      ]
    );
  }
  #[test]
  fn errors_extended_output_and_unknown_notifications() {
    assert_eq!(
      Parser::default()
        .advance(
          b"%begin 1 2 1\nno pane\n%error 1 2 1\n%extended-output %2 4 : hi\\015\n%future thing\n"
        )
        .unwrap(),
      vec![
        Event::Response {
          failed: true,
          lines: vec![b"no pane".to_vec()]
        },
        Event::Output {
          pane: 2,
          bytes: b"hi\r".to_vec()
        },
        Event::Notification("%future thing".into())
      ]
    );
    assert!(unescape(b"\\400").is_err());
    assert!(unescape(b"\\12").is_err());
    assert!(quote("name\nkill-server").is_err());
  }
}
