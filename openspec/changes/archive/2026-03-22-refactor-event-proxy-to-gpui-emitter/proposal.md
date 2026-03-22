## Why

The `EventProxy` struct currently uses an `async_channel::Sender` to forward alacritty `Term` events to a background task via a channel. This introduces unnecessary indirection — a channel + receiver loop — when GPUI's `EventEmitter` trait already provides the same pub/sub mechanism natively. The second `cx.spawn` loop that consumes `events_rx` is currently a stub that does nothing with the events. Replacing the channel-based proxy with direct `cx.emit()` calls simplifies the event flow and aligns with how the rest of the codebase (and GPUI idioms) handle entity-to-view communication.

## What Changes

- Remove the `EventProxy` struct and its `EventListener` impl that forwards alacritty events over an `async_channel`.
- Remove the `(events_tx, events_rx)` channel and the stub `cx.spawn` loop that consumes it.
- Introduce a new `EventProxy` that holds a GPUI `Entity<Terminal>` handle and an `AppContext` (or equivalent) to call `cx.emit()` directly when alacritty fires events (Title, Wakeup, Bell, Exit).
- Map alacritty `Event` variants to `TerminalEvent` variants inside the new proxy's `send_event` impl.
- Wire `process_event` logic into the new proxy so events are handled immediately rather than queued.
- Remove `async_channel` dependency from `terminal.rs` (if no longer used elsewhere).

## Capabilities

### New Capabilities

_None — this is a pure internal refactor with no new user-facing capabilities._

### Modified Capabilities

_None — no spec-level behavior changes._

## Impact

- **Code**: `src/terminal/terminal.rs` — primary changes to `EventProxy`, `Terminal::new`, removal of channel plumbing and stub consumer task.
- **Code**: `src/terminal/content.rs` — remove `impl EventEmitter<TerminalEvent> for TerminalContent` if no longer needed (events now emitted from `Terminal` only).
- **Dependencies**: `async_channel` crate may be removable from `terminal.rs` imports if no other usage remains.
- **APIs**: No public API changes. `Terminal` already implements `EventEmitter<TerminalEvent>`.
