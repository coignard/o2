// This file is part of o2.
//
// Copyright (c) 2026  René Coignard <contact@renecoignard.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! The o2 grid: cells, locks, ports, variables, and core grid operations.
//!
//! This module contains the [`EditorState`] struct, which is the single source of
//! truth for the entire application state, covering both the grid engine and
//! the client/UI state.
//!
//! # Grid representation
//!
//! The grid is stored as a flat `Vec<char>` of width `w` and height `h`, indexed
//! row-major: cell `(x, y)` lives at index `y * w + x`. Empty cells hold `'.'`.
//!
//! Each frame, [`EditorState::operate`] scans the grid for operator glyphs, runs them in
//! reading order (top-to-bottom, left-to-right), and writes their outputs back.
//! Cells may be *locked* by an operator to prevent other operators from
//! overwriting them during the same frame.

use arboard::Clipboard;
use std::path::PathBuf;

use crate::core::io::MidiState;
use crate::editor::history::History;
use crate::ui::theme::StyleType;

/// The text-entry mode of the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Standard navigation; character keys write glyphs and do not advance the
    /// cursor automatically.
    Normal,
    /// Insert mode: writing a glyph advances the cursor one step to the right.
    Append,
    /// Rectangle selection is active; arrow keys extend the selection bounds.
    Selection,
    /// Slide mode: arrow keys drag the current selection across the grid.
    Slide,
}

/// Identifies the intent of a [`PopupType::Prompt`] dialogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptPurpose {
    /// The user is entering a file path to open.
    Open,
    /// The user is entering a file path to save to.
    SaveAs {
        /// When `true`, the application exits after the file is saved successfully.
        quit_after: bool,
    },
    /// The user is entering a BPM value.
    SetBpm,
    /// The user is entering a new grid size in `WxH` notation.
    SetGridSize,
}

/// Describes which overlay is currently visible on top of the grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopupType {
    /// Keyboard shortcut reference card.
    Controls,
    /// Operator glyph reference card.
    Operators,
    /// About / version information screen.
    About {
        /// The time the popup was opened, used for animation.
        opened_at: std::time::Instant,
    },
    /// Main application menu with item selection.
    MainMenu {
        /// Index of the currently highlighted menu item.
        selected: usize,
    },
    /// MIDI output device picker.
    MidiMenu {
        /// Index of the currently highlighted device.
        selected: usize,
        /// Names of all available output devices.
        devices: Vec<String>,
    },
    /// Confirmation dialogue shown before erasing the grid.
    ConfirmNew {
        /// `0` = Cancel, `1` = Create New File.
        selected: usize,
    },
    /// Auto-fit grid size selection.
    AutofitMenu {
        /// `0` = Nicely (snap to grid), `1` = Tightly (fit content exactly).
        selected: usize,
    },
    /// Clock and timing settings.
    ClockMenu {
        /// Index of the currently highlighted option (currently always `0`).
        selected: usize,
    },
    /// Confirmation dialogue shown when quitting with unsaved changes.
    ConfirmQuit {
        /// Index of the currently highlighted option.
        selected: usize,
        /// `true` if a file is currently open (adds a "Save" option).
        has_file: bool,
    },
    /// Single-line text input dialogue.
    Prompt {
        /// What the input will be used for.
        purpose: PromptPurpose,
        /// Text the user has typed so far.
        input: String,
        /// Current cursor index in the input string.
        cursor: usize,
    },
    /// Generic informational or error message overlay.
    Msg {
        /// Short title shown in the border.
        title: String,
        /// Body text; may contain newlines.
        text: String,
    },
    /// ROFL COPTER!!!
    RoflCopter,
}

/// Core execution engine state. Pure data containing the grid and execution variables.
#[derive(Debug)]
pub struct Engine {
    /// Width of the grid in columns.
    pub w: usize,
    /// Height of the grid in rows.
    pub h: usize,
    /// Flat row-major buffer of all grid glyphs; length is always `w * h`.
    pub cells: Vec<char>,
    /// Per-cell lock flags; a locked cell cannot be overwritten by another
    /// operator during the current frame.
    pub locks: Vec<bool>,
    /// Per-cell port type used by the renderer to colour operator inputs and
    /// outputs. `None` means the cell has no port decoration.
    pub ports: Vec<Option<StyleType>>,
    /// Per-cell port name and originating operator glyph, used to populate the
    /// inspector in the status bar.
    pub port_names: Vec<Option<(&'static str, char)>>,
    /// Global variable store indexed by ASCII character code.
    /// Slots that have not been written hold `'.'`.
    pub variables: [char; 128],
    /// Current frame counter, incremented once per clock tick.
    pub f: usize,
    /// Internal state for the xorshift64-based pseudo-random number generator.
    pub rng_state: u64,
    /// Reusable operator list populated each frame to avoid per-tick heap allocations.
    pub(crate) ops_cache: Vec<(usize, usize, char)>,
}

impl Engine {
    /// Creates a new engine with a blank `w`-by-`h` grid and the given RNG seed.
    pub fn new(w: usize, h: usize, seed: u64) -> Self {
        Self {
            w,
            h,
            cells: vec!['.'; w * h],
            locks: vec![false; w * h],
            ports: vec![None; w * h],
            port_names: vec![None; w * h],
            variables: ['.'; 128],
            f: 0,
            rng_state: seed,
            ops_cache: Vec::with_capacity(256),
        }
    }
}

/// The complete application state: engine, cursor, MIDI, history, and UI overlays.
pub struct EditorState {
    /// The core grid engine containing cells, locks, ports, and the frame counter.
    pub engine: Engine,

    /// Horizontal spacing between grid marker lines.
    pub grid_w: usize,
    /// Vertical spacing between grid marker lines.
    pub grid_h: usize,

    /// Horizontal scroll offset: the grid column shown at the left edge of the viewport.
    pub scroll_x: usize,
    /// Vertical scroll offset: the grid row shown at the top edge of the viewport.
    pub scroll_y: usize,

    /// Cursor column.
    pub cx: usize,
    /// Cursor row.
    pub cy: usize,
    /// Selection width (may be negative for a leftward selection).
    pub cw: isize,
    /// Selection height (may be negative for an upward selection).
    pub ch: isize,
    /// Left edge of the normalised selection bounding box.
    pub min_x: usize,
    /// Right edge of the normalised selection bounding box.
    pub max_x: usize,
    /// Top edge of the normalised selection bounding box.
    pub min_y: usize,
    /// Bottom edge of the normalised selection bounding box.
    pub max_y: usize,

    /// Current text-entry mode.
    pub mode: InputMode,
    /// When `true`, the clock is stopped and [`operate`](EditorState::operate) is not
    /// called automatically.
    pub paused: bool,
    /// Set to `false` to signal the main loop to shut down.
    pub running: bool,
    /// Whether the commander prompt is currently open.
    pub commander_active: bool,
    /// Text that the user has typed into the commander prompt.
    pub query: String,
    /// Previously executed commander commands, for up/down history navigation.
    pub command_history: Vec<String>,
    /// Current position within [`command_history`](EditorState::command_history) while
    /// navigating with the arrow keys.
    pub command_index: usize,
    /// Current playback tempo in beats per minute.
    pub bpm: usize,
    /// Tempo that `bpm` is smoothly interpolating towards.
    pub bpm_target: usize,
    /// Screen cell where a mouse drag began, used to compute selection bounds.
    pub mouse_from: Option<(usize, usize)>,
    /// When `true`, [`update_scroll`](EditorState::update_scroll) applies no scroll margin.
    pub last_input_was_mouse: bool,

    /// Path to the file currently open in the editor, if any.
    pub current_file: Option<PathBuf>,
    /// Undo/redo snapshot stack.
    pub history: History,
    /// MIDI output state, note stacks, and UDP socket.
    pub midi: MidiState,

    /// Whether to broadcast MIDI Beat Clock (0xF8) messages.
    pub midi_bclock: bool,

    /// Stack of currently visible overlay screens, rendered front-to-back.
    /// The last element is the topmost (focused) overlay.
    pub popup: Vec<PopupType>,

    /// ROFL BUFFER!!!
    pub rofl_buffer: String,
}

impl EditorState {
    /// Creates a new application with a blank `w`-by-`h` grid.
    ///
    /// The grid is initialised to all `'.'` and an initial history snapshot is recorded.
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::core::app::EditorState;
    ///
    /// let app = EditorState::new(57, 25, 1, 100);
    /// assert_eq!(app.engine.w, 57);
    /// assert_eq!(app.engine.h, 25);
    /// assert_eq!(app.glyph_at(0, 0), '.');
    /// ```
    pub fn new(w: usize, h: usize, seed: u64, undo_limit: usize) -> Self {
        let mut history = History::new();
        history.limit = undo_limit;

        let mut app = Self {
            engine: Engine::new(w, h, seed),
            grid_w: 8,
            grid_h: 8,
            scroll_x: 0,
            scroll_y: 0,
            cx: 0,
            cy: 0,
            cw: 0,
            ch: 0,
            min_x: 0,
            max_x: 0,
            min_y: 0,
            max_y: 0,
            mode: InputMode::Normal,
            paused: true,
            running: true,
            commander_active: false,
            query: String::new(),
            command_history: Vec::new(),
            command_index: 0,
            bpm: 120,
            bpm_target: 120,
            mouse_from: None,
            last_input_was_mouse: false,
            current_file: None,
            history,
            midi: MidiState::new(),
            midi_bclock: false,
            popup: Vec::new(),
            rofl_buffer: String::with_capacity(4),
        };
        app.calc_bounds();
        app.history.record(&app.engine.cells);
        app.history.saved_absolute_index = Some(app.history.offset + app.history.index);
        app
    }

    /// Adjusts [`scroll_x`](EditorState::scroll_x) and [`scroll_y`](EditorState::scroll_y)
    /// so the cursor stays visible within the viewport.
    ///
    /// A margin of up to three cells is maintained around the cursor when the
    /// last input was from the keyboard. When [`last_input_was_mouse`](EditorState::last_input_was_mouse)
    /// is `true` no margin is applied, so the view does not shift unexpectedly
    /// after a mouse click.
    ///
    /// Both scroll offsets are clamped so the viewport never extends beyond the
    /// grid boundaries.
    pub fn update_scroll(&mut self, viewport_w: usize, viewport_h: usize) {
        let margin_x = if self.last_input_was_mouse {
            0
        } else {
            3.min(viewport_w / 4)
        };
        let margin_y = if self.last_input_was_mouse {
            0
        } else {
            3.min(viewport_h / 4)
        };

        if self.cx < self.scroll_x + margin_x {
            self.scroll_x = self.cx.saturating_sub(margin_x);
        } else if self.cx >= self.scroll_x + viewport_w.saturating_sub(margin_x) {
            self.scroll_x = (self.cx + margin_x + 1).saturating_sub(viewport_w);
        }

        if self.cy < self.scroll_y + margin_y {
            self.scroll_y = self.cy.saturating_sub(margin_y);
        } else if self.cy >= self.scroll_y + viewport_h.saturating_sub(margin_y) {
            self.scroll_y = (self.cy + margin_y + 1).saturating_sub(viewport_h);
        }

        let max_scroll_x = self.engine.w.saturating_sub(viewport_w);
        let max_scroll_y = self.engine.h.saturating_sub(viewport_h);
        self.scroll_x = self.scroll_x.min(max_scroll_x);
        self.scroll_y = self.scroll_y.min(max_scroll_y);
    }

    /// Returns the bounding box of all non-empty cells as `(width, height)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::core::app::EditorState;
    ///
    /// let mut app = EditorState::new(10, 10, 1, 100);
    /// assert_eq!(app.content_bounds(), (1, 1));
    /// app.write_silent(4, 4, 'A');
    /// assert_eq!(app.content_bounds(), (5, 5));
    /// ```
    pub fn content_bounds(&self) -> (usize, usize) {
        let mut max_x = 0;
        let mut max_y = 0;
        for (i, &c) in self.engine.cells.iter().enumerate() {
            if c != '.' {
                max_x = max_x.max(i % self.engine.w);
                max_y = max_y.max(i / self.engine.w);
            }
        }
        (max_x + 1, max_y + 1)
    }

    /// Loads a text document into the grid, replacing all current content.
    pub fn load(&mut self, content: &str, path: Option<PathBuf>) {
        self.current_file = path;
        let lines: Vec<&str> = content.trim_end().lines().collect();
        let file_h = lines.len().max(1);
        let file_w = lines
            .iter()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(1)
            .max(1);

        let mut new_cells = vec!['.'; file_w * file_h];

        for (y, line) in lines.iter().enumerate() {
            if y >= file_h {
                break;
            }
            for (x, c) in line.chars().enumerate() {
                if x >= file_w {
                    break;
                }
                if Self::is_allowed(c) {
                    new_cells[y * file_w + x] = c;
                }
            }
        }

        self.engine.w = file_w;
        self.engine.h = file_h;
        self.engine.cells = new_cells;
        self.engine.locks = vec![false; self.engine.w * self.engine.h];
        self.engine.ports = vec![None; self.engine.w * self.engine.h];
        self.engine.port_names = vec![None; self.engine.w * self.engine.h];

        self.history.clear();
        self.history.record(&self.engine.cells);
        self.history.saved_absolute_index = Some(self.history.offset + self.history.index);
        self.select(self.cx as isize, self.cy as isize, self.cw, self.ch);
    }

    /// Serialises the grid to disk at [`current_file`](EditorState::current_file).
    pub fn save(&mut self) -> bool {
        let path = self
            .current_file
            .clone()
            .unwrap_or_else(|| PathBuf::from("untitled.o2"));
        let mut content = String::with_capacity((self.engine.w + 1) * self.engine.h);
        for y in 0..self.engine.h {
            for x in 0..self.engine.w {
                content.push(self.engine.cells[y * self.engine.w + x]);
            }
            content.push('\n');
        }
        let success = std::fs::write(path, content.trim_end()).is_ok();
        if success {
            self.history.saved_absolute_index = Some(self.history.offset + self.history.index);
        }
        success
    }

    /// Returns `true` if there are unsaved changes since the last save or load.
    pub fn is_dirty(&self) -> bool {
        self.history
            .saved_absolute_index
            .is_none_or(|saved| saved != (self.history.offset + self.history.index))
    }

    /// Reverts the grid to the previous history snapshot (Ctrl+Z).
    pub fn undo(&mut self) {
        self.history.undo(&mut self.engine.cells);
    }

    /// Re-applies a previously undone change (Ctrl+Shift+Z).
    pub fn redo(&mut self) {
        self.history.redo(&mut self.engine.cells);
    }

    /// Returns `true` if the character `g` is permitted in the grid.
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::core::app::EditorState;
    ///
    /// assert!(EditorState::is_allowed('.'));
    /// assert!(EditorState::is_allowed('A'));
    /// assert!(EditorState::is_allowed(':'));
    /// assert!(!EditorState::is_allowed(' '));
    /// assert!(!EditorState::is_allowed('-'));
    /// ```
    pub fn is_allowed(g: char) -> bool {
        let gl = g.to_ascii_lowercase();
        gl == '.' || gl.is_ascii_alphanumeric() || "*#$!%:?=;_".contains(gl)
    }

    /// Returns the flat-array index for cell `(x, y)`, or `None` if out of bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::core::app::EditorState;
    ///
    /// let app = EditorState::new(10, 10, 1, 100);
    /// assert_eq!(app.index_at(0, 0), Some(0));
    /// assert_eq!(app.index_at(10, 0), None);
    /// ```
    pub fn index_at(&self, x: usize, y: usize) -> Option<usize> {
        if x < self.engine.w && y < self.engine.h {
            Some(y * self.engine.w + x)
        } else {
            None
        }
    }

    /// Returns `true` if `(x, y)` lies within the grid boundaries.
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::core::app::EditorState;
    ///
    /// let app = EditorState::new(10, 10, 1, 100);
    /// assert!(app.is_in_bounds(0, 0));
    /// assert!(!app.is_in_bounds(-1, 0));
    /// assert!(!app.is_in_bounds(10, 0));
    /// ```
    pub fn is_in_bounds(&self, x: isize, y: isize) -> bool {
        x >= 0 && x < self.engine.w as isize && y >= 0 && y < self.engine.h as isize
    }

    /// Resizes the grid to at least `(new_w, new_h)`, preserving existing cell
    /// content.
    pub fn resize(&mut self, new_w: usize, new_h: usize) {
        let (bounds_w, bounds_h) = self.content_bounds();

        let min_w = bounds_w.max(self.max_x + 1).max(self.cx + 1);
        let min_h = bounds_h.max(self.max_y + 1).max(self.cy + 1);

        let final_w = new_w.max(min_w).max(1);
        let final_h = new_h.max(min_h).max(1);

        if final_w == self.engine.w && final_h == self.engine.h {
            return;
        }

        let mut new_cells = vec!['.'; final_w * final_h];
        let mut new_locks = vec![false; final_w * final_h];
        let mut new_ports = vec![None; final_w * final_h];
        let mut new_port_names = vec![None; final_w * final_h];

        for y in 0..self.engine.h.min(final_h) {
            for x in 0..self.engine.w.min(final_w) {
                let old_idx = y * self.engine.w + x;
                let new_idx = y * final_w + x;
                new_cells[new_idx] = self.engine.cells[old_idx];
                new_locks[new_idx] = self.engine.locks[old_idx];
                new_ports[new_idx] = self.engine.ports[old_idx];
                new_port_names[new_idx] = self.engine.port_names[old_idx];
            }
        }

        self.engine.w = final_w;
        self.engine.h = final_h;
        self.engine.cells = new_cells;
        self.engine.locks = new_locks;
        self.engine.ports = new_ports;
        self.engine.port_names = new_port_names;

        self.select(self.cx as isize, self.cy as isize, self.cw, self.ch);
        self.history.clear();
        self.history.record(&self.engine.cells);
        self.history.saved_absolute_index = None;
    }

    /// Returns the glyph at `(x, y)`, or `'.'` if the coordinates are out of bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::core::app::EditorState;
    ///
    /// let mut app = EditorState::new(5, 5, 1, 100);
    /// app.write_silent(2, 2, 'Z');
    /// assert_eq!(app.glyph_at(2, 2), 'Z');
    /// assert_eq!(app.glyph_at(99, 99), '.');
    /// ```
    pub fn glyph_at(&self, x: usize, y: usize) -> char {
        if let Some(idx) = self.index_at(x, y) {
            self.engine.cells[idx]
        } else {
            '.'
        }
    }

    /// Writes `g` to cell `(x, y)` without triggering any side-effects.
    pub fn write_silent(&mut self, x: usize, y: usize, g: char) {
        if let Some(idx) = self.index_at(x, y) {
            self.engine.cells[idx] = if Self::is_allowed(g) { g } else { '.' };
        }
    }

    /// Returns `true` if the cell at `(x, y)` is locked for this frame.
    pub fn is_locked(&self, x: usize, y: usize) -> bool {
        if let Some(idx) = self.index_at(x, y) {
            self.engine.locks[idx]
        } else {
            false
        }
    }

    /// Returns the port style decoration for cell `(x, y)`, if any.
    pub fn port_at(&self, x: usize, y: usize) -> Option<StyleType> {
        if let Some(idx) = self.index_at(x, y) {
            self.engine.ports[idx]
        } else {
            None
        }
    }

    /// Returns the port name and originating operator glyph for cell `(x, y)`.
    pub fn port_name_at(&self, x: usize, y: usize) -> Option<(&'static str, char)> {
        if let Some(idx) = self.index_at(x, y) {
            self.engine.port_names[idx]
        } else {
            None
        }
    }

    /// Directly sets the port style and name for cell `(x, y)`.
    pub fn set_port(
        &mut self,
        x: usize,
        y: usize,
        val: Option<StyleType>,
        name: Option<(&'static str, char)>,
    ) {
        if let Some(idx) = self.index_at(x, y) {
            self.engine.ports[idx] = val;
            self.engine.port_names[idx] = name;
        }
    }

    /// Reads the value stored in variable slot `key`.
    pub fn var_read(&self, key: char) -> char {
        if key.is_ascii() {
            self.engine.variables[key as usize]
        } else {
            '.'
        }
    }

    /// Writes `val` into variable slot `key`.
    pub fn var_write(&mut self, key: char, val: char) {
        if key.is_ascii() {
            self.engine.variables[key as usize] = val;
        }
    }

    /// Advances the simulation by one frame.
    pub fn operate(&mut self) {
        if self.bpm < self.bpm_target {
            self.bpm += 1;
        } else if self.bpm > self.bpm_target {
            self.bpm -= 1;
        }

        self.engine.locks.fill(false);
        self.engine.ports.fill(None);
        self.engine.port_names.fill(None);
        self.engine.variables.fill('.');

        let mut ops = std::mem::take(&mut self.engine.ops_cache);
        ops.clear();

        for y in 0..self.engine.h {
            for x in 0..self.engine.w {
                let g = self.engine.cells[y * self.engine.w + x];
                if g != '.' && !g.is_ascii_digit() && EditorState::is_operator(g) {
                    ops.push((x, y, g));
                }
            }
        }

        for &(x, y, g) in &ops {
            let idx = y * self.engine.w + x;
            if self.engine.locks[idx] {
                continue;
            }
            crate::core::vm::run(self, x, y, g, false, false);
        }

        self.engine.ops_cache = ops;
    }

    /// Runs all operators in dry-run mode to update port decorations.
    pub fn update_ports(&mut self) {
        self.engine.ports.fill(None);
        self.engine.port_names.fill(None);
        self.engine.locks.fill(false);

        let mut ops = std::mem::take(&mut self.engine.ops_cache);
        ops.clear();

        for y in 0..self.engine.h {
            for x in 0..self.engine.w {
                let g = self.engine.cells[y * self.engine.w + x];
                if g != '.' && !g.is_ascii_digit() && EditorState::is_operator(g) {
                    ops.push((x, y, g));
                }
            }
        }
        for &(x, y, g) in &ops {
            let idx = y * self.engine.w + x;
            if self.engine.locks[idx] {
                continue;
            }
            crate::core::vm::run(self, x, y, g, false, true);
        }

        self.engine.ops_cache = ops;
    }

    /// Returns `true` if `g` is a recognised operator glyph.
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::core::app::EditorState;
    ///
    /// assert!(EditorState::is_operator('A'));
    /// assert!(EditorState::is_operator('*'));
    /// assert!(!EditorState::is_operator('5'));
    /// ```
    pub fn is_operator(g: char) -> bool {
        let gl = g.to_ascii_lowercase();
        gl.is_ascii_alphabetic() || "*#$!%:?=;".contains(gl)
    }

    /// Converts a base-36 glyph to its numeric value.
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::core::app::EditorState;
    ///
    /// assert_eq!(EditorState::value_of('0'), 0);
    /// assert_eq!(EditorState::value_of('9'), 9);
    /// assert_eq!(EditorState::value_of('a'), 10);
    /// assert_eq!(EditorState::value_of('z'), 35);
    /// assert_eq!(EditorState::value_of('.'), 0);
    /// ```
    pub fn value_of(g: char) -> usize {
        g.to_digit(36).unwrap_or(0) as usize
    }

    /// Converts a numeric value to its base-36 glyph representation.
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::core::app::EditorState;
    ///
    /// assert_eq!(EditorState::key_of(0, false), '0');
    /// assert_eq!(EditorState::key_of(10, false), 'a');
    /// assert_eq!(EditorState::key_of(10, true), 'A');
    /// assert_eq!(EditorState::key_of(36, false), '0');
    /// ```
    pub fn key_of(val: usize, uppercase: bool) -> char {
        let c = std::char::from_digit((val % 36) as u32, 36).unwrap_or('0');
        if uppercase { c.to_ascii_uppercase() } else { c }
    }

    /// Reads the glyph at position `(x + dx, y + dy)`.
    pub fn listen(&self, x: usize, y: usize, dx: isize, dy: isize) -> char {
        let px = x as isize + dx;
        let py = y as isize + dy;
        if self.is_in_bounds(px, py) {
            self.engine.cells[(py as usize) * self.engine.w + (px as usize)]
        } else {
            '.'
        }
    }

    /// Reads the numeric value at `(x + dx, y + dy)`, clamped to `[min, max]`.
    pub fn listen_val(
        &self,
        x: usize,
        y: usize,
        dx: isize,
        dy: isize,
        min: usize,
        max: usize,
    ) -> usize {
        let g = self.listen(x, y, dx, dy);
        Self::value_of(g).clamp(min, max)
    }

    /// Registers a port at `(x + dx, y + dy)` for visual decoration and optional locking.
    #[allow(clippy::too_many_arguments)]
    pub fn add_port(
        &mut self,
        x: usize,
        y: usize,
        dx: isize,
        dy: isize,
        is_output: bool,
        should_lock: bool,
        draws_port: bool,
        name: Option<&'static str>,
    ) {
        let px = x as isize + dx;
        let py = y as isize + dy;
        if self.is_in_bounds(px, py) {
            let idx = (py as usize) * self.engine.w + (px as usize);
            if should_lock {
                self.engine.locks[idx] = true;
            }
            if draws_port {
                let port_type = if is_output {
                    StyleType::Output
                } else if dx < 0 || dy < 0 {
                    StyleType::Haste
                } else {
                    StyleType::Input
                };
                self.engine.ports[idx] = Some(port_type);
                let op_g = self.engine.cells[y * self.engine.w + x];
                self.engine.port_names[idx] = name.map(|n| (n, op_g));
            }
        }
    }

    /// Locks the cell at `(x + dx, y + dy)` for the current frame.
    pub fn lock(&mut self, x: usize, y: usize, dx: isize, dy: isize) {
        let px = x as isize + dx;
        let py = y as isize + dy;
        if self.is_in_bounds(px, py) {
            self.engine.locks[(py as usize) * self.engine.w + (px as usize)] = true;
        }
    }

    /// Marks `(x, y)` as the operator cell itself with [`StyleType::Operator`] decoration.
    pub fn add_op_port(&mut self, x: usize, y: usize, name: Option<&'static str>) {
        if let Some(idx) = self.index_at(x, y) {
            self.engine.ports[idx] = Some(StyleType::Operator);
            self.engine.port_names[idx] = name.map(|n| (n, '.'));
        }
    }

    /// Writes `g` to the cell at `(x + dx, y + dy)` and locks it.
    pub fn write_port(&mut self, x: usize, y: usize, dx: isize, dy: isize, g: char) {
        let px = x as isize + dx;
        let py = y as isize + dy;
        if self.is_in_bounds(px, py) {
            let idx = (py as usize) * self.engine.w + (px as usize);
            self.engine.cells[idx] = g;
            self.engine.locks[idx] = true;
        }
    }

    /// Moves operator glyph `g` from `(x, y)` one step in direction `(dx, dy)`.
    pub fn move_op(&mut self, x: usize, y: usize, dx: isize, dy: isize, g: char) {
        let px = x as isize + dx;
        let py = y as isize + dy;

        if self.is_in_bounds(px, py) {
            let idx = (py as usize) * self.engine.w + (px as usize);
            if self.engine.cells[idx] == '.' {
                let old_idx = y * self.engine.w + x;
                self.engine.cells[old_idx] = '.';
                self.write_port(x, y, dx, dy, g);
                return;
            }
        }
        self.write_silent(x, y, '*');
    }

    /// Returns `true` if any of the four orthogonal neighbours of `(x, y)` holds a bang (`'*'`).
    pub fn has_neighbor_bang(&self, x: usize, y: usize) -> bool {
        let dirs = [(0, 1), (0, -1), (1, 0), (-1, 0)];
        for &(dx, dy) in &dirs {
            let px = x as isize + dx;
            let py = y as isize + dy;
            if self.is_in_bounds(px, py)
                && self.engine.cells[(py as usize) * self.engine.w + (px as usize)] == '*'
            {
                return true;
            }
        }
        false
    }

    /// Returns `true` when the operator at `(x, y)` should produce uppercase output.
    pub fn should_uppercase(&self, x: usize, y: usize) -> bool {
        let right_val = self.listen(x, y, 1, 0);
        right_val.is_ascii_uppercase() && right_val.is_ascii_alphabetic()
    }

    /// Returns a deterministic pseudo-random integer in the inclusive range `[min(a,b), max(a,b)]`.
    pub fn random(&self, x: usize, y: usize, a: usize, b: usize) -> usize {
        let min = a.min(b);
        let max = a.max(b);
        if min == max {
            return min;
        }

        let mut key = (self.engine.rng_state as usize)
            .wrapping_add(y.wrapping_mul(self.engine.w).wrapping_add(x))
            ^ (self.engine.f << 16);

        key = (key ^ 61) ^ (key >> 16);
        key = key.wrapping_add(key << 3);
        key = key ^ (key >> 4);
        key = key.wrapping_mul(0x27d4eb2d);
        key = key ^ (key >> 15);

        min + (key % (max - min + 1))
    }

    /// Manually triggers the operator under the cursor (Ctrl+P / Enter).
    pub fn trigger(&mut self) {
        let g = self.glyph_at(self.cx, self.cy);
        if g != '.' && Self::is_operator(g) {
            crate::core::vm::run(self, self.cx, self.cy, g, true, false);
        }
    }

    /// Copies the current selection to the system clipboard.
    pub fn copy(&mut self) {
        let mut s = String::new();
        for y in self.min_y..=self.max_y {
            for x in self.min_x..=self.max_x {
                s.push(self.glyph_at(x, y));
            }
            if y < self.max_y {
                s.push('\n');
            }
        }
        if let Ok(mut ctx) = Clipboard::new() {
            let _ = ctx.set_text(s);
        }
    }

    /// Copies the current selection to the clipboard and erases it.
    pub fn cut(&mut self) {
        self.copy();
        self.erase();
    }

    /// Pastes text from the system clipboard at the cursor position.
    pub fn paste(&mut self) {
        if let Ok(mut ctx) = Clipboard::new()
            && let Ok(text) = ctx.get_text()
        {
            self.paste_text(&text);
        }
    }

    /// Inserts `text` into the grid at the current selection origin.
    pub fn paste_text(&mut self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }

        let normalized = trimmed.replace("\r\n", "\n").replace('\r', "\n");
        let lines: Vec<&str> = normalized.split('\n').collect();

        for (j, line) in lines.iter().enumerate() {
            for (i, c) in line.chars().enumerate() {
                if self.mode == InputMode::Append && c == '.' {
                    continue;
                }
                self.write_silent(self.min_x + i, self.min_y + j, c);
            }
        }

        let w = lines[0].chars().count().saturating_sub(1) as isize;
        let h = lines.len().saturating_sub(1) as isize;

        self.select(self.min_x as isize, self.min_y as isize, w, h);
        self.history.record(&self.engine.cells);
    }

    pub(crate) fn calc_bounds(&mut self) {
        let end_x = (self.cx as isize + self.cw).max(0) as usize;
        let end_y = (self.cy as isize + self.ch).max(0) as usize;
        self.min_x = self.cx.min(end_x);
        self.max_x = self.cx.max(end_x);
        self.min_y = self.cy.min(end_y);
        self.max_y = self.cy.max(end_y);
    }

    /// Returns the names of all available MIDI output devices.
    pub fn get_midi_output_devices(&self) -> Vec<String> {
        let mut devices = Vec::new();
        if let Ok(midi_out) = midir::MidiOutput::new("o2") {
            for port in midi_out.ports() {
                if let Ok(name) = midi_out.port_name(&port) {
                    devices.push(name);
                }
            }
        }
        devices
    }

    /// Connects to the MIDI output device at `index` in the device list.
    pub fn set_midi_device(&mut self, index: usize) {
        if let Ok(midi) = midir::MidiOutput::new("o2") {
            let ports = midi.ports();
            if index < ports.len() {
                let port = &ports[index];
                if let Ok(name) = midi.port_name(port) {
                    self.midi.device_name = name;
                    self.midi.output_index = index as i32;
                    self.midi.out = midi.connect(port, "o2-output").ok();
                }
            } else {
                self.midi.output_index = -1;
                self.midi.device_name = String::from("No Output Device");
                self.midi.out = None;
            }
        }
    }
}

impl std::fmt::Debug for EditorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditorState")
            .field("engine_w", &self.engine.w)
            .field("engine_h", &self.engine.h)
            .field("grid_w", &self.grid_w)
            .field("grid_h", &self.grid_h)
            .field("scroll_x", &self.scroll_x)
            .field("scroll_y", &self.scroll_y)
            .field("cx", &self.cx)
            .field("cy", &self.cy)
            .field("cw", &self.cw)
            .field("ch", &self.ch)
            .field("mode", &self.mode)
            .field("paused", &self.paused)
            .field("f", &self.engine.f)
            .field("bpm", &self.bpm)
            .field("bpm_target", &self.bpm_target)
            .field("last_input_was_mouse", &self.last_input_was_mouse)
            .field("midi_bclock", &self.midi_bclock)
            .field("midi", &self.midi)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_app(w: usize, h: usize) -> EditorState {
        EditorState::new(w, h, 42, 100)
    }

    fn run_grid(input: &str, frames: usize) -> String {
        let input = input.trim_matches('\n');
        let lines: Vec<&str> = input.lines().collect();
        let h = lines.len().max(1);
        let w = lines
            .iter()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(1)
            .max(1);
        let mut app = EditorState::new(w, h, 42, 100);
        app.load(input, None);
        for _ in 0..frames {
            app.operate();
            app.engine.f += 1;
        }
        let mut output = String::new();
        for y in 0..app.engine.h {
            for x in 0..app.engine.w {
                output.push(app.glyph_at(x, y));
            }
            if y < app.engine.h - 1 {
                output.push('\n');
            }
        }
        output
    }

    #[test]
    fn test_index_at() {
        let app = create_app(10, 10);
        assert_eq!(app.index_at(0, 0), Some(0));
        assert_eq!(app.index_at(9, 9), Some(99));
        assert_eq!(app.index_at(5, 5), Some(55));
        assert_eq!(app.index_at(10, 9), None);
        assert_eq!(app.index_at(9, 10), None);
        assert_eq!(app.index_at(10, 10), None);
    }

    #[test]
    fn test_is_in_bounds() {
        let app = create_app(10, 10);
        assert!(app.is_in_bounds(0, 0));
        assert!(app.is_in_bounds(9, 9));
        assert!(app.is_in_bounds(5, 5));
        assert!(!app.is_in_bounds(-1, 0));
        assert!(!app.is_in_bounds(0, -1));
        assert!(!app.is_in_bounds(10, 0));
        assert!(!app.is_in_bounds(0, 10));
        assert!(!app.is_in_bounds(10, 10));
        assert!(!app.is_in_bounds(-5, -5));
    }

    #[test]
    fn test_content_bounds() {
        let mut app = create_app(10, 10);
        assert_eq!(app.content_bounds(), (1, 1));
        app.write_silent(3, 4, 'A');
        assert_eq!(app.content_bounds(), (4, 5));
        app.write_silent(9, 9, 'B');
        assert_eq!(app.content_bounds(), (10, 10));
    }

    #[test]
    fn test_glyph_at() {
        let mut app = create_app(3, 3);
        app.engine.cells = vec!['1', '2', '3', '4', '5', '6', '7', '8', '9'];
        assert_eq!(app.glyph_at(0, 0), '1');
        assert_eq!(app.glyph_at(2, 0), '3');
        assert_eq!(app.glyph_at(1, 1), '5');
        assert_eq!(app.glyph_at(2, 2), '9');
        assert_eq!(app.glyph_at(3, 3), '.');
        assert_eq!(app.glyph_at(10, 10), '.');
    }

    #[test]
    fn test_write_silent() {
        let mut app = create_app(5, 5);
        app.write_silent(2, 2, 'X');
        assert_eq!(app.glyph_at(2, 2), 'X');
        app.write_silent(2, 2, '-');
        assert_eq!(app.glyph_at(2, 2), '.');
        app.write_silent(2, 2, ' ');
        assert_eq!(app.glyph_at(2, 2), '.');
        app.write_silent(10, 10, 'A');
        assert_eq!(app.glyph_at(10, 10), '.');
    }

    #[test]
    fn test_is_allowed() {
        let allowed = [
            '.', '0', '9', 'a', 'z', 'A', 'Z', '*', '#', '$', '!', '%', ':', '?', '=', ';', '_',
        ];
        for c in allowed {
            assert!(EditorState::is_allowed(c));
        }
        let disallowed = [
            ' ', '-', '+', '@', '&', ',', '<', '>', '/', '(', ')', '[', ']', '{', '}',
        ];
        for c in disallowed {
            assert!(!EditorState::is_allowed(c));
        }
    }

    #[test]
    fn test_value_of_and_key_of() {
        assert_eq!(EditorState::value_of('0'), 0);
        assert_eq!(EditorState::value_of('9'), 9);
        assert_eq!(EditorState::value_of('a'), 10);
        assert_eq!(EditorState::value_of('z'), 35);
        assert_eq!(EditorState::value_of('A'), 10);
        assert_eq!(EditorState::value_of('Z'), 35);
        assert_eq!(EditorState::value_of('.'), 0);
        assert_eq!(EditorState::value_of('*'), 0);
        assert_eq!(EditorState::value_of('#'), 0);

        assert_eq!(EditorState::key_of(0, false), '0');
        assert_eq!(EditorState::key_of(9, false), '9');
        assert_eq!(EditorState::key_of(10, false), 'a');
        assert_eq!(EditorState::key_of(35, false), 'z');
        assert_eq!(EditorState::key_of(36, false), '0');
        assert_eq!(EditorState::key_of(37, false), '1');

        assert_eq!(EditorState::key_of(10, true), 'A');
        assert_eq!(EditorState::key_of(35, true), 'Z');
        assert_eq!(EditorState::key_of(0, true), '0');
        assert_eq!(EditorState::key_of(9, true), '9');
    }

    #[test]
    fn test_base36_roundtrip() {
        for i in 0..=35 {
            let ch_lower = EditorState::key_of(i, false);
            assert_eq!(EditorState::value_of(ch_lower), i);

            let ch_upper = EditorState::key_of(i, true);
            assert_eq!(EditorState::value_of(ch_upper), i);
        }
    }

    #[test]
    fn test_resize() {
        let mut app = create_app(2, 2);
        app.write_silent(0, 0, '1');
        app.write_silent(1, 0, '2');
        app.write_silent(0, 1, '3');
        app.write_silent(1, 1, '4');

        app.resize(4, 4);
        assert_eq!(app.engine.w, 4);
        assert_eq!(app.engine.h, 4);
        assert_eq!(app.glyph_at(0, 0), '1');
        assert_eq!(app.glyph_at(1, 1), '4');
        assert_eq!(app.glyph_at(2, 2), '.');

        app.resize(1, 1);
        assert_eq!(app.engine.w, 2);
        assert_eq!(app.engine.h, 2);
    }

    #[test]
    fn test_load() {
        let mut app = create_app(1, 1);
        let content = "123\n456\n789";
        app.load(content, None);
        assert_eq!(app.engine.w, 3);
        assert_eq!(app.engine.h, 3);
        assert_eq!(app.glyph_at(0, 0), '1');
        assert_eq!(app.glyph_at(2, 0), '3');
        assert_eq!(app.glyph_at(0, 2), '7');
        assert_eq!(app.glyph_at(2, 2), '9');

        let content_with_disallowed = "1 3\n4-6";
        app.load(content_with_disallowed, None);
        assert_eq!(app.glyph_at(1, 0), '.');
        assert_eq!(app.glyph_at(1, 1), '.');
    }

    #[test]
    fn test_listen() {
        let mut app = create_app(5, 5);
        app.write_silent(2, 2, 'X');
        assert_eq!(app.listen(2, 2, 0, 0), 'X');
        assert_eq!(app.listen(1, 1, 1, 1), 'X');
        assert_eq!(app.listen(3, 3, -1, -1), 'X');
        assert_eq!(app.listen(4, 4, 1, 1), '.');
        assert_eq!(app.listen(0, 0, -1, 0), '.');
    }

    #[test]
    fn test_listen_val() {
        let mut app = create_app(5, 5);
        app.write_silent(1, 1, 'z');
        app.write_silent(2, 2, '0');
        app.write_silent(3, 3, 'a');

        assert_eq!(app.listen_val(1, 1, 0, 0, 0, 36), 35);
        assert_eq!(app.listen_val(1, 1, 0, 0, 0, 10), 10);
        assert_eq!(app.listen_val(2, 2, 0, 0, 5, 10), 5);
        assert_eq!(app.listen_val(3, 3, 0, 0, 0, 36), 10);
    }

    #[test]
    fn test_has_neighbor_bang() {
        let mut app = create_app(5, 5);
        assert!(!app.has_neighbor_bang(2, 2));

        app.write_silent(2, 1, '*');
        assert!(app.has_neighbor_bang(2, 2));

        app.write_silent(2, 1, '.');
        app.write_silent(2, 3, '*');
        assert!(app.has_neighbor_bang(2, 2));

        app.write_silent(2, 3, '.');
        app.write_silent(1, 2, '*');
        assert!(app.has_neighbor_bang(2, 2));

        app.write_silent(1, 2, '.');
        app.write_silent(3, 2, '*');
        assert!(app.has_neighbor_bang(2, 2));
    }

    #[test]
    fn test_should_uppercase() {
        let mut app = create_app(5, 5);
        app.write_silent(2, 2, 'a');
        app.write_silent(3, 2, 'B');
        assert!(app.should_uppercase(2, 2));

        app.write_silent(3, 2, 'b');
        assert!(!app.should_uppercase(2, 2));

        app.write_silent(3, 2, '5');
        assert!(!app.should_uppercase(2, 2));

        app.write_silent(3, 2, '*');
        assert!(!app.should_uppercase(2, 2));
    }

    #[test]
    fn test_variables() {
        let mut app = create_app(5, 5);
        app.var_write('a', '1');
        app.var_write('Z', '2');
        app.var_write('0', '3');

        assert_eq!(app.var_read('a'), '1');
        assert_eq!(app.var_read('Z'), '2');
        assert_eq!(app.var_read('0'), '3');
        assert_eq!(app.var_read('b'), '.');
        assert_eq!(app.var_read('Б'), '.');

        app.var_write('a', '9');
        assert_eq!(app.var_read('a'), '9');
    }

    #[test]
    fn test_move_op() {
        let mut app = create_app(5, 5);
        app.write_silent(2, 2, 'E');
        app.move_op(2, 2, 1, 0, 'E');
        assert_eq!(app.glyph_at(2, 2), '.');
        assert_eq!(app.glyph_at(3, 2), 'E');
        assert!(app.is_locked(3, 2));

        app.write_silent(4, 2, 'X');
        app.move_op(3, 2, 1, 0, 'E');
        assert_eq!(app.glyph_at(3, 2), '*');

        app.write_silent(0, 0, 'W');
        app.move_op(0, 0, -1, 0, 'W');
        assert_eq!(app.glyph_at(0, 0), '*');
    }

    #[test]
    fn test_move_op_off_grid() {
        let mut app = create_app(3, 3);
        app.write_silent(0, 1, 'W');
        app.move_op(0, 1, -1, 0, 'W');
        assert_eq!(app.glyph_at(0, 1), '*');

        app.write_silent(2, 1, 'E');
        app.move_op(2, 1, 1, 0, 'E');
        assert_eq!(app.glyph_at(2, 1), '*');

        app.write_silent(1, 0, 'N');
        app.move_op(1, 0, 0, -1, 'N');
        assert_eq!(app.glyph_at(1, 0), '*');

        app.write_silent(1, 2, 'S');
        app.move_op(1, 2, 0, 1, 'S');
        assert_eq!(app.glyph_at(1, 2), '*');
    }

    #[test]
    fn test_operate_lifecycle() {
        let mut app = create_app(5, 5);
        app.write_silent(1, 1, 'A');
        app.write_silent(0, 1, '1');
        app.write_silent(2, 1, '2');

        assert_eq!(app.engine.f, 0);
        app.operate();
        assert_eq!(app.glyph_at(1, 2), '3');
        assert!(app.is_locked(1, 2));

        app.write_silent(1, 1, 'a');
        app.operate();
        assert_eq!(app.glyph_at(1, 2), '3');
    }

    #[test]
    fn test_random() {
        let mut app = create_app(5, 5);
        for i in 0..100 {
            app.engine.f = i;
            let val = app.random(2, 2, 5, 10);
            assert!(val >= 5 && val <= 10);
        }
        for i in 0..100 {
            app.engine.f = i;
            let val = app.random(3, 3, 10, 5);
            assert!(val >= 5 && val <= 10);
        }
        let val = app.random(0, 0, 7, 7);
        assert_eq!(val, 7);
    }

    #[test]
    fn test_port_registration() {
        let mut app = create_app(5, 5);
        app.add_port(2, 2, -1, 0, false, true, true, Some("a"));
        assert!(app.is_locked(1, 2));
        assert_eq!(app.port_at(1, 2), Some(StyleType::Haste));
        assert_eq!(app.port_name_at(1, 2).map(|n| n.0), Some("a"));

        app.add_port(2, 2, 0, 1, true, false, true, Some("out"));
        assert!(!app.is_locked(2, 3));
        assert_eq!(app.port_at(2, 3), Some(StyleType::Output));
    }

    #[test]
    fn test_port_name_at() {
        let mut app = create_app(5, 5);
        app.set_port(1, 1, Some(StyleType::Input), Some(("test", 'T')));

        let info = app.port_name_at(1, 1);
        assert!(info.is_some());
        let (name, glyph) = info.unwrap();
        assert_eq!(name, "test");
        assert_eq!(glyph, 'T');

        assert!(app.port_name_at(2, 2).is_none());
    }

    #[test]
    fn test_update_scroll() {
        let mut app = create_app(20, 20);
        app.update_scroll(10, 10);
        assert_eq!(app.scroll_x, 0);
        assert_eq!(app.scroll_y, 0);

        app.cx = 15;
        app.cy = 15;
        app.update_scroll(10, 10);
        assert_eq!(app.scroll_x, 8);
        assert_eq!(app.scroll_y, 8);

        app.cx = 6;
        app.cy = 8;
        app.update_scroll(10, 10);
        assert_eq!(app.scroll_x, 4);
        assert_eq!(app.scroll_y, 6);
    }

    #[test]
    fn test_update_scroll_mouse() {
        let mut app = create_app(20, 20);
        app.last_input_was_mouse = true;

        app.cx = 15;
        app.cy = 15;
        app.update_scroll(10, 10);
        assert_eq!(app.scroll_x, 6);
        assert_eq!(app.scroll_y, 6);
    }

    #[test]
    fn test_write_port_locks() {
        let mut app = create_app(5, 5);
        app.write_port(2, 2, 1, 0, 'A');
        assert_eq!(app.glyph_at(3, 2), 'A');
        assert!(app.is_locked(3, 2));
    }

    #[test]
    fn test_operate_clears_state() {
        let mut app = create_app(5, 5);
        app.engine.locks[0] = true;
        app.engine.variables[97] = 'X';
        app.engine.ports[0] = Some(StyleType::Input);

        app.operate();

        assert!(!app.engine.locks[0]);
        assert_eq!(app.engine.variables[97], '.');
        assert_eq!(app.engine.ports[0], None);
    }

    #[test]
    fn test_trigger_operator() {
        let mut app = create_app(5, 5);
        app.write_silent(1, 1, 'a');
        app.write_silent(0, 1, '1');
        app.write_silent(2, 1, '2');
        app.cx = 1;
        app.cy = 1;

        app.trigger();

        assert_eq!(app.glyph_at(1, 2), '3');
        assert!(app.is_locked(1, 2));
    }

    #[test]
    fn test_execution_order() {
        let mut app = create_app(5, 5);
        app.load("E.\n.W", None);
        app.operate();
        assert_eq!(app.glyph_at(0, 0), '.');
        assert_eq!(app.glyph_at(1, 0), 'E');
        assert_eq!(app.glyph_at(1, 1), '.');
        assert_eq!(app.glyph_at(0, 1), 'W');

        app.load("S\n.", None);
        app.operate();
        assert_eq!(app.glyph_at(0, 0), '.');
        assert_eq!(app.glyph_at(0, 1), 'S');
    }

    #[test]
    fn test_lock_prevents_execution() {
        let mut app = create_app(5, 5);
        app.load("1A2\n.A.", None);
        app.operate();
        assert_eq!(app.glyph_at(1, 1), '3');
    }

    #[test]
    fn test_random_deterministic_and_variant() {
        let app1 = EditorState::new(10, 10, 42, 100);
        let mut app2 = EditorState::new(10, 10, 42, 100);

        assert_eq!(
            app1.random(5, 5, 0, 1_000_000),
            app2.random(5, 5, 0, 1_000_000)
        );

        app2.engine.f = 1;
        assert_ne!(
            app1.random(5, 5, 0, 1_000_000),
            app2.random(5, 5, 0, 1_000_000)
        );

        assert_ne!(
            app1.random(5, 5, 0, 1_000_000),
            app1.random(6, 5, 0, 1_000_000)
        );
        assert_ne!(
            app1.random(5, 5, 0, 1_000_000),
            app1.random(5, 6, 0, 1_000_000)
        );

        let app3 = EditorState::new(10, 10, 99, 100);
        assert_ne!(
            app1.random(5, 5, 0, 1_000_000),
            app3.random(5, 5, 0, 1_000_000)
        );

        let val_reverse = app1.random(0, 0, 35, 10);
        assert!(val_reverse >= 10 && val_reverse <= 35);

        assert_eq!(app1.random(0, 0, 7, 7), 7);
    }

    #[test]
    fn test_resize_extreme_values() {
        let mut app = create_app(10, 10);

        app.resize(0, 0);

        assert_eq!(app.engine.w, 1);
        assert_eq!(app.engine.h, 1);
        assert_eq!(app.engine.cells.len(), 1);
        assert_eq!(app.engine.locks.len(), 1);
    }

    #[test]
    fn test_integration_logic() {
        let initial = "\
8C8.............C8.................
.78T012AGag.....68T012AGag.........
.aV.............bVg................
...................................
3Ka.b.3Ka.b.3Ka.b.3Ka.b.3Ka.b.3Ka.b
...Ag....Bg....Cg....Rg....Mg....Vg
...g.....e.....e.....5.....0.......
...................................
3Ka.b.3Ka.b.3Ka.b.3Ka.b.......3K..a
...Ig....Dg....Fg....Lg..........V.
...5.................*.............";

        let frame_16 = "\
8C8.............C8.................
.18T012AGag.....78T012AGag.........
.aV1............bV.................
...................................
3Ka.b.3Ka.b.3Ka.b.3Ka.b.3Ka.b.3Ka.b
..1A....1B....1C....1R....1M....1V.
...1.....1.....e.....0.....0.......
...................................
3Ka.b.3Ka.b.3Ka.b.3Ka.b.......3K..a
..1I....1D....1F....1L...........V1
...0.....*...........0.............";

        let frame_255 = "\
8C8.............C8.................
.78T012AGag.....68T012AGag.........
.aV.............bVg................
...................................
3Ka.b.3Ka.b.3Ka.b.3Ka.b.3Ka.b.3Ka.b
...Ag....Bg....Cg....Rg....Mg....Vg
...g.....g.....e.....6.....0.......
...................................
3Ka.b.3Ka.b.3Ka.b.3Ka.b.......3K..a
...Ig....Dg....Fg....Lg..........V.
...0.................0.............";

        assert_eq!(run_grid(initial, 16), frame_16);
        assert_eq!(run_grid(initial, 255), frame_255);
    }

    #[test]
    fn test_integration_cardinals() {
        let initial = "\
..2D4.....D4......2D4....D4.
32X.............32X.........
......H...............H.....
......E...H...........S.....
......j...S...........j.....
..........j................0
............................
.........................H..
..........S..........H...Ny.
...........H.........Ey..E.0
..........xW................
......0.....................";

        let frame_100 = "\
..2D4.....D4......2D4....D4.
32X.............32X.........
......H...............H.....
......E...H...........S.....
......j...S...........j.....
.........Ej................0
...........................N
.........................H..
.....................HS..Ny.
...........H.........Ey....0
..........xW................
......0W....................";

        let frame_153 = "\
..2D4.....D4......2D4....D4.
32X*......*.....32X*.....*..
......H...............H.....
......E...H...........S.....
.....*j...S..........*j.....
......E...j...........S....0
............................
.........................H..
.....................H...Ny.
..........*H.........Ey...*0
..........xW................
......0...W.................";

        let frame_349 = "\
..2D4.....D4......2D4....D4.
32X.......*.....32X......*..
......H...............H.....
......E...H...........S.....
......j...S...........j.....
.........*j................0
..........S................*
.........................H..
.....................H*..Ny.
...........H.........EyE...0
..........xW................
......0*....................";

        assert_eq!(run_grid(initial, 100), frame_100);
        assert_eq!(run_grid(initial, 153), frame_153);
        assert_eq!(run_grid(initial, 349), frame_349);
    }

    #[test]
    fn test_integration_tables() {
        let initial = "\
..Cf..fCf...................................................................
xV9..yV5....................................................................
............................................................................
..3Kx.y..............3Kx.y..............3Kx.y..............3Kx.y............
2Kxy9M5............2Kxy9L5............2Kxy9B5............2Kxy9A5............
..95X9...............95X5...............95X4...............95Xe.............
....000000000000000....000000000000000....0123456789abcde....0123456789abcde
....0123456789abcde....011111111111111....10123456789abcd....123456789abcdef
....02468acegikmoqs....012222222222222....210123456789abc....23456789abcdefg
....0369cfilorux036....012333333333333....3210123456789ab....3456789abcdefgh
....048cgkosw048cgk....012344444444444....43210123456789a....456789abcdefghi
....05afkpuz49ejoty....012345555555555....543210123456789....56789abcdefghij
....06ciou06ciou06c....012345666666666....654321012345678....6789abcdefghijk
....07elsz6dkry5cjq....012345677777777....765432101234567....789abcdefghijkl
....08gow4cks08gow4....012345678888888....876543210123456....89abcdefghijklm
....09ir09ir09ir09i....012345678999999....987654321012345....9abcdefghijklmn
....0aku4eoy8is2cmw....0123456789aaaaa....a98765432101234....abcdefghijklmno
....0bmx8ju5gr2doza....0123456789abbbb....ba9876543210123....bcdefghijklmnop
....0co0co0co0co0co....0123456789abccc....cba987654321012....cdefghijklmnopq
....0dq3gt6jw9mzcp2....0123456789abcdd....dcba98765432101....defghijklmnopqr
....0es6kycq4iwao2g....0123456789abcde....edcba9876543210....efghijklmnopqrs";

        let frame_225 = "\
..Cf..fCf...................................................................
xVe..yVe....................................................................
............................................................................
..3Kx.y..............3Kx.y..............3Kx.y..............3Kx.y............
2KxyeMe............2KxyeLe............2KxyeBe............2KxyeAe............
..eeXg...............eeXe...............eeX0...............eeXs.............
....000000000000000....000000000000000....0123456789abcde....0123456789abcde
....0123456789abcde....011111111111111....10123456789abcd....123456789abcdef
....02468acegikmoqs....012222222222222....210123456789abc....23456789abcdefg
....0369cfilorux036....012333333333333....3210123456789ab....3456789abcdefgh
....048cgkosw048cgk....012344444444444....43210123456789a....456789abcdefghi
....05afkpuz49ejoty....012345555555555....543210123456789....56789abcdefghij
....06ciou06ciou06c....012345666666666....654321012345678....6789abcdefghijk
....07elsz6dkry5cjq....012345677777777....765432101234567....789abcdefghijkl
....08gow4cks08gow4....012345678888888....876543210123456....89abcdefghijklm
....09ir09ir09ir09i....012345678999999....987654321012345....9abcdefghijklmn
....0aku4eoy8is2cmw....0123456789aaaaa....a98765432101234....abcdefghijklmno
....0bmx8ju5gr2doza....0123456789abbbb....ba9876543210123....bcdefghijklmnop
....0co0co0co0co0co....0123456789abccc....cba987654321012....cdefghijklmnopq
....0dq3gt6jw9mzcp2....0123456789abcdd....dcba98765432101....defghijklmnopqr
....0es6kycq4iwao2g....0123456789abcde....edcba9876543210....efghijklmnopqrs";

        assert_eq!(run_grid(initial, 225), frame_225);
    }

    #[test]
    fn test_integration_rw() {
        let initial = "\
.................................2C4..
#.READ.#........................2M1...
...............................lV2....
C8...........Cg...........Vl..........
30O01234567..b8T01234567..202Q01234567
..3............3............23........
......................................
#.WRITE.#.............................
......................................
C8.C8........Cg.C8........Vl..........
30X3.........b8P3.........202G01......
..01234567.....01234567......0101.101.";

        let frame_8 = "\
.................................2C4..
#.READ.#........................2M3...
...............................lV6....
C8...........Cg...........Vl..........
70O01234567..78T01234567..602Q01234567
..7............7............67........
......................................
#.WRITE.#.............................
......................................
C8.C8........Cg.C8........Vl..........
70X7.........78P7.........602G01......
..01234567.....01234567......01010101.";

        let frame_100 = "\
.................................2C4..
#.READ.#........................2M1...
...............................lV2....
C8...........Cg...........Vl..........
30O01234567..38T01234567..202Q01234567
..3............3............23........
......................................
#.WRITE.#.............................
......................................
C8.C8........Cg.C8........Vl..........
30X3.........38P3.........202G01......
..01234567.....01234567......01010101.";

        assert_eq!(run_grid(initial, 8), frame_8);
        assert_eq!(run_grid(initial, 100), frame_100);
    }

    #[test]
    fn test_integration_sequencer() {
        let initial = "\
#.SEQUENCER.#....................Cw...Cw
...............................4Aa..1Aa.
..............................aVe..bVb..
........................................
Va.Vb..0.......1.......2.......3........
e1ObxT#.................................
2V.1V.#................................#
Va.Vb..0................................
e1ObxT#.................................
4V.3V.#................................#
Va.Vb..0................................
e1ObxT#.................................
6V.5V.#................................#
Va.Vb..0................................
e1ObxT#.................................
8V.7V.#................................#
Va.Vb..0................................
e1ObxT#.................................
aV.9V.#................................#
........................................
H...V1..H...V3..H...V5..H...V7..H...V9..
*:03....*:23....*:43....*:63....*:83....
H...V2..H...V4..H...V6..H...V8..H...Va..
*:13....*:33....*:53....*:73....*:a3....";

        let frame_16 = "\
#.SEQUENCER.#....................Cw...Cw
...............................4Af..1Af.
..............................aVj..bVg..
........................................
Va.Vb..0.......1.......2.......3........
j1OgxT#.................................
2V.1V.#................................#
Va.Vb..0................................
j1OgxT#.................................
4V.3V.#................................#
Va.Vb..0................................
j1OgxT#.................................
6V.5V.#................................#
Va.Vb..0................................
j1OgxT#.................................
8V.7V.#................................#
Va.Vb..0................................
j1OgxT#.................................
aV.9V.#................................#
........................................
H...V1..H...V3..H...V5..H...V7..H...V9..
*:03....*:23....*:43....*:63....*:83....
H...V2..H...V4..H...V6..H...V8..H...Va..
*:13....*:33....*:53....*:73....*:a3....";

        let frame_150 = "\
#.SEQUENCER.#....................Cw...Cw
...............................4Al..1Al.
..............................aVp..bVm..
........................................
Va.Vb..0.......1.......2.......3........
p1OmxT#.................................
2V.1V.#................................#
Va.Vb..0................................
p1OmxT#.................................
4V.3V.#................................#
Va.Vb..0................................
p1OmxT#.................................
6V.5V.#................................#
Va.Vb..0................................
p1OmxT#.................................
8V.7V.#................................#
Va.Vb..0................................
p1OmxT#.................................
aV.9V.#................................#
........................................
H...V1..H...V3..H...V5..H...V7..H...V9..
*:03....*:23....*:43....*:63....*:83....
H...V2..H...V4..H...V6..H...V8..H...Va..
*:13....*:33....*:53....*:73....*:a3....";

        assert_eq!(run_grid(initial, 16), frame_16);
        assert_eq!(run_grid(initial, 150), frame_150);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_value_of_and_key_of_roundtrip(val in any::<usize>()) {
            let ch_lower = EditorState::key_of(val, false);
            assert_eq!(EditorState::value_of(ch_lower), val % 36);
            let ch_upper = EditorState::key_of(val, true);
            assert_eq!(EditorState::value_of(ch_upper), val % 36);
        }

        #[test]
        fn prop_is_allowed(c in any::<char>()) {
            let allowed = EditorState::is_allowed(c);
            let cl = c.to_ascii_lowercase();
            if cl == '.' || cl.is_ascii_alphanumeric() || "*#$!%:?=;_".contains(cl) {
                assert!(allowed);
            } else {
                assert!(!allowed);
            }
        }

        #[test]
        fn prop_resize_maintains_data(
            target_w in 1usize..100,
            target_h in 1usize..100,
            x in 0usize..100,
            y in 0usize..100
        ) {
            let mut app = EditorState::new(100, 100, 42, 100);

            app.write_silent(x, y, 'A');
            app.resize(target_w, target_h);

            assert!(app.engine.w >= target_w);
            assert!(app.engine.h >= target_h);

            if x < app.engine.w && y < app.engine.h {
                assert_eq!(app.glyph_at(x, y), 'A');
            }
        }

        #[test]
        fn prop_random_bounds(a in any::<usize>(), b in any::<usize>(), x in any::<usize>(), y in any::<usize>(), f in any::<usize>()) {
            let mut app = EditorState::new(10, 10, 42, 100);
            app.engine.f = f;
            let val = app.random(x, y, a, b);
            let min = a.min(b);
            let max = a.max(b);
            assert!(val >= min && val <= max);
        }

        #[test]
        fn prop_selection_bounds(x in any::<isize>(), y in any::<isize>(), w in any::<isize>(), h in any::<isize>()) {
            let mut app = EditorState::new(100, 100, 42, 100);
            app.select(x, y, w, h);

            assert!(app.min_x <= app.max_x);
            assert!(app.min_y <= app.max_y);
            assert!(app.cx >= app.min_x && app.cx <= app.max_x);
            assert!(app.cy >= app.min_y && app.cy <= app.max_y);

            assert!(app.max_x < app.engine.w);
            assert!(app.max_y < app.engine.h);
        }

        #[test]
        fn prop_listen_never_panics(
            x in 0usize..100,
            y in 0usize..100,
            dx in any::<isize>(),
            dy in any::<isize>()
        ) {
            let app = EditorState::new(50, 50, 42, 100);
            let _ = app.listen(x, y, dx, dy);
            let _ = app.listen_val(x, y, dx, dy, 0, 36);
        }
    }
}
