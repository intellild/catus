## ADDED Requirements

### Requirement: Content refresh minimizes lock hold time
The system SHALL extract content from `Mutex<Term>` with minimal lock hold time.

#### Scenario: Lock is released immediately after data extraction
- **WHEN** `refresh_content()` is called
- **THEN** the `Mutex<Term>` lock SHALL be acquired only for the duration of data extraction
- **AND** the lock SHALL be released before any content processing or copying

### Requirement: Wakeup event triggers content refresh
The system SHALL automatically refresh terminal content when a `Wakeup` event is received.

#### Scenario: Wakeup event updates content
- **WHEN** a `Wakeup` event is emitted by alacritty terminal
- **THEN** `Terminal` SHALL call `refresh_content()` to update `TerminalContent`
- **AND** the updated content SHALL be available for the next render cycle

### Requirement: TerminalContent contains all renderable data
The system SHALL ensure `TerminalContent` contains all data needed for rendering.

#### Scenario: Content extraction captures display state
- **WHEN** `refresh_content()` extracts content from `Term`
- **THEN** it SHALL capture: cells (with text, colors, flags), cursor position and shape, display offset, selection, and mode
- **AND** the extracted content SHALL match the current terminal state

### Requirement: Content refresh is idempotent
The system SHALL ensure consecutive content refreshes produce consistent results.

#### Scenario: Multiple refreshes with no terminal changes
- **WHEN** `refresh_content()` is called multiple times without terminal state changes
- **THEN** the resulting `TerminalContent` SHALL be identical across calls
