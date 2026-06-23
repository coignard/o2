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

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum StyleType {
    Operator,
    Haste,
    Input,
    Output,
    Selected,
    Locked,
    Reader,
    Clock,
    #[default]
    Default,
}

pub struct OxygenEngine {
    pub w: usize,
    pub h: usize,
    pub cells: Vec<char>,
    pub locks: Vec<bool>,
    pub ports: Vec<Option<StyleType>>,
    pub port_names: Vec<Option<(&'static str, char)>>,
    pub variables: [Option<char>; 128],
    pub f: usize,
    pub rng_state: u64,
    pub ops_cache: Vec<(usize, usize, char)>,
    pub commands: Vec<(String, Option<(usize, usize)>)>,
}

impl OxygenEngine {
    pub fn new(w: usize, h: usize, seed: u64) -> Self {
        Self {
            w,
            h,
            cells: vec!['.'; w * h],
            locks: vec![false; w * h],
            ports: vec![None; w * h],
            port_names: vec![None; w * h],
            variables: [None; 128],
            f: 0,
            rng_state: seed,
            ops_cache: Vec::with_capacity(256),
            commands: Vec::new(),
        }
    }

    pub fn index_at(&self, x: usize, y: usize) -> Option<usize> {
        if x < self.w && y < self.h {
            Some(y * self.w + x)
        } else {
            None
        }
    }

    pub fn is_in_bounds(&self, x: isize, y: isize) -> bool {
        x >= 0 && x < self.w as isize && y >= 0 && y < self.h as isize
    }

    pub fn write_silent(&mut self, x: usize, y: usize, g: char) {
        if let Some(idx) = self.index_at(x, y) {
            self.cells[idx] = if crate::core::glyph::is_allowed(g) {
                g
            } else {
                '.'
            };
        }
    }

    pub fn set_port(
        &mut self,
        x: usize,
        y: usize,
        val: Option<StyleType>,
        name: Option<(&'static str, char)>,
    ) {
        if let Some(idx) = self.index_at(x, y) {
            self.ports[idx] = val;
            self.port_names[idx] = name;
        }
    }

    pub fn var_read(&self, key: char) -> char {
        if key.is_ascii() {
            self.variables[key as usize].unwrap_or('.')
        } else {
            '.'
        }
    }

    pub fn var_write(&mut self, key: char, val: char) {
        if key.is_ascii() {
            self.variables[key as usize] = Some(val);
        }
    }

    pub fn listen(&self, x: usize, y: usize, dx: isize, dy: isize) -> char {
        let px = x as isize + dx;
        let py = y as isize + dy;
        if self.is_in_bounds(px, py) {
            self.cells[(py as usize) * self.w + (px as usize)]
        } else {
            '.'
        }
    }

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
        crate::core::glyph::value_of(g).clamp(min, max)
    }

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
            let idx = (py as usize) * self.w + (px as usize);
            if should_lock {
                self.locks[idx] = true;
            }
            if draws_port {
                let port_type = if is_output {
                    StyleType::Output
                } else if dx < 0 || dy < 0 {
                    StyleType::Haste
                } else {
                    StyleType::Input
                };
                self.ports[idx] = Some(port_type);
                let op_g = self.cells[y * self.w + x];
                self.port_names[idx] = name.map(|n| (n, op_g));
            }
        }
    }

    pub fn lock(&mut self, x: usize, y: usize, dx: isize, dy: isize) {
        let px = x as isize + dx;
        let py = y as isize + dy;
        if self.is_in_bounds(px, py) {
            self.locks[(py as usize) * self.w + (px as usize)] = true;
        }
    }

    pub fn add_op_port(&mut self, x: usize, y: usize, name: Option<&'static str>) {
        if let Some(idx) = self.index_at(x, y) {
            self.ports[idx] = Some(StyleType::Operator);
            self.port_names[idx] = name.map(|n| (n, '.'));
        }
    }

    pub fn write_port(&mut self, x: usize, y: usize, dx: isize, dy: isize, g: char) {
        let px = x as isize + dx;
        let py = y as isize + dy;
        if self.is_in_bounds(px, py) {
            let idx = (py as usize) * self.w + (px as usize);
            self.cells[idx] = g;
            self.locks[idx] = true;
        }
    }

    pub fn move_op(&mut self, x: usize, y: usize, dx: isize, dy: isize, g: char) {
        let px = x as isize + dx;
        let py = y as isize + dy;

        if self.is_in_bounds(px, py) {
            let idx = (py as usize) * self.w + (px as usize);
            if self.cells[idx] == '.' {
                let old_idx = y * self.w + x;
                self.cells[old_idx] = '.';
                self.write_port(x, y, dx, dy, g);
                return;
            }
        }
        self.write_silent(x, y, '*');
    }

    pub fn has_neighbor_bang(&self, x: usize, y: usize) -> bool {
        let dirs = [(0, 1), (0, -1), (1, 0), (-1, 0)];
        for &(dx, dy) in &dirs {
            let px = x as isize + dx;
            let py = y as isize + dy;
            if self.is_in_bounds(px, py)
                && self.cells[(py as usize) * self.w + (px as usize)] == '*'
            {
                return true;
            }
        }
        false
    }

    pub fn should_uppercase(&self, x: usize, y: usize) -> bool {
        let right_val = self.listen(x, y, 1, 0);
        right_val.is_ascii_uppercase() && right_val.is_ascii_alphabetic()
    }

    pub fn random(&self, x: usize, y: usize, a: usize, b: usize) -> usize {
        let min = a.min(b);
        let max = a.max(b);
        if min == max {
            return min;
        }

        let mut key = (self.rng_state as usize)
            .wrapping_add(y.wrapping_mul(self.w).wrapping_add(x))
            ^ (self.f << 16);

        key = (key ^ 61) ^ (key >> 16);
        key = key.wrapping_add(key << 3);
        key = key ^ (key >> 4);
        key = key.wrapping_mul(0x27d4eb2d);
        key = key ^ (key >> 15);

        min + (key % (max - min + 1))
    }

    pub fn load_grid(&mut self, content: &str) {
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
            for (x, c) in line.chars().enumerate() {
                if x < file_w && crate::core::glyph::is_allowed(c) {
                    new_cells[y * file_w + x] = c;
                }
            }
        }

        self.w = file_w;
        self.h = file_h;
        self.cells = new_cells;
        self.locks = vec![false; file_w * file_h];
        self.ports = vec![None; file_w * file_h];
        self.port_names = vec![None; file_w * file_h];
    }

    pub fn resize_grid(&mut self, new_w: usize, new_h: usize) {
        let new_w = new_w.max(1);
        let new_h = new_h.max(1);
        if new_w == self.w && new_h == self.h {
            return;
        }

        let mut cells = vec!['.'; new_w * new_h];
        let mut locks = vec![false; new_w * new_h];
        let mut ports = vec![None; new_w * new_h];
        let mut port_names = vec![None; new_w * new_h];
        for y in 0..self.h.min(new_h) {
            for x in 0..self.w.min(new_w) {
                cells[y * new_w + x] = self.cells[y * self.w + x];
                locks[y * new_w + x] = self.locks[y * self.w + x];
                ports[y * new_w + x] = self.ports[y * self.w + x];
                port_names[y * new_w + x] = self.port_names[y * self.w + x];
            }
        }
        self.w = new_w;
        self.h = new_h;
        self.cells = cells;
        self.locks = locks;
        self.ports = ports;
        self.port_names = port_names;
    }

    pub fn tick(&mut self, midi: &mut crate::core::midi::MidiEngine) {
        self.locks.fill(false);
        self.ports.fill(None);
        self.port_names.fill(None);
        self.variables.fill(None);

        let mut ops = std::mem::take(&mut self.ops_cache);
        ops.clear();
        for y in 0..self.h {
            for x in 0..self.w {
                let g = self.cells[y * self.w + x];
                if g != '.' && !g.is_ascii_digit() && crate::core::glyph::is_operator(g) {
                    ops.push((x, y, g));
                }
            }
        }

        for &(x, y, g) in &ops {
            let idx = y * self.w + x;
            if self.locks[idx] {
                continue;
            }
            crate::core::operators::run(self, midi, x, y, g, false);
        }

        self.ops_cache = ops;
    }
}
