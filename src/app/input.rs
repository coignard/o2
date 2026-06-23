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

use crate::app::clipboard;
use crate::app::commander::{preview_command, run_command};
use crate::app::editor::EditorState;
use crate::app::types::{InputMode, PopupType, PromptPurpose};
use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

const MENU_EMPTY_ROWS: [usize; 5] = [4, 8, 10, 12, 16];
const ESC: char = '\u{1b}';
const BEL: char = '\u{07}';

fn default_patch_name() -> String {
    format!("patch-{}.o2", arvelie_neralie())
}

fn main_menu_up(mut sel: usize) -> usize {
    loop {
        sel = sel.saturating_sub(1);
        if !MENU_EMPTY_ROWS.contains(&sel) || sel == 0 {
            break;
        }
    }
    sel
}

fn main_menu_down(mut sel: usize) -> usize {
    loop {
        sel = (sel + 1).min(17);
        if !MENU_EMPTY_ROWS.contains(&sel) || sel == 17 {
            break;
        }
    }
    sel
}

pub fn datetime_to_arvelie_neralie<T: TimeZone>(datetime: &DateTime<T>) -> String {
    let year = datetime.year();
    let y_str = format!("{:02}", year.rem_euclid(100));

    let doty = datetime.ordinal() - 1;

    let m = if doty == 364 || doty == 365 {
        '+'
    } else {
        (b'A' + (doty / 14) as u8) as char
    };

    let d = if doty == 365 { 2 } else { (doty % 14) + 1 };

    let ms_since_midnight = (datetime.num_seconds_from_midnight() as u64) * 1000
        + (datetime.nanosecond() / 1_000_000) as u64;

    let neralie = (ms_since_midnight * 1_000_000) / 86_400_000;

    format!("{}{}{:02}-{:06}", y_str, m, d, neralie)
}

pub fn arvelie_neralie() -> String {
    datetime_to_arvelie_neralie(&Local::now())
}

pub fn handle_mouse(app: &mut EditorState, mouse_event: MouseEvent) {
    app.last_input_was_mouse = true;
    let col = mouse_event.column;
    let row = mouse_event.row;

    if !app.popup.is_empty() {
        let (term_cols, term_rows) =
            crossterm::terminal::size().unwrap_or((app.o2.w as u16, app.o2.h as u16));
        let term_area = Rect::new(0, 0, term_cols, term_rows);

        let mut top_rect = Rect::default();
        let mut prev_rect = None;
        for p in &app.popup {
            top_rect = crate::app::ui::render::get_popup_rect(term_area, p, prev_rect);
            prev_rect = Some(top_rect);
        }

        if col >= top_rect.x
            && col < top_rect.x + top_rect.width
            && row >= top_rect.y
            && row < top_rect.y + top_rect.height
        {
            let rel_y = (row.saturating_sub(top_rect.y + 1)) as usize;
            let mut trigger_action = false;
            let mut pop_current = false;
            let mut trigger_clock_toggle = false;

            match app.popup.last_mut().unwrap() {
                PopupType::MainMenu { selected } => {
                    if let MouseEventKind::ScrollUp = mouse_event.kind {
                        *selected = main_menu_up(*selected);
                    } else if let MouseEventKind::ScrollDown = mouse_event.kind {
                        *selected = main_menu_down(*selected);
                    } else if let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind
                        && rel_y <= 17
                        && !MENU_EMPTY_ROWS.contains(&rel_y)
                    {
                        *selected = rel_y;
                        trigger_action = true;
                    }
                }
                PopupType::MidiMenu { selected, devices } => {
                    if let MouseEventKind::ScrollUp = mouse_event.kind {
                        *selected = selected.saturating_sub(1);
                    } else if let MouseEventKind::ScrollDown = mouse_event.kind {
                        *selected = (*selected + 1).min(devices.len().saturating_sub(1));
                    } else if let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind
                        && rel_y < devices.len()
                    {
                        *selected = rel_y;
                        trigger_action = true;
                    }
                }
                PopupType::AutofitMenu { selected } => {
                    if let MouseEventKind::ScrollUp | MouseEventKind::ScrollDown = mouse_event.kind
                    {
                        *selected = 1 - *selected;
                    } else if let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind
                        && rel_y <= 1
                    {
                        *selected = rel_y;
                        trigger_action = true;
                    }
                }
                PopupType::ConfirmNew { selected } => {
                    if let MouseEventKind::ScrollUp | MouseEventKind::ScrollDown = mouse_event.kind
                    {
                        *selected = 1 - *selected;
                    } else if let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind
                        && rel_y <= 1
                    {
                        *selected = rel_y;
                        trigger_action = true;
                    }
                }
                PopupType::ConfirmQuit { selected, has_file } => {
                    let options_count = if *has_file { 4 } else { 3 };
                    if let MouseEventKind::ScrollUp = mouse_event.kind {
                        *selected = selected.saturating_sub(1);
                    } else if let MouseEventKind::ScrollDown = mouse_event.kind {
                        *selected = (*selected + 1).min(options_count - 1);
                    } else if let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind
                        && rel_y < options_count
                    {
                        *selected = rel_y;
                        trigger_action = true;
                    }
                }
                PopupType::ClockMenu { selected } => {
                    if let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind
                        && rel_y == 0
                    {
                        *selected = 0;
                        trigger_clock_toggle = true;
                    }
                }
                PopupType::Controls
                | PopupType::Operators
                | PopupType::About { .. }
                | PopupType::Msg { .. }
                | PopupType::RoflCopter => {
                    if let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind {
                        pop_current = true;
                    }
                }
                PopupType::Prompt { .. } => {}
            }

            if pop_current {
                app.popup.pop();
            } else if trigger_clock_toggle {
                app.midi_bclock = !app.midi_bclock;
            } else if trigger_action {
                handle_key(
                    app,
                    KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
                );
            }
        } else if let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind {
            app.popup.pop();
        }
        return;
    }

    let col = col as usize;
    let row = row as usize;

    let (_term_cols, term_rows) =
        crossterm::terminal::size().unwrap_or((app.o2.w as u16, app.o2.h as u16));
    let viewport_h = term_rows.saturating_sub(2) as usize;

    let scroll_x = app.scroll_x;
    let scroll_y = app.scroll_y;
    let grid_x = col + scroll_x;
    let grid_y = row + scroll_y;

    match mouse_event.kind {
        MouseEventKind::Down(MouseButton::Left) if row < viewport_h && grid_y < app.o2.h => {
            app.mouse_from = Some((grid_x, grid_y));
            app.select(grid_x as isize, grid_y as isize, 0, 0);
        }
        MouseEventKind::Down(MouseButton::Right) => {
            app.cut();
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some((sx, sy)) = app.mouse_from {
                let col_clamped = grid_x.min(app.o2.w.saturating_sub(1));
                let row_clamped = grid_y.min(app.o2.h.saturating_sub(1));
                app.select(
                    sx as isize,
                    sy as isize,
                    col_clamped as isize - sx as isize,
                    row_clamped as isize - sy as isize,
                );
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if let Some((sx, sy)) = app.mouse_from {
                let col_clamped = grid_x.min(app.o2.w.saturating_sub(1));
                let row_clamped = grid_y.min(app.o2.h.saturating_sub(1));
                app.select(
                    sx as isize,
                    sy as isize,
                    col_clamped as isize - sx as isize,
                    row_clamped as isize - sy as isize,
                );
            }
            app.mouse_from = None;
        }
        MouseEventKind::ScrollUp => {
            if app.mode == InputMode::Slide {
                app.drag(0, 1);
            } else {
                app.move_cursor(0, 1);
            }
        }
        MouseEventKind::ScrollDown => {
            if app.mode == InputMode::Slide {
                app.drag(0, -1);
            } else {
                app.move_cursor(0, -1);
            }
        }
        MouseEventKind::ScrollLeft => {
            if app.mode == InputMode::Slide {
                app.drag(-1, 0);
            } else {
                app.move_cursor(-1, 0);
            }
        }
        MouseEventKind::ScrollRight => {
            if app.mode == InputMode::Slide {
                app.drag(1, 0);
            } else {
                app.move_cursor(1, 0);
            }
        }
        _ => {}
    }
}

fn sanitize_paste(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ESC {
            match chars.peek() {
                Some('[') => {
                    chars.next();
                    while let Some(&n) = chars.peek() {
                        chars.next();
                        if ('@'..='~').contains(&n) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(&n) = chars.peek() {
                        chars.next();
                        if n == BEL {
                            break;
                        }
                        if n == ESC {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            }
        } else if c == '\n' || c == '\r' || !c.is_control() {
            out.push(c);
        }
    }
    out
}

pub fn handle_paste(app: &mut EditorState, text: &str) {
    app.last_input_was_mouse = false;
    let text = sanitize_paste(text);
    if app.commander.active {
        let clean_text = text.replace(['\n', '\r'], "");
        app.commander.query.push_str(&clean_text);
        preview_command(app);
    } else {
        app.paste_text(&text);
    }
}

pub fn handle_key(app: &mut EditorState, key: KeyEvent) -> bool {
    if key.kind != KeyEventKind::Press && key.kind != KeyEventKind::Repeat {
        return false;
    }

    app.last_input_was_mouse = false;

    let ctrl =
        key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::META);

    if ctrl && matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q')) {
        let already_confirming = app
            .popup
            .iter()
            .any(|p| matches!(p, PopupType::ConfirmQuit { .. }));
        if already_confirming {
            return false;
        }

        if app.is_dirty() {
            app.popup.push(PopupType::ConfirmQuit {
                selected: 0,
                has_file: app.current_file.is_some(),
            });
        } else {
            app.running = false;
        }
        return true;
    }

    if let Some(mut popup) = app.popup.pop() {
        let (close_popup, pop_parent, spawn_popups) = handle_popup_key(app, &mut popup, key);

        if !close_popup {
            app.popup.push(popup);
        } else if pop_parent {
            app.popup.pop();
        }

        app.popup.extend(spawn_popups);

        return true;
    }

    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    if app.commander.active {
        handle_commander_key(app, key, ctrl, alt);
        true
    } else {
        handle_main_key(app, key, ctrl, shift, alt)
    }
}

fn handle_popup_key(
    app: &mut EditorState,
    popup: &mut PopupType,
    key: KeyEvent,
) -> (bool, bool, Vec<PopupType>) {
    let mut close_popup = false;
    let mut pop_parent = false;
    let mut spawn_popups = Vec::new();

    match popup {
        PopupType::Controls
        | PopupType::Operators
        | PopupType::About { .. }
        | PopupType::Msg { .. }
        | PopupType::RoflCopter => {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') | KeyCode::Enter
            ) {
                close_popup = true;
            }
        }
        PopupType::MainMenu { selected } => match key.code {
            KeyCode::Esc | KeyCode::Left => close_popup = true,
            KeyCode::Up => *selected = main_menu_up(*selected),
            KeyCode::Down => *selected = main_menu_down(*selected),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char(' ') => match *selected {
                0 => spawn_popups.push(PopupType::ConfirmNew { selected: 0 }),
                1 => spawn_popups.push(PopupType::Prompt {
                    purpose: PromptPurpose::Open,
                    input: String::new(),
                    cursor: 0,
                }),
                2 => {
                    if app.current_file.is_some() {
                        if app.save() {
                            spawn_popups.push(PopupType::Msg {
                                title: "Saved".into(),
                                text: "File saved successfully.".into(),
                            });
                        } else {
                            spawn_popups.push(PopupType::Msg {
                                title: "Error".into(),
                                text: "Could not save file.".into(),
                            });
                        }
                    } else {
                        let default_name = default_patch_name();
                        spawn_popups.push(PopupType::Prompt {
                            purpose: PromptPurpose::SaveAs { quit_after: false },
                            cursor: default_name.chars().count(),
                            input: default_name,
                        });
                    }
                }
                3 => {
                    let default_name = if let Some(path) = &app.current_file {
                        path.to_string_lossy().into_owned()
                    } else {
                        default_patch_name()
                    };
                    spawn_popups.push(PopupType::Prompt {
                        purpose: PromptPurpose::SaveAs { quit_after: false },
                        cursor: default_name.chars().count(),
                        input: default_name,
                    });
                }
                5 => {
                    let val = app.bpm.to_string();
                    spawn_popups.push(PopupType::Prompt {
                        purpose: PromptPurpose::SetBpm,
                        cursor: val.chars().count(),
                        input: val,
                    });
                }
                6 => {
                    let val = format!("{}x{}", app.o2.w, app.o2.h);
                    spawn_popups.push(PopupType::Prompt {
                        purpose: PromptPurpose::SetGridSize,
                        cursor: val.chars().count(),
                        input: val,
                    });
                }
                7 => spawn_popups.push(PopupType::AutofitMenu { selected: 0 }),
                9 => {
                    let devices = app.get_midi_output_devices();
                    spawn_popups.push(PopupType::MidiMenu {
                        selected: 0,
                        devices,
                    });
                }
                11 => spawn_popups.push(PopupType::ClockMenu { selected: 0 }),
                13 => spawn_popups.push(PopupType::Controls),
                14 => spawn_popups.push(PopupType::Operators),
                15 => spawn_popups.push(PopupType::About {
                    opened_at: std::time::Instant::now(),
                }),
                17 => {
                    if app.is_dirty() {
                        let already_confirming = app
                            .popup
                            .iter()
                            .any(|p| matches!(p, PopupType::ConfirmQuit { .. }));
                        if !already_confirming {
                            spawn_popups.push(PopupType::ConfirmQuit {
                                selected: 0,
                                has_file: app.current_file.is_some(),
                            });
                        }
                    } else {
                        app.running = false;
                    }
                }
                _ => {}
            },
            _ => {}
        },
        PopupType::MidiMenu { selected, devices } => match key.code {
            KeyCode::Esc | KeyCode::Left => close_popup = true,
            KeyCode::Up => *selected = selected.saturating_sub(1),
            KeyCode::Down => *selected = (*selected + 1).min(devices.len().saturating_sub(1)),
            KeyCode::Char(' ') => {
                if let Some(name) = devices.get(*selected).cloned() {
                    app.set_midi_device(&name);
                }
            }
            KeyCode::Enter | KeyCode::Right => {}
            _ => {}
        },
        PopupType::ConfirmNew { selected } => match key.code {
            KeyCode::Esc | KeyCode::Left => close_popup = true,
            KeyCode::Up | KeyCode::Down => *selected = 1 - *selected,
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Right => {
                if *selected == 1 {
                    app.o2.cells.fill('.');
                    app.history.clear();
                    app.history.record(&app.o2.cells);
                    app.history.saved_absolute_index = Some(app.history.offset + app.history.index);
                    app.current_file = None;
                    pop_parent = true;
                }
                close_popup = true;
            }
            _ => {}
        },
        PopupType::ConfirmQuit { selected, has_file } => {
            let options_count = if *has_file { 4 } else { 3 };
            match key.code {
                KeyCode::Esc | KeyCode::Left => close_popup = true,
                KeyCode::Up => *selected = selected.saturating_sub(1),
                KeyCode::Down => *selected = (*selected + 1).min(options_count - 1),
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Right => {
                    match (*has_file, *selected) {
                        (true, 0) => {
                            if app.current_file.is_some() {
                                if app.save() {
                                    app.running = false;
                                } else {
                                    spawn_popups.push(PopupType::Msg {
                                        title: "Error".into(),
                                        text: "Could not save file.".into(),
                                    });
                                }
                                close_popup = true;
                            } else {
                                let default_name = default_patch_name();
                                spawn_popups.push(PopupType::Prompt {
                                    purpose: PromptPurpose::SaveAs { quit_after: true },
                                    cursor: default_name.chars().count(),
                                    input: default_name,
                                });
                            }
                        }
                        (true, 1) | (false, 0) => {
                            let default_name = if let Some(path) = &app.current_file {
                                path.to_string_lossy().into_owned()
                            } else {
                                default_patch_name()
                            };
                            spawn_popups.push(PopupType::Prompt {
                                purpose: PromptPurpose::SaveAs { quit_after: true },
                                cursor: default_name.chars().count(),
                                input: default_name,
                            });
                        }
                        (true, 2) | (false, 1) => {
                            app.running = false;
                            close_popup = true;
                        }
                        _ => {
                            close_popup = true;
                        }
                    }
                }
                _ => {}
            }
        }
        PopupType::AutofitMenu { selected } => {
            let mut do_autofit = false;
            match key.code {
                KeyCode::Esc | KeyCode::Left => close_popup = true,
                KeyCode::Up | KeyCode::Down => *selected = 1 - *selected,
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Right => {
                    do_autofit = true;
                    close_popup = true;
                    pop_parent = true;
                }
                _ => {}
            }

            if do_autofit {
                let (cols, rows) =
                    crossterm::terminal::size().unwrap_or((app.o2.w as u16, app.o2.h as u16));
                let (mut new_w, mut new_h) = (cols as usize, rows.saturating_sub(2) as usize);
                if *selected == 0 {
                    new_w = (new_w / app.grid_w) * app.grid_w + 1;
                    new_h = (new_h / app.grid_h) * app.grid_h + 1;
                }
                app.resize(new_w.max(1), new_h.max(1));
            }
        }
        PopupType::ClockMenu { selected } => match key.code {
            KeyCode::Esc | KeyCode::Left => close_popup = true,
            KeyCode::Up | KeyCode::Down => *selected = 0,
            KeyCode::Char(' ') => {
                app.midi_bclock = !app.midi_bclock;
            }
            KeyCode::Enter | KeyCode::Right => {}
            _ => {}
        },
        PopupType::Prompt {
            purpose,
            input,
            cursor,
        } => match key.code {
            KeyCode::Esc => close_popup = true,
            KeyCode::Tab => {
                if matches!(purpose, PromptPurpose::Open | PromptPurpose::SaveAs { .. })
                    && let Some(comp) = autocomplete_path(input)
                {
                    input.push_str(&comp);
                    *cursor = input.chars().count();
                }
            }
            KeyCode::Left => {
                *cursor = cursor.saturating_sub(1);
            }
            KeyCode::Right => {
                *cursor = (*cursor + 1).min(input.chars().count());
            }
            KeyCode::Home => {
                *cursor = 0;
            }
            KeyCode::End => {
                *cursor = input.chars().count();
            }
            KeyCode::Backspace if *cursor > 0 => {
                *cursor -= 1;
                let byte_idx = input
                    .char_indices()
                    .nth(*cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(input.len());
                input.remove(byte_idx);
            }
            KeyCode::Delete if *cursor < input.chars().count() => {
                if let Some((byte_idx, _)) = input.char_indices().nth(*cursor) {
                    input.remove(byte_idx);
                }
            }
            KeyCode::Char(c) => {
                let byte_idx = input
                    .char_indices()
                    .nth(*cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(input.len());
                input.insert(byte_idx, c);
                *cursor += 1;
            }
            KeyCode::Enter => match purpose {
                PromptPurpose::Open => {
                    if let Ok(content) = std::fs::read_to_string(&*input) {
                        app.load(&content, Some(input.clone().into()));
                        let (cols, rows) = crossterm::terminal::size()
                            .unwrap_or((app.o2.w as u16, app.o2.h as u16));
                        app.resize(cols as usize, rows.saturating_sub(2) as usize);
                        app.history.saved_absolute_index =
                            Some(app.history.offset + app.history.index);
                        close_popup = true;
                        pop_parent = true;
                    } else {
                        spawn_popups.push(PopupType::Msg {
                            title: "Error".into(),
                            text: format!("Cannot open {}", input),
                        });
                    }
                }
                PromptPurpose::SaveAs { quit_after } => {
                    app.current_file = Some(input.clone().into());
                    if app.save() {
                        close_popup = true;
                        pop_parent = true;
                        if *quit_after {
                            app.running = false;
                        } else {
                            spawn_popups.push(PopupType::Msg {
                                title: "Saved".into(),
                                text: "File saved successfully.".into(),
                            });
                        }
                    } else {
                        spawn_popups.push(PopupType::Msg {
                            title: "Error".into(),
                            text: "Could not save file.".into(),
                        });
                    }
                }
                PromptPurpose::SetBpm => {
                    if let Ok(b) = input.parse() {
                        app.set_bpm(b);
                    }
                    close_popup = true;
                }
                PromptPurpose::SetGridSize => {
                    if let Some((ws, hs)) = input.split_once('x')
                        && let (Ok(w), Ok(h)) = (ws.parse(), hs.parse())
                    {
                        app.resize(w, h);
                    }
                    close_popup = true;
                }
            },
            _ => {}
        },
    }

    (close_popup, pop_parent, spawn_popups)
}

fn handle_commander_key(app: &mut EditorState, key: KeyEvent, ctrl: bool, alt: bool) {
    match key.code {
        KeyCode::Esc => {
            app.commander.active = false;
            app.commander.query.clear();
            app.commander.index = app.commander.history.len();
            preview_command(app);
        }
        KeyCode::Char('k') | KeyCode::Char('K') if ctrl => {
            app.commander.active = false;
            app.commander.query.clear();
            app.commander.index = app.commander.history.len();
            preview_command(app);
        }
        KeyCode::Backspace | KeyCode::Delete => {
            app.commander.query.pop();
            preview_command(app);
        }
        KeyCode::Char('h') | KeyCode::Char('H') if ctrl => {
            app.commander.query.pop();
            preview_command(app);
        }
        KeyCode::Enter => {
            let query = app.commander.query.clone();
            if !query.is_empty() {
                if app.commander.history.last() != Some(&query) {
                    app.commander.history.push(query.clone());
                }
                app.commander.index = app.commander.history.len();
            }
            run_command(app, &query, None);
            app.commander.active = false;
            app.commander.query.clear();
        }
        KeyCode::Up if app.commander.index > 0 => {
            app.commander.index -= 1;
            app.commander.query = app.commander.history[app.commander.index].clone();
            preview_command(app);
        }
        KeyCode::Down => {
            if app.commander.index + 1 < app.commander.history.len() {
                app.commander.index += 1;
                app.commander.query = app.commander.history[app.commander.index].clone();
                preview_command(app);
            } else if app.commander.index < app.commander.history.len() {
                app.commander.index = app.commander.history.len();
                app.commander.query.clear();
                preview_command(app);
            }
        }
        KeyCode::Char(c) => {
            if !ctrl && !alt {
                app.commander.query.push(c);
                preview_command(app);
            } else if ctrl
                && (c == 'v' || c == 'V')
                && let Some(text) = clipboard::paste()
            {
                handle_paste(app, &text);
            }
        }
        _ => {}
    }
}

fn handle_main_key(
    app: &mut EditorState,
    key: KeyEvent,
    ctrl: bool,
    shift: bool,
    alt: bool,
) -> bool {
    let leap_x = app.grid_w as isize;
    let leap_y = app.grid_h as isize;

    let is_char =
        matches!(key.code, KeyCode::Char(c) if !ctrl && !alt && EditorState::is_allowed(c));
    if !is_char {
        app.rofl_buffer.clear();
    }

    let mut needs_draw = true;

    match key.code {
        KeyCode::Esc => {
            app.guide = false;
            app.select(app.cursor.cx as isize, app.cursor.cy as isize, 0, 0);
            app.mode = InputMode::Normal;
        }

        KeyCode::Char('s') | KeyCode::Char('S') if ctrl => {
            if app.current_file.is_some() {
                if app.save() {
                    app.popup.push(PopupType::Msg {
                        title: "Saved".into(),
                        text: "File saved successfully.".into(),
                    });
                } else {
                    app.popup.push(PopupType::Msg {
                        title: "Error".into(),
                        text: "Could not save file.".into(),
                    });
                }
            } else {
                let default_name = default_patch_name();
                app.popup.push(PopupType::Prompt {
                    purpose: PromptPurpose::SaveAs { quit_after: false },
                    cursor: default_name.chars().count(),
                    input: default_name,
                });
            }
        }

        KeyCode::Char('z') | KeyCode::Char('Z') if ctrl && shift => app.redo(),
        KeyCode::Char('z') | KeyCode::Char('Z') if ctrl && !shift => app.undo(),
        KeyCode::Char('u') | KeyCode::Char('U') if ctrl && !shift => app.undo(),

        KeyCode::Char('c') | KeyCode::Char('C') if ctrl => app.copy(),
        KeyCode::Char('x') | KeyCode::Char('X') if ctrl => app.cut(),
        KeyCode::Char('v') | KeyCode::Char('V') if ctrl => app.paste(),

        KeyCode::Char('u') | KeyCode::Char('U') if ctrl && shift => app.make_uppercase(),
        KeyCode::Char('l') | KeyCode::Char('L') if ctrl && shift => app.make_lowercase(),

        KeyCode::Char('a') | KeyCode::Char('A') if ctrl => app.select_all(),

        KeyCode::Tab | KeyCode::Insert => {
            app.mode = if app.mode == InputMode::Append {
                InputMode::Normal
            } else {
                InputMode::Append
            };
        }
        KeyCode::Char('i') | KeyCode::Char('I') if ctrl => {
            app.mode = if app.mode == InputMode::Append {
                InputMode::Normal
            } else {
                InputMode::Append
            };
        }

        KeyCode::Char('\'') => {
            app.mode = if app.mode == InputMode::Selection {
                InputMode::Normal
            } else {
                InputMode::Selection
            };
        }
        KeyCode::Char('`') | KeyCode::Char('~') => {
            app.mode = if app.mode == InputMode::Slide {
                InputMode::Normal
            } else {
                InputMode::Slide
            };
        }

        KeyCode::Char('k') | KeyCode::Char('K') if ctrl => {
            app.commander.active = !app.commander.active;
            if app.commander.active {
                app.commander.query.clear();
            }
        }

        KeyCode::Char('p') | KeyCode::Char('P') if ctrl => app.trigger(),
        KeyCode::Char('/') | KeyCode::Char('_') | KeyCode::Char('7') if ctrl => {
            app.toggle_comment()
        }

        KeyCode::Char('f') | KeyCode::Char('F') if ctrl => {
            if !app.paused {
                app.paused = true;
                app.midi.silence();
                app.midi.send_clock_stop();
            }
            app.operate();
            app.midi.flush();
            app.o2.f += 1;
        }

        KeyCode::Char('r') | KeyCode::Char('R') if ctrl => {
            app.o2.f = 0;
        }

        KeyCode::Char('d') | KeyCode::Char('D') if ctrl => {
            app.popup.push(PopupType::MainMenu { selected: 0 });
        }
        KeyCode::F(1) => {
            app.popup.push(PopupType::MainMenu { selected: 0 });
        }

        KeyCode::Char('g') | KeyCode::Char('G') if ctrl => {
            app.guide = !app.guide;
        }

        KeyCode::Up => {
            let leap = if ctrl { leap_y } else { 1 };
            if alt || app.mode == InputMode::Slide {
                app.drag(0, leap)
            } else if shift || app.mode == InputMode::Selection {
                app.scale_cursor(0, leap)
            } else {
                app.move_cursor(0, leap)
            }
        }
        KeyCode::Down => {
            let leap = if ctrl { -leap_y } else { -1 };
            if alt || app.mode == InputMode::Slide {
                app.drag(0, leap)
            } else if shift || app.mode == InputMode::Selection {
                app.scale_cursor(0, leap)
            } else {
                app.move_cursor(0, leap)
            }
        }
        KeyCode::Left => {
            let leap = if ctrl { -leap_x } else { -1 };
            if alt || app.mode == InputMode::Slide {
                app.drag(leap, 0)
            } else if shift || app.mode == InputMode::Selection {
                app.scale_cursor(leap, 0)
            } else {
                app.move_cursor(leap, 0)
            }
        }
        KeyCode::Right => {
            let leap = if ctrl { leap_x } else { 1 };
            if alt || app.mode == InputMode::Slide {
                app.drag(leap, 0)
            } else if shift || app.mode == InputMode::Selection {
                app.scale_cursor(leap, 0)
            } else {
                app.move_cursor(leap, 0)
            }
        }

        KeyCode::Char('(') => {
            let new_w = app.o2.w.saturating_sub(app.grid_w).max(1);
            let h = app.o2.h;
            app.resize(new_w, h);
        }
        KeyCode::Char(')') => {
            let new_w = app.o2.w + app.grid_w;
            let h = app.o2.h;
            app.resize(new_w, h);
        }
        KeyCode::Char('-') => {
            let new_h = app.o2.h.saturating_sub(app.grid_h).max(1);
            let w = app.o2.w;
            app.resize(w, new_h);
        }
        KeyCode::Char('+') => {
            let new_h = app.o2.h + app.grid_h;
            let w = app.o2.w;
            app.resize(w, new_h);
        }

        KeyCode::Char(']') => {
            app.grid_w = (app.grid_w + 1).clamp(4, 16);
        }
        KeyCode::Char('[') => {
            app.grid_w = (app.grid_w.saturating_sub(1)).clamp(4, 16);
        }
        KeyCode::Char('}') => {
            app.grid_h = (app.grid_h + 1).clamp(4, 16);
        }
        KeyCode::Char('{') => {
            app.grid_h = (app.grid_h.saturating_sub(1)).clamp(4, 16);
        }

        KeyCode::Char('>') => {
            if ctrl {
                app.mod_bpm_target(10);
            } else {
                app.mod_bpm(1);
            }
        }
        KeyCode::Char('<') => {
            if ctrl {
                app.mod_bpm_target(-10);
            } else {
                app.mod_bpm(-1);
            }
        }

        KeyCode::Char('.') => {
            if ctrl {
                app.midi.select_next_output();
                app.o2.f = 0;
            } else {
                app.mod_bpm(1);
            }
        }
        KeyCode::Char(',') => {
            if ctrl {
                app.midi.select_next_input();
                app.o2.f = 0;
            } else {
                app.mod_bpm(-1);
            }
        }

        KeyCode::Char(' ') if ctrl => {
            app.paused = !app.paused;
            if app.paused {
                app.midi.silence();
                app.midi.send_clock_stop();
            } else {
                app.midi.send_clock_start();
            }
        }
        KeyCode::Char(' ') => {
            if app.mode == InputMode::Append {
                app.move_cursor(1, 0);
            } else {
                app.paused = !app.paused;
                if app.paused {
                    app.midi.silence();
                }
            }
        }
        KeyCode::Enter => {
            app.trigger();
        }
        KeyCode::Backspace | KeyCode::Delete => {
            app.erase();
            if app.mode == InputMode::Append {
                app.move_cursor(-1, 0);
            }
        }
        KeyCode::Char('h') | KeyCode::Char('H') if ctrl => {
            app.erase();
            if app.mode == InputMode::Append {
                app.move_cursor(-1, 0);
            }
        }
        KeyCode::Char(c) if !ctrl && !alt && EditorState::is_allowed(c) => {
            app.write_cursor(c);

            if app.mode == InputMode::Append {
                match (app.rofl_buffer.as_str(), c.to_ascii_lowercase()) {
                    (_, 'r') => {
                        app.rofl_buffer.clear();
                        app.rofl_buffer.push('r');
                    }
                    ("r", 'o') => app.rofl_buffer.push('o'),
                    ("ro", 'f') => app.rofl_buffer.push('f'),
                    ("rof", 'l') => {
                        app.rofl_buffer.clear();

                        if app.bpm == 360 && !app.paused {
                            app.popup.push(PopupType::RoflCopter);
                        }
                    }
                    _ => app.rofl_buffer.clear(),
                }
            } else {
                app.rofl_buffer.clear();
                needs_draw = app.paused;
            }
        }
        _ => {}
    }

    needs_draw
}

pub fn autocomplete_path(input: &str) -> Option<String> {
    let path = std::path::Path::new(input);
    let (dir, file_prefix) = if input.is_empty() {
        (std::path::Path::new("."), "")
    } else if input.ends_with('/') || input.ends_with(std::path::MAIN_SEPARATOR) {
        (path, "")
    } else {
        let parent = path.parent().unwrap_or(std::path::Path::new(""));
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let dir = if parent.as_os_str().is_empty() {
            std::path::Path::new(".")
        } else {
            parent
        };
        (dir, file_name)
    };

    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut matches = Vec::new();
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string()
                && name.starts_with(file_prefix)
            {
                if file_prefix.is_empty() && name.starts_with('.') {
                    continue;
                }
                matches.push((name, entry.file_type().map(|t| t.is_dir()).unwrap_or(false)));
            }
        }
        matches.sort_by(|a, b| a.0.cmp(&b.0));
        if let Some((mut name, is_dir)) = matches.into_iter().next() {
            if is_dir {
                name.push(std::path::MAIN_SEPARATOR);
            }
            let remainder = &name[file_prefix.len()..];
            if !remainder.is_empty() {
                return Some(remainder.to_string());
            }
        }
    }
    None
}
