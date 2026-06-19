# Catus

A local terminal client built with [Rust](https://www.rust-lang.org/) and [GPUI](https://github.com/zed-industries/gpui).

## Features

- **Multiple workspaces** — a left sidebar lists all workspaces; switch, close, or add new ones.
  - **Local** workspaces launch the system default shell.
  - **SSH** workspaces launch a local `ssh` process (e.g. `ssh user@host`) as the command — they reuse the local PTY, so no remote PTY / SFTP runtime is involved.
- Multiple tabs and horizontal/vertical pane splitting per workspace.
- Terminal input, scrolling, selection, copy, and paste.

## Tech Stack

- UI: `gpui`, `gpui-component`
- Terminal emulation: `alacritty_terminal`
- PTY: `portable-pty`
- Async & IPC: GPUI task API, `async-channel`, `async-lock`, `async-trait`

## Getting Started

```bash
cargo run
```

Requires a Rust toolchain. On first build, dependencies are fetched automatically.

## Disclaimer

> **This project is vibe coded without careful review. Use it at your own risk.**

The code was written iteratively and may contain bugs, rough edges, or incomplete
behavior. It is not audited or production-ready. Do not rely on it for anything
important without reviewing the code yourself first.
