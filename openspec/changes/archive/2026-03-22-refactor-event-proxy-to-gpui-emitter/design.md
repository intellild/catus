## Context

The `Terminal` entity currently uses two mechanisms for alacritty event handling:

1. **`EventProxy`** — a channel-based `EventListener` impl that sends alacritty `Event`s over an `async_channel::Sender` from inside the `Term<EventProxy>` lock.
2. **A stub consumer loop** — a second `cx.spawn` that receives from the channel but does nothing (`entity.read_with(cx, |this, cx| {})`).

Meanwhile, `Terminal` already implements `EventEmitter<TerminalEvent>` and the PTY reader task already emits `TerminalEvent::Wakeup` via `entity.update(cx, |_, cx| cx.emit(...))`. The channel-based proxy and its consumer are dead code that adds complexity without function.

**Constraint**: alacritty's `EventListener::send_event(&self, event)` is called from within the `Term` processing under an `Arc<Mutex<Term>>` lock. It takes `&self`, not `&mut self`, and cannot directly access a GPUI `Context`. Some form of signaling out of the lock is still required.

## Goals / Non-Goals

**Goals:**
- Remove the dead `events_rx` consumer loop
- Wire the `EventProxy` channel into the existing `entity.update()` pattern so alacritty events (Title, Bell, Exit, etc.) are mapped to `TerminalEvent` variants and emitted via GPUI
- Consolidate event handling: one consumer loop that handles both PTY data and alacritty events, or two focused loops that both emit via `cx.emit()`
- Extend `TerminalEvent` with variants needed for alacritty events (Bell, ClipboardStore, etc.) as appropriate

**Non-Goals:**
- Eliminating the channel entirely — `send_event(&self)` is called under a mutex lock, so we need a lightweight signaling mechanism to cross the lock boundary. A channel remains appropriate for this.
- Handling all alacritty `Event` variants — only implement the ones relevant to the current terminal feature set (Title, Wakeup, Bell, Exit). Others can remain no-ops with TODO comments.
- Changing the PTY reader architecture or the rendering pipeline.

## Decisions

### 1. Keep the channel, fix the consumer

**Decision**: Retain `async_channel` for `EventProxy::send_event` but replace the stub consumer with a real one that maps alacritty events to `TerminalEvent` and emits them.

**Rationale**: The channel is necessary because `send_event` is called under the `Term` mutex lock and takes `&self`. We can't hold a GPUI context reference inside the proxy. The channel is the lightest-weight way to cross this boundary. What's broken is the consumer, not the producer.

**Alternative considered**: Using `std::sync::mpsc` or a `crossbeam` channel instead of `async_channel`. Not worth the dependency change — `async_channel` is already used and integrates well with the async `cx.spawn` loop.

### 2. Merge the event consumer into the PTY reader loop or keep separate

**Decision**: Keep two separate `cx.spawn` loops — one for PTY data, one for alacritty events.

**Rationale**: The PTY reader processes bulk data and calls `term.advance()`. The alacritty event consumer handles discrete events. Merging them would require `select!` or similar, adding complexity. Two simple loops are easier to reason about.

### 3. Map alacritty events to TerminalEvent in the consumer

**Decision**: The consumer loop calls `entity.update(cx, |terminal, cx| { ... })` where it matches on the alacritty `Event` and either mutates `Terminal` state (e.g., `self.title = title`) or emits a `TerminalEvent`.

**Event mapping**:
| alacritty Event | Action |
|---|---|
| `Title(s)` | Set `self.title = s`, emit `TerminalEvent::TitleChanged(s)` |
| `Wakeup` | `cx.notify()` (already handled by PTY reader, this is a secondary signal) |
| `Bell` | Emit `TerminalEvent::Bell` (add variant) |
| `Exit` | Emit `TerminalEvent::Closed` |
| `ChildExit(code)` | Emit `TerminalEvent::Closed` |
| Others | No-op for now |

### 4. Add `Bell` variant to TerminalEvent

**Decision**: Add `TerminalEvent::Bell` so the view layer can respond (e.g., visual bell, sound).

## Risks / Trade-offs

- **[Risk] Channel backpressure under heavy event load** → Mitigated by using unbounded channel (already the case). Alacritty events are low-frequency (title changes, bell) so this is not a concern.
- **[Risk] Duplicate Wakeup signals** → The PTY reader already emits `TerminalEvent::Wakeup` after `term.advance()`. Alacritty may also fire `Event::Wakeup`. The consumer can simply call `cx.notify()` for the alacritty wakeup without emitting a duplicate event.
- **[Trade-off] Two spawn loops vs one** → Slightly more spawned tasks, but clearer separation of concerns. The overhead of an extra async task is negligible.
