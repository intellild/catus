# tmux control mode

Catus can attach a tmux session as a workspace. Windows become native tabs and
panes become native terminal views; tmux owns their lifecycle. No Tokio runtime
or per-command tmux subprocess is used.

## Connect

In **Add Workspace**, select **tmux** under **Workspace Type**, then enter your
command (tmux must be installed). For example:

```text
tmux -CC new-session -A -s work
tmux -CC attach-session -t work
tmux -L development -CC new-session -A -s work
ssh -tt -oBatchMode=yes user@host tmux -CC new-session -A -s work
```

The same command can be saved in `~/.config/catus/config.toml`:

```toml
[[workspaces]]
mode = "tmux"
command = "tmux -CC new-session -A -s work"
```

Workspace type and command are independent. **Regular** uses Catus-owned tabs
and panes; **tmux** uses the control mode client, including when your command is
a custom wrapper script. Switching type does not change the command, and Catus
does not append control flags. Use `-CC` when starting tmux on the PTY.

An empty Regular command starts the default shell. A tmux command is required.
Starting tmux inside an existing shell does not convert that workspace into a
tmux workspace. Older configuration entries without `mode` retain their former
command-based detection; saving writes an explicit mode. An explicitly selected
`regular` mode always takes precedence over the command's contents.

SSH needs a remote PTY (`-tt`) and authentication/host trust established outside
Catus. The control workspace does not implement an interactive SSH login prompt.
As with other workspace commands, arguments are separated by whitespace; shell
quoting, pipelines and expansions are not interpreted.

## Behavior

- The tab `+` button creates a tmux window. Tab selection and closing send
  `select-window` and `kill-window`.
- Cmd-D / Cmd-Shift-D split the active tmux pane; Cmd-W kills that pane, including
  the last pane in a window. These actions affect the server session.
- Window names, active windows/panes, nested split proportions and zoomed layouts
  synchronize from server notifications, including changes by another client.
- Input is sent as hexadecimal bytes (`send-keys -H`). Output notifications are
  decoded without converting terminal bytes to UTF-8. Terminal query replies
  are left to tmux to avoid replying twice.
- Attaching restores the visible screen, up to 2,000 lines of history, cursor,
  alternate-screen flag, cursor/keypad, mouse and bracketed-paste modes. This is
  a reconstructed screen, not a complete serialization of terminal state: saved
  cursors, custom tab stops and the inactive screen buffer are not imported.
- Each pane's rendered size is translated into a control-client window size.
  Other attached clients and tmux's `window-size` setting can also constrain it.
- Closing the workspace terminates only its control client. The tmux session
  survives and can be attached again. Closing a tab/pane explicitly kills the
  corresponding server window/pane.
- Disconnects and command errors appear above the terminal area. Existing tabs
  retain their last screen on disconnect. Reconnect by opening a new workspace;
  automatic reconnect is not implemented.

The parser supports the standard tmux rectangular `{}` / `[]` layout format.
Unknown future layout formats produce an error while retaining the previous
layout. tmux-specific floating panes, copy-mode UI and popup windows are not
implemented. Local integration tests have been run against tmux 3.7c; remote SSH
and older tmux versions need separate environment-specific testing.

## Implementation

```text
src/workspace_spec.rs
  hold independent mode and startup command
       ↓
src/workspace.rs — Workspace
  stable UI facade holding Box<dyn WorkspaceDelegate>
       ↓
src/workspace/delegate.rs — LocalWorkspaceDelegate
  Catus-owned tab lifecycle and local terminal creation
or
src/workspace/tmux.rs — TmuxWorkspaceDelegate
  tmux window/tab mapping, pane/view registry, snapshot reconciliation
       ↓
src/tmux/client.rs — ControlClient
  LocalPty transport, attach handshake, FIFO command/response pairing
       ↓
src/tmux/protocol.rs / layout.rs
  byte stream framing, octal decoding, recursive layout parsing
       ↓
PanePty → Terminal → TerminalView
  reuse the existing terminal parser, input and rendering
```

The UI reads tabs through Workspace methods. Tmux delegate mutations wait for
server snapshots, preserve entities for unchanged pane IDs and retain subscriptions
only for the current pane tree. The control tasks are owned by the delegate and
use weak Workspace handles. Dropping it cancels the tasks and closes pane streams.

Capture and cursor-state commands are written together as independent command
lines. Each has its own response entry, so failure of the first does not cancel
or misattribute the second. Notifications outside guarded command output are
handled independently; percent-prefixed lines inside a capture remain screen data.

## References

Rust implementations were found, so an iTerm2 fallback was not necessary:

- [tmux-cmc protocol parser](https://github.com/ArcavenAE/tmux-cmc/blob/main/src/protocol.rs),
  [reader](https://github.com/ArcavenAE/tmux-cmc/blob/main/src/reader.rs) and
  [pending queue](https://github.com/ArcavenAE/tmux-cmc/blob/main/src/queue.rs):
  persistent connection, initial handshake, ordered response delivery.
- [WezTerm tmux parser](https://github.com/wezterm/wezterm/blob/main/wezterm-escape-parser/src/tmux_cc/mod.rs):
  guarded response bodies, byte unescaping and recursive layout handling.
- [tmux control mode specification](https://github.com/tmux/tmux/wiki/Control-Mode):
  notifications, stable IDs, client sizing and hexadecimal input.

The Catus implementation adapts these protocol patterns to its existing GPUI
and Pty abstractions rather than adding a separate runtime or terminal stack.

## Verification

Normal tests use FakePty and include fragmented/binary protocol input, guarded
percent-prefixed data, errors, nested layouts, command routing, snapshot updates,
entity reuse and cleanup. Run the real-server test separately:

```bash
cargo test --features test-support real_tmux_lifecycle -- --ignored --nocapture
```

It creates a unique tmux socket with `/dev/null` configuration and cleans up only
that test server. It verifies attach, input/output, resize, split/close, tab
creation/close, detach persistence and reattach.
