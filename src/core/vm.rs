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

//! Operator dispatcher: routes each glyph to its concrete implementation.
//!
//! The public entry point is [`run`], which is called once per cell per frame
//! by [`EditorState::operate`].
//!
//! # Operator categories
//!
//! * **Arithmetic** (`A`, `B`, `M`, `L`) -- binary operations on base-36 values.
//! * **Flow / time** (`C`, `D`, `F`, `G`, `H`, `I`, `J`, `K`, `O`, `P`, `Q`,
//!   `R`, `T`, `U`, `V`, `X`, `Y`, `Z`) -- data routing and sequencing.
//! * **Movement** (`E`, `N`, `S`, `W`) -- operators that slide across the grid.
//! * **Special** (`*`, `#`) -- bang erasure and line comment.
//! * **MIDI / IO** (`:`, `%`, `!`, `?`, `=`, `;`, `$`) -- output operators.
//!
//! # Activation rules
//!
//! An uppercase glyph or special symbol is *auto-run* every frame regardless of
//! its neighbours. A lowercase glyph only executes when an adjacent `'*'` bang
//! is present or when `force` is `true` (manual trigger via Ctrl+P).

use crate::core::app::EditorState;
use crate::core::io::{MidiCc, MidiMessage, MidiNote, MidiPb};
use crate::core::operator::operator_name;
use crate::core::transpose::transpose;
use crate::editor::commander::run_command;

struct VmContext<'a> {
    app: &'a mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
    triggered: bool,
}

impl<'a> VmContext<'a> {
    #[inline]
    fn add_port(&mut self, dx: isize, dy: isize, is_output: bool, name: Option<&'static str>) {
        self.app.add_port(
            self.x,
            self.y,
            dx,
            dy,
            is_output,
            self.is_active,
            self.draws_ports,
            name,
        );
    }

    #[inline]
    fn execute<F: FnOnce(&mut EditorState, usize, usize)>(&mut self, f: F) {
        if self.should_run {
            f(self.app, self.x, self.y);
        }
    }

    #[inline]
    fn execute_triggered<F: FnOnce(&mut EditorState, usize, usize)>(&mut self, f: F) {
        if self.should_run && self.triggered {
            f(self.app, self.x, self.y);
        }
    }

    #[inline]
    fn listen(&self, dx: isize, dy: isize) -> char {
        self.app.listen(self.x, self.y, dx, dy)
    }

    #[inline]
    fn listen_val(&self, dx: isize, dy: isize, min: usize, max: usize) -> usize {
        self.app.listen_val(self.x, self.y, dx, dy, min, max)
    }

    #[inline]
    fn lock(&mut self, dx: isize, dy: isize) {
        self.app.lock(self.x, self.y, dx, dy)
    }

    #[inline]
    fn clear_port(&mut self) {
        self.app.set_port(self.x, self.y, None, None);
    }
}

/// Executes the operator at grid position `(x, y)` with glyph `g`.
///
/// # Parameters
///
/// * `force` -- when `true`, the operator fires unconditionally (Ctrl+P).
/// * `dry_run` -- when `true`, only port decorations are written; the grid and
///   MIDI state are left untouched. Used by [`EditorState::update_ports`] during pause.
pub fn run(app: &mut EditorState, x: usize, y: usize, g: char, force: bool, dry_run: bool) {
    let gl = g.to_ascii_lowercase();
    let is_uppercase = g.is_ascii_uppercase();
    let is_special = !g.is_ascii_alphanumeric();

    let auto_run = is_uppercase || is_special;
    let banged = app.has_neighbor_bang(x, y);

    let is_active = auto_run || banged || force;
    let should_run = !dry_run && is_active;
    let draws_ports = auto_run;

    if draws_ports {
        app.add_op_port(x, y, Some(operator_name(gl)));
    }

    let mut ctx = VmContext {
        app,
        x,
        y,
        is_active,
        should_run,
        draws_ports,
        triggered: banged || force,
    };

    match gl {
        'a' => op_add(&mut ctx),
        'b' => op_sub(&mut ctx),
        'c' => op_clock(&mut ctx),
        'd' => op_delay(&mut ctx),
        'e' => op_east(&mut ctx, g),
        'f' => op_if(&mut ctx),
        'g' => op_gen(&mut ctx),
        'h' => op_halt(&mut ctx),
        'i' => op_inc(&mut ctx),
        'j' => op_jumper(&mut ctx, g),
        'k' => op_konkat(&mut ctx),
        'l' => op_lesser(&mut ctx),
        'm' => op_mult(&mut ctx),
        'n' => op_north(&mut ctx, g),
        'o' => op_read(&mut ctx),
        'p' => op_push(&mut ctx),
        'q' => op_query(&mut ctx),
        'r' => op_rand(&mut ctx),
        's' => op_south(&mut ctx, g),
        't' => op_track(&mut ctx),
        'u' => op_uclid(&mut ctx),
        'v' => op_var(&mut ctx),
        'w' => op_west(&mut ctx, g),
        'x' => op_write(&mut ctx),
        'y' => op_jymper(&mut ctx, g),
        'z' => op_lerp(&mut ctx),

        '*' => op_bang(&mut ctx),
        '#' => op_comment(&mut ctx),

        ':' | '%' => op_midi_mono(&mut ctx, g),
        '!' => op_cc(&mut ctx),
        '?' => op_pb(&mut ctx),
        '=' => op_osc(&mut ctx),
        ';' => op_udp(&mut ctx),
        '$' => op_self(&mut ctx),
        _ => {}
    }
}

fn op_add(ctx: &mut VmContext) {
    ctx.add_port(-1, 0, false, Some("a"));
    ctx.add_port(1, 0, false, Some("b"));
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|app, x, y| {
        let a = app.listen_val(x, y, -1, 0, 0, 36);
        let b = app.listen_val(x, y, 1, 0, 0, 36);
        let uc = app.should_uppercase(x, y);
        app.write_port(x, y, 0, 1, EditorState::key_of(a + b, uc));
    });
}

fn op_sub(ctx: &mut VmContext) {
    ctx.add_port(-1, 0, false, Some("a"));
    ctx.add_port(1, 0, false, Some("b"));
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|app, x, y| {
        let a = app.listen_val(x, y, -1, 0, 0, 36);
        let b = app.listen_val(x, y, 1, 0, 0, 36);
        let diff = (b as isize - a as isize).unsigned_abs();
        let uc = app.should_uppercase(x, y);
        app.write_port(x, y, 0, 1, EditorState::key_of(diff, uc));
    });
}

fn op_mult(ctx: &mut VmContext) {
    ctx.add_port(-1, 0, false, Some("a"));
    ctx.add_port(1, 0, false, Some("b"));
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|app, x, y| {
        let a = app.listen_val(x, y, -1, 0, 0, 36);
        let b = app.listen_val(x, y, 1, 0, 0, 36);
        let uc = app.should_uppercase(x, y);
        app.write_port(x, y, 0, 1, EditorState::key_of(a * b, uc));
    });
}

fn op_lesser(ctx: &mut VmContext) {
    ctx.add_port(-1, 0, false, Some("a"));
    ctx.add_port(1, 0, false, Some("b"));
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|app, x, y| {
        let a = app.listen_val(x, y, -1, 0, 0, 36);
        let b = app.listen_val(x, y, 1, 0, 0, 36);
        let uc = app.should_uppercase(x, y);
        let min_val = a.min(b);
        app.write_port(x, y, 0, 1, EditorState::key_of(min_val, uc));
    });
}

fn op_clock(ctx: &mut VmContext) {
    ctx.add_port(-1, 0, false, Some("rate"));
    ctx.add_port(1, 0, false, Some("mod"));
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|app, x, y| {
        let rate = app.listen_val(x, y, -1, 0, 1, 36);
        let m = app.listen_val(x, y, 1, 0, 0, 36);
        if m > 0 {
            let val = (app.engine.f / rate) % m;
            let uc = app.should_uppercase(x, y);
            app.write_port(x, y, 0, 1, EditorState::key_of(val, uc));
        }
    });
}

fn op_delay(ctx: &mut VmContext) {
    ctx.add_port(-1, 0, false, Some("rate"));
    ctx.add_port(1, 0, false, Some("mod"));
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|app, x, y| {
        let rate = app.listen_val(x, y, -1, 0, 1, 36);
        let m = app.listen_val(x, y, 1, 0, 1, 36);
        let res = app.engine.f % (m * rate);
        let out_char = if res == 0 || m == 1 { '*' } else { '.' };
        app.write_port(x, y, 0, 1, out_char);
    });
}

fn op_if(ctx: &mut VmContext) {
    ctx.add_port(-1, 0, false, Some("a"));
    ctx.add_port(1, 0, false, Some("b"));
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|app, x, y| {
        let a = app.listen(x, y, -1, 0);
        let b = app.listen(x, y, 1, 0);
        let out_char = if a == b { '*' } else { '.' };
        app.write_port(x, y, 0, 1, out_char);
    });
}

fn op_gen(ctx: &mut VmContext) {
    ctx.add_port(-3, 0, false, Some("x"));
    ctx.add_port(-2, 0, false, Some("y"));
    ctx.add_port(-1, 0, false, Some("len"));

    if ctx.is_active {
        let px = ctx.listen_val(-3, 0, 0, 36) as isize;
        let py = ctx.listen_val(-2, 0, 0, 36) as isize + 1;
        let len = ctx.listen_val(-1, 0, 1, 36);

        for offset in 0..len {
            let in_x = offset as isize + 1;
            let out_x = px + offset as isize;
            ctx.add_port(in_x, 0, false, Some("in"));
            ctx.add_port(out_x, py, true, Some("out"));
            ctx.execute(|app, x, y| {
                let res = app.listen(x, y, in_x, 0);
                app.write_port(x, y, out_x, py, res);
            });
        }
    }
}

fn op_halt(ctx: &mut VmContext) {
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|app, x, y| {
        let val = app.listen(x, y, 0, 1);
        app.write_port(x, y, 0, 1, val);
    });
}

fn op_inc(ctx: &mut VmContext) {
    ctx.add_port(-1, 0, false, Some("step"));
    ctx.add_port(1, 0, false, Some("mod"));
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|app, x, y| {
        let step = app.listen_val(x, y, -1, 0, 0, 36);
        let m = app.listen_val(x, y, 1, 0, 0, 36);
        let val = app.listen_val(x, y, 0, 1, 0, 36);
        let uc = app.should_uppercase(x, y);
        let res = if m > 0 {
            EditorState::key_of((val + step) % m, uc)
        } else {
            '0'
        };
        app.write_port(x, y, 0, 1, res);
    });
}

fn op_jumper(ctx: &mut VmContext, g: char) {
    if ctx.is_active {
        let upper = g.to_ascii_uppercase();
        let val = ctx.listen(0, -1);
        if val != upper {
            let mut i = 1;
            while ctx.app.is_in_bounds(ctx.x as isize, ctx.y as isize + i) {
                if ctx.listen(0, i) != g {
                    break;
                }
                i += 1;
            }
            ctx.add_port(0, -1, false, Some("in"));
            ctx.add_port(0, i, true, Some("out"));
            ctx.execute(|app, x, y| {
                app.write_port(x, y, 0, i, val);
            });
        }
    }
}

fn op_konkat(ctx: &mut VmContext) {
    ctx.add_port(-1, 0, false, Some("len"));
    if ctx.is_active {
        let len = ctx.listen_val(-1, 0, 1, 36);
        for offset in 0..len {
            let key = ctx.listen(offset as isize + 1, 0);
            ctx.lock(offset as isize + 1, 0);
            if key != '.' {
                ctx.add_port(offset as isize + 1, 0, false, Some("in"));
                ctx.add_port(offset as isize + 1, 1, true, Some("out"));
                ctx.execute(|app, x, y| {
                    let res = app.var_read(key);
                    app.write_port(x, y, offset as isize + 1, 1, res);
                });
            }
        }
    }
}

fn op_read(ctx: &mut VmContext) {
    ctx.add_port(-2, 0, false, Some("x"));
    ctx.add_port(-1, 0, false, Some("y"));
    if ctx.is_active {
        let px = ctx.listen_val(-2, 0, 0, 36) as isize;
        let py = ctx.listen_val(-1, 0, 0, 36) as isize;
        ctx.add_port(px + 1, py, false, Some("read"));
        ctx.add_port(0, 1, true, Some("out"));
        ctx.execute(|app, x, y| {
            let val = app.listen(x, y, px + 1, py);
            app.write_port(x, y, 0, 1, val);
        });
    }
}

fn op_push(ctx: &mut VmContext) {
    ctx.add_port(-2, 0, false, Some("key"));
    ctx.add_port(-1, 0, false, Some("len"));
    ctx.add_port(1, 0, false, Some("val"));
    if ctx.is_active {
        let key = ctx.listen_val(-2, 0, 0, 36);
        let len = ctx.listen_val(-1, 0, 1, 36);
        for offset in 0..len {
            ctx.lock(offset as isize, 1);
        }
        let out_x = (key % len) as isize;
        ctx.add_port(out_x, 1, true, Some("out"));
        ctx.execute(|app, x, y| {
            let val = app.listen(x, y, 1, 0);
            app.write_port(x, y, out_x, 1, val);
        });
    }
}

fn op_query(ctx: &mut VmContext) {
    ctx.add_port(-3, 0, false, Some("x"));
    ctx.add_port(-2, 0, false, Some("y"));
    ctx.add_port(-1, 0, false, Some("len"));
    if ctx.is_active {
        let px = ctx.listen_val(-3, 0, 0, 36) as isize;
        let py = ctx.listen_val(-2, 0, 0, 36) as isize;
        let len = ctx.listen_val(-1, 0, 1, 36);
        for offset in 0..len {
            let in_x = px + offset as isize + 1;
            let out_x = offset as isize - len as isize + 1;
            ctx.add_port(in_x, py, false, Some("in"));
            ctx.add_port(out_x, 1, true, Some("out"));
            ctx.execute(|app, x, y| {
                let res = app.listen(x, y, in_x, py);
                app.write_port(x, y, out_x, 1, res);
            });
        }
    }
}

fn op_rand(ctx: &mut VmContext) {
    ctx.add_port(-1, 0, false, Some("a"));
    ctx.add_port(1, 0, false, Some("b"));
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|app, x, y| {
        let a = app.listen_val(x, y, -1, 0, 0, 36);
        let b = app.listen_val(x, y, 1, 0, 0, 36);
        let val = app.random(x, y, a, b);
        let uc = app.should_uppercase(x, y);
        app.write_port(x, y, 0, 1, EditorState::key_of(val, uc));
    });
}

fn op_track(ctx: &mut VmContext) {
    ctx.add_port(-2, 0, false, Some("key"));
    ctx.add_port(-1, 0, false, Some("len"));
    if ctx.is_active {
        let key = ctx.listen_val(-2, 0, 0, 36);
        let len = ctx.listen_val(-1, 0, 1, 36);
        for offset in 0..len {
            ctx.lock(offset as isize + 1, 0);
        }
        let in_x = (key % len) as isize + 1;
        ctx.add_port(in_x, 0, false, Some("val"));
        ctx.add_port(0, 1, true, Some("out"));
        ctx.execute(|app, x, y| {
            let val = app.listen(x, y, in_x, 0);
            app.write_port(x, y, 0, 1, val);
        });
    }
}

fn op_uclid(ctx: &mut VmContext) {
    ctx.add_port(-1, 0, false, Some("step"));
    ctx.add_port(1, 0, false, Some("max"));
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|app, x, y| {
        let step = app.listen_val(x, y, -1, 0, 0, 36) as u64;
        let max = app.listen_val(x, y, 1, 0, 1, 36) as u64;
        let bucket = (step * (app.engine.f as u64 + max - 1)) % max + step;
        let out_char = if bucket >= max { '*' } else { '.' };
        app.write_port(x, y, 0, 1, out_char);
    });
}

fn op_var(ctx: &mut VmContext) {
    ctx.add_port(-1, 0, false, Some("write"));
    ctx.add_port(1, 0, false, Some("read"));
    if ctx.is_active {
        let write_key = ctx.listen(-1, 0);
        let read_key = ctx.listen(1, 0);

        if write_key == '.' && read_key != '.' {
            ctx.add_port(0, 1, true, Some("out"));
        }
        ctx.execute(|app, x, y| {
            if write_key != '.' {
                app.var_write(write_key, read_key);
            } else if read_key != '.' {
                let res = app.var_read(read_key);
                app.write_port(x, y, 0, 1, res);
            }
        });
    }
}

fn op_write(ctx: &mut VmContext) {
    ctx.add_port(-2, 0, false, Some("x"));
    ctx.add_port(-1, 0, false, Some("y"));
    ctx.add_port(1, 0, false, Some("val"));
    if ctx.is_active {
        let px = ctx.listen_val(-2, 0, 0, 36) as isize;
        let py = ctx.listen_val(-1, 0, 0, 36) as isize + 1;
        ctx.add_port(px, py, true, Some("out"));
        ctx.execute(|app, x, y| {
            let val = app.listen(x, y, 1, 0);
            app.write_port(x, y, px, py, val);
        });
    }
}

fn op_jymper(ctx: &mut VmContext, g: char) {
    if ctx.is_active {
        let upper = g.to_ascii_uppercase();
        let val = ctx.listen(-1, 0);
        if val != upper {
            let mut i = 1;
            while ctx.app.is_in_bounds(ctx.x as isize + i, ctx.y as isize) {
                if ctx.listen(i, 0) != g {
                    break;
                }
                i += 1;
            }
            ctx.add_port(-1, 0, false, Some("in"));
            ctx.add_port(i, 0, true, Some("out"));
            ctx.execute(|app, x, y| {
                app.write_port(x, y, i, 0, val);
            });
        }
    }
}

fn op_lerp(ctx: &mut VmContext) {
    ctx.add_port(-1, 0, false, Some("rate"));
    ctx.add_port(1, 0, false, Some("target"));
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|app, x, y| {
        let rate = app.listen_val(x, y, -1, 0, 0, 36) as isize;
        let target = app.listen_val(x, y, 1, 0, 0, 36) as isize;
        let val = app.listen_val(x, y, 0, 1, 0, 36) as isize;
        let md = if val <= target - rate {
            rate
        } else if val >= target + rate {
            -rate
        } else {
            target - val
        };
        let uc = app.should_uppercase(x, y);
        let result = (val + md).max(0) as usize;
        app.write_port(x, y, 0, 1, EditorState::key_of(result, uc));
    });
}

fn op_east(ctx: &mut VmContext, g: char) {
    ctx.clear_port();
    ctx.execute(|app, x, y| app.move_op(x, y, 1, 0, g));
}

fn op_west(ctx: &mut VmContext, g: char) {
    ctx.clear_port();
    ctx.execute(|app, x, y| app.move_op(x, y, -1, 0, g));
}

fn op_north(ctx: &mut VmContext, g: char) {
    ctx.clear_port();
    ctx.execute(|app, x, y| app.move_op(x, y, 0, -1, g));
}

fn op_south(ctx: &mut VmContext, g: char) {
    ctx.clear_port();
    ctx.execute(|app, x, y| app.move_op(x, y, 0, 1, g));
}

fn op_bang(ctx: &mut VmContext) {
    ctx.clear_port();
    ctx.execute(|app, x, y| app.write_silent(x, y, '.'));
}

fn op_comment(ctx: &mut VmContext) {
    if ctx.is_active {
        ctx.clear_port();
        ctx.lock(0, 0);
        let mut i = 1;
        while ctx.x + i < ctx.app.engine.w {
            let px = ctx.x + i;
            let idx = ctx.y * ctx.app.engine.w + px;
            ctx.app.engine.locks[idx] = true;
            if ctx.app.engine.cells[idx] == '#' {
                break;
            }
            i += 1;
        }
    }
}

fn op_midi_mono(ctx: &mut VmContext, g: char) {
    ctx.add_port(1, 0, false, Some("channel"));
    ctx.add_port(2, 0, false, Some("octave"));
    ctx.add_port(3, 0, false, Some("note"));
    ctx.add_port(4, 0, false, Some("velocity"));
    ctx.add_port(5, 0, false, Some("length"));

    ctx.execute_triggered(|app, x, y| {
        app.set_port(x, y, None, None);

        let ch_g = app.listen(x, y, 1, 0);
        let oct_g = app.listen(x, y, 2, 0);
        let note_g = app.listen(x, y, 3, 0);

        if ch_g == '.' || oct_g == '.' || note_g == '.' || !note_g.is_ascii_alphabetic() {
            return;
        }

        let channel = EditorState::value_of(ch_g);
        if channel > 15 {
            return;
        }

        let octave = EditorState::value_of(oct_g).clamp(0, 8);

        let vel_g = app.listen(x, y, 4, 0);
        let velocity_raw = if vel_g == '.' || vel_g == '*' {
            15
        } else {
            EditorState::value_of(vel_g).clamp(0, 16)
        };
        let velocity = ((velocity_raw as f32 / 16.0) * 127.0) as u8;

        let len_g = app.listen(x, y, 5, 0);

        let is_note_off = len_g == '0';
        let is_tied = len_g == '&';

        let length = if is_tied {
            usize::MAX
        } else if len_g == '.' || len_g == '*' {
            1
        } else {
            // NB: historically (0, 32) in JS version (why?)
            EditorState::value_of(len_g).clamp(0, 35)
        };

        if let Some(note_id) = transpose(note_g, octave as i32) {
            let is_mono = g == '%';

            let midi = &mut app.midi;
            let out_ref = &mut midi.out;
            let mono_stack = &mut midi.mono_stack;
            let stack = &mut midi.stack;

            if is_note_off {
                if is_mono {
                    if let Some(existing) = &mut mono_stack[channel] {
                        if existing.is_played {
                            if let Some(conn) = out_ref.as_mut() {
                                let _ = conn.send(&[0x80 + existing.channel, existing.note_id, 0]);
                            }
                        }
                        mono_stack[channel] = None;
                    }
                } else {
                    stack.retain_mut(|note| {
                        if note.channel == channel as u8 && note.octave == octave as u8 && note.note == note_g {
                            if note.is_played {
                                if let Some(conn) = out_ref.as_mut() {
                                    let _ = conn.send(&[0x80 + note.channel, note.note_id, 0]);
                                }
                            }
                            false
                        } else {
                            true
                        }
                    });
                }
                return;
            }

            let new_note = MidiNote {
                channel: channel as u8,
                octave: octave as u8,
                note: note_g,
                note_id,
                velocity,
                length,
                is_played: false,
            };

            if is_mono {
                let mut skip_note_on = false;

                if let Some(existing) = &mut mono_stack[channel] {
                    if is_tied && existing.note == note_g && existing.octave == octave as u8 {
                        existing.length = length;
                        skip_note_on = true;
                    } else {
                        if existing.is_played {
                            if let Some(conn) = out_ref.as_mut() {
                                let _ = conn.send(&[0x80 + existing.channel, existing.note_id, 0]);
                            }
                        }
                    }
                }

                if !skip_note_on {
                    mono_stack[channel] = Some(new_note);
                }
            } else {
                let mut skip_note_on = false;

                stack.retain_mut(|note| {
                    if note.channel == channel as u8 && note.octave == octave as u8 && note.note == note_g {
                        if is_tied {
                            note.length = length;
                            skip_note_on = true;
                            true
                        } else {
                            if note.is_played {
                                if let Some(conn) = out_ref.as_mut() {
                                    let _ = conn.send(&[0x80 + note.channel, note.note_id, 0]);
                                }
                            }
                            false
                        }
                    } else {
                        true
                    }
                });

                if !skip_note_on {
                    stack.push(new_note);
                }
            }
        }
    });
}

fn op_cc(ctx: &mut VmContext) {
    ctx.add_port(1, 0, false, Some("channel"));
    ctx.add_port(2, 0, false, Some("knob"));
    ctx.add_port(3, 0, false, Some("value"));

    ctx.execute_triggered(|app, x, y| {
        app.set_port(x, y, None, None);
        let ch_g = app.listen(x, y, 1, 0);
        let knob_g = app.listen(x, y, 2, 0);
        let val_g = app.listen(x, y, 3, 0);

        if ch_g == '.' || knob_g == '.' {
            return;
        }

        let channel = EditorState::value_of(ch_g);
        if channel > 15 {
            return;
        }

        let knob = EditorState::value_of(knob_g);
        let raw_val = if val_g == '.' {
            0
        } else {
            EditorState::value_of(val_g)
        };
        let value = ((127.0 * raw_val as f32) / 35.0).ceil() as u8;

        app.midi.cc_stack.push(MidiMessage::Cc(MidiCc {
            channel: channel as u8,
            knob: knob as u8,
            value,
        }));
    });
}

fn op_pb(ctx: &mut VmContext) {
    ctx.add_port(1, 0, false, Some("channel"));
    ctx.add_port(2, 0, false, Some("lsb"));
    ctx.add_port(3, 0, false, Some("msb"));

    ctx.execute_triggered(|app, x, y| {
        app.set_port(x, y, None, None);
        let ch_g = app.listen(x, y, 1, 0);
        let lsb_g = app.listen(x, y, 2, 0);
        let msb_g = app.listen(x, y, 3, 0);

        if ch_g == '.' || lsb_g == '.' {
            return;
        }

        let channel = EditorState::value_of(ch_g).clamp(0, 15);

        let raw_lsb = EditorState::value_of(lsb_g);
        let lsb = ((127.0 * raw_lsb as f32) / 35.0).ceil() as u8;

        let raw_msb = if msb_g == '.' {
            0
        } else {
            EditorState::value_of(msb_g)
        };
        let msb = ((127.0 * raw_msb as f32) / 35.0).ceil() as u8;

        app.midi.cc_stack.push(MidiMessage::Pb(MidiPb {
            channel: channel as u8,
            lsb,
            msb,
        }));
    });
}

fn op_osc(ctx: &mut VmContext) {
    ctx.add_port(1, 0, false, Some("path"));
    if ctx.is_active {
        for i in 2..=36 {
            let g = ctx.listen(i, 0);
            ctx.lock(i, 0);
            if g == '.' {
                break;
            }
        }
    }
    ctx.execute_triggered(|app, x, y| {
        app.set_port(x, y, None, None);
        let path_g = app.listen(x, y, 1, 0);
        if path_g != '.' {
            let mut msg = String::with_capacity(35);
            for i in 2..=36 {
                let g = app.listen(x, y, i, 0);
                if g == '.' {
                    break;
                }
                msg.push(g);
            }
            app.midi.osc_stack.push((path_g.to_string(), msg));
        }
    });
}

fn op_udp(ctx: &mut VmContext) {
    if ctx.is_active {
        for i in 1..=36 {
            let g = ctx.listen(i, 0);
            ctx.lock(i, 0);
            if g == '.' {
                break;
            }
        }
    }
    ctx.execute_triggered(|app, x, y| {
        app.set_port(x, y, None, None);
        let mut msg = String::with_capacity(35);
        for i in 1..=36 {
            let g = app.listen(x, y, i, 0);
            if g == '.' {
                break;
            }
            msg.push(g);
        }
        if !msg.is_empty() {
            app.midi.udp_stack.push(msg);
        }
    });
}

fn op_self(ctx: &mut VmContext) {
    if ctx.is_active {
        ctx.app.add_op_port(ctx.x, ctx.y, Some("self"));
        for i in 1..=36 {
            let g = ctx.listen(i, 0);
            ctx.lock(i, 0);
            if g == '.' {
                break;
            }
        }
    }
    ctx.execute_triggered(|app, x, y| {
        app.set_port(x, y, None, None);
        let mut msg = String::with_capacity(35);
        for i in 1..=36 {
            let g = app.listen(x, y, i, 0);
            if g == '.' {
                break;
            }
            msg.push(g);
        }
        if !msg.is_empty() {
            run_command(app, &msg, Some((x, y + 1)));
        }
    });
}

#[cfg(test)]
mod tests {
    use crate::core::app::EditorState;

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
    fn test_op_add() {
        assert_eq!(run_grid("1A2\n...", 1), "1A2\n.3.");
        assert_eq!(run_grid("aA5\n...", 1), "aA5\n.f.");
        assert_eq!(run_grid("1AA\n...", 1), "1AA\n.B.");
        assert_eq!(run_grid("1Aa\n...", 1), "1Aa\n.b.");
        assert_eq!(run_grid("1A.\n...", 1), "1A.\n.1.");
        assert_eq!(run_grid("zAz\n...", 1), "zAz\n.y.");
    }

    #[test]
    fn test_op_sub() {
        assert_eq!(run_grid("5B2\n...", 1), "5B2\n.3.");
        assert_eq!(run_grid("2B5\n...", 1), "2B5\n.3.");
        assert_eq!(run_grid("aBa\n...", 1), "aBa\n.0.");
        assert_eq!(run_grid("1BC\n...", 1), "1BC\n.B.");
        assert_eq!(run_grid(".B.\n...", 1), ".B.\n.0.");
    }

    #[test]
    fn test_op_mult() {
        assert_eq!(run_grid("3M4\n...", 1), "3M4\n.c.");
        assert_eq!(run_grid("aM0\n...", 1), "aM0\n.0.");
        assert_eq!(run_grid("2M.\n...", 1), "2M.\n.0.");
        assert_eq!(run_grid("zM2\n...", 1), "zM2\n.y.");
    }

    #[test]
    fn test_op_lesser() {
        assert_eq!(run_grid("3L5\n...", 1), "3L5\n.3.");
        assert_eq!(run_grid("7L2\n...", 1), "7L2\n.2.");
        assert_eq!(run_grid("zL.\n...", 1), "zL.\n.0.");
        assert_eq!(run_grid("aLA\n...", 1), "aLA\n.A.");
    }

    #[test]
    fn test_op_clock() {
        assert_eq!(run_grid("1C4\n...", 1), "1C4\n.0.");
        assert_eq!(run_grid("1C4\n...", 2), "1C4\n.1.");
        assert_eq!(run_grid("2C4\n...", 2), "2C4\n.0.");
        assert_eq!(run_grid("2C4\n...", 3), "2C4\n.1.");
        assert_eq!(run_grid(".C4\n...", 2), ".C4\n.1.");
        assert_eq!(run_grid("1C.\n...", 2), "1C.\n...");
        assert_eq!(run_grid("1C0\n...", 1), "1C0\n...");
        assert_eq!(run_grid("0C4\n...", 2), "0C4\n.1.");
    }

    #[test]
    fn test_op_delay() {
        assert_eq!(run_grid("1D4\n...", 1), "1D4\n.*.");
        assert_eq!(run_grid("1D4\n...", 2), "1D4\n...");
        assert_eq!(run_grid("1D4\n...", 4), "1D4\n...");
        assert_eq!(run_grid("1D4\n...", 5), "1D4\n.*.");
        assert_eq!(run_grid("1D1\n...", 1), "1D1\n.*.");
        assert_eq!(run_grid("1D1\n...", 2), "1D1\n.*.");
        assert_eq!(run_grid("0D4\n...", 1), "0D4\n.*.");
        assert_eq!(run_grid("1D0\n...", 1), "1D0\n.*.");
    }

    #[test]
    fn test_op_if() {
        assert_eq!(run_grid("3F3\n...", 1), "3F3\n.*.");
        assert_eq!(run_grid("3F4\n...", 1), "3F4\n...");
        assert_eq!(run_grid(".F.\n...", 1), ".F.\n.*.");
        assert_eq!(run_grid("aFa\n...", 1), "aFa\n.*.");
        assert_eq!(run_grid("aFA\n...", 1), "aFA\n...");
    }

    #[test]
    fn test_op_generator() {
        assert_eq!(run_grid("....\n22G.\n....", 1), "....\n22G.\n....");
        assert_eq!(run_grid("103Gabc\n.......", 1), "103Gabc\n....abc");
        assert_eq!(run_grid("204Gabcd\n........", 1), "204Gabcd\n.....abc");
        assert_eq!(run_grid("100Gabc\n.......", 1), "100Gabc\n....a..");
        assert_eq!(run_grid("10zGabc\n.......", 1), "10zGabc\n....abc");
        assert_eq!(run_grid("999Gabc\n.......", 1), "999Gabc\n.......");
    }

    #[test]
    fn test_op_halt() {
        assert_eq!(run_grid("H\nS\n.", 1), "H\nS\n.");
        assert_eq!(run_grid("H\nE\n.", 1), "H\nE\n.");
        assert_eq!(run_grid("H\nE\n.", 2), "H\nE\n.");
        assert_eq!(run_grid("H\n.\n.", 1), "H\n.\n.");
    }

    #[test]
    fn test_op_increment() {
        assert_eq!(run_grid("1I4\n.0.", 1), "1I4\n.1.");
        assert_eq!(run_grid("1I4\n.3.", 1), "1I4\n.0.");
        assert_eq!(run_grid(".I4\n.2.", 1), ".I4\n.2.");
        assert_eq!(run_grid("1I.\n.2.", 1), "1I.\n.0.");
        assert_eq!(run_grid("1I0\n.2.", 1), "1I0\n.0.");
        assert_eq!(run_grid("fIG\n.0.", 1), "fIG\n.F.");
        assert_eq!(run_grid("zIz\n.0.", 1), "zIz\n.0.");
    }

    #[test]
    fn test_op_jumper() {
        assert_eq!(run_grid(".1.\n.J.\n.J.\n...", 1), ".1.\n.J.\n.J.\n.1.");
        assert_eq!(run_grid(".1.\n.J.\n.x.\n...", 1), ".1.\n.J.\n.1.\n...");
        assert_eq!(run_grid(".a.\n.J.\n.J.\n...", 1), ".a.\n.J.\n.J.\n.a.");
        assert_eq!(run_grid(".a.\n.J.\n.x.\n...", 1), ".a.\n.J.\n.a.\n...");
        assert_eq!(run_grid(".1.\n.J.\n...", 1), ".1.\n.J.\n.1.");
        assert_eq!(run_grid(".J.\n.J.\n...", 1), ".J.\n.J.\n...");
        assert_eq!(
            run_grid(".1.\n.J.\n.J.\n.J.\n.J.\n...", 1),
            ".1.\n.J.\n.J.\n.J.\n.J.\n.1."
        );
    }

    #[test]
    fn test_op_konkat() {
        assert_eq!(
            run_grid("aV1.bV2\n2Kab...\n.......", 1),
            "aV1.bV2\n2Kab...\n..12..."
        );
        assert_eq!(run_grid("3K...\n.....", 1), "3K...\n.....");
        assert_eq!(
            run_grid("aV1.bV2.cV3\n4Kabcd.....\n...........", 1),
            "aV1.bV2.cV3\n4Kabcd.....\n..123......"
        );
        assert_eq!(run_grid("0Kabc\n.....", 1), "0Kabc\n.....");
        assert_eq!(run_grid("2K.a.\n.....", 1), "2K.a.\n.....");
    }

    #[test]
    fn test_op_read() {
        assert_eq!(run_grid("01O.\n...5", 1), "01O.\n..55");
        assert_eq!(run_grid("99O.\n....", 1), "99O.\n....");
        assert_eq!(run_grid("z1O.\n...5", 1), "z1O.\n...5");
        assert_eq!(run_grid("00O4\n....", 1), "00O4\n..4.");
    }

    #[test]
    fn test_op_push() {
        assert_eq!(run_grid("02P5\n....", 1), "02P5\n..5.");
        assert_eq!(run_grid("12P5\n....", 1), "12P5\n...5");
        assert_eq!(run_grid("92P5\n....", 1), "92P5\n...5");
        assert_eq!(run_grid("00P5\n....", 1), "00P5\n..5.");
    }

    #[test]
    fn test_op_query() {
        assert_eq!(run_grid("112Q..\n.....a", 1), "112Q..\n..a..a");
        assert_eq!(run_grid("013Q...\n...abc.", 1), "013Q...\n.bc.bc.");
        assert_eq!(run_grid("993Q...\n.......", 1), "993Q...\n.......");
        assert_eq!(run_grid("003Q...\n.......", 1), "003Q...\n.......");
    }

    #[test]
    fn test_op_random() {
        let mut app = EditorState::new(3, 2, 42, 100);
        app.load("1R5\n...", None);
        app.operate();
        let out = app.glyph_at(1, 1);
        let val = EditorState::value_of(out);
        assert!(val >= 1 && val <= 5);

        let mut app = EditorState::new(3, 2, 42, 100);
        app.load("5R1\n...", None);
        app.operate();
        let out = app.glyph_at(1, 1);
        let val = EditorState::value_of(out);
        assert!(val >= 1 && val <= 5);

        let mut app = EditorState::new(3, 2, 42, 100);
        app.load("aRa\n...", None);
        app.operate();
        let out = app.glyph_at(1, 1);
        let val = EditorState::value_of(out);
        assert_eq!(val, 10);
    }

    #[test]
    fn test_op_track() {
        assert_eq!(run_grid("13Tabc\n......", 1), "13Tabc\n..b...");
        assert_eq!(run_grid("33Tabc\n......", 1), "33Tabc\n..a...");
        assert_eq!(run_grid("03Tabc\n......", 1), "03Tabc\n..a...");
        assert_eq!(run_grid("00Tabc\n......", 1), "00Tabc\n..a...");
    }

    #[test]
    fn test_op_uclid() {
        assert_eq!(run_grid("3U8\n...", 1), "3U8\n.*.");
        assert_eq!(run_grid("3U8\n...", 2), "3U8\n...");
        assert_eq!(run_grid("3U8\n...", 3), "3U8\n...");
        assert_eq!(run_grid("3U8\n...", 4), "3U8\n.*.");
        assert_eq!(run_grid("1U1\n...", 1), "1U1\n.*.");
        assert_eq!(run_grid("1U1\n...", 2), "1U1\n.*.");
        assert_eq!(run_grid("0U8\n...", 1), "0U8\n...");
        assert_eq!(run_grid("3U0\n...", 1), "3U0\n.*.");
        assert_eq!(run_grid(".U.\n...", 1), ".U.\n...");
    }

    #[test]
    fn test_op_variable() {
        assert_eq!(run_grid("aV1\n...\n.Va\n...", 1), "aV1\n...\n.Va\n.1.");
        assert_eq!(run_grid("1V2\n...\n.V1\n...", 1), "1V2\n...\n.V1\n.2.");
        assert_eq!(run_grid(".Va\n...", 1), ".Va\n...");
        assert_eq!(run_grid("aV1.Va\n......", 1), "aV1.Va\n....1.");
        assert_eq!(run_grid("aV1\naV2\n.Va\n...", 1), "aV1\naV2\n.Va\n.2.");
        assert_eq!(run_grid("aV1.Vb\n......", 1), "aV1.Vb\n......");
    }

    #[test]
    fn test_op_write() {
        assert_eq!(run_grid("00X5\n....", 1), "00X5\n..5.");
        assert_eq!(run_grid("11X5\n....\n....", 1), "11X5\n....\n...5");
        assert_eq!(run_grid("20X5\n....", 1), "20X5\n....");
        assert_eq!(run_grid("99X5\n....", 1), "99X5\n....");
    }

    #[test]
    fn test_op_jymper() {
        assert_eq!(run_grid("1YYY.", 1), "1YYY1");
        assert_eq!(run_grid("2YYY.", 1), "2YYY2");
        assert_eq!(run_grid("aYY..", 1), "aYYa.");
        assert_eq!(run_grid("YYY..", 1), "YYY..");
        assert_eq!(run_grid("1YYYYY.", 1), "1YYYYY1");
        assert_eq!(run_grid("1YxY.", 1), "1Y1Y1");
    }

    #[test]
    fn test_op_lerp() {
        assert_eq!(run_grid("1Z5\n.1.", 1), "1Z5\n.2.");
        assert_eq!(run_grid("2Z1\n.4.", 1), "2Z1\n.2.");
        assert_eq!(run_grid("5Z5\n.0.", 1), "5Z5\n.5.");
        assert_eq!(run_grid("1Z5\n.5.", 1), "1Z5\n.5.");
        assert_eq!(run_grid(".Z5\n.1.", 1), ".Z5\n.1.");
        assert_eq!(run_grid("9Z0\n.z.", 1), "9Z0\n.q.");
    }

    #[test]
    fn test_op_movement() {
        assert_eq!(run_grid("E..", 1), ".E.");
        assert_eq!(run_grid("..W", 1), ".W.");
        assert_eq!(run_grid(".\nS\n.", 1), ".\n.\nS");
        assert_eq!(run_grid(".\nN\n.", 1), "N\n.\n.");
        assert_eq!(run_grid("E..\n...", 2), "..E\n...");
    }

    #[test]
    fn test_op_movement_collisions() {
        assert_eq!(run_grid("E1", 1), "*1");
        assert_eq!(run_grid("1W", 1), "1*");
        assert_eq!(run_grid("N\n1", 1), "*\n1");
        assert_eq!(run_grid("1\nS", 1), "1\n*");
        assert_eq!(run_grid("..E", 1), "..*");
        assert_eq!(run_grid("W..", 1), "*..");
        assert_eq!(run_grid("N\n.\n.", 1), "*\n.\n.");
        assert_eq!(run_grid(".\n.\nS", 1), ".\n.\n*");
        assert_eq!(run_grid("E\nW", 1), "*\n*");
    }

    #[test]
    fn test_op_bang() {
        assert_eq!(run_grid("1*2\n...", 1), "1.2\n...");
        assert_eq!(run_grid(".E.\n.*.", 1), "..E\n...");
        assert_eq!(run_grid("*..", 1), "...");
    }

    #[test]
    fn test_op_comment() {
        assert_eq!(run_grid("#N#\n...", 1), "#N#\n...");
        assert_eq!(run_grid("#E.\n...", 1), "#E.\n...");
        assert_eq!(run_grid("#1A2\n....", 1), "#1A2\n....");
        assert_eq!(run_grid("#.#\n...", 1), "#.#\n...");
    }

    #[test]
    fn test_midi_ops() {
        let mut app = EditorState::new(10, 10, 42, 100);
        app.load(":03C..\n*.....\n!023..\n*.....\n?045..\n*.....\n=a12..\n*.....\n;msg..\n*.....\n%$3C..\n*.....", None);
        app.operate();

        assert_eq!(app.midi.stack.len(), 1);
        assert_eq!(app.midi.cc_stack.len(), 2);
        assert_eq!(app.midi.osc_stack.len(), 1);
        assert_eq!(app.midi.udp_stack.len(), 1);
        assert!(app.midi.mono_stack[0].is_some());
    }

    #[test]
    fn test_lowercase_ops_dont_run_without_bang() {
        assert_eq!(run_grid("1a2\n...", 1), "1a2\n...");
        assert_eq!(run_grid("e..", 1), "e..");
        assert_eq!(run_grid("1a2\n.*.", 1), "1a2\n.3.");
        assert_eq!(run_grid("*..\n.e.", 1), "...\n.e.");
    }

    #[test]
    fn test_dry_run_does_not_mutate_state() {
        let mut app = EditorState::new(5, 5, 42, 100);
        app.load("1A2\n...", None);

        crate::core::vm::run(&mut app, 1, 0, 'A', false, true);

        assert_eq!(app.glyph_at(1, 1), '.');
        assert_eq!(app.port_at(0, 0), Some(crate::ui::theme::StyleType::Haste));
        assert_eq!(app.port_at(2, 0), Some(crate::ui::theme::StyleType::Input));
        assert_eq!(app.port_at(1, 1), Some(crate::ui::theme::StyleType::Output));
    }

    #[test]
    fn test_force_flag_executes_lowercase() {
        let mut app = EditorState::new(5, 5, 42, 100);
        app.load("1a2\n...", None);

        crate::core::vm::run(&mut app, 1, 0, 'a', false, false);
        assert_eq!(app.glyph_at(1, 1), '.');

        crate::core::vm::run(&mut app, 1, 0, 'a', true, false);
        assert_eq!(app.glyph_at(1, 1), '3');
    }

    #[test]
    fn test_operator_out_of_bounds_safety() {
        let mut app = EditorState::new(2, 2, 42, 100);
        app.load("X1\n..", None);
        crate::core::vm::run(&mut app, 0, 0, 'X', false, false);

        app.load("N.\n..", None);
        crate::core::vm::run(&mut app, 0, 0, 'N', false, false);
        assert_eq!(app.glyph_at(0, 0), '*');
    }

    #[test]
    fn test_midi_operator_garbage_input() {
        let mut app = EditorState::new(10, 10, 42, 100);

        app.load(":g3C..\n*.....", None);
        app.operate();
        assert!(app.midi.stack.is_empty());

        app.load(":035..\n*.....", None);
        app.operate();
        assert!(app.midi.stack.is_empty());

        app.load("!g99\n*...", None);
        app.operate();
        assert!(app.midi.cc_stack.is_empty());

        app.load("?.99\n*...", None);
        app.operate();
        assert!(app.midi.cc_stack.is_empty());

        app.load("?0.9\n*...", None);
        app.operate();
        assert!(app.midi.cc_stack.is_empty());
    }

    #[test]
    fn test_pitch_bend_clamps_channel() {
        let mut app = EditorState::new(10, 10, 42, 100);

        app.load("?g99\n*...", None);
        app.operate();

        assert_eq!(app.midi.cc_stack.len(), 1);
        match &app.midi.cc_stack[0] {
            crate::core::io::MidiMessage::Pb(pb) => {
                assert_eq!(pb.channel, 15);
            }
            _ => panic!("Expected Pitch Bend message"),
        }
    }

    #[test]
    fn test_midi_standard_polyphony() {
        let mut app = EditorState::new(10, 10, 42, 100);
        app.load(":03C.2\n*.....", None);

        app.operate();
        assert_eq!(app.midi.stack.len(), 1);
        assert_eq!(app.midi.stack[0].note_id, 60);
        assert_eq!(app.midi.stack[0].length, 2);

        app.midi.run();
        assert_eq!(app.midi.stack[0].length, 1);

        app.midi.run();
        assert_eq!(app.midi.stack[0].length, 0);

        app.midi.run();
        assert!(app.midi.stack.is_empty());
    }

    #[test]
    fn test_midi_kill_note() {
        let mut app = EditorState::new(10, 10, 42, 100);
        app.load(":03C.&\n*.....", None);
        app.operate();
        assert_eq!(app.midi.stack.len(), 1);

        app.load(":03C.0\n*.....", None);
        app.operate();

        assert!(app.midi.stack.is_empty());
    }

    #[test]
    fn test_midi_tied_note_sustain_and_retrigger_prevention() {
        let mut app = EditorState::new(10, 10, 42, 100);

        app.load(":03C.&\n*.....", None);
        app.operate();

        assert_eq!(app.midi.stack.len(), 1);
        assert_eq!(app.midi.stack[0].length, usize::MAX);

        for _ in 0..100 {
            app.midi.run();
        }
        assert_eq!(app.midi.stack.len(), 1);
        assert!(app.midi.stack[0].length > 1000000);

        app.write_silent(0, 1, '*');
        app.operate();
        assert_eq!(app.midi.stack.len(), 1);
        assert_eq!(app.midi.stack[0].length, usize::MAX);
    }

    #[test]
    fn test_midi_mono_legato_transition() {
        let mut app = EditorState::new(10, 10, 42, 100);
        app.load("%03C.&\n*.....", None);
        app.operate();

        assert!(app.midi.mono_stack[0].is_some());
        assert_eq!(app.midi.mono_stack[0].unwrap().note_id, 60);

        app.load("%03D.&\n*.....", None);
        app.operate();

        assert!(app.midi.mono_stack[0].is_some());
        assert_eq!(app.midi.mono_stack[0].unwrap().note_id, 62);
    }

    #[test]
    fn test_midi_tied_interrupted_by_normal_note() {
        let mut app = EditorState::new(10, 10, 42, 100);
        app.load(":03C.&\n*.....", None);
        app.operate();

        app.load(":03C.5\n*.....", None);
        app.operate();

        assert_eq!(app.midi.stack.len(), 1);
        assert_eq!(app.midi.stack[0].length, 5);
    }
}
