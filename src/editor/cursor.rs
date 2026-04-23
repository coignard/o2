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

//! Cursor movement, selection management, and cell editing.
//!
//! All methods are implemented as `impl EditorState` blocks and operate on the cursor
//! coordinates (`cx`, `cy`, `cw`, `ch`) and their derived bounding box
//! (`min_x`, `max_x`, `min_y`, `max_y`).

use crate::core::app::{EditorState, InputMode};

impl EditorState {
    /// Moves the cursor and selection to `(x, y)` with dimensions `(w, h)`.
    ///
    /// All four parameters are clamped so the resulting selection stays
    /// entirely within the grid boundaries. The selection bounding box is
    /// recalculated after every call.
    pub fn select(&mut self, x: isize, y: isize, w: isize, h: isize) {
        let max_grid_x = (self.engine.w.saturating_sub(1)) as isize;
        let max_grid_y = (self.engine.h.saturating_sub(1)) as isize;

        self.cx = x.clamp(0, max_grid_x) as usize;
        self.cy = y.clamp(0, max_grid_y) as usize;

        let min_cw = -(self.cx as isize);
        let max_cw = max_grid_x - (self.cx as isize);
        self.cw = w.clamp(min_cw, max_cw);

        let min_ch = -(self.cy as isize);
        let max_ch = max_grid_y - (self.cy as isize);
        self.ch = h.clamp(min_ch, max_ch);

        self.calc_bounds();
    }

    /// Selects the entire grid and switches to [`InputMode::Selection`].
    pub fn select_all(&mut self) {
        self.select(0, 0, self.engine.w as isize - 1, self.engine.h as isize - 1);
        self.mode = InputMode::Selection;
    }

    /// Moves the cursor by `(dx, -dy)` cells, preserving the current selection
    /// dimensions. Note the sign inversion on `dy`: positive `dy` moves upward.
    pub fn move_cursor(&mut self, dx: isize, dy: isize) {
        let max_grid_x = (self.engine.w.saturating_sub(1)) as isize;
        let max_grid_y = (self.engine.h.saturating_sub(1)) as isize;

        let min_x_allowed = 0isize.max(-self.cw);
        let max_x_allowed = max_grid_x.min(max_grid_x - self.cw);

        let min_y_allowed = 0isize.max(-self.ch);
        let max_y_allowed = max_grid_y.min(max_grid_y - self.ch);

        let target_x = (self.cx as isize + dx).clamp(min_x_allowed, max_x_allowed);
        let target_y = (self.cy as isize - dy).clamp(min_y_allowed, max_y_allowed);

        self.select(target_x, target_y, self.cw, self.ch);
    }

    /// Extends or contracts the selection by moving the cursor anchor to `(cx + dw, cy - dh)`.
    ///
    /// Unlike [`move_cursor`](EditorState::move_cursor), which translates both the
    /// cursor and the selection origin together, `scale_cursor` keeps the opposite
    /// corner of the selection fixed and repositions only the anchor. This
    /// produces a rubber-band resize effect used when Shift or Selection mode is
    /// active with the arrow keys.
    ///
    /// Both the new anchor and the resulting selection are clamped to the grid
    /// boundaries via [`select`](EditorState::select).
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::core::app::EditorState;
    ///
    /// let mut app = EditorState::new(10, 10, 1, 100);
    /// app.select(5, 5, 0, 0);
    /// app.scale_cursor(2, -2); // move anchor right 2, down 2
    /// assert_eq!(app.cx, 7);
    /// assert_eq!(app.cy, 7);
    /// assert_eq!(app.cw, -2);
    /// assert_eq!(app.ch, -2);
    /// ```
    pub fn scale_cursor(&mut self, dw: isize, dh: isize) {
        let max_grid_x = (self.engine.w.saturating_sub(1)) as isize;
        let max_grid_y = (self.engine.h.saturating_sub(1)) as isize;

        let target_cx = (self.cx as isize + dw).clamp(0, max_grid_x);
        let target_cy = (self.cy as isize - dh).clamp(0, max_grid_y);

        let origin_x = self.cx as isize + self.cw;
        let origin_y = self.cy as isize + self.ch;

        self.select(
            target_cx,
            target_cy,
            origin_x - target_cx,
            origin_y - target_cy,
        );
    }

    /// Moves the current selection contents by `(dx, -dy)` cells.
    ///
    /// The selected region is read, the original cells are erased, the cursor
    /// is moved, and the block is written at the new position. A history
    /// snapshot is recorded and the port/lock caches are cleared to prevent
    /// visual artefacts until the next frame.
    pub fn drag(&mut self, dx: isize, dy: isize) {
        if self.mode == InputMode::Append {
            self.mode = InputMode::Normal;
        }

        let max_x_allowed = self.engine.w.saturating_sub(1);
        let max_y_allowed = self.engine.h.saturating_sub(1);

        let actual_dx = dx.clamp(
            -(self.min_x as isize),
            (max_x_allowed.saturating_sub(self.max_x)) as isize,
        );

        let actual_dy = (-dy).clamp(
            -(self.min_y as isize),
            (max_y_allowed.saturating_sub(self.max_y)) as isize,
        );

        if actual_dx == 0 && actual_dy == 0 {
            return;
        }

        let rows_count = (self.max_y - self.min_y) + 1;
        let cols_count = (self.max_x - self.min_x) + 1;

        let mut block = Vec::with_capacity(rows_count * cols_count);

        for y in self.min_y..=self.max_y {
            for x in self.min_x..=self.max_x {
                if let Some(idx) = self.index_at(x, y) {
                    block.push((
                        self.engine.cells[idx],
                        self.engine.ports[idx],
                        self.engine.port_names[idx],
                        self.engine.locks[idx],
                    ));
                } else {
                    block.push(('.', None, None, false));
                }
            }
        }

        for y in self.min_y..=self.max_y {
            for x in self.min_x..=self.max_x {
                if let Some(idx) = self.index_at(x, y) {
                    self.engine.cells[idx] = '.';
                    self.engine.ports[idx] = None;
                    self.engine.port_names[idx] = None;
                    self.engine.locks[idx] = false;
                }
            }
        }

        self.move_cursor(actual_dx, -actual_dy);

        let mut block_iter = block.into_iter();
        for y in self.min_y..=self.max_y {
            for x in self.min_x..=self.max_x {
                if let Some((g, port, port_name, lock)) = block_iter.next()
                    && let Some(idx) = self.index_at(x, y)
                {
                    self.engine.cells[idx] = g;
                    self.engine.ports[idx] = port;
                    self.engine.port_names[idx] = port_name;
                    self.engine.locks[idx] = lock;
                }
            }
        }

        self.history.record(&self.engine.cells);
    }

    /// Returns `true` if `(x, y)` lies within the normalised selection bounding
    /// box.
    pub fn is_selected(&self, x: usize, y: usize) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }

    /// Writes `g` at the cursor position.
    ///
    /// In [`InputMode::Append`] the cursor advances one cell to the right after
    /// a successful write. A history snapshot is only recorded when the cell
    /// value actually changes.
    pub fn write_cursor(&mut self, g: char) {
        if let Some(idx) = self.index_at(self.cx, self.cy) {
            let allowed_g = if Self::is_allowed(g) { g } else { '.' };
            if self.engine.cells[idx] != allowed_g {
                self.engine.cells[idx] = allowed_g;
                if self.mode == InputMode::Append {
                    self.move_cursor(1, 0);
                }
                self.history.record(&self.engine.cells);
            }
        }
    }

    /// Fills the selection bounding box with `'.'` and records a history
    /// snapshot.
    pub fn erase(&mut self) {
        for y in self.min_y..=self.max_y {
            for x in self.min_x..=self.max_x {
                if let Some(idx) = self.index_at(x, y) {
                    // Who you gon' call? (Ghostbusters!)
                    self.engine.cells[idx] = '.';
                    self.engine.ports[idx] = None;
                    self.engine.port_names[idx] = None;
                    self.engine.locks[idx] = false;
                }
            }
        }
        self.history.record(&self.engine.cells);
    }

    /// Converts all lowercase letters in the selection to uppercase and records
    /// a history snapshot.
    pub fn make_uppercase(&mut self) {
        for y in self.min_y..=self.max_y {
            for x in self.min_x..=self.max_x {
                let g = self.glyph_at(x, y);
                if g.is_ascii_lowercase() {
                    self.write_silent(x, y, g.to_ascii_uppercase());
                }
            }
        }
        self.history.record(&self.engine.cells);
    }

    /// Converts all uppercase letters in the selection to lowercase and records
    /// a history snapshot.
    pub fn make_lowercase(&mut self) {
        for y in self.min_y..=self.max_y {
            for x in self.min_x..=self.max_x {
                let g = self.glyph_at(x, y);
                if g.is_ascii_uppercase() {
                    self.write_silent(x, y, g.to_ascii_lowercase());
                }
            }
        }
        self.history.record(&self.engine.cells);
    }

    /// Toggles a `'#'` comment block on the left and right edges of the
    /// selection.
    ///
    /// If the first cell of the selection already holds `'#'`, the comment
    /// characters are removed (replaced with `'.'`). Otherwise they are added.
    /// For single-column selections both the left and right operations act on
    /// the same cell.
    pub fn toggle_comment(&mut self) {
        let first_char = self.glyph_at(self.min_x, self.min_y);
        let c = if first_char == '#' { '.' } else { '#' };

        for y in self.min_y..=self.max_y {
            let width = self.max_x - self.min_x + 1;
            if width > 1 {
                self.write_silent(self.min_x, y, c);
                self.write_silent(self.max_x, y, c);
            } else {
                // NB: original ORCΛ implementation has a bug with single
                // character selection, this is fixed here
                self.write_silent(self.min_x, y, c);
            }
        }
        self.history.record(&self.engine.cells);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_app() -> EditorState {
        let mut app = EditorState::new(10, 10, 1, 100);
        app.load("0123456789\nabcdefghij\nklmnopqrst\nuvwxyzABCD\nEFGHIJKLMN\nOPQRSTUVWX\nYZ01234567\n89abcdefgh\nijklmnopqr\nstuvwxyzAB", None);
        app
    }

    #[test]
    fn test_select_bounds_clamping() {
        let mut app = create_app();
        app.select(-5, -5, 20, 20);
        assert_eq!(app.cx, 0);
        assert_eq!(app.cy, 0);
        assert_eq!(app.cw, 9);
        assert_eq!(app.ch, 9);
        assert_eq!(app.min_x, 0);
        assert_eq!(app.max_x, 9);
        assert_eq!(app.min_y, 0);
        assert_eq!(app.max_y, 9);

        app.select(15, 15, -20, -20);
        assert_eq!(app.cx, 9);
        assert_eq!(app.cy, 9);
        assert_eq!(app.cw, -9);
        assert_eq!(app.ch, -9);
        assert_eq!(app.min_x, 0);
        assert_eq!(app.max_x, 9);
        assert_eq!(app.min_y, 0);
        assert_eq!(app.max_y, 9);
    }

    #[test]
    fn test_select_positive_out_of_bounds() {
        let mut app = create_app();
        app.select(9, 9, 20, 20);
        assert_eq!(app.cx, 9);
        assert_eq!(app.cy, 9);
        assert_eq!(app.cw, 0);
        assert_eq!(app.ch, 0);
        assert_eq!(app.max_x, 9);
        assert_eq!(app.max_y, 9);
    }

    #[test]
    fn test_calc_bounds() {
        let mut app = create_app();
        app.select(5, 5, 2, 3);
        assert_eq!(app.min_x, 5);
        assert_eq!(app.max_x, 7);
        assert_eq!(app.min_y, 5);
        assert_eq!(app.max_y, 8);

        app.select(5, 5, -2, -3);
        assert_eq!(app.min_x, 3);
        assert_eq!(app.max_x, 5);
        assert_eq!(app.min_y, 2);
        assert_eq!(app.max_y, 5);
    }

    #[test]
    fn test_is_selected() {
        let mut app = create_app();
        app.select(2, 2, 2, 2);
        assert!(app.is_selected(2, 2));
        assert!(app.is_selected(4, 4));
        assert!(app.is_selected(3, 3));
        assert!(!app.is_selected(1, 2));
        assert!(!app.is_selected(5, 5));
    }

    #[test]
    fn test_erase() {
        let mut app = create_app();
        app.select(1, 1, 1, 1);
        app.erase();
        assert_eq!(app.glyph_at(1, 1), '.');
        assert_eq!(app.glyph_at(2, 1), '.');
        assert_eq!(app.glyph_at(1, 2), '.');
        assert_eq!(app.glyph_at(2, 2), '.');
        assert_eq!(app.glyph_at(0, 0), '0');
        assert_eq!(app.glyph_at(3, 3), 'x');
    }

    #[test]
    fn test_drag() {
        let mut app = create_app();
        app.select(1, 1, 1, 1);
        app.drag(2, -2);

        assert_eq!(app.cx, 3);
        assert_eq!(app.cy, 3);
        assert_eq!(app.glyph_at(1, 1), '.');
        assert_eq!(app.glyph_at(2, 2), '.');
        assert_eq!(app.glyph_at(3, 3), 'b');
        assert_eq!(app.glyph_at(4, 3), 'c');
        assert_eq!(app.glyph_at(3, 4), 'l');
        assert_eq!(app.glyph_at(4, 4), 'm');
    }

    #[test]
    fn test_drag_out_of_bounds_clamp() {
        let mut app = create_app();
        app.select(8, 8, 0, 0);
        app.drag(5, -5);

        assert_eq!(app.cx, 9);
        assert_eq!(app.cy, 9);
        assert_eq!(app.glyph_at(9, 9), 'q');
    }

    #[test]
    fn test_make_uppercase_lowercase() {
        let mut app = create_app();
        app.select(1, 1, 1, 0);
        app.make_uppercase();
        assert_eq!(app.glyph_at(1, 1), 'B');
        assert_eq!(app.glyph_at(2, 1), 'C');
        assert_eq!(app.glyph_at(0, 1), 'a');

        app.select(1, 3, 1, 0);
        app.make_lowercase();
        assert_eq!(app.glyph_at(1, 3), 'v');
        assert_eq!(app.glyph_at(2, 3), 'w');
    }

    #[test]
    fn test_toggle_comment() {
        let mut app = create_app();
        app.select(1, 1, 2, 1);
        app.toggle_comment();
        assert_eq!(app.glyph_at(1, 1), '#');
        assert_eq!(app.glyph_at(2, 1), 'c');
        assert_eq!(app.glyph_at(3, 1), '#');
        assert_eq!(app.glyph_at(1, 2), '#');
        assert_eq!(app.glyph_at(2, 2), 'm');
        assert_eq!(app.glyph_at(3, 2), '#');

        app.toggle_comment();
        assert_eq!(app.glyph_at(1, 1), '.');
        assert_eq!(app.glyph_at(3, 1), '.');
        assert_eq!(app.glyph_at(1, 2), '.');
        assert_eq!(app.glyph_at(3, 2), '.');

        app.select(5, 5, 0, 0);
        app.toggle_comment();
        assert_eq!(app.glyph_at(5, 5), '#');
    }

    #[test]
    fn test_select_all() {
        let mut app = create_app();
        app.select_all();
        assert_eq!(app.min_x, 0);
        assert_eq!(app.min_y, 0);
        assert_eq!(app.max_x, 9);
        assert_eq!(app.max_y, 9);
        assert_eq!(app.mode, InputMode::Selection);
    }

    #[test]
    fn test_move_cursor() {
        let mut app = create_app();
        app.select(5, 5, 2, 2);
        app.move_cursor(1, 1);
        assert_eq!(app.cx, 6);
        assert_eq!(app.cy, 4);
        assert_eq!(app.cw, 2);
        assert_eq!(app.ch, 2);

        app.move_cursor(-20, -20);
        assert_eq!(app.cx, 0);
        assert_eq!(app.cy, 7);
        assert_eq!(app.cw, 2);
        assert_eq!(app.ch, 2);
    }

    #[test]
    fn test_scale_cursor() {
        let mut app = create_app();
        app.select(5, 5, 0, 0);
        app.scale_cursor(2, -2);
        assert_eq!(app.cx, 7);
        assert_eq!(app.cy, 7);
        assert_eq!(app.cw, -2);
        assert_eq!(app.ch, -2);

        app.scale_cursor(-10, 10);
        assert_eq!(app.cx, 0);
        assert_eq!(app.cy, 0);
        assert_eq!(app.cw, 5);
        assert_eq!(app.ch, 5);
    }

    #[test]
    fn test_paste_text() {
        let mut app = create_app();
        app.select(1, 1, 0, 0);
        app.paste_text("12\n34");
        assert_eq!(app.glyph_at(1, 1), '1');
        assert_eq!(app.glyph_at(2, 1), '2');
        assert_eq!(app.glyph_at(1, 2), '3');
        assert_eq!(app.glyph_at(2, 2), '4');
        assert_eq!(app.cw, 1);
        assert_eq!(app.ch, 1);

        app.mode = InputMode::Append;
        app.paste_text("X.");
        assert_eq!(app.glyph_at(1, 1), 'X');
        assert_eq!(app.glyph_at(2, 1), '2');
    }

    #[test]
    fn test_select_underflow_overflow() {
        let mut app = create_app();
        app.select(isize::MIN, isize::MIN, isize::MIN, isize::MIN);
        assert_eq!(app.cx, 0);
        assert_eq!(app.cy, 0);
        assert_eq!(app.cw, 0);
        assert_eq!(app.ch, 0);

        app.select(isize::MAX, isize::MAX, isize::MAX, isize::MAX);
        assert_eq!(app.cx, app.engine.w - 1);
        assert_eq!(app.cy, app.engine.h - 1);
        assert_eq!(app.cw, 0);
        assert_eq!(app.ch, 0);
    }

    #[test]
    fn test_paste_massive_string() {
        let mut app = create_app();
        app.select(app.engine.w as isize - 1, app.engine.h as isize - 1, 0, 0);

        let huge_string = "A".repeat(10000);
        app.paste_text(&huge_string);

        assert_eq!(app.glyph_at(app.engine.w - 1, app.engine.h - 1), 'A');
        assert_eq!(app.glyph_at(app.engine.w, app.engine.h), '.');
    }

    #[test]
    fn test_drag_out_of_bounds_negative() {
        let mut app = create_app();
        app.select(0, 0, 2, 2);
        app.drag(-10, 10);

        assert_eq!(app.cx, 0);
        assert_eq!(app.cy, 0);
    }
}
