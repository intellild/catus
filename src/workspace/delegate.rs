use super::{TabId, TabItem, Workspace};
use crate::pane::pane_node::{PaneLeafId, SplitDirection};
use crate::terminal::TerminalView;
use crate::tmux::client::ClientEvent;
use crate::workspace_spec::WorkspaceSpec;
use gpui::{Context, Entity};

#[derive(Default)]
pub struct WorkspaceState {
  pub tabs: Vec<TabItem>,
  pub active_tab_id: Option<TabId>,
  pub connecting: bool,
  pub status: Option<String>,
}
/// Backend operations use the Workspace context so observers retain one stable
/// entity regardless of which backend implements a workspace.
pub trait WorkspaceDelegate {
  fn state(&self) -> &WorkspaceState;
  #[cfg(test)]
  fn state_mut(&mut self) -> &mut WorkspaceState;
  fn add_terminal_tab(&mut self, cx: &mut Context<Workspace>) -> Result<(), String>;
  fn close_tab(&mut self, id: TabId, cx: &mut Context<Workspace>) -> bool;
  fn activate_tab(&mut self, id: TabId, cx: &mut Context<Workspace>) -> bool;
  fn set_tab_title(
    &mut self,
    id: TabId,
    title: Option<String>,
    cx: &mut Context<Workspace>,
  ) -> bool;
  fn split_pane(
    &mut self,
    pane: PaneLeafId,
    direction: SplitDirection,
    cx: &mut Context<Workspace>,
  ) -> Result<Option<Entity<TerminalView>>, String>;
  /// True allows the local pane tree to remove the leaf immediately. Server
  /// backends return false and reconcile their tree from notifications.
  fn close_pane(&mut self, pane: PaneLeafId, cx: &mut Context<Workspace>) -> bool;
  fn focus_pane(&mut self, _pane: PaneLeafId, _cx: &mut Context<Workspace>) {}
  fn handle_tmux_event(&mut self, _event: ClientEvent, _cx: &mut Context<Workspace>) {}
}

pub struct LocalWorkspaceDelegate {
  spec: WorkspaceSpec,
  state: WorkspaceState,
}
impl LocalWorkspaceDelegate {
  pub fn new(spec: WorkspaceSpec, cx: &mut Context<Workspace>) -> Self {
    match Workspace::make_tab(cx, &spec) {
      Ok(tab) => Self::with_tab(spec, tab),
      Err(error) => Self {
        spec,
        state: WorkspaceState {
          status: Some(error),
          ..Default::default()
        },
      },
    }
  }
  pub fn with_tab(spec: WorkspaceSpec, tab: TabItem) -> Self {
    Self {
      spec,
      state: WorkspaceState {
        active_tab_id: Some(tab.id),
        tabs: vec![tab],
        ..Default::default()
      },
    }
  }
}
impl WorkspaceDelegate for LocalWorkspaceDelegate {
  fn state(&self) -> &WorkspaceState {
    &self.state
  }
  #[cfg(test)]
  fn state_mut(&mut self) -> &mut WorkspaceState {
    &mut self.state
  }
  fn add_terminal_tab(&mut self, cx: &mut Context<Workspace>) -> Result<(), String> {
    let tab = Workspace::make_tab(cx, &self.spec)?;
    self.state.active_tab_id = Some(tab.id);
    self.state.tabs.push(tab);
    cx.notify();
    Ok(())
  }
  fn close_tab(&mut self, id: TabId, cx: &mut Context<Workspace>) -> bool {
    let Some(index) = self.state.tabs.iter().position(|t| t.id == id) else {
      return false;
    };
    self.state.tabs.remove(index);
    if self.state.active_tab_id == Some(id) {
      self.state.active_tab_id = self.state.tabs.get(index.saturating_sub(1)).map(|t| t.id);
    }
    cx.notify();
    true
  }
  fn activate_tab(&mut self, id: TabId, cx: &mut Context<Workspace>) -> bool {
    if !self.state.tabs.iter().any(|t| t.id == id) {
      return false;
    }
    self.state.active_tab_id = Some(id);
    cx.notify();
    true
  }
  fn set_tab_title(
    &mut self,
    id: TabId,
    title: Option<String>,
    cx: &mut Context<Workspace>,
  ) -> bool {
    let Some(tab) = self.state.tabs.iter_mut().find(|t| t.id == id) else {
      return false;
    };
    tab.title_override = title;
    cx.notify();
    true
  }
  fn split_pane(
    &mut self,
    _: PaneLeafId,
    _: SplitDirection,
    cx: &mut Context<Workspace>,
  ) -> Result<Option<Entity<TerminalView>>, String> {
    Workspace::create_terminal_view(cx, &self.spec).map(Some)
  }
  fn close_pane(&mut self, _: PaneLeafId, _: &mut Context<Workspace>) -> bool {
    true
  }
}
