# Changelog

## 0.1.1

### Added

- OSC and UDP output extracted into dedicated `Osc` and `Udp` structs under `core/io`
- `send_midi_msg()` method on `MidiState` centralising all outgoing MIDI dispatch
- Plogue Bidule support: `--osc-midi-bidule` CLI flag mirrors outgoing MIDI as OSC packets to a configurable path
- `ip:` commander command to set the destination IP address for OSC and UDP output
- `osc:` commander command to set the OSC output port
- Tied note operator `_` for infinite sustain via `usize::MAX` length with `saturating_sub` decay
- Explicit note kill via length `0` in the `:` and `%` operators
- Retrigger prevention for already-playing tied notes to avoid restarting the attack envelope

### Changed

- `osc_stack` and `udp_stack` fields on `MidiState` replaced by `osc: Osc` and `udp: Udp`
- `udp_port` field replaced by `ip: String` and per-protocol port fields on `Osc`/`Udp`
- Note Off messages in `op_midi_mono` now collected and dispatched through `send_midi_msg()` instead of writing directly to the connection
- Note length clamp corrected from 32 to 35 to cover the full base-36 range

## 0.1.0

### Added

- Cursor navigation to prompt dialogs including mid-string editing and support for Left/Right/Home/End/Delete keys
- Blinking cursor indicator with autocomplete rendering in prompt dialogs
- Autocomplete functionality using the Tab key for path prompts, including a preview renderer
- Unsaved changes detection system tracking history modifications via `saved_absolute_index` and `offset`
- `ConfirmQuit` popup dialog triggered by Ctrl+Q with save, save-as, quit, and cancel options
- Panic safety mechanism using `TerminalGuard` RAII for terminal cleanup upon crashes
- `emergency_save()` function to preserve grid data during unexpected crashes
- Persistent scroll state (`scroll_x`/`scroll_y`) supporting a keyboard scrolling margin of up to 3 cells
- Mouse awareness for scrolling to track input types and suppress unwanted scroll shifts after clicks
- `darken()` helper function to the theme system for autocomplete text colors
- ROFL COPTER!!!

### Changed

- Refactored Arvelie-Neralie time calculations to use the `chrono` crate for correct local-time formatting
- Refactored `vm.rs` operators to use a unified `VmContext`, significantly reducing parameter repetition
- Extracted `run_app()` from the `main()` function to provide cleaner separation of concerns
- Refactored prompt text rendering to use `enumerate()` instead of maintaining a manual counter
- Updated `ratatui` API calls to utilize newer implementations like `Style::new()` and `Block::bordered()`
- Modified mouse scrolling behavior to respect Slide mode, functioning as a drag instead of a move
- Updated visual behavior of the terminal: cursor hides on startup, resets color on exit, and clears on resize to avoid rendering artifacts
- Updated documentation comments project-wide, changing references from 'Orca' to 'ORCΛ'

### Fixed

- Out-of-bounds cursor movement by properly clamping targets based on selection dimensions (width and height)
- Backspace key behavior in prompts to delete characters exactly at the cursor position rather than only from the end of the string
- Issue where `saved_absolute_index` was not being set after successfully opening a file
- Bug causing a duplicate `ConfirmQuit` popup to appear when pressing Ctrl+Q
- Save menu item logic to automatically open the SaveAs prompt when no file is currently open
- Resize event handling to correctly preserve ports and port names instead of resetting them, using grid-aware dimensions
- Drag interactions to ensure ports and locks are preserved when moving a selection block
- `scale_cursor` implementation to accurately move the anchor point, enabling proper rubber-band resize semantics
- Erase tool logic to ensure ports and locks are cleared alongside standard cell glyphs
