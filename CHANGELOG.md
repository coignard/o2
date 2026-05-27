# Changelog

## 0.2.6

### Fixed

- Clipboard copy on Linux now correctly uses `wl-copy`, `xclip`, or `xsel` as the primary backend; `arboard` is only consulted as a last-resort fallback on systems where none of the three subprocess tools are available. Previously `arboard::Clipboard::set_text()` returned `Ok` on Wayland and X11 even though the clipboard selection was immediately lost when the temporary `Clipboard` instance was dropped, which prevented the existing subprocess fallback from ever being reached and made `Ctrl+C` silently fail in every Linux terminal (#1)
- `pipe_to()` now waits on the spawned clipboard child process to prevent zombie accumulation, and reports write success based on the actual `write_all()` result instead of treating any non-`None` return as success

## 0.2.5

### Changed

- Port decorations are no longer computed as a separate dry-run pass; operators now write ports directly during the normal scan, removing the `update_ports()` call after every edit operation
- Variable slots internally represented as `Option<char>` instead of using `'.'` as a sentinel value; external behaviour is unchanged
- UI redraws are now skipped for keystrokes that do not modify state (e.g. unrecognised characters typed while paused), reducing unnecessary terminal output

### Fixed

- BPM changes are now applied to the clock thread immediately: `next_tick` is clamped forward when the tempo increases, preventing a long stall before the first tick at the new rate
- Clock thread now drains the command channel during its sleep window via `recv_timeout` instead of blocking unconditionally; control messages (Silence, SelectOutput, etc.) are processed without waiting for the next tick
- IO event count shown in the status bar is now captured at `flush()` time instead of being read mid-frame, giving a stable value that matches the events actually dispatched on that tick; OSC and UDP queues are included in the count alongside notes and CC messages

## 0.2.4

### Added

- `inject` command now accepts a coordinate suffix: `inject:file;x;y` places the patch at the given grid position instead of the cursor; each coordinate is optional and falls back to the cursor position

### Changed

- `--monochrome` flag renamed to `--bw`; conflicts with `--contrast`
- UI now renders at a fixed 30 fps independently of the engine tick rate; previously the display only updated on BPM ticks, causing the refresh rate to drop at low tempos

### Fixed

- `inject` command now also resolves `.orca` file extensions alongside `.o2`; negative target coordinates are silently clamped to the grid boundary
- `%` operator (`op_midi_mono`) no longer clears the output port when the transposed note is out of range, preserving the previous port decoration
- Puppet mode indicator in the status bar now uses the Output style instead of the Clock style, matching the JS reference behaviour
- Custom colours set via the `color` command now apply correctly to the status bar; previously `B_MED` and `B_HIGH` overrides had no effect on the bottom two rows
- Operator output glyphs (e.g. `*` under `D`) now appear in the same render frame as the keypress rather than waiting for the next BPM tick

## 0.2.3

### Added

- MIDI input device selection; incoming 0xF8 clock pulses switch the engine to puppet mode, ticking every 6 pulses in sync with the external source instead of the internal timer; puppet mode exits after 2 s of inactivity
- MIDI transport: incoming 0xFA (Start), 0xFB (Continue), and 0xFC (Stop) messages applied to the play/pause state
- `inject` commander command (alias `in`) loads a `.o2` file from disk and writes its contents at the cursor position
- `color` commander command (alias `cl`) overrides `B_MED` and `B_HIGH` via semicolon-separated hex RGB strings (e.g. `color:72dec2;eeeeee`)

### Fixed

- `?` character could not be typed; it was intercepted as a Controls popup shortcut
- Guide overlay remained visible behind commander output when `inject` was run
- `color` command incorrectly remapped operator and output port text foreground; only `B_MED` and `B_HIGH` are affected

## 0.2.2

### Added

- `--contrast` CLI flag under the Display options group; uses colours only where they aid usability: editing elements and menus stay coloured while grid dots and crosses are rendered in white
- Guide overlay showing the full operator reference; displayed automatically on every launch and toggled with `Ctrl+G`, dismissed by pressing `Esc` or moving the cursor

### Changed

- Engine now starts unpaused by default; the guide overlay is shown on launch to orient new users before the first cursor movement
- Operators reference popup is now rendered in multiple columns
- MIDI Beat Clock and note dispatch moved to a dedicated `midi-clock` OS thread in `core::io::clock`, running a sleep-then-spin timing loop; clock jitter is now sub-10 μs and fully isolated from terminal rendering and keyboard input

### Fixed

- Note kill messages in `operators.rs` mixed `[u8; 3]` array literals and `Vec<u8>` values in the same `Vec`, causing a type inference error; all `kill_notes.push()` call sites now consistently use `vec!`

## 0.2.1

### Added

- `--monochrome` CLI flag under a new Display options group; renders the entire UI in pure black and white instead of the full colour palette, including the grid, status bar, popups, and prompt dialogs

### Fixed

- Scroll now triggers only when the cursor leaves the visible area, instead of three cells early; the previous look-ahead margin caused the view to shift before the cursor reached the viewport edge

### Changed

- Renamed `core::app` module to `core::oxygen`, `core::vm` to `core::operators`, and `core::operator` to `core::glyph`
- Split `EditorState` fields into dedicated sub-structs: `o2` for the core engine, `cursor` for selection state, and `commander` for the command prompt state
- Decomposed `draw()` in `render.rs` into `draw_grid()` and `draw_status_bar()`
- Decomposed `draw_popup_content()` into individual per-variant functions
- Extracted `handle_popup_key()` into a standalone function in `input.rs`
- Extracted `parse_command()` helper in `commander.rs`
- Extracted `grid_bounds()` helper method in `cursor.rs`
- Extracted `BPM_MIN` and `BPM_MAX` constants in `clock.rs`
- Extracted `MIDI_NOTE_ON`, `MIDI_NOTE_OFF`, `MIDI_CC` and related byte constants in `midi.rs`
- Version string in the status bar now reads from `CARGO_PKG_VERSION` at compile time instead of being hardcoded

### Added

- `send_clock_pulse()` method on `MidiState` for direct clock pulse dispatch, bypassing the OSC/Bidule forwarding path to preserve tight timing
- `History::with_limit()` constructor
- `editor::types` module

## 0.1.2

### Fixed

- Port decorations now update immediately on every edit operation (write, erase, cut, paste, undo, redo, drag, make uppercase/lowercase, toggle comment, trigger, resize, load) instead of only when the engine was paused; `update_ports()` was previously called conditionally from the main loop after each event rather than from within the operations themselves, causing stale port highlights after edits
- `Delete` key now works identically to `Backspace` in both the main editing layer and the commander prompt
- `Ctrl+H` now deletes the last character in the commander prompt and in the main editing layer (standard terminal backspace binding)

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
