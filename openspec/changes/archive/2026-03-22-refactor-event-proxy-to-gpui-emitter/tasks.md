## 1. Extend TerminalEvent

- [x] 1.1 Add `Bell` variant to `TerminalEvent` enum in `src/terminal/content.rs`

## 2. Fix the alacritty event consumer loop

- [x] 2.1 Replace the stub `cx.spawn` consumer loop in `Terminal::new` with a real one that calls `entity.update(cx, |terminal, cx| { ... })` to process each alacritty `Event`
- [x] 2.2 Map alacritty events inside the consumer: `Title(s)` → set `self.title`, emit `TitleChanged(s)`; `Bell` → emit `Bell`; `Exit`/`ChildExit` → emit `Closed`; `Wakeup` → `cx.notify()`; others → no-op

## 3. Remove dead code

- [x] 3.1 Remove `Terminal::process_event` static method (now inlined into the consumer loop)
- [x] 3.2 Remove `impl EventEmitter<TerminalEvent> for TerminalContent` in `content.rs` if `TerminalContent` never emits events directly (only `Terminal` does)

## 4. Verify

- [x] 4.1 Run `cargo check` to confirm compilation
- [x] 4.2 Run `cargo clippy` to confirm no warnings on changed code
