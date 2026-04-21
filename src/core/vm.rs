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

    match gl {
        'a' => op_add(app, x, y, is_active, should_run, draws_ports),
        'b' => op_sub(app, x, y, is_active, should_run, draws_ports),
        'c' => op_clock(app, x, y, is_active, should_run, draws_ports),
        'd' => op_delay(app, x, y, is_active, should_run, draws_ports),
        'e' => {
            app.set_port(x, y, None, None);
            if should_run {
                op_east(app, x, y, g, should_run)
            }
        }
        'f' => op_if(app, x, y, is_active, should_run, draws_ports),
        'g' => op_gen(app, x, y, is_active, should_run, draws_ports),
        'h' => op_halt(app, x, y, is_active, should_run, draws_ports),
        'i' => op_inc(app, x, y, is_active, should_run, draws_ports),
        'j' => op_jumper(app, x, y, g, is_active, should_run, draws_ports),
        'k' => op_konkat(app, x, y, is_active, should_run, draws_ports),
        'l' => op_lesser(app, x, y, is_active, should_run, draws_ports),
        'm' => op_mult(app, x, y, is_active, should_run, draws_ports),
        'n' => {
            app.set_port(x, y, None, None);
            if should_run {
                op_north(app, x, y, g, should_run)
            }
        }
        'o' => op_read(app, x, y, is_active, should_run, draws_ports),
        'p' => op_push(app, x, y, is_active, should_run, draws_ports),
        'q' => op_query(app, x, y, is_active, should_run, draws_ports),
        'r' => op_rand(app, x, y, is_active, should_run, draws_ports),
        's' => {
            app.set_port(x, y, None, None);
            if should_run {
                op_south(app, x, y, g, should_run)
            }
        }
        't' => op_track(app, x, y, is_active, should_run, draws_ports),
        'u' => op_uclid(app, x, y, is_active, should_run, draws_ports),
        'v' => op_var(app, x, y, is_active, should_run, draws_ports),
        'w' => {
            app.set_port(x, y, None, None);
            if should_run {
                op_west(app, x, y, g, should_run)
            }
        }
        'x' => op_write(app, x, y, is_active, should_run, draws_ports),
        'y' => op_jymper(app, x, y, g, is_active, should_run, draws_ports),
        'z' => op_lerp(app, x, y, is_active, should_run, draws_ports),

        '*' => op_bang(app, x, y, should_run),
        '#' => op_comment(app, x, y, is_active, should_run),

        ':' | '%' => op_midi_mono(
            app,
            x,
            y,
            g,
            is_active,
            should_run,
            draws_ports,
            banged || force,
        ),
        '!' => op_cc(
            app,
            x,
            y,
            is_active,
            should_run,
            draws_ports,
            banged || force,
        ),
        '?' => op_pb(
            app,
            x,
            y,
            is_active,
            should_run,
            draws_ports,
            banged || force,
        ),
        '=' => op_osc(
            app,
            x,
            y,
            is_active,
            should_run,
            draws_ports,
            banged || force,
        ),
        ';' => op_udp(
            app,
            x,
            y,
            is_active,
            should_run,
            draws_ports,
            banged || force,
        ),
        '$' => op_self(app, x, y, is_active, should_run, banged || force),
        _ => {}
    }
}

fn op_add(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("a"));
    app.add_port(x, y, 1, 0, false, is_active, draws_ports, Some("b"));
    app.add_port(x, y, 0, 1, true, is_active, draws_ports, Some("out"));
    if should_run {
        let a = app.listen_val(x, y, -1, 0, 0, 36);
        let b = app.listen_val(x, y, 1, 0, 0, 36);
        let uc = app.should_uppercase(x, y);
        app.write_port(x, y, 0, 1, EditorState::key_of(a + b, uc));
    }
}

fn op_sub(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("a"));
    app.add_port(x, y, 1, 0, false, is_active, draws_ports, Some("b"));
    app.add_port(x, y, 0, 1, true, is_active, draws_ports, Some("out"));
    if should_run {
        let a = app.listen_val(x, y, -1, 0, 0, 36);
        let b = app.listen_val(x, y, 1, 0, 0, 36);
        let diff = (b as isize - a as isize).unsigned_abs();
        let uc = app.should_uppercase(x, y);
        app.write_port(x, y, 0, 1, EditorState::key_of(diff, uc));
    }
}

fn op_mult(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("a"));
    app.add_port(x, y, 1, 0, false, is_active, draws_ports, Some("b"));
    app.add_port(x, y, 0, 1, true, is_active, draws_ports, Some("out"));
    if should_run {
        let a = app.listen_val(x, y, -1, 0, 0, 36);
        let b = app.listen_val(x, y, 1, 0, 0, 36);
        let uc = app.should_uppercase(x, y);
        app.write_port(x, y, 0, 1, EditorState::key_of(a * b, uc));
    }
}

fn op_lesser(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("a"));
    app.add_port(x, y, 1, 0, false, is_active, draws_ports, Some("b"));
    app.add_port(x, y, 0, 1, true, is_active, draws_ports, Some("out"));
    if should_run {
        let a = app.listen_val(x, y, -1, 0, 0, 36);
        let b = app.listen_val(x, y, 1, 0, 0, 36);
        let uc = app.should_uppercase(x, y);
        let min_val = a.min(b);
        app.write_port(x, y, 0, 1, EditorState::key_of(min_val, uc));
    }
}

fn op_clock(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("rate"));
    app.add_port(x, y, 1, 0, false, is_active, draws_ports, Some("mod"));
    app.add_port(x, y, 0, 1, true, is_active, draws_ports, Some("out"));
    if should_run {
        let rate = app.listen_val(x, y, -1, 0, 1, 36);
        let m = app.listen_val(x, y, 1, 0, 0, 36);
        if m > 0 {
            let val = (app.engine.f / rate) % m;
            let uc = app.should_uppercase(x, y);
            app.write_port(x, y, 0, 1, EditorState::key_of(val, uc));
        }
    }
}

fn op_delay(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("rate"));
    app.add_port(x, y, 1, 0, false, is_active, draws_ports, Some("mod"));
    app.add_port(x, y, 0, 1, true, is_active, draws_ports, Some("out"));
    if should_run {
        let rate = app.listen_val(x, y, -1, 0, 1, 36);
        let m = app.listen_val(x, y, 1, 0, 1, 36);
        let res = app.engine.f % (m * rate);
        let out_char = if res == 0 || m == 1 { '*' } else { '.' };
        app.write_port(x, y, 0, 1, out_char);
    }
}

fn op_if(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("a"));
    app.add_port(x, y, 1, 0, false, is_active, draws_ports, Some("b"));
    app.add_port(x, y, 0, 1, true, is_active, draws_ports, Some("out"));
    if should_run {
        let a = app.listen(x, y, -1, 0);
        let b = app.listen(x, y, 1, 0);
        let out_char = if a == b { '*' } else { '.' };
        app.write_port(x, y, 0, 1, out_char);
    }
}

fn op_gen(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -3, 0, false, is_active, draws_ports, Some("x"));
    app.add_port(x, y, -2, 0, false, is_active, draws_ports, Some("y"));
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("len"));

    if is_active {
        let px = app.listen_val(x, y, -3, 0, 0, 36) as isize;
        let py = app.listen_val(x, y, -2, 0, 0, 36) as isize + 1;
        let len = app.listen_val(x, y, -1, 0, 1, 36);

        for offset in 0..len {
            let in_x = offset as isize + 1;
            let out_x = px + offset as isize;
            app.add_port(x, y, in_x, 0, false, is_active, draws_ports, Some("in"));
            app.add_port(x, y, out_x, py, true, is_active, draws_ports, Some("out"));
            if should_run {
                let res = app.listen(x, y, in_x, 0);
                app.write_port(x, y, out_x, py, res);
            }
        }
    }
}

fn op_halt(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, 0, 1, true, is_active, draws_ports, Some("out"));
    if should_run {
        let val = app.listen(x, y, 0, 1);
        app.write_port(x, y, 0, 1, val);
    }
}

fn op_inc(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("step"));
    app.add_port(x, y, 1, 0, false, is_active, draws_ports, Some("mod"));
    app.add_port(x, y, 0, 1, true, is_active, draws_ports, Some("out"));
    if should_run {
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
    }
}

fn op_jumper(
    app: &mut EditorState,
    x: usize,
    y: usize,
    g: char,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    if is_active {
        let upper = g.to_ascii_uppercase();
        let val = app.listen(x, y, 0, -1);
        if val != upper {
            let mut i = 1;
            while app.is_in_bounds(x as isize, y as isize + i) {
                if app.listen(x, y, 0, i) != g {
                    break;
                }
                i += 1;
            }
            app.add_port(x, y, 0, -1, false, is_active, draws_ports, Some("in"));
            app.add_port(x, y, 0, i, true, is_active, draws_ports, Some("out"));
            if should_run {
                app.write_port(x, y, 0, i, val);
            }
        }
    }
}

fn op_konkat(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("len"));
    if is_active {
        let len = app.listen_val(x, y, -1, 0, 1, 36);
        for offset in 0..len {
            let key = app.listen(x, y, offset as isize + 1, 0);
            app.lock(x, y, offset as isize + 1, 0);
            if key != '.' {
                app.add_port(
                    x,
                    y,
                    offset as isize + 1,
                    0,
                    false,
                    is_active,
                    draws_ports,
                    Some("in"),
                );
                app.add_port(
                    x,
                    y,
                    offset as isize + 1,
                    1,
                    true,
                    is_active,
                    draws_ports,
                    Some("out"),
                );
                if should_run {
                    let res = app.var_read(key);
                    app.write_port(x, y, offset as isize + 1, 1, res);
                }
            }
        }
    }
}

fn op_read(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -2, 0, false, is_active, draws_ports, Some("x"));
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("y"));
    if is_active {
        let px = app.listen_val(x, y, -2, 0, 0, 36) as isize;
        let py = app.listen_val(x, y, -1, 0, 0, 36) as isize;
        app.add_port(
            x,
            y,
            px + 1,
            py,
            false,
            is_active,
            draws_ports,
            Some("read"),
        );
        app.add_port(x, y, 0, 1, true, is_active, draws_ports, Some("out"));
        if should_run {
            let val = app.listen(x, y, px + 1, py);
            app.write_port(x, y, 0, 1, val);
        }
    }
}

fn op_push(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -2, 0, false, is_active, draws_ports, Some("key"));
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("len"));
    app.add_port(x, y, 1, 0, false, is_active, draws_ports, Some("val"));
    if is_active {
        let key = app.listen_val(x, y, -2, 0, 0, 36);
        let len = app.listen_val(x, y, -1, 0, 1, 36);
        for offset in 0..len {
            app.lock(x, y, offset as isize, 1);
        }
        let out_x = (key % len) as isize;
        app.add_port(x, y, out_x, 1, true, is_active, draws_ports, Some("out"));
        if should_run {
            let val = app.listen(x, y, 1, 0);
            app.write_port(x, y, out_x, 1, val);
        }
    }
}

fn op_query(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -3, 0, false, is_active, draws_ports, Some("x"));
    app.add_port(x, y, -2, 0, false, is_active, draws_ports, Some("y"));
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("len"));
    if is_active {
        let px = app.listen_val(x, y, -3, 0, 0, 36) as isize;
        let py = app.listen_val(x, y, -2, 0, 0, 36) as isize;
        let len = app.listen_val(x, y, -1, 0, 1, 36);
        for offset in 0..len {
            let in_x = px + offset as isize + 1;
            let out_x = offset as isize - len as isize + 1;
            app.add_port(x, y, in_x, py, false, is_active, draws_ports, Some("in"));
            app.add_port(x, y, out_x, 1, true, is_active, draws_ports, Some("out"));
            if should_run {
                let res = app.listen(x, y, in_x, py);
                app.write_port(x, y, out_x, 1, res);
            }
        }
    }
}

fn op_rand(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("a"));
    app.add_port(x, y, 1, 0, false, is_active, draws_ports, Some("b"));
    app.add_port(x, y, 0, 1, true, is_active, draws_ports, Some("out"));
    if should_run {
        let a = app.listen_val(x, y, -1, 0, 0, 36);
        let b = app.listen_val(x, y, 1, 0, 0, 36);
        let val = app.random(x, y, a, b);
        let uc = app.should_uppercase(x, y);
        app.write_port(x, y, 0, 1, EditorState::key_of(val, uc));
    }
}

fn op_track(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -2, 0, false, is_active, draws_ports, Some("key"));
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("len"));
    if is_active {
        let key = app.listen_val(x, y, -2, 0, 0, 36);
        let len = app.listen_val(x, y, -1, 0, 1, 36);
        for offset in 0..len {
            app.lock(x, y, offset as isize + 1, 0);
        }
        let in_x = (key % len) as isize + 1;
        app.add_port(x, y, in_x, 0, false, is_active, draws_ports, Some("val"));
        app.add_port(x, y, 0, 1, true, is_active, draws_ports, Some("out"));
        if should_run {
            let val = app.listen(x, y, in_x, 0);
            app.write_port(x, y, 0, 1, val);
        }
    }
}

fn op_uclid(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("step"));
    app.add_port(x, y, 1, 0, false, is_active, draws_ports, Some("max"));
    app.add_port(x, y, 0, 1, true, is_active, draws_ports, Some("out"));
    if should_run {
        let step = app.listen_val(x, y, -1, 0, 0, 36) as u64;
        let max = app.listen_val(x, y, 1, 0, 1, 36) as u64;
        let bucket = (step * (app.engine.f as u64 + max - 1)) % max + step;
        let out_char = if bucket >= max { '*' } else { '.' };
        app.write_port(x, y, 0, 1, out_char);
    }
}

fn op_var(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("write"));
    app.add_port(x, y, 1, 0, false, is_active, draws_ports, Some("read"));
    if is_active {
        let write_key = app.listen(x, y, -1, 0);
        let read_key = app.listen(x, y, 1, 0);

        if write_key == '.' && read_key != '.' {
            app.add_port(x, y, 0, 1, true, is_active, draws_ports, Some("out"));
        }
        if should_run {
            if write_key != '.' {
                app.var_write(write_key, read_key);
            } else if read_key != '.' {
                let res = app.var_read(read_key);
                app.write_port(x, y, 0, 1, res);
            }
        }
    }
}

fn op_write(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -2, 0, false, is_active, draws_ports, Some("x"));
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("y"));
    app.add_port(x, y, 1, 0, false, is_active, draws_ports, Some("val"));
    if is_active {
        let px = app.listen_val(x, y, -2, 0, 0, 36) as isize;
        let py = app.listen_val(x, y, -1, 0, 0, 36) as isize + 1;
        app.add_port(x, y, px, py, true, is_active, draws_ports, Some("out"));
        if should_run {
            let val = app.listen(x, y, 1, 0);
            app.write_port(x, y, px, py, val);
        }
    }
}

fn op_jymper(
    app: &mut EditorState,
    x: usize,
    y: usize,
    g: char,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    if is_active {
        let upper = g.to_ascii_uppercase();
        let val = app.listen(x, y, -1, 0);
        if val != upper {
            let mut i = 1;
            while app.is_in_bounds(x as isize + i, y as isize) {
                if app.listen(x, y, i, 0) != g {
                    break;
                }
                i += 1;
            }
            app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("in"));
            app.add_port(x, y, i, 0, true, is_active, draws_ports, Some("out"));
            if should_run {
                app.write_port(x, y, i, 0, val);
            }
        }
    }
}

fn op_lerp(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
) {
    app.add_port(x, y, -1, 0, false, is_active, draws_ports, Some("rate"));
    app.add_port(x, y, 1, 0, false, is_active, draws_ports, Some("target"));
    app.add_port(x, y, 0, 1, true, is_active, draws_ports, Some("out"));
    if should_run {
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
    }
}

fn op_east(app: &mut EditorState, x: usize, y: usize, g: char, should_run: bool) {
    if should_run {
        app.move_op(x, y, 1, 0, g);
    }
}

fn op_west(app: &mut EditorState, x: usize, y: usize, g: char, should_run: bool) {
    if should_run {
        app.move_op(x, y, -1, 0, g);
    }
}

fn op_north(app: &mut EditorState, x: usize, y: usize, g: char, should_run: bool) {
    if should_run {
        app.move_op(x, y, 0, -1, g);
    }
}

fn op_south(app: &mut EditorState, x: usize, y: usize, g: char, should_run: bool) {
    if should_run {
        app.move_op(x, y, 0, 1, g);
    }
}

fn op_bang(app: &mut EditorState, x: usize, y: usize, should_run: bool) {
    app.set_port(x, y, None, None);
    if should_run {
        app.write_silent(x, y, '.');
    }
}

fn op_comment(app: &mut EditorState, x: usize, y: usize, is_active: bool, _should_run: bool) {
    if is_active {
        app.set_port(x, y, None, None);
        app.lock(x, y, 0, 0);
        let mut i = 1;
        while x + i < app.engine.w {
            let px = x + i;
            let idx = y * app.engine.w + px;
            app.engine.locks[idx] = true;
            if app.engine.cells[idx] == '#' {
                break;
            }
            i += 1;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn op_midi_mono(
    app: &mut EditorState,
    x: usize,
    y: usize,
    g: char,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
    banged: bool,
) {
    app.add_port(x, y, 1, 0, false, is_active, draws_ports, Some("channel"));
    app.add_port(x, y, 2, 0, false, is_active, draws_ports, Some("octave"));
    app.add_port(x, y, 3, 0, false, is_active, draws_ports, Some("note"));
    app.add_port(x, y, 4, 0, false, is_active, draws_ports, Some("velocity"));
    app.add_port(x, y, 5, 0, false, is_active, draws_ports, Some("length"));

    if should_run && banged {
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
        let length = if len_g == '.' || len_g == '*' {
            1
        } else {
            EditorState::value_of(len_g).clamp(0, 32)
        };

        if let Some(note_id) = transpose(note_g, octave as i32) {
            let is_mono = g == '%';

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
                if let Some(existing) = app.midi.mono_stack[channel]
                    && existing.is_played
                    && let Some(conn) = &mut app.midi.out
                {
                    let _ = conn.send(&[0x80 + existing.channel, existing.note_id, 0]);
                }
                app.midi.mono_stack[channel] = Some(new_note);
            } else {
                if let Some(conn) = &mut app.midi.out {
                    app.midi.stack.retain(|note| {
                        if note.channel == channel as u8
                            && note.octave == octave as u8
                            && note.note == note_g
                        {
                            if note.is_played {
                                let _ = conn.send(&[0x80 + note.channel, note.note_id, 0]);
                            }
                            false
                        } else {
                            true
                        }
                    });
                } else {
                    app.midi.stack.retain(|note| {
                        !(note.channel == channel as u8
                            && note.octave == octave as u8
                            && note.note == note_g)
                    });
                }
                app.midi.stack.push(new_note);
            }
        }
    }
}

fn op_cc(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
    banged: bool,
) {
    app.add_port(x, y, 1, 0, false, is_active, draws_ports, Some("channel"));
    app.add_port(x, y, 2, 0, false, is_active, draws_ports, Some("knob"));
    app.add_port(x, y, 3, 0, false, is_active, draws_ports, Some("value"));

    if should_run && banged {
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
    }
}

fn op_pb(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
    banged: bool,
) {
    app.add_port(x, y, 1, 0, false, is_active, draws_ports, Some("channel"));
    app.add_port(x, y, 2, 0, false, is_active, draws_ports, Some("lsb"));
    app.add_port(x, y, 3, 0, false, is_active, draws_ports, Some("msb"));

    if should_run && banged {
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
    }
}

fn op_osc(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
    banged: bool,
) {
    app.add_port(x, y, 1, 0, false, is_active, draws_ports, Some("path"));
    if is_active {
        for i in 2..=36 {
            let g = app.listen(x, y, i, 0);
            app.lock(x, y, i, 0);
            if g == '.' {
                break;
            }
        }
    }
    if should_run && banged {
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
    }
}

fn op_udp(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    _draws_ports: bool,
    banged: bool,
) {
    if is_active {
        for i in 1..=36 {
            let g = app.listen(x, y, i, 0);
            app.lock(x, y, i, 0);
            if g == '.' {
                break;
            }
        }
    }
    if should_run && banged {
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
    }
}

fn op_self(
    app: &mut EditorState,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    banged: bool,
) {
    if is_active {
        app.add_op_port(x, y, Some("self"));
        for i in 1..=36 {
            let g = app.listen(x, y, i, 0);
            app.lock(x, y, i, 0);
            if g == '.' {
                break;
            }
        }
    }
    if should_run && banged {
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
    }
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
}
