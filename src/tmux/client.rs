use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow, ensure};
use async_channel::{Receiver, Sender, unbounded};
use async_trait::async_trait;
use gpui::{Context, Task};

use super::protocol::{Event, Parser};
use crate::terminal::{Pty, TerminalSize};
use crate::workspace::Workspace;

#[derive(Clone, Debug)]
pub enum RequestKind {
  Command,
  Windows,
  Capture(u64),
  State(u64),
}
struct Request {
  text: String,
  kinds: Vec<RequestKind>,
}
#[derive(Debug)]
pub enum ClientEvent {
  Ready,
  Response(RequestKind, bool, Vec<Vec<u8>>),
  Output(u64, Vec<u8>),
  Notification(String),
  Disconnected(String),
}
#[derive(Clone)]
pub struct CommandSender(Sender<Request>);
impl CommandSender {
  pub fn send(&self, text: String, kind: RequestKind) -> Result<()> {
    ensure!(
      !text.contains(['\n', '\r', '\0']),
      "invalid control command"
    );
    self
      .0
      .try_send(Request {
        text,
        kinds: vec![kind],
      })
      .map_err(|_| anyhow!("tmux connection closed"))
  }
  pub fn snapshot(&self, pane: u64) -> Result<()> {
    let state = [
      "cursor_x",
      "cursor_y",
      "cursor_flag",
      "insert_flag",
      "keypad_cursor_flag",
      "keypad_flag",
      "mouse_standard_flag",
      "mouse_button_flag",
      "mouse_any_flag",
      "mouse_utf8_flag",
      "mouse_sgr_flag",
      "bracket_paste_flag",
      "alternate_on",
    ];
    let format = state
      .iter()
      .map(|name| format!("#{{{name}}}"))
      .collect::<Vec<_>>()
      .join(" ");
    // Queue both commands in one write, with independent command lines so an
    // error in capture cannot cancel the state response and desynchronize FIFO.
    let text = format!(
      "capture-pane -p -e -C -S -2000 -t %{pane}\ndisplay-message -p -t %{pane} '{format}'"
    );
    self
      .0
      .try_send(Request {
        text,
        kinds: vec![RequestKind::Capture(pane), RequestKind::State(pane)],
      })
      .map_err(|_| anyhow!("tmux connection closed"))
  }
  pub fn command(&self, text: String) -> Result<()> {
    self.send(text, RequestKind::Command)
  }
  pub fn close(&self) {
    self.0.close();
  }
}

pub struct ControlClient {
  pub sender: CommandSender,
  _reader: Task<()>,
  _writer: Task<()>,
  _transport: Arc<dyn Pty>,
}
impl ControlClient {
  pub fn new(
    transport: Arc<dyn Pty>,
    cx: &mut Context<Workspace>,
  ) -> (Self, Receiver<ClientEvent>) {
    let (command_tx, command_rx) = unbounded::<Request>();
    let (event_tx, event_rx) = unbounded();
    let (ready_tx, ready_rx) = async_channel::bounded(1);
    let pending = Arc::new(Mutex::new(VecDeque::<RequestKind>::new()));
    let output = transport.reader();
    let reader_pending = pending.clone();
    let reader_events = event_tx.clone();
    let reader_commands = command_tx.clone();
    let reader = cx.spawn(async move |_, _| {
      let mut parser = Parser::default();
      let mut ready = false;
      let mut reason = "tmux disconnected".to_string();
      'read: while let Ok(bytes) = output.recv().await {
        let events = match parser.advance(&bytes) {
          Ok(events) => events,
          Err(error) => {
            reason = error.to_string();
            break;
          }
        };
        for event in events {
          let event = match event {
            Event::Response { failed, lines } if !ready => {
              if failed {
                reason = response_text(&lines);
                break 'read;
              }
              ready = true;
              let _ = ready_tx.try_send(());
              ClientEvent::Ready
            }
            Event::Response { failed, lines } => {
              let Some(kind) = reader_pending.lock().unwrap().pop_front() else {
                reason = "unexpected tmux command response".into();
                break 'read;
              };
              ClientEvent::Response(kind, failed, lines)
            }
            Event::Output { pane, bytes } => ClientEvent::Output(pane, bytes),
            Event::Notification(line) if line == "%exit" || line.starts_with("%exit ") => {
              reason = line;
              break 'read;
            }
            Event::Notification(line) => ClientEvent::Notification(line),
          };
          if reader_events.send(event).await.is_err() {
            break 'read;
          }
        }
      }
      reader_commands.close();
      let _ = reader_events.send(ClientEvent::Disconnected(reason)).await;
    });
    let writer_transport = transport.clone();
    let writer = cx.spawn(async move |_, _| {
      // Never write before -CC has disabled terminal echo and completed attach.
      if ready_rx.recv().await.is_err() {
        return;
      }
      while let Ok(request) = command_rx.recv().await {
        pending.lock().unwrap().extend(request.kinds);
        if let Err(error) = writer_transport
          .write(format!("{}\n", request.text).into_bytes())
          .await
        {
          command_rx.close();
          let _ = event_tx
            .send(ClientEvent::Disconnected(error.to_string()))
            .await;
          break;
        }
      }
    });
    (
      Self {
        sender: CommandSender(command_tx),
        _reader: reader,
        _writer: writer,
        _transport: transport,
      },
      event_rx,
    )
  }
}
impl Drop for ControlClient {
  fn drop(&mut self) {
    self.sender.close();
  }
}
pub fn response_text(lines: &[Vec<u8>]) -> String {
  lines
    .iter()
    .map(|l| String::from_utf8_lossy(l))
    .collect::<Vec<_>>()
    .join("\n")
}

/// A tmux pane has no local child process. Its input is encoded as literal hex
/// bytes and its output is routed from control notifications by the delegate.
pub struct PanePty {
  pub output: Sender<Vec<u8>>,
  input: Mutex<Option<Receiver<Vec<u8>>>>,
  commands: CommandSender,
  pane: u64,
  window: u64,
  /// Server window and pane geometry, used to scale client resizes to a window.
  geometry: Mutex<(u16, u16, u16, u16)>,
}
impl PanePty {
  pub fn new(pane: u64, window: u64, commands: CommandSender) -> Self {
    let (output, input) = unbounded();
    Self {
      output,
      input: Mutex::new(Some(input)),
      commands,
      pane,
      window,
      geometry: Mutex::new((80, 24, 80, 24)),
    }
  }
  pub fn set_geometry(&self, pane_w: u16, pane_h: u16, window_w: u16, window_h: u16) {
    *self.geometry.lock().unwrap() = (pane_w, pane_h, window_w, window_h);
  }
  pub fn close(&self) {
    self.output.close();
  }
}
#[async_trait]
impl Pty for PanePty {
  fn needs_terminal_responses(&self) -> bool {
    false
  }
  fn initial_size(&self) -> TerminalSize {
    let (w, h, _, _) = *self.geometry.lock().unwrap();
    TerminalSize::new(h, w, 0, 0)
  }
  async fn write(&self, data: Vec<u8>) -> Result<()> {
    for chunk in data.chunks(256) {
      let hex = chunk
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
      self
        .commands
        .command(format!("send-keys -H -t %{} {hex}", self.pane))?;
    }
    Ok(())
  }
  async fn resize(&self, size: TerminalSize) -> Result<()> {
    let (pw, ph, ww, wh) = *self.geometry.lock().unwrap();
    let cols = ((u32::from(size.cols) * u32::from(ww)) / u32::from(pw)).clamp(1, 10000);
    let rows = ((u32::from(size.rows) * u32::from(wh)) / u32::from(ph)).clamp(1, 10000);
    self
      .commands
      .command(format!("refresh-client -C @{}:{cols}x{rows}", self.window))
  }
  fn reader(&self) -> Receiver<Vec<u8>> {
    self
      .input
      .lock()
      .unwrap()
      .take()
      .expect("pane reader taken twice")
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[gpui::test]
  async fn pane_input_is_binary_safe_chunked_and_targeted() {
    let (tx, rx) = unbounded();
    let pty = PanePty::new(42, 7, CommandSender(tx));
    let input: Vec<u8> = (0..=255).chain([0, 255, 13]).collect();
    pty.write(input.clone()).await.unwrap();
    let mut restored = vec![];
    while let Ok(request) = rx.try_recv() {
      let hex = request.text.strip_prefix("send-keys -H -t %42 ").unwrap();
      restored.extend(hex.split(' ').map(|h| u8::from_str_radix(h, 16).unwrap()));
    }
    assert_eq!(restored, input);
    assert!(!pty.needs_terminal_responses());
    pty.set_geometry(40, 24, 80, 24);
    pty.resize(TerminalSize::new(30, 50, 0, 0)).await.unwrap();
    assert_eq!(rx.try_recv().unwrap().text, "refresh-client -C @7:100x30");
  }
}
