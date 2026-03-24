## 1. Implement Content Refresh Function

- [x] 1.1 Add `refresh_content()` method to `Terminal` struct in `src/terminal/terminal.rs`
- [x] 1.2 Implement content extraction logic: acquire lock, get renderable content from `Term`, release lock
- [x] 1.3 Convert alacritty `RenderableContent` to `TerminalContent` (cells, cursor, mode, etc.)

## 2. Integrate with Wakeup Event

- [x] 2.1 Modify `Wakeup` event handler in `Terminal::new()` to call `refresh_content()`
- [x] 2.2 Ensure `refresh_content()` updates `self.content` field
- [x] 2.3 Verify `cx.notify()` is called after content refresh to trigger re-render

## 3. Update TerminalElement

- [x] 3.1 Modify `TerminalElement::prepaint()` to use `Terminal` content directly
- [x] 3.2 Remove any direct `Mutex<Term>` access from `TerminalElement`
- [x] 3.3 Ensure `TerminalElement` receives updated content after Wakeup event

## 4. Testing & Verification

- [x] 4.1 Run cargo check to verify no compilation errors
- [x] 4.2 Run rustfmt to format code
- [x] 4.3 Test terminal rendering still works correctly after refactoring
