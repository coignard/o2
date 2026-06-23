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

use crate::core::glyph::operator_name;
use crate::core::midi::{MIDI_NOTE_OFF, MidiCc, MidiEngine, MidiMessage, MidiNote, MidiPb};
use crate::core::oxygen::OxygenEngine;
use crate::core::transpose::transpose;

macro_rules! op_binary {
    ($name:ident, $lhs_name:literal, $rhs_name:literal, $f:expr) => {
        fn $name(ctx: &mut OpContext) {
            ctx.add_port(-1, 0, false, Some($lhs_name));
            ctx.add_port(1, 0, false, Some($rhs_name));
            ctx.add_port(0, 1, true, Some("out"));
            ctx.execute(|engine, x, y| {
                let lhs = engine.listen_val(x, y, -1, 0, 0, 36);
                let rhs = engine.listen_val(x, y, 1, 0, 0, 36);
                let uc = engine.should_uppercase(x, y);
                engine.write_port(x, y, 0, 1, crate::core::glyph::key_of($f(lhs, rhs), uc));
            });
        }
    };
}

struct OpContext<'a> {
    engine: &'a mut OxygenEngine,
    midi: &'a mut MidiEngine,
    x: usize,
    y: usize,
    is_active: bool,
    should_run: bool,
    draws_ports: bool,
    triggered: bool,
}

impl<'a> OpContext<'a> {
    #[inline]
    fn add_port(&mut self, dx: isize, dy: isize, is_output: bool, name: Option<&'static str>) {
        self.engine.add_port(
            self.x,
            self.y,
            dx,
            dy,
            is_output,
            self.should_run,
            self.draws_ports,
            name,
        );
    }

    #[inline]
    fn execute<F: FnOnce(&mut OxygenEngine, usize, usize)>(&mut self, f: F) {
        if self.should_run {
            f(self.engine, self.x, self.y);
        }
    }

    #[inline]
    fn execute_triggered<F: FnOnce(&mut OxygenEngine, usize, usize)>(&mut self, f: F) {
        if self.should_run && self.triggered {
            f(self.engine, self.x, self.y);
        }
    }

    #[inline]
    fn execute_triggered_midi<F: FnOnce(&mut OxygenEngine, &mut MidiEngine, usize, usize)>(
        &mut self,
        f: F,
    ) {
        if self.should_run && self.triggered {
            f(self.engine, self.midi, self.x, self.y);
        }
    }

    #[inline]
    fn listen(&self, dx: isize, dy: isize) -> char {
        self.engine.listen(self.x, self.y, dx, dy)
    }

    #[inline]
    fn listen_val(&self, dx: isize, dy: isize, min: usize, max: usize) -> usize {
        self.engine.listen_val(self.x, self.y, dx, dy, min, max)
    }

    #[inline]
    fn lock(&mut self, dx: isize, dy: isize) {
        self.engine.lock(self.x, self.y, dx, dy)
    }

    #[inline]
    fn clear_port(&mut self) {
        self.engine.set_port(self.x, self.y, None, None);
    }
}

pub fn run(
    engine: &mut OxygenEngine,
    midi: &mut MidiEngine,
    x: usize,
    y: usize,
    g: char,
    force: bool,
) {
    let gl = g.to_ascii_lowercase();
    let is_uppercase = g.is_ascii_uppercase();
    let is_special = !g.is_ascii_alphanumeric();

    let auto_run = is_uppercase || is_special;
    let banged = engine.has_neighbor_bang(x, y);

    let is_active = auto_run || banged || force;
    let should_run = is_active;
    let draws_ports = auto_run;

    if draws_ports {
        engine.add_op_port(x, y, Some(operator_name(gl)));
    }

    let mut ctx = OpContext {
        engine,
        midi,
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

op_binary!(op_add, "a", "b", |lhs, rhs| lhs + rhs);
op_binary!(op_sub, "a", "b", |lhs: usize, rhs: usize| (rhs as isize
    - lhs as isize)
    .unsigned_abs());
op_binary!(op_mult, "a", "b", |lhs, rhs| lhs * rhs);
op_binary!(op_lesser, "a", "b", |lhs: usize, rhs: usize| lhs.min(rhs));

fn op_clock(ctx: &mut OpContext) {
    ctx.add_port(-1, 0, false, Some("rate"));
    ctx.add_port(1, 0, false, Some("mod"));
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|engine, x, y| {
        let rate = engine.listen_val(x, y, -1, 0, 1, 36);
        let m = engine.listen_val(x, y, 1, 0, 0, 36);
        if m > 0 {
            let val = (engine.f / rate) % m;
            let uc = engine.should_uppercase(x, y);
            engine.write_port(x, y, 0, 1, crate::core::glyph::key_of(val, uc));
        }
    });
}

fn op_delay(ctx: &mut OpContext) {
    ctx.add_port(-1, 0, false, Some("rate"));
    ctx.add_port(1, 0, false, Some("mod"));
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|engine, x, y| {
        let rate = engine.listen_val(x, y, -1, 0, 1, 36);
        let m = engine.listen_val(x, y, 1, 0, 1, 36);
        let res = engine.f % (m * rate);
        let out_char = if res == 0 || m == 1 { '*' } else { '.' };
        engine.write_port(x, y, 0, 1, out_char);
    });
}

fn op_if(ctx: &mut OpContext) {
    ctx.add_port(-1, 0, false, Some("a"));
    ctx.add_port(1, 0, false, Some("b"));
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|engine, x, y| {
        let a = engine.listen(x, y, -1, 0);
        let b = engine.listen(x, y, 1, 0);
        let out_char = if a == b { '*' } else { '.' };
        engine.write_port(x, y, 0, 1, out_char);
    });
}

fn op_gen(ctx: &mut OpContext) {
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
            ctx.execute(|engine, x, y| {
                let res = engine.listen(x, y, in_x, 0);
                engine.write_port(x, y, out_x, py, res);
            });
        }
    }
}

fn op_halt(ctx: &mut OpContext) {
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|engine, x, y| {
        let val = engine.listen(x, y, 0, 1);
        engine.write_port(x, y, 0, 1, val);
    });
}

fn op_inc(ctx: &mut OpContext) {
    ctx.add_port(-1, 0, false, Some("step"));
    ctx.add_port(1, 0, false, Some("mod"));
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|engine, x, y| {
        let step = engine.listen_val(x, y, -1, 0, 0, 36);
        let m = engine.listen_val(x, y, 1, 0, 0, 36);
        let val = engine.listen_val(x, y, 0, 1, 0, 36);
        let uc = engine.should_uppercase(x, y);
        let res = if m > 0 {
            crate::core::glyph::key_of((val + step) % m, uc)
        } else {
            '0'
        };
        engine.write_port(x, y, 0, 1, res);
    });
}

fn op_jumper(ctx: &mut OpContext, g: char) {
    if ctx.is_active {
        let upper = g.to_ascii_uppercase();
        let val = ctx.listen(0, -1);
        if val != upper {
            let mut i = 1;
            while ctx.engine.is_in_bounds(ctx.x as isize, ctx.y as isize + i) {
                if ctx.listen(0, i) != g {
                    break;
                }
                i += 1;
            }
            ctx.add_port(0, -1, false, Some("in"));
            ctx.add_port(0, i, true, Some("out"));
            ctx.execute(|engine, x, y| {
                engine.write_port(x, y, 0, i, val);
            });
        }
    }
}

fn op_konkat(ctx: &mut OpContext) {
    ctx.add_port(-1, 0, false, Some("len"));
    if ctx.is_active {
        let len = ctx.listen_val(-1, 0, 1, 36);
        for offset in 0..len {
            let key = ctx.listen(offset as isize + 1, 0);
            ctx.lock(offset as isize + 1, 0);
            if key != '.' {
                ctx.add_port(offset as isize + 1, 0, false, Some("in"));
                ctx.add_port(offset as isize + 1, 1, true, Some("out"));
                ctx.execute(|engine, x, y| {
                    let res = engine.var_read(key);
                    engine.write_port(x, y, offset as isize + 1, 1, res);
                });
            }
        }
    }
}

fn op_read(ctx: &mut OpContext) {
    ctx.add_port(-2, 0, false, Some("x"));
    ctx.add_port(-1, 0, false, Some("y"));
    if ctx.is_active {
        let px = ctx.listen_val(-2, 0, 0, 36) as isize;
        let py = ctx.listen_val(-1, 0, 0, 36) as isize;
        ctx.add_port(px + 1, py, false, Some("read"));
        ctx.add_port(0, 1, true, Some("out"));
        ctx.execute(|engine, x, y| {
            let val = engine.listen(x, y, px + 1, py);
            engine.write_port(x, y, 0, 1, val);
        });
    }
}

fn op_push(ctx: &mut OpContext) {
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
        ctx.execute(|engine, x, y| {
            let val = engine.listen(x, y, 1, 0);
            engine.write_port(x, y, out_x, 1, val);
        });
    }
}

fn op_query(ctx: &mut OpContext) {
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
            ctx.execute(|engine, x, y| {
                let res = engine.listen(x, y, in_x, py);
                engine.write_port(x, y, out_x, 1, res);
            });
        }
    }
}

fn op_rand(ctx: &mut OpContext) {
    ctx.add_port(-1, 0, false, Some("a"));
    ctx.add_port(1, 0, false, Some("b"));
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|engine, x, y| {
        let a = engine.listen_val(x, y, -1, 0, 0, 36);
        let b = engine.listen_val(x, y, 1, 0, 0, 36);
        let val = engine.random(x, y, a, b);
        let uc = engine.should_uppercase(x, y);
        engine.write_port(x, y, 0, 1, crate::core::glyph::key_of(val, uc));
    });
}

fn op_track(ctx: &mut OpContext) {
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
        ctx.execute(|engine, x, y| {
            let val = engine.listen(x, y, in_x, 0);
            engine.write_port(x, y, 0, 1, val);
        });
    }
}

fn op_uclid(ctx: &mut OpContext) {
    ctx.add_port(-1, 0, false, Some("step"));
    ctx.add_port(1, 0, false, Some("max"));
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|engine, x, y| {
        let step = engine.listen_val(x, y, -1, 0, 0, 36) as u64;
        let max = engine.listen_val(x, y, 1, 0, 1, 36) as u64;
        let bucket = (step * (engine.f as u64 + max - 1)) % max + step;
        let out_char = if bucket >= max { '*' } else { '.' };
        engine.write_port(x, y, 0, 1, out_char);
    });
}

fn op_var(ctx: &mut OpContext) {
    ctx.add_port(-1, 0, false, Some("write"));
    ctx.add_port(1, 0, false, Some("read"));
    if ctx.is_active {
        let write_key = ctx.listen(-1, 0);
        let read_key = ctx.listen(1, 0);

        if write_key == '.' && read_key != '.' {
            ctx.add_port(0, 1, true, Some("out"));
        }
        ctx.execute(|engine, x, y| {
            if write_key != '.' {
                engine.var_write(write_key, read_key);
            } else if read_key != '.' {
                let res = engine.var_read(read_key);
                engine.write_port(x, y, 0, 1, res);
            }
        });
    }
}

fn op_write(ctx: &mut OpContext) {
    ctx.add_port(-2, 0, false, Some("x"));
    ctx.add_port(-1, 0, false, Some("y"));
    ctx.add_port(1, 0, false, Some("val"));
    if ctx.is_active {
        let px = ctx.listen_val(-2, 0, 0, 36) as isize;
        let py = ctx.listen_val(-1, 0, 0, 36) as isize + 1;
        ctx.add_port(px, py, true, Some("out"));
        ctx.execute(|engine, x, y| {
            let val = engine.listen(x, y, 1, 0);
            engine.write_port(x, y, px, py, val);
        });
    }
}

fn op_jymper(ctx: &mut OpContext, g: char) {
    if ctx.is_active {
        let upper = g.to_ascii_uppercase();
        let val = ctx.listen(-1, 0);
        if val != upper {
            let mut i = 1;
            while ctx.engine.is_in_bounds(ctx.x as isize + i, ctx.y as isize) {
                if ctx.listen(i, 0) != g {
                    break;
                }
                i += 1;
            }
            ctx.add_port(-1, 0, false, Some("in"));
            ctx.add_port(i, 0, true, Some("out"));
            ctx.execute(|engine, x, y| {
                engine.write_port(x, y, i, 0, val);
            });
        }
    }
}

fn op_lerp(ctx: &mut OpContext) {
    ctx.add_port(-1, 0, false, Some("rate"));
    ctx.add_port(1, 0, false, Some("target"));
    ctx.add_port(0, 1, true, Some("out"));
    ctx.execute(|engine, x, y| {
        let rate = engine.listen_val(x, y, -1, 0, 0, 36) as isize;
        let target = engine.listen_val(x, y, 1, 0, 0, 36) as isize;
        let val = engine.listen_val(x, y, 0, 1, 0, 36) as isize;
        let md = if val <= target - rate {
            rate
        } else if val >= target + rate {
            -rate
        } else {
            target - val
        };
        let uc = engine.should_uppercase(x, y);
        let result = (val + md).max(0) as usize;
        engine.write_port(x, y, 0, 1, crate::core::glyph::key_of(result, uc));
    });
}

fn op_east(ctx: &mut OpContext, g: char) {
    ctx.clear_port();
    ctx.execute(|engine, x, y| engine.move_op(x, y, 1, 0, g));
}

fn op_west(ctx: &mut OpContext, g: char) {
    ctx.clear_port();
    ctx.execute(|engine, x, y| engine.move_op(x, y, -1, 0, g));
}

fn op_north(ctx: &mut OpContext, g: char) {
    ctx.clear_port();
    ctx.execute(|engine, x, y| engine.move_op(x, y, 0, -1, g));
}

fn op_south(ctx: &mut OpContext, g: char) {
    ctx.clear_port();
    ctx.execute(|engine, x, y| engine.move_op(x, y, 0, 1, g));
}

fn op_bang(ctx: &mut OpContext) {
    ctx.clear_port();
    ctx.execute(|engine, x, y| engine.write_silent(x, y, '.'));
}

fn op_comment(ctx: &mut OpContext) {
    if ctx.is_active {
        ctx.clear_port();
        ctx.lock(0, 0);
        let mut i = 1;
        while ctx.x + i < ctx.engine.w {
            let px = ctx.x + i;
            let idx = ctx.y * ctx.engine.w + px;
            ctx.engine.locks[idx] = true;
            if ctx.engine.cells[idx] == '#' {
                break;
            }
            i += 1;
        }
    }
}

fn op_midi_mono(ctx: &mut OpContext, g: char) {
    ctx.add_port(1, 0, false, Some("channel"));
    ctx.add_port(2, 0, false, Some("octave"));
    ctx.add_port(3, 0, false, Some("note"));
    ctx.add_port(4, 0, false, Some("velocity"));
    ctx.add_port(5, 0, false, Some("length"));

    ctx.execute_triggered_midi(|engine, midi, x, y| {
        let ch_g = engine.listen(x, y, 1, 0);
        let oct_g = engine.listen(x, y, 2, 0);
        let note_g = engine.listen(x, y, 3, 0);

        if ch_g == '.' || oct_g == '.' || note_g == '.' || !note_g.is_ascii_alphabetic() {
            return;
        }

        let channel = crate::core::glyph::value_of(ch_g);
        if channel > 15 {
            return;
        }

        let octave = crate::core::glyph::value_of(oct_g).clamp(0, 8);

        let vel_g = engine.listen(x, y, 4, 0);
        let velocity_raw = if vel_g == '.' || vel_g == '*' {
            15
        } else {
            crate::core::glyph::value_of(vel_g).clamp(0, 16)
        };
        let velocity = ((velocity_raw as f32 / 16.0) * 127.0) as u8;

        let len_g = engine.listen(x, y, 5, 0);

        let is_note_off = len_g == '0';
        let is_tied = len_g == '_';

        let length = if is_tied {
            usize::MAX
        } else if len_g == '.' || len_g == '*' {
            1
        } else {
            crate::core::glyph::value_of(len_g).clamp(0, 35)
        };

        if let Some(note_id) = transpose(note_g, octave as i32) {
            engine.set_port(x, y, None, None);
            let is_mono = g == '%';
            let mut kill_notes = Vec::new();

            if is_note_off {
                if is_mono {
                    if let Some(existing) = &mut midi.mono_stack[channel] {
                        if existing.is_played {
                            kill_notes.push(vec![
                                MIDI_NOTE_OFF + existing.channel,
                                existing.note_id,
                                0,
                            ]);
                        }
                        midi.mono_stack[channel] = None;
                    }
                } else {
                    midi.stack.retain_mut(|note| {
                        if note.channel == channel as u8
                            && note.octave == octave as u8
                            && note.note == note_g
                        {
                            if note.is_played {
                                kill_notes.push(vec![
                                    MIDI_NOTE_OFF + note.channel,
                                    note.note_id,
                                    0,
                                ]);
                            }
                            false
                        } else {
                            true
                        }
                    });
                }
            } else {
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

                    if let Some(existing) = &mut midi.mono_stack[channel] {
                        if is_tied && existing.note == note_g && existing.octave == octave as u8 {
                            existing.length = length;
                            skip_note_on = true;
                        } else if existing.is_played {
                            kill_notes.push(vec![
                                MIDI_NOTE_OFF + existing.channel,
                                existing.note_id,
                                0,
                            ]);
                        }
                    }

                    if !skip_note_on {
                        midi.mono_stack[channel] = Some(new_note);
                    }
                } else {
                    let mut skip_note_on = false;

                    midi.stack.retain_mut(|note| {
                        if note.channel == channel as u8
                            && note.octave == octave as u8
                            && note.note == note_g
                        {
                            if is_tied {
                                note.length = length;
                                skip_note_on = true;
                                true
                            } else {
                                if note.is_played {
                                    kill_notes.push(vec![
                                        MIDI_NOTE_OFF + note.channel,
                                        note.note_id,
                                        0,
                                    ]);
                                }
                                false
                            }
                        } else {
                            true
                        }
                    });

                    if !skip_note_on {
                        midi.stack.push(new_note);
                    }
                }
            }

            for msg in kill_notes {
                midi.send_midi_msg(&msg);
            }
        }
    });
}

fn op_cc(ctx: &mut OpContext) {
    ctx.add_port(1, 0, false, Some("channel"));
    ctx.add_port(2, 0, false, Some("knob"));
    ctx.add_port(3, 0, false, Some("value"));

    ctx.execute_triggered_midi(|engine, midi, x, y| {
        let ch_g = engine.listen(x, y, 1, 0);
        let knob_g = engine.listen(x, y, 2, 0);
        let val_g = engine.listen(x, y, 3, 0);

        if ch_g == '.' || knob_g == '.' {
            return;
        }

        let channel = crate::core::glyph::value_of(ch_g);
        if channel > 15 {
            return;
        }

        let knob = crate::core::glyph::value_of(knob_g);
        let raw_val = if val_g == '.' {
            0
        } else {
            crate::core::glyph::value_of(val_g)
        };
        let value = ((127.0 * raw_val as f32) / 35.0).ceil() as u8;

        engine.set_port(x, y, None, None);
        midi.cc_stack.push(MidiMessage::Cc(MidiCc {
            channel: channel as u8,
            knob: knob as u8,
            value,
        }));
    });
}

fn op_pb(ctx: &mut OpContext) {
    ctx.add_port(1, 0, false, Some("channel"));
    ctx.add_port(2, 0, false, Some("lsb"));
    ctx.add_port(3, 0, false, Some("msb"));

    ctx.execute_triggered_midi(|engine, midi, x, y| {
        let ch_g = engine.listen(x, y, 1, 0);
        let lsb_g = engine.listen(x, y, 2, 0);
        let msb_g = engine.listen(x, y, 3, 0);

        if ch_g == '.' || lsb_g == '.' {
            return;
        }

        let channel = crate::core::glyph::value_of(ch_g).clamp(0, 15);

        let raw_lsb = crate::core::glyph::value_of(lsb_g);
        let lsb = ((127.0 * raw_lsb as f32) / 35.0).ceil() as u8;

        let raw_msb = if msb_g == '.' {
            0
        } else {
            crate::core::glyph::value_of(msb_g)
        };
        let msb = ((127.0 * raw_msb as f32) / 35.0).ceil() as u8;

        engine.set_port(x, y, None, None);
        midi.cc_stack.push(MidiMessage::Pb(MidiPb {
            channel: channel as u8,
            lsb,
            msb,
        }));
    });
}

fn op_osc(ctx: &mut OpContext) {
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
    ctx.execute_triggered_midi(|engine, midi, x, y| {
        let path_g = engine.listen(x, y, 1, 0);
        if path_g == '.' {
            return;
        }

        let mut msg = String::with_capacity(35);
        for i in 2..=36 {
            let g = engine.listen(x, y, i, 0);
            if g == '.' {
                break;
            }
            msg.push(g);
        }

        engine.set_port(x, y, None, None);
        midi.osc_buf.push((path_g.to_string(), msg));
    });
}

fn op_udp(ctx: &mut OpContext) {
    if ctx.is_active {
        for i in 1..=36 {
            let g = ctx.listen(i, 0);
            ctx.lock(i, 0);
            if g == '.' {
                break;
            }
        }
    }
    ctx.execute_triggered_midi(|engine, midi, x, y| {
        let mut msg = String::with_capacity(35);
        for i in 1..=36 {
            let g = engine.listen(x, y, i, 0);
            if g == '.' {
                break;
            }
            msg.push(g);
        }

        engine.set_port(x, y, None, None);
        midi.udp_buf.push(msg);
    });
}

fn op_self(ctx: &mut OpContext) {
    if ctx.is_active {
        ctx.engine.add_op_port(ctx.x, ctx.y, Some("self"));
        for i in 1..=36 {
            let g = ctx.listen(i, 0);
            ctx.lock(i, 0);
            if g == '.' {
                break;
            }
        }
    }
    ctx.execute_triggered(|engine, x, y| {
        let mut msg = String::with_capacity(35);
        for i in 1..=36 {
            let g = engine.listen(x, y, i, 0);
            if g == '.' {
                break;
            }
            msg.push(g);
        }
        if msg.is_empty() {
            return;
        }

        engine.set_port(x, y, None, None);
        engine.commands.push((msg, Some((x, y + 1))));
    });
}
