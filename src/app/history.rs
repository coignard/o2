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

use std::collections::VecDeque;

#[derive(Debug)]
pub struct History {
    pub frames: VecDeque<Vec<char>>,
    pub index: usize,
    pub limit: usize,
    pub offset: usize,
    pub saved_absolute_index: Option<usize>,
}

impl History {
    pub fn new() -> Self {
        Self {
            frames: VecDeque::with_capacity(32),
            index: 0,
            limit: 128,
            offset: 0,
            saved_absolute_index: None,
        }
    }

    pub fn with_limit(limit: usize) -> Self {
        Self {
            limit,
            ..Self::new()
        }
    }

    pub fn record(&mut self, cells: &[char]) {
        if self.limit == 0 {
            return;
        }

        if !self.frames.is_empty()
            && self.index < self.frames.len()
            && self.frames[self.index] == cells
        {
            return;
        }

        if self.index + 1 < self.frames.len() {
            if let Some(saved) = self.saved_absolute_index
                && saved > self.offset + self.index
            {
                self.saved_absolute_index = None;
            }
            self.frames.truncate(self.index + 1);
        }
        self.frames.push_back(cells.to_vec());
        if self.frames.len() > self.limit {
            self.frames.pop_front();
            self.offset += 1;
        }
        self.index = self.frames.len().saturating_sub(1);
    }

    pub fn undo(&mut self, cells: &mut Vec<char>) {
        if self.index > 0 {
            self.index -= 1;
            *cells = self.frames[self.index].clone();
        }
    }

    pub fn redo(&mut self, cells: &mut Vec<char>) {
        if self.index + 1 < self.frames.len() {
            self.index += 1;
            *cells = self.frames[self.index].clone();
        }
    }

    pub fn clear(&mut self) {
        self.frames.clear();
        self.index = 0;
        self.offset = 0;
        self.saved_absolute_index = None;
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}
