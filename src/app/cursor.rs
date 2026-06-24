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
use crate::app::types::InputMode;

impl EditorState {
    fn grid_bounds(&self) -> (isize, isize) {
        (
            (self.o2.w.saturating_sub(1)) as isize,
            (self.o2.h.saturating_sub(1)) as isize,
        )
    }

    pub fn select(&mut self, x: isize, y: isize, w: isize, h: isize) {
        let (max_grid_x, max_grid_y) = self.grid_bounds();

        self.cursor.cx = x.clamp(0, max_grid_x) as usize;
        self.cursor.cy = y.clamp(0, max_grid_y) as usize;

        let min_cw = -(self.cursor.cx as isize);
        let max_cw = max_grid_x - (self.cursor.cx as isize);
        self.cursor.cw = w.clamp(min_cw, max_cw);

        let min_ch = -(self.cursor.cy as isize);
        let max_ch = max_grid_y - (self.cursor.cy as isize);
        self.cursor.ch = h.clamp(min_ch, max_ch);

        self.cursor.calc_bounds();
        self.guide = false;
    }

    pub fn select_all(&mut self) {
        self.select(0, 0, self.o2.w as isize - 1, self.o2.h as isize - 1);
        self.mode = InputMode::Selection;
    }

    pub fn move_cursor(&mut self, dx: isize, dy: isize) {
        let (max_grid_x, max_grid_y) = self.grid_bounds();

        let min_x_allowed = 0isize.max(-self.cursor.cw);
        let max_x_allowed = max_grid_x.min(max_grid_x - self.cursor.cw);

        let min_y_allowed = 0isize.max(-self.cursor.ch);
        let max_y_allowed = max_grid_y.min(max_grid_y - self.cursor.ch);

        let target_x = (self.cursor.cx as isize + dx).clamp(min_x_allowed, max_x_allowed);
        let target_y = (self.cursor.cy as isize - dy).clamp(min_y_allowed, max_y_allowed);

        self.select(target_x, target_y, self.cursor.cw, self.cursor.ch);
    }

    pub fn scale_cursor(&mut self, dw: isize, dh: isize) {
        self.select(
            self.cursor.cx as isize,
            self.cursor.cy as isize,
            self.cursor.cw + dw,
            self.cursor.ch - dh,
        );
    }

    pub fn drag(&mut self, dx: isize, dy: isize) {
        if self.mode == InputMode::Append {
            self.mode = InputMode::Normal;
        }

        let max_x_allowed = self.o2.w.saturating_sub(1);
        let max_y_allowed = self.o2.h.saturating_sub(1);

        let actual_dx = dx.clamp(
            -(self.cursor.min_x as isize),
            (max_x_allowed.saturating_sub(self.cursor.max_x)) as isize,
        );

        let actual_dy = (-dy).clamp(
            -(self.cursor.min_y as isize),
            (max_y_allowed.saturating_sub(self.cursor.max_y)) as isize,
        );

        if actual_dx == 0 && actual_dy == 0 {
            return;
        }

        let rows_count = (self.cursor.max_y - self.cursor.min_y) + 1;
        let cols_count = (self.cursor.max_x - self.cursor.min_x) + 1;

        let mut block = Vec::with_capacity(rows_count * cols_count);

        for y in self.cursor.min_y..=self.cursor.max_y {
            for x in self.cursor.min_x..=self.cursor.max_x {
                if let Some(idx) = self.o2.index_at(x, y) {
                    block.push(self.o2.cells[idx]);
                } else {
                    block.push('.');
                }
            }
        }

        for y in self.cursor.min_y..=self.cursor.max_y {
            for x in self.cursor.min_x..=self.cursor.max_x {
                if let Some(idx) = self.o2.index_at(x, y) {
                    self.o2.cells[idx] = '.';
                }
            }
        }

        self.move_cursor(actual_dx, -actual_dy);

        let mut block_iter = block.into_iter();
        for y in self.cursor.min_y..=self.cursor.max_y {
            for x in self.cursor.min_x..=self.cursor.max_x {
                if let Some(g) = block_iter.next()
                    && let Some(idx) = self.o2.index_at(x, y)
                {
                    self.o2.cells[idx] = g;
                }
            }
        }

        self.history.record(&self.o2.cells);
    }

    pub fn is_selected(&self, x: usize, y: usize) -> bool {
        x >= self.cursor.min_x
            && x <= self.cursor.max_x
            && y >= self.cursor.min_y
            && y <= self.cursor.max_y
    }

    pub fn write_cursor(&mut self, g: char) {
        let allowed_g = if Self::is_allowed(g) { g } else { '.' };

        if self.mode == InputMode::Append {
            if let Some(idx) = self.o2.index_at(self.cursor.cx, self.cursor.cy) {
                self.o2.cells[idx] = allowed_g;
                self.move_cursor(1, 0);
                self.history.record(&self.o2.cells);
            }
        } else {
            let mut changed = false;

            for y in self.cursor.min_y..=self.cursor.max_y {
                for x in self.cursor.min_x..=self.cursor.max_x {
                    if let Some(idx) = self.o2.index_at(x, y)
                        && self.o2.cells[idx] != allowed_g
                    {
                        self.o2.cells[idx] = allowed_g;
                        changed = true;
                    }
                }
            }

            if changed {
                self.history.record(&self.o2.cells);
            }
        }
    }

    pub fn erase(&mut self) {
        for y in self.cursor.min_y..=self.cursor.max_y {
            for x in self.cursor.min_x..=self.cursor.max_x {
                if let Some(idx) = self.o2.index_at(x, y) {
                    self.o2.cells[idx] = '.';
                }
            }
        }
        self.history.record(&self.o2.cells);
    }

    pub fn make_uppercase(&mut self) {
        for y in self.cursor.min_y..=self.cursor.max_y {
            for x in self.cursor.min_x..=self.cursor.max_x {
                let g = self.glyph_at(x, y);
                if g.is_ascii_lowercase() {
                    self.o2.write_silent(x, y, g.to_ascii_uppercase());
                }
            }
        }
        self.history.record(&self.o2.cells);
    }

    pub fn make_lowercase(&mut self) {
        for y in self.cursor.min_y..=self.cursor.max_y {
            for x in self.cursor.min_x..=self.cursor.max_x {
                let g = self.glyph_at(x, y);
                if g.is_ascii_uppercase() {
                    self.o2.write_silent(x, y, g.to_ascii_lowercase());
                }
            }
        }
        self.history.record(&self.o2.cells);
    }

    pub fn toggle_comment(&mut self) {
        let first_char = self.glyph_at(self.cursor.min_x, self.cursor.min_y);
        let c = if first_char == '#' { '.' } else { '#' };

        for y in self.cursor.min_y..=self.cursor.max_y {
            let width = self.cursor.max_x - self.cursor.min_x + 1;
            if width > 1 {
                self.o2.write_silent(self.cursor.min_x, y, c);
                self.o2.write_silent(self.cursor.max_x, y, c);
            } else {
                self.o2.write_silent(self.cursor.min_x, y, c);
            }
        }
        self.history.record(&self.o2.cells);
    }
}

#[cfg(test)]
mod tests {
    use crate::app::editor::EditorState;
    use crate::app::types::InputMode;

    fn editor(w: usize, h: usize) -> EditorState {
        EditorState::new(w, h, 1, 128)
    }

    #[test]
    fn select_clamps_origin_into_grid() {
        let mut e = editor(8, 8);
        e.select(100, 100, 0, 0);
        assert_eq!((e.cursor.cx, e.cursor.cy), (7, 7));
    }

    #[test]
    fn select_clamps_negative_origin_to_zero() {
        let mut e = editor(8, 8);
        e.select(-5, -5, 0, 0);
        assert_eq!((e.cursor.cx, e.cursor.cy), (0, 0));
    }

    #[test]
    fn select_clamps_extent_to_grid_edge() {
        let mut e = editor(8, 8);
        e.select(2, 2, 100, 100);
        assert_eq!((e.cursor.cw, e.cursor.ch), (5, 5));
    }

    #[test]
    fn select_computes_inclusive_bounds() {
        let mut e = editor(8, 8);
        e.select(2, 2, 3, 3);
        assert_eq!((e.cursor.min_x, e.cursor.max_x), (2, 5));
        assert_eq!((e.cursor.min_y, e.cursor.max_y), (2, 5));
    }

    #[test]
    fn select_all_spans_whole_grid() {
        let mut e = editor(8, 8);
        e.select_all();
        assert_eq!((e.cursor.min_x, e.cursor.max_x), (0, 7));
        assert_eq!((e.cursor.min_y, e.cursor.max_y), (0, 7));
        assert_eq!(e.mode, InputMode::Selection);
    }

    #[test]
    fn move_cursor_clamps_at_eastern_edge() {
        let mut e = editor(8, 8);
        e.move_cursor(100, 0);
        assert_eq!(e.cursor.cx, 7);
    }

    #[test]
    fn move_cursor_steps_within_grid() {
        let mut e = editor(8, 8);
        e.move_cursor(3, 0);
        assert_eq!(e.cursor.cx, 3);
    }

    #[test]
    fn scale_cursor_grows_selection_width() {
        let mut e = editor(8, 8);
        e.select(0, 0, 0, 0);
        e.scale_cursor(3, 0);
        assert_eq!(e.cursor.cw, 3);
    }

    #[test]
    fn drag_moves_block_and_cursor() {
        let mut e = editor(4, 4);
        e.o2.write_silent(0, 0, 'A');
        e.select(0, 0, 0, 0);
        e.drag(1, 0);
        assert_eq!(e.glyph_at(0, 0), '.');
        assert_eq!(e.glyph_at(1, 0), 'A');
        assert_eq!(e.cursor.cx, 1);
    }
}
