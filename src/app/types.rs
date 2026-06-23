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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Append,
    Selection,
    Slide,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptPurpose {
    Open,
    SaveAs { quit_after: bool },
    SetBpm,
    SetGridSize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopupType {
    Controls,
    Operators,
    About {
        opened_at: std::time::Instant,
    },
    MainMenu {
        selected: usize,
    },
    MidiMenu {
        selected: usize,
        devices: Vec<String>,
    },
    ConfirmNew {
        selected: usize,
    },
    AutofitMenu {
        selected: usize,
    },
    ClockMenu {
        selected: usize,
    },
    ConfirmQuit {
        selected: usize,
        has_file: bool,
    },
    Prompt {
        purpose: PromptPurpose,
        input: String,
        cursor: usize,
    },
    Msg {
        title: String,
        text: String,
    },
    RoflCopter,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CursorState {
    pub cx: usize,
    pub cy: usize,
    pub cw: isize,
    pub ch: isize,
    pub min_x: usize,
    pub max_x: usize,
    pub min_y: usize,
    pub max_y: usize,
}

impl CursorState {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn calc_bounds(&mut self) {
        let end_x = (self.cx as isize + self.cw).max(0) as usize;
        let end_y = (self.cy as isize + self.ch).max(0) as usize;
        self.min_x = self.cx.min(end_x);
        self.max_x = self.cx.max(end_x);
        self.min_y = self.cy.min(end_y);
        self.max_y = self.cy.max(end_y);
    }
}

#[derive(Debug, Default)]
pub struct CommanderState {
    pub active: bool,
    pub query: String,
    pub history: Vec<String>,
    pub index: usize,
}

impl CommanderState {
    pub fn new() -> Self {
        Self::default()
    }
}
