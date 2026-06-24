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

use crate::app::editor::EditorState;

const BPM_MIN: usize = 1;
const BPM_MAX: usize = 360;

impl EditorState {
    pub fn set_bpm_target(&mut self, target: usize) {
        self.bpm_target = target.clamp(BPM_MIN, BPM_MAX);
    }

    pub fn set_bpm(&mut self, bpm: usize) {
        let c = bpm.clamp(BPM_MIN, BPM_MAX);
        self.bpm = c;
        self.bpm_target = c;
    }

    pub fn mod_bpm_target(&mut self, diff: isize) {
        let new_target =
            (self.bpm_target as isize + diff).clamp(BPM_MIN as isize, BPM_MAX as isize) as usize;
        self.bpm_target = new_target;
    }

    pub fn mod_bpm(&mut self, diff: isize) {
        let new_val = (self.bpm as isize + diff).clamp(BPM_MIN as isize, BPM_MAX as isize) as usize;
        self.bpm = new_val;
        self.bpm_target = new_val;
    }
}

#[cfg(test)]
mod tests {
    use crate::app::editor::EditorState;

    fn editor() -> EditorState {
        EditorState::new(8, 8, 1, 128)
    }

    #[test]
    fn set_bpm_clamps_below_minimum() {
        let mut e = editor();
        e.set_bpm(0);
        assert_eq!(e.bpm, 1);
    }

    #[test]
    fn set_bpm_clamps_above_maximum() {
        let mut e = editor();
        e.set_bpm(10_000);
        assert_eq!(e.bpm, 360);
    }

    #[test]
    fn set_bpm_syncs_target() {
        let mut e = editor();
        e.set_bpm(150);
        assert_eq!(e.bpm_target, 150);
    }

    #[test]
    fn mod_bpm_clamps_to_range() {
        let mut e = editor();
        e.set_bpm(120);
        e.mod_bpm(-1000);
        assert_eq!(e.bpm, 1);
    }
}
