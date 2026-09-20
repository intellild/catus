use super::{TabId, TabItem, Workspace, WorkspaceDelegate, WorkspaceState, generate_tab_id};
use crate::pane::{
  PaneGroup,
  any_view::PaneView,
  pane_node::{PaneLeafId, PaneNode, SplitDirection},
};
use crate::terminal::{LocalPty, Pty, PtyCommand, TerminalSize, TerminalView};
use crate::tmux::{
  client::{ClientEvent, ControlClient, PanePty, RequestKind, response_text},
  layout::{Layout, LayoutKind},
  protocol::{parse_id, quote, unescape},
};
use anyhow::{Result, anyhow, ensure};
use gpui::{AppContext, Context, Entity, Task};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

const WINDOWS: &str = "list-windows -F '#{window_id}\t#{window_active}\t#{pane_id}\t#{window_layout}\t#{window_visible_layout}\t#{window_name}'";
struct Pane {
  pty: Arc<PanePty>,
  view: Entity<TerminalView>,
  initialized: bool,
  capture: Option<Vec<u8>>,
  pending_output: Vec<u8>,
}
struct Window {
  id: u64,
  active: bool,
  pane: u64,
  layout: Layout,
  visible: Layout,
  name: String,
}
impl Window {
  fn parse(line: &[u8]) -> Result<Self> {
    let line = std::str::from_utf8(line)?;
    let f: Vec<_> = line.splitn(6, '\t').collect();
    ensure!(f.len() == 6, "invalid tmux window snapshot");
    Ok(Self {
      id: parse_id(f[0].as_bytes(), b'@')?,
      active: f[1] == "1",
      pane: parse_id(f[2].as_bytes(), b'%')?,
      layout: Layout::parse(f[3])?,
      visible: Layout::parse(f[4])?,
      name: f[5].into(),
    })
  }
}

pub struct TmuxWorkspaceDelegate {
  state: WorkspaceState,
  client: Option<ControlClient>,
  _events: Option<Task<()>>,
  timeout: Option<Task<()>>,
  windows: HashMap<TabId, u64>,
  panes: HashMap<u64, Pane>,
  refreshing: bool,
  dirty: bool,
}
impl TmuxWorkspaceDelegate {
  pub fn new(command: &PtyCommand, cx: &mut Context<Workspace>) -> Self {
    if matches!(command, PtyCommand::DefaultShell) {
      return Self::failed("Enter a tmux control mode command.".into());
    }
    match LocalPty::new_with_command(TerminalSize::default_size(), command.clone()) {
      Ok(pty) => Self::with_transport(Arc::new(pty), cx),
      Err(error) => Self::failed(error.to_string()),
    }
  }
  fn failed(error: String) -> Self {
    Self {
      state: WorkspaceState {
        status: Some(error),
        ..Default::default()
      },
      client: None,
      _events: None,
      timeout: None,
      windows: HashMap::new(),
      panes: HashMap::new(),
      refreshing: false,
      dirty: false,
    }
  }
  pub(super) fn with_transport(transport: Arc<dyn Pty>, cx: &mut Context<Workspace>) -> Self {
    let (client, events) = ControlClient::new(transport, cx);
    let timeout = cx.spawn(async move |workspace, cx| {
      cx.background_executor()
        .timer(std::time::Duration::from_secs(20))
        .await;
      let _ = workspace.update(cx, |ws, cx| {
        ws.handle_tmux_event(
          ClientEvent::Disconnected(
            "tmux attach timed out; check the command and SSH authentication".into(),
          ),
          cx,
        )
      });
    });
    let task = cx.spawn(async move |workspace, cx| {
      while let Ok(event) = events.recv().await {
        if workspace
          .update(cx, |ws, cx| ws.handle_tmux_event(event, cx))
          .is_err()
        {
          break;
        }
      }
    });
    Self {
      state: WorkspaceState {
        connecting: true,
        status: Some("Connecting to tmux…".into()),
        ..Default::default()
      },
      client: Some(client),
      _events: Some(task),
      timeout: Some(timeout),
      windows: HashMap::new(),
      panes: HashMap::new(),
      refreshing: false,
      dirty: false,
    }
  }
  fn send(&self, text: String, kind: RequestKind) -> Result<()> {
    self
      .client
      .as_ref()
      .ok_or_else(|| anyhow!("tmux disconnected"))?
      .sender
      .send(text, kind)
  }
  fn command(&self, text: String) -> Result<()> {
    self.send(text, RequestKind::Command)
  }
  fn refresh(&mut self) -> Result<()> {
    if self.refreshing {
      self.dirty = true;
      return Ok(());
    }
    self.send(WINDOWS.into(), RequestKind::Windows)?;
    self.refreshing = true;
    Ok(())
  }
  fn request(&mut self, command: Result<String>, cx: &mut Context<Workspace>) -> bool {
    match command.and_then(|c| self.command(c)) {
      Ok(()) => true,
      Err(error) => {
        self.state.status = Some(error.to_string());
        cx.notify();
        false
      }
    }
  }
  fn reconcile(&mut self, lines: Vec<Vec<u8>>, cx: &mut Context<Workspace>) -> Result<()> {
    // Parse everything before mutating so malformed server data cannot remove tabs.
    let windows = lines
      .iter()
      .map(|l| Window::parse(l))
      .collect::<Result<Vec<_>>>()?;
    let mut live = HashSet::new();
    let mut tabs = Vec::new();
    let mut ids = HashMap::new();
    let mut active = None;
    for window in windows {
      let mut geometry = vec![];
      window.layout.panes(&mut geometry);
      for (id, width, height) in geometry {
        live.insert(id);
        if !self.panes.contains_key(&id) {
          let sender = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow!("tmux disconnected"))?
            .sender
            .clone();
          let pty = Arc::new(PanePty::new(id, window.id, sender));
          pty.set_geometry(width, height, window.layout.width, window.layout.height);
          let view =
            Workspace::create_terminal_view_with_pty(cx, pty.clone(), format!("tmux %{id}"))
              .map_err(|e| anyhow!(e))?;
          self.panes.insert(
            id,
            Pane {
              pty,
              view,
              initialized: false,
              capture: None,
              pending_output: Vec::new(),
            },
          );
          self.client.as_ref().unwrap().sender.snapshot(id)?;
        }
        self.panes[&id]
          .pty
          .set_geometry(width, height, window.layout.width, window.layout.height);
      }
      let root = self.build_tree(&window.visible)?;
      let mut visible_panes = Vec::new();
      window.visible.panes(&mut visible_panes);
      for &(id, w, h) in &visible_panes {
        self.panes[&id]
          .pty
          .set_geometry(w, h, window.visible.width, window.visible.height);
      }
      let visible_geometry: HashMap<_, _> = visible_panes
        .into_iter()
        .map(|(id, w, h)| (PaneLeafId(id), (w, h)))
        .collect();
      let existing = self
        .windows
        .iter()
        .find(|(_, id)| **id == window.id)
        .and_then(|(tab, _)| self.state.tabs.iter().find(|t| t.id == *tab))
        .cloned();
      let tab = if let Some(mut tab) = existing {
        tab.pane_group.update(cx, |group, cx| {
          group.set_server_layout(root, PaneLeafId(window.pane), visible_geometry.clone(), cx)
        });
        tab.title_override = Some(window.name);
        tab
      } else {
        let workspace = cx.entity().downgrade();
        let group = cx.new(|cx| {
          PaneGroup::from_server_layout(
            workspace,
            root,
            PaneLeafId(window.pane),
            visible_geometry.clone(),
            cx,
          )
        });
        TabItem {
          id: generate_tab_id(),
          pane_group: group,
          title_override: Some(window.name),
        }
      };
      if window.active {
        active = Some(tab.id);
      }
      ids.insert(tab.id, window.id);
      tabs.push(tab);
    }
    self.panes.retain(|id, pane| {
      if live.contains(id) {
        true
      } else {
        pane.pty.close();
        false
      }
    });
    self.windows = ids;
    self.state.tabs = tabs;
    self.state.active_tab_id = active.or_else(|| self.state.tabs.first().map(|t| t.id));
    self.state.connecting = false;
    self.state.status = None;
    cx.notify();
    Ok(())
  }
  fn build_tree(&self, layout: &Layout) -> Result<PaneNode> {
    Ok(match &layout.kind {
      LayoutKind::Pane(id) => PaneNode::new_leaf(
        PaneLeafId(*id),
        PaneView::Terminal(
          self
            .panes
            .get(id)
            .ok_or_else(|| anyhow!("unknown pane %{id}"))?
            .view
            .clone(),
        ),
      ),
      LayoutKind::Split {
        horizontal,
        children,
      } => PaneNode::Split {
        direction: if *horizontal {
          SplitDirection::Horizontal
        } else {
          SplitDirection::Vertical
        },
        children: children
          .iter()
          .map(|l| self.build_tree(l))
          .collect::<Result<_>>()?,
      },
    })
  }
  fn event(&mut self, event: ClientEvent, cx: &mut Context<Workspace>) -> Result<()> {
    match event {
      ClientEvent::Ready => {
        self.timeout.take();
        self.command("refresh-client -f no-output".into())?;
        self.refresh()?;
        // Output is enabled after the window snapshot; capture replies form the
        // boundary between initial screen contents and incremental output.
        self.command("refresh-client -f !no-output".into())?;
      }
      ClientEvent::Response(kind, failed, lines) => {
        if failed {
          if matches!(kind, RequestKind::Windows) {
            self.refreshing = false;
            self.state.connecting = false;
          }
          return Err(anyhow!(response_text(&lines)));
        }
        match kind {
          RequestKind::Windows => {
            self.refreshing = false;
            self.reconcile(lines, cx)?;
            if std::mem::take(&mut self.dirty) {
              self.refresh()?;
            }
          }
          RequestKind::Capture(id) => {
            let mut screen = b"\x1b[0m\x1b[2J\x1b[H".to_vec();
            for (index, line) in lines.iter().enumerate() {
              if index > 0 {
                screen.extend_from_slice(b"\r\n");
              }
              screen.extend(unescape(line)?);
            }
            if let Some(pane) = self.panes.get_mut(&id) {
              pane.capture = Some(screen);
            }
          }
          RequestKind::State(id) => {
            let state = response_text(&lines);
            let values = state
              .split_whitespace()
              .map(str::parse::<u16>)
              .collect::<std::result::Result<Vec<_>, _>>()?;
            ensure!(values.len() == 13, "invalid pane state");
            if let Some(pane) = self.panes.get_mut(&id) {
              if let Some(mut screen) = pane.capture.take() {
                if values[12] != 0 {
                  let mut alternate = b"\x1b[?1049h".to_vec();
                  alternate.extend(screen);
                  screen = alternate;
                }
                screen.extend(
                  format!(
                    "\x1b[{};{}H",
                    u32::from(values[1]) + 1,
                    u32::from(values[0]) + 1
                  )
                  .bytes(),
                );
                for (index, mode) in [
                  (2, "?25"),
                  (3, "4"),
                  (4, "?1"),
                  (6, "?1000"),
                  (7, "?1002"),
                  (8, "?1003"),
                  (9, "?1005"),
                  (10, "?1006"),
                  (11, "?2004"),
                ] {
                  screen.extend(
                    format!("\x1b[{mode}{}", if values[index] == 0 { 'l' } else { 'h' }).bytes(),
                  );
                }
                screen.extend_from_slice(if values[5] == 0 { b"\x1b>" } else { b"\x1b=" });
                screen.append(&mut pane.pending_output);
                let _ = pane.pty.output.try_send(screen);
              }
              pane.initialized = true;
            }
          }
          RequestKind::Command => {}
        }
      }
      ClientEvent::Output(id, bytes) => {
        if let Some(pane) = self.panes.get_mut(&id) {
          if pane.initialized {
            let _ = pane.pty.output.try_send(bytes);
          } else if pane.capture.is_some() {
            ensure!(
              pane.pending_output.len() + bytes.len() <= 4 * 1024 * 1024,
              "tmux initial output overflow"
            );
            pane.pending_output.extend(bytes);
          }
        }
      }
      ClientEvent::Notification(line) => {
        let name = line.split_whitespace().next().unwrap_or("");
        if matches!(
          name,
          "%window-add"
            | "%window-close"
            | "%window-renamed"
            | "%layout-change"
            | "%session-changed"
            | "%session-window-changed"
            | "%window-pane-changed"
        ) {
          self.refresh()?;
        }
      }
      ClientEvent::Disconnected(reason) => {
        self.timeout.take();
        self.state.connecting = false;
        self.state.status = Some(reason);
        for pane in self.panes.values() {
          pane.pty.close();
        }
        self.client.take();
      }
    }
    cx.notify();
    Ok(())
  }
}
impl WorkspaceDelegate for TmuxWorkspaceDelegate {
  fn state(&self) -> &WorkspaceState {
    &self.state
  }
  #[cfg(test)]
  fn state_mut(&mut self) -> &mut WorkspaceState {
    &mut self.state
  }
  fn add_terminal_tab(&mut self, _: &mut Context<Workspace>) -> Result<(), String> {
    self.command("new-window".into()).map_err(|e| e.to_string())
  }
  fn close_tab(&mut self, id: TabId, cx: &mut Context<Workspace>) -> bool {
    let Some(window) = self.windows.get(&id) else {
      return false;
    };
    self.request(Ok(format!("kill-window -t @{window}")), cx)
  }
  fn activate_tab(&mut self, id: TabId, cx: &mut Context<Workspace>) -> bool {
    let Some(window) = self.windows.get(&id) else {
      return false;
    };
    self.request(Ok(format!("select-window -t @{window}")), cx)
  }
  fn set_tab_title(
    &mut self,
    id: TabId,
    title: Option<String>,
    cx: &mut Context<Workspace>,
  ) -> bool {
    let Some(window) = self.windows.get(&id) else {
      return false;
    };
    let command = match title {
      Some(title) => quote(&title).map(|title| format!("rename-window -t @{window} {title}")),
      None => Ok(format!(
        "set-window-option -t @{window} automatic-rename on"
      )),
    };
    self.request(command, cx)
  }
  fn split_pane(
    &mut self,
    pane: PaneLeafId,
    direction: SplitDirection,
    _: &mut Context<Workspace>,
  ) -> Result<Option<Entity<TerminalView>>, String> {
    self
      .command(format!(
        "split-window -{} -t %{}",
        if direction == SplitDirection::Horizontal {
          "h"
        } else {
          "v"
        },
        pane.0
      ))
      .map_err(|e| e.to_string())?;
    Ok(None)
  }
  fn close_pane(&mut self, pane: PaneLeafId, cx: &mut Context<Workspace>) -> bool {
    self.request(Ok(format!("kill-pane -t %{}", pane.0)), cx);
    false
  }
  fn focus_pane(&mut self, pane: PaneLeafId, cx: &mut Context<Workspace>) {
    self.request(Ok(format!("select-pane -t %{}", pane.0)), cx);
  }
  fn handle_tmux_event(&mut self, event: ClientEvent, cx: &mut Context<Workspace>) {
    if let Err(error) = self.event(event, cx) {
      self.state.status = Some(error.to_string());
      cx.notify();
    }
  }
}
impl Drop for TmuxWorkspaceDelegate {
  fn drop(&mut self) {
    for pane in self.panes.values() {
      pane.pty.close();
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::terminal::{FakePty, flush_pty_output};
  use crate::workspace_spec::{WorkspaceMode, WorkspaceSpec};
  use gpui::TestAppContext;

  fn workspace(cx: &mut TestAppContext) -> (Entity<Workspace>, Arc<FakePty>) {
    let fake = Arc::new(FakePty::new());
    let ws = cx.new(|cx| Workspace {
      spec: WorkspaceSpec::new(WorkspaceMode::Tmux, "tmux -CC new-session"),
      delegate: Box::new(TmuxWorkspaceDelegate::with_transport(fake.clone(), cx)),
    });
    (ws, fake)
  }
  fn response(fake: &FakePty, number: u64, text: &str) {
    fake
      .push_bytes(&format!("%begin 1 {number} 1\n{text}%end 1 {number} 1\n"))
      .unwrap();
  }
  fn connect(cx: &mut TestAppContext) -> (Entity<Workspace>, Arc<FakePty>) {
    let (ws, fake) = workspace(cx);
    cx.run_until_parked();
    assert!(
      fake.writes().is_empty(),
      "do not send commands before handshake"
    );
    response(&fake, 1, "");
    cx.run_until_parked();
    response(&fake, 2, ""); // no-output
    response(
      &fake,
      3,
      "@2\t1\t%4\tabcd,80x24,0,0,4\tabcd,80x24,0,0,4\twork\n",
    );
    response(&fake, 4, ""); // enable output
    cx.run_until_parked();
    response(&fake, 5, "hello\n");
    response(&fake, 6, "5 0 1 0 0 0 0 0 0 0 0 1 0\n");
    cx.run_until_parked();
    assert_eq!(
      ws.read_with(cx, |ws, _| ws.status().map(str::to_owned)),
      None
    );
    (ws, fake)
  }
  #[gpui::test]
  fn server_snapshot_and_actions_use_workspace_facade(cx: &mut TestAppContext) {
    let (ws, fake) = connect(cx);
    let tab = ws.read_with(cx, |ws, cx| {
      assert_eq!(ws.tabs().len(), 1);
      assert_eq!(ws.active_tab().unwrap().title(cx), "work");
      ws.active_tab_id().unwrap()
    });
    ws.update(cx, |ws, cx| {
      ws.add_terminal_tab(cx).unwrap();
      assert!(ws.activate_tab(tab, cx));
      assert!(ws.set_tab_title(tab, Some("name ; kill-server".into()), cx));
      assert!(
        ws.split_pane(PaneLeafId(4), SplitDirection::Horizontal, cx)
          .unwrap()
          .is_none()
      );
      assert!(!ws.close_pane(PaneLeafId(4), cx));
      assert!(ws.close_tab(tab, cx));
      assert_eq!(
        ws.tabs().len(),
        1,
        "wait for authoritative server notification"
      );
    });
    cx.run_until_parked();
    let written = fake.writes_string();
    assert!(written.contains("new-window\nselect-window -t @2\nrename-window -t @2 \"name ; kill-server\"\nsplit-window -h -t %4\nkill-pane -t %4\nkill-window -t @2\n"),"{written}");
  }
  #[gpui::test]
  fn layout_changes_preserve_views_and_external_close_removes_tab(cx: &mut TestAppContext) {
    let (ws, fake) = connect(cx);
    let group = ws.read_with(cx, |ws, _| ws.active_tab().unwrap().pane_group.clone());
    let first = group.read_with(cx, |g, _| g.view_for_leaf(PaneLeafId(4)).unwrap());
    fake
      .push_bytes("%layout-change @2 ignored\n%window-renamed @2 renamed\n")
      .unwrap();
    cx.run_until_parked();
    let layout = "abcd,80x24,0,0{39x24,0,0,4,40x24,40,0,8}";
    response(
      &fake,
      7,
      &format!("@2\t1\t%8\t{layout}\t{layout}\trenamed\n"),
    );
    cx.run_until_parked();
    group.read_with(cx, |g, _| {
      assert_eq!(g.leaf_count_for_test(), 2);
      assert_eq!(g.view_for_leaf(PaneLeafId(4)).unwrap(), first);
      assert_eq!(g.active_leaf_id_for_test(), Some(PaneLeafId(8)));
    });
    // Snapshot for the new pane, then coalesced layout refresh.
    response(&fake, 8, "second\n");
    response(&fake, 9, "6 0 1 0 0 0 0 0 0 0 0 0 0\n");
    response(&fake, 10, "");
    cx.run_until_parked();
    assert!(ws.read_with(cx, |ws, _| ws.tabs().is_empty()));
  }
  #[gpui::test]
  fn pane_output_is_demultiplexed_and_disconnect_keeps_last_screen(cx: &mut TestAppContext) {
    let (ws, fake) = connect(cx);
    let group = ws.read_with(cx, |ws, _| ws.active_tab().unwrap().pane_group.clone());
    fake
      .push_bytes("%output %4 \\033]2;remote-title\\007\n")
      .unwrap();
    flush_pty_output(cx);
    assert_eq!(
      group.read_with(cx, |g, cx| g.active_leaf_title(cx)),
      Some("remote-title".into())
    );
    fake.close_reader();
    flush_pty_output(cx);
    assert!(ws.read_with(cx, |ws, _| ws.status().unwrap().contains("disconnected")));
    assert_eq!(ws.read_with(cx, |ws, _| ws.tabs().len()), 1);
    ws.update(cx, |ws, cx| assert!(ws.add_terminal_tab(cx).is_err()));
  }
  #[gpui::test]
  fn tmux_answers_queries_and_failed_commands_do_not_desynchronize(cx: &mut TestAppContext) {
    let (ws, fake) = connect(cx);
    let before = fake.writes_string();
    fake.push_bytes("%output %4 \\033[6n\n").unwrap();
    flush_pty_output(cx);
    assert_eq!(
      fake.writes_string(),
      before,
      "tmux already supplies the cursor reply"
    );
    ws.update(cx, |ws, cx| ws.add_terminal_tab(cx).unwrap());
    cx.run_until_parked();
    fake
      .push_bytes("%begin 1 7 1\ncommand denied\n%error 1 7 1\n%window-renamed @2 external\n")
      .unwrap();
    cx.run_until_parked();
    assert_eq!(
      ws.read_with(cx, |ws, _| ws.status().map(str::to_owned)),
      Some("command denied".into())
    );
    response(
      &fake,
      8,
      "@2\t1\t%4\tabcd,80x24,0,0,4\tabcd,80x24,0,0,4\texternal\n",
    );
    cx.run_until_parked();
    assert_eq!(
      ws.read_with(cx, |ws, cx| ws.active_tab().unwrap().title(cx)),
      "external"
    );
  }

  #[gpui::test]
  fn drop_releases_workspace_views_and_control_transport(cx: &mut TestAppContext) {
    let (ws, fake) = connect(cx);
    let weak = ws.downgrade();
    let group = ws.read_with(cx, |ws, _| ws.active_tab().unwrap().pane_group.downgrade());
    let transport = Arc::downgrade(&fake);
    drop(ws);
    drop(fake);
    cx.update(|_| {});
    cx.run_until_parked();
    assert!(weak.upgrade().is_none());
    assert!(group.upgrade().is_none());
    assert!(transport.upgrade().is_none());
  }
  #[gpui::test]
  fn attach_errors_are_visible_without_creating_local_tabs(cx: &mut TestAppContext) {
    let (ws, fake) = workspace(cx);
    fake
      .push_bytes("%begin 1 1 0\nno sessions\n%error 1 1 0\n")
      .unwrap();
    cx.run_until_parked();
    ws.read_with(cx, |ws, _| {
      assert!(!ws.is_connecting());
      assert!(ws.tabs().is_empty());
      assert_eq!(ws.status(), Some("no sessions"));
    });
  }
}

#[cfg(test)]
mod real_tests {
  use super::*;
  use crate::workspace_spec::{WorkspaceMode, WorkspaceSpec};
  use gpui::TestAppContext;
  use std::{
    process::Command,
    time::{Duration, Instant},
  };

  struct Server(String);
  impl Server {
    fn command(&self, args: &[&str]) -> std::process::Output {
      Command::new("tmux")
        .args(["-L", &self.0])
        .args(args)
        .output()
        .unwrap()
    }
  }
  impl Drop for Server {
    fn drop(&mut self) {
      self.command(&["kill-server"]);
    }
  }
  fn wait(cx: &mut TestAppContext, mut predicate: impl FnMut(&mut TestAppContext) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
      crate::terminal::flush_pty_output(cx);
      if predicate(cx) {
        break;
      }
      assert!(Instant::now() < deadline, "timed out waiting for real tmux");
      std::thread::sleep(Duration::from_millis(10));
    }
  }
  #[gpui::test]
  #[ignore = "requires tmux and permission to create a local Unix socket"]
  fn real_tmux_lifecycle(cx: &mut TestAppContext) {
    let server = Server(format!(
      "catus-test-{}-{}",
      std::process::id(),
      std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
    ));
    let command = format!(
      "tmux -L {} -f /dev/null -CC new-session -s catus /bin/sh",
      server.0
    );
    let ws = cx.new(|cx| Workspace::new(WorkspaceSpec::new(WorkspaceMode::Tmux, &command), cx));
    wait(cx, |cx| ws.read_with(cx, |ws, _| !ws.tabs().is_empty()));
    let group = ws.read_with(cx, |ws, _| ws.active_tab().unwrap().pane_group.clone());
    let pane = group.read_with(cx, |g, _| g.active_leaf_id_for_test().unwrap());
    let view = group.read_with(cx, |g, _| g.view_for_leaf(pane).unwrap());
    let terminal = view.read_with(cx, |v, _| v.terminal_for_test());
    terminal.update(cx, |t, cx| {
      t.input(
        cx,
        "printf '\\033]2;tmux-smoke\\007ready-\\134marker-café\\n'\n"
          .as_bytes()
          .to_vec(),
      )
    });
    wait(cx, |cx| {
      view.read_with(cx, |v, cx| v.title(cx) == "tmux-smoke")
    });
    terminal.update(cx, |t, cx| {
      t.sync_size(
        gpui::Bounds {
          origin: gpui::point(gpui::px(0.), gpui::px(0.)),
          size: gpui::size(gpui::px(800.), gpui::px(480.)),
        },
        gpui::px(8.),
        gpui::px(16.),
        cx,
      );
    });
    wait(cx, |_| {
      String::from_utf8_lossy(
        &server
          .command(&[
            "display-message",
            "-p",
            "-t",
            "catus",
            "#{window_width}x#{window_height}",
          ])
          .stdout,
      )
      .trim()
        == "100x30"
    });
    ws.update(cx, |ws, cx| {
      ws.split_pane(pane, SplitDirection::Horizontal, cx).unwrap();
    });
    wait(cx, |cx| {
      group.read_with(cx, |g, _| g.leaf_count_for_test() == 2)
    });
    let second = group.read_with(cx, |g, _| g.active_leaf_id_for_test().unwrap());
    ws.update(cx, |ws, cx| {
      ws.close_pane(second, cx);
    });
    wait(cx, |cx| {
      group.read_with(cx, |g, _| g.leaf_count_for_test() == 1)
    });
    ws.update(cx, |ws, cx| ws.add_terminal_tab(cx).unwrap());
    wait(cx, |cx| ws.read_with(cx, |ws, _| ws.tabs().len() == 2));
    let tab = ws.read_with(cx, |ws, _| ws.active_tab_id().unwrap());
    ws.update(cx, |ws, cx| {
      ws.close_tab(tab, cx);
    });
    wait(cx, |cx| ws.read_with(cx, |ws, _| ws.tabs().len() == 1));
    drop(terminal);
    drop(view);
    drop(group);
    drop(ws);
    cx.update(|_| {});
    cx.run_until_parked();
    assert!(
      server
        .command(&["has-session", "-t", "catus"])
        .status
        .success(),
      "detaching must preserve the session"
    );
    let command = format!("tmux -L {} -CC attach-session -t catus", server.0);
    let restored =
      cx.new(|cx| Workspace::new(WorkspaceSpec::new(WorkspaceMode::Tmux, &command), cx));
    wait(cx, |cx| {
      restored.read_with(cx, |ws, _| !ws.tabs().is_empty() && ws.status().is_none())
    });
    let restored_view = restored.read_with(cx, |ws, cx| {
      let group = ws.active_tab().unwrap().pane_group.read(cx);
      group
        .view_for_leaf(group.active_leaf_id_for_test().unwrap())
        .unwrap()
    });
    let restored_terminal = restored_view.read_with(cx, |view, _| view.terminal_for_test());
    wait(cx, |cx| {
      restored.read_with(cx, |ws, _| {
        assert!(ws.status().is_none(), "{:?}", ws.status())
      });
      restored_terminal.update(cx, |terminal, cx| {
        terminal.refresh_content(cx);
        terminal
          .content()
          .cells
          .iter()
          .map(|c| c.cell.c)
          .collect::<String>()
          .contains("ready-\\marker-café")
      })
    });
    drop(restored_terminal);
    drop(restored_view);
    drop(restored);
    cx.update(|_| {});
    cx.run_until_parked();
  }
}
