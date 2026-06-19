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

## Git Hooks (optional, recommended)

A pre-commit hook runs `cargo fmt --check`, `cargo check`, `cargo clippy`, and
`cargo test` whenever `.rs` / `Cargo.toml` / `Cargo.lock` changes are staged.
Install it once per clone:

```bash
./scripts/install-git-hooks.sh
```

Skip it for a single commit with `git commit --no-verify`, and uninstall with
`git config --unset core.hooksPath`.

Running tests requires the `test-support` feature: `cargo test --features test-support`.

## Visual Tests (Midscene)

End-to-end visual tests drive the running Catus app with
[Midscene](https://midscenejs.com/) via `@midscene/computer` (AI-powered desktop
automation) and are run with [Rstest](https://rstest.rs/). Tests live in
`e2e/`, the app-launch fixture in `fixtures/`, and the rest of the
JS tooling (`package.json`, `rstest.config.ts`, `setup.ts`, `tsconfig.json`,
`.env.example`) sits at the repo root.

### Prerequisites

- Node.js >= 20.19.0 and pnpm.
- A Rust toolchain (the tests launch the locally built `catus` binary).
- **macOS:** grant Accessibility permission to the app that runs the tests
  (Terminal / iTerm / your editor) under
  *System Settings > Privacy & Security > Accessibility*, otherwise Midscene
  cannot control the keyboard and mouse. See
  [Midscene macOS setup](https://midscenejs.com/zh/computer-getting-started.html).
- An OpenAI-compatible multimodal model with visual grounding (e.g. Qwen3-VL,
  GLM-4.6V, Doubao Seed, Gemini 3.x). See the
  [model strategy](https://midscenejs.com/zh/model-strategy.html).

### Setup

Install JS dependencies and configure the model:

```bash
pnpm install
cp .env.example .env   # then fill in your model credentials
```

The `.env` file provides `MIDSCENE_MODEL_BASE_URL`, `MIDSCENE_MODEL_API_KEY`,
`MIDSCENE_MODEL_NAME`, and `MIDSCENE_MODEL_FAMILY`. It is loaded automatically
by `setup.ts` (registered via `setupFiles` in `rstest.config.ts`).

### Run

```bash
pnpm test        # builds the catus binary (cargo build) then runs the visual tests
pnpm test:watch  # same, in watch mode
```

Midscene writes HTML reports and dumps under `midscene_run/` (git-ignored).

### Debug logging

The catus binary logs via `tracing`. Its output target is controlled by the
`CATUS_LOG_DIR` environment variable:

- **Set `CATUS_LOG_DIR`** to a directory path → the app writes its logs to
  `<dir>/catus.log`. The e2e harness writes its orchestration log to
  `<dir>/e2e.log` (each AI action/assert, query results, and the catus process
  stdout/stderr). This is the recommended way to diagnose flaky visual tests.
- **Unset** → the app logs to the standard streams: `WARN`/`ERROR` to stderr,
  `INFO`/`DEBUG`/`TRACE` to stdout. Useful when running `catus` directly from a
  terminal.

```bash
CATUS_LOG_DIR=logs pnpm test   # capture both e2e.log and catus.log under logs/
./target/debug/catus           # logs go to stdout/stderr
```

The log level is controlled by `RUST_LOG` (default `catus=debug,warn` when
`CATUS_LOG_DIR` is set, otherwise `catus=info,warn`). The default `logs/` and
`.e2e-debug/` directories are git-ignored; if you point `CATUS_LOG_DIR`
elsewhere, make sure that path is ignored too.

## Disclaimer

> **This project is vibe coded without careful review. Use it at your own risk.**

The code was written iteratively and may contain bugs, rough edges, or incomplete
behavior. It is not audited or production-ready. Do not rely on it for anything
important without reviewing the code yourself first.
