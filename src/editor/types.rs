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

//! Editor types: input modes, popup variants, cursor state, and commander state.
//!
//! These types describe the interactive editor layer that sits on top of the
//! core grid engine.  They are used by the input handler, renderer, and
//! commander but have no direct relationship to operator execution.

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

/// Cursor position, selection geometry, and derived bounding box.
#[derive(Debug, Clone, Copy, Default)]
pub struct CursorState {
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
}

impl CursorState {
    /// Creates a new cursor at `(0, 0)` with no selection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Recalculates the normalised bounding box from the cursor position and
    /// selection dimensions.
    pub(crate) fn calc_bounds(&mut self) {
        let end_x = (self.cx as isize + self.cw).max(0) as usize;
        let end_y = (self.cy as isize + self.ch).max(0) as usize;
        self.min_x = self.cx.min(end_x);
        self.max_x = self.cx.max(end_x);
        self.min_y = self.cy.min(end_y);
        self.max_y = self.cy.max(end_y);
    }
}

/// State for the commander prompt bar.
#[derive(Debug, Default)]
pub struct CommanderState {
    /// Whether the commander prompt is currently open.
    pub active: bool,
    /// Text that the user has typed into the commander prompt.
    pub query: String,
    /// Previously executed commands, for up/down history navigation.
    pub history: Vec<String>,
    /// Current position within [`history`](CommanderState::history) while
    /// navigating with the arrow keys.
    pub index: usize,
}

impl CommanderState {
    /// Creates a new inactive commander with no history.
    pub fn new() -> Self {
        Self::default()
    }
}
