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

//! Keyboard, mouse, and bracketed paste event handling.
//!
//! This module translates raw crossterm events into calls on [`EditorState`].  It is
//! the sole entry point for all user input; the main loop forwards every
//! [`Event`] it receives to one of the three public functions here.
//!
//! # Structure
//!
//! - [`handle_key`] -- dispatches key presses.  When a popup is open it is
//!   handled first; unconsumed keys fall through to the commander or the main
//!   editing layer.
//! - [`handle_mouse`] -- handles click, drag, and scroll events.  Popup-aware:
//!   clicks inside the topmost popup interact with that popup; clicks outside
//!   dismiss it.
//! - [`handle_paste`] -- routes bracketed paste text either to the commander
//!   query string or to the grid via [`EditorState::paste_text`].

use crate::core::app::EditorState;
use crate::core::app::{InputMode, PopupType, PromptPurpose};
use crate::editor::commander::{preview_command, run_command};
use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

fn main_menu_up(mut sel: usize) -> usize {
    let empty = [4, 8, 10, 12, 16];
    loop {
        sel = sel.saturating_sub(1);
        if !empty.contains(&sel) || sel == 0 {
            break;
        }
    }
    sel
}

fn main_menu_down(mut sel: usize) -> usize {
    let empty = [4, 8, 10, 12, 16];
    loop {
        sel = (sel + 1).min(17);
        if !empty.contains(&sel) || sel == 17 {
            break;
        }
    }
    sel
}

/// Converts a given `chrono::DateTime` to an Arvelie-Neralie date-time string.
///
/// The Arvelie calendar divides the year into 26 fortnights labelled `A`-`Z`,
/// plus a short overflow period labelled `+`. The Neralie time is expressed as
/// a six-digit decimal fraction of the day (0-999999).
///
/// # Examples
///
/// ```
/// use chrono::{TimeZone, Utc};
/// use o2_rs::editor::input::datetime_to_arvelie_neralie;
///
/// // 1970-01-01 00:00:00.000 UTC → year 70, fortnight A, day 01, neralie 000000
/// let dt = Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap();
/// assert_eq!(datetime_to_arvelie_neralie(&dt), "70A01-000000");
/// ```
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

/// Returns the current local wall-clock time formatted as an Arvelie-Neralie string.
///
/// Reads the system clock via [`chrono::Local`] and delegates to
/// [`datetime_to_arvelie_neralie`].
pub fn arvelie_neralie() -> String {
    datetime_to_arvelie_neralie(&Local::now())
}

/// Handles a mouse event, routing it to the active popup or to the grid.
///
/// When one or more popups are open, mouse events are delivered only to the
/// topmost popup.  A left-click inside the popup interacts with its items; a
/// left-click outside dismisses the popup.  Scroll events on the grid move
/// the cursor one cell in the corresponding direction.
pub fn handle_mouse(app: &mut EditorState, mouse_event: MouseEvent) {
    app.last_input_was_mouse = true;
    let col = mouse_event.column;
    let row = mouse_event.row;

    if !app.popup.is_empty() {
        let (term_cols, term_rows) =
            crossterm::terminal::size().unwrap_or((app.engine.w as u16, app.engine.h as u16));
        let term_area = Rect::new(0, 0, term_cols, term_rows);

        let mut top_rect = Rect::default();
        let mut prev_rect = None;
        for p in &app.popup {
            top_rect = crate::ui::render::get_popup_rect(term_area, p, prev_rect);
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
                    } else if let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind {
                        let empty = [4, 8, 10, 12, 16];
                        if rel_y <= 17 && !empty.contains(&rel_y) {
                            *selected = rel_y;
                            trigger_action = true;
                        }
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
        crossterm::terminal::size().unwrap_or((app.engine.w as u16, app.engine.h as u16));
    let viewport_h = term_rows.saturating_sub(2) as usize;

    let scroll_x = app.scroll_x;
    let scroll_y = app.scroll_y;
    let grid_x = col + scroll_x;
    let grid_y = row + scroll_y;

    match mouse_event.kind {
        MouseEventKind::Down(MouseButton::Left) if row < viewport_h && grid_y < app.engine.h => {
            app.mouse_from = Some((grid_x, grid_y));
            app.select(grid_x as isize, grid_y as isize, 0, 0);
        }
        MouseEventKind::Down(MouseButton::Right) => {
            app.cut();
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some((sx, sy)) = app.mouse_from {
                let col_clamped = grid_x.min(app.engine.w.saturating_sub(1));
                let row_clamped = grid_y.min(app.engine.h.saturating_sub(1));
                app.select(
                    col_clamped as isize,
                    row_clamped as isize,
                    sx as isize - col_clamped as isize,
                    sy as isize - row_clamped as isize,
                );
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if let Some((sx, sy)) = app.mouse_from {
                let col_clamped = grid_x.min(app.engine.w.saturating_sub(1));
                let row_clamped = grid_y.min(app.engine.h.saturating_sub(1));
                app.select(
                    col_clamped as isize,
                    row_clamped as isize,
                    sx as isize - col_clamped as isize,
                    sy as isize - row_clamped as isize,
                );
            }
            app.mouse_from = None;
        }
        MouseEventKind::ScrollUp => {
            app.move_cursor(0, 1);
        }
        MouseEventKind::ScrollDown => {
            app.move_cursor(0, -1);
        }
        MouseEventKind::ScrollLeft => {
            app.move_cursor(-1, 0);
        }
        MouseEventKind::ScrollRight => {
            app.move_cursor(1, 0);
        }
        _ => {}
    }
}

/// Handles a bracketed paste event from the terminal.
///
/// When the commander is active, newlines are stripped from the pasted text
/// and it is appended to the query string, triggering a live preview.
/// Otherwise the text is passed directly to [`EditorState::paste_text`].
pub fn handle_paste(app: &mut EditorState, text: &str) {
    app.last_input_was_mouse = false;
    if app.commander_active {
        let clean_text = text.replace(['\n', '\r'], "");
        app.query.push_str(&clean_text);
        preview_command(app);
    } else {
        app.paste_text(text);
    }
}

/// Handles a key press event, dispatching it to the active layer.
///
/// The dispatch order is:
///
/// 1. If a popup is open, the key is offered to the topmost popup first.
///    Popups that consume the key return early.  Some popups spawn child
///    popups or request that the parent popup be dismissed.
/// 2. If the commander is active, [`handle_commander_key`] is called.
/// 3. Otherwise [`handle_main_key`] processes the key in the main editing
///    layer.
///
/// Key repeat events (`KeyEventKind::Repeat`) are treated identically to
/// presses.  Release events are ignored.
pub fn handle_key(app: &mut EditorState, key: KeyEvent) {
    if key.kind != KeyEventKind::Press && key.kind != KeyEventKind::Repeat {
        return;
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
            return;
        }

        if app.is_dirty() {
            app.popup.push(PopupType::ConfirmQuit {
                selected: 0,
                has_file: app.current_file.is_some(),
            });
        } else {
            app.running = false;
        }
        return;
    }

    if let Some(mut popup) = app.popup.pop() {
        let mut close_popup = false;
        let mut pop_parent = false;
        let mut spawn_popups = Vec::new();

        match &mut popup {
            PopupType::Controls
            | PopupType::Operators
            | PopupType::About { .. }
            | PopupType::Msg { .. }
            | PopupType::RoflCopter => {
                if matches!(
                    key.code,
                    KeyCode::Esc
                        | KeyCode::Left
                        | KeyCode::Right
                        | KeyCode::Char(' ')
                        | KeyCode::Enter
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
                            let default_name = format!("patch-{}.o2", arvelie_neralie());
                            spawn_popups.push(PopupType::Prompt {
                                purpose: PromptPurpose::SaveAs { quit_after: false },
                                input: default_name,
                            });
                        }
                    }
                    3 => {
                        let default_name = if let Some(path) = &app.current_file {
                            path.to_string_lossy().into_owned()
                        } else {
                            format!("patch-{}.o2", arvelie_neralie())
                        };
                        spawn_popups.push(PopupType::Prompt {
                            purpose: PromptPurpose::SaveAs { quit_after: false },
                            input: default_name,
                        });
                    }
                    5 => spawn_popups.push(PopupType::Prompt {
                        purpose: PromptPurpose::SetBpm,
                        input: app.bpm.to_string(),
                    }),
                    6 => spawn_popups.push(PopupType::Prompt {
                        purpose: PromptPurpose::SetGridSize,
                        input: format!("{}x{}", app.engine.w, app.engine.h),
                    }),
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
                    app.set_midi_device(*selected);
                }
                KeyCode::Enter | KeyCode::Right => {}
                _ => {}
            },
            PopupType::ConfirmNew { selected } => match key.code {
                KeyCode::Esc | KeyCode::Left => close_popup = true,
                KeyCode::Up | KeyCode::Down => *selected = 1 - *selected,
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Right => {
                    if *selected == 1 {
                        app.engine.cells.fill('.');
                        app.history.clear();
                        app.history.record(&app.engine.cells);
                        app.history.saved_absolute_index =
                            Some(app.history.offset + app.history.index);
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
                                    let default_name = format!("patch-{}.o2", arvelie_neralie());
                                    spawn_popups.push(PopupType::Prompt {
                                        purpose: PromptPurpose::SaveAs { quit_after: true },
                                        input: default_name,
                                    });
                                }
                            }
                            (true, 1) | (false, 0) => {
                                let default_name = if let Some(path) = &app.current_file {
                                    path.to_string_lossy().into_owned()
                                } else {
                                    format!("patch-{}.o2", arvelie_neralie())
                                };
                                spawn_popups.push(PopupType::Prompt {
                                    purpose: PromptPurpose::SaveAs { quit_after: true },
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
                    let (cols, rows) = crossterm::terminal::size()
                        .unwrap_or((app.engine.w as u16, app.engine.h as u16));
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
            PopupType::Prompt { purpose, input } => match key.code {
                KeyCode::Esc => close_popup = true,
                KeyCode::Tab => {
                    if matches!(purpose, PromptPurpose::Open | PromptPurpose::SaveAs { .. })
                        && let Some(comp) = autocomplete_path(input)
                    {
                        input.push_str(&comp);
                    }
                }
                KeyCode::Left | KeyCode::Right => {}
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => input.push(c),
                KeyCode::Enter => match purpose {
                    PromptPurpose::Open => {
                        if let Ok(content) = std::fs::read_to_string(&*input) {
                            app.load(&content, Some(input.clone().into()));
                            let (cols, rows) = crossterm::terminal::size()
                                .unwrap_or((app.engine.w as u16, app.engine.h as u16));
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
                        let parts: Vec<&str> = input.split('x').collect();
                        if parts.len() == 2
                            && let (Ok(w), Ok(h)) = (parts[0].parse(), parts[1].parse())
                        {
                            app.resize(w, h);
                        }
                        close_popup = true;
                    }
                },
                _ => {}
            },
        };

        if !close_popup {
            app.popup.push(popup);
        } else if pop_parent {
            app.popup.pop();
        }

        app.popup.extend(spawn_popups);

        return;
    }

    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    if app.commander_active {
        handle_commander_key(app, key, ctrl, alt);
    } else {
        handle_main_key(app, key, ctrl, shift, alt);
    }
}

fn handle_commander_key(app: &mut EditorState, key: KeyEvent, ctrl: bool, alt: bool) {
    match key.code {
        KeyCode::Esc => {
            app.commander_active = false;
            app.query.clear();
            app.command_index = app.command_history.len();
            preview_command(app);
        }
        KeyCode::Char('k') | KeyCode::Char('K') if ctrl => {
            app.commander_active = false;
            app.query.clear();
            app.command_index = app.command_history.len();
            preview_command(app);
        }
        KeyCode::Backspace => {
            app.query.pop();
            preview_command(app);
        }
        KeyCode::Enter => {
            let query = app.query.clone();
            if !query.is_empty() {
                if app.command_history.last() != Some(&query) {
                    app.command_history.push(query.clone());
                }
                app.command_index = app.command_history.len();
            }
            run_command(app, &query, None);
            app.commander_active = false;
            app.query.clear();
        }
        KeyCode::Up if app.command_index > 0 => {
            app.command_index -= 1;
            app.query = app.command_history[app.command_index].clone();
            preview_command(app);
        }
        KeyCode::Down => {
            if app.command_index + 1 < app.command_history.len() {
                app.command_index += 1;
                app.query = app.command_history[app.command_index].clone();
                preview_command(app);
            } else if app.command_index < app.command_history.len() {
                app.command_index = app.command_history.len();
                app.query.clear();
                preview_command(app);
            }
        }
        KeyCode::Char(c) => {
            if !ctrl && !alt {
                app.query.push(c);
                preview_command(app);
            } else if ctrl
                && (c == 'v' || c == 'V')
                && let Ok(mut ctx) = arboard::Clipboard::new()
                && let Ok(text) = ctx.get_text()
            {
                handle_paste(app, &text);
            }
        }
        _ => {}
    }
}

fn handle_main_key(app: &mut EditorState, key: KeyEvent, ctrl: bool, shift: bool, alt: bool) {
    let leap_x = app.grid_w as isize;
    let leap_y = app.grid_h as isize;

    let is_char =
        matches!(key.code, KeyCode::Char(c) if !ctrl && !alt && EditorState::is_allowed(c));
    if !is_char {
        app.rofl_buffer.clear();
    }

    match key.code {
        KeyCode::Esc => {
            app.select(app.cx as isize, app.cy as isize, 0, 0);
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
                let default_name = format!("patch-{}.o2", arvelie_neralie());
                app.popup.push(PopupType::Prompt {
                    purpose: PromptPurpose::SaveAs { quit_after: false },
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
            app.commander_active = !app.commander_active;
            if app.commander_active {
                app.query.clear();
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
            app.midi.run();
            app.engine.f += 1;
        }

        KeyCode::Char('r') | KeyCode::Char('R') if ctrl => {
            app.engine.f = 0;
        }

        KeyCode::Char('d') | KeyCode::Char('D') if ctrl => {
            app.popup.push(PopupType::MainMenu { selected: 0 });
        }
        KeyCode::F(1) => {
            app.popup.push(PopupType::MainMenu { selected: 0 });
        }

        KeyCode::Char('g') | KeyCode::Char('G') if ctrl => {
            app.popup.push(PopupType::Operators);
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
            let new_w = app.engine.w.saturating_sub(app.grid_w).max(1);
            let h = app.engine.h;
            app.resize(new_w, h);
        }
        KeyCode::Char(')') => {
            let new_w = app.engine.w + app.grid_w;
            let h = app.engine.h;
            app.resize(new_w, h);
        }
        KeyCode::Char('_') | KeyCode::Char('-') => {
            let new_h = app.engine.h.saturating_sub(app.grid_h).max(1);
            let w = app.engine.w;
            app.resize(w, new_h);
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            let new_h = app.engine.h + app.grid_h;
            let w = app.engine.w;
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
                app.engine.f = 0;
            } else {
                app.mod_bpm(1);
            }
        }
        KeyCode::Char(',') => {
            if ctrl {
                app.midi.select_next_input();
                app.engine.f = 0;
            } else {
                app.mod_bpm(-1);
            }
        }

        KeyCode::Char(' ') => {
            if app.mode == InputMode::Append {
                app.move_cursor(1, 0);
            } else {
                app.paused = !app.paused;
                if app.paused {
                    app.midi.silence();
                    app.midi.send_clock_stop();
                } else {
                    app.midi.send_clock_start();
                }
            }
        }
        KeyCode::Enter => {
            app.trigger();
        }
        KeyCode::Backspace => {
            app.erase();
            if app.mode == InputMode::Append {
                app.move_cursor(-1, 0);
            }
        }
        KeyCode::Char('?') if !ctrl && !alt => {
            app.popup.push(PopupType::Controls);
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
            }
        }
        _ => {}
    }
}

/// Returns the shortest suffix needed to complete `input` to the first
/// matching filesystem entry, or `None` if there is no match or the input
/// is already complete.
///
/// The completion is prefix-based: the directory portion of `input` is
/// scanned and the first entry (sorted lexicographically) whose name starts
/// with the file-name portion of `input` is returned. Hidden entries (names
/// starting with `'.'`) are skipped unless the prefix itself starts with
/// `'.'`. A trailing path separator is appended automatically when the
/// matched entry is a directory.
///
/// Used by the [`PopupType::Prompt`] renderer and the Tab key handler to
/// provide interactive path completion in Open and Save As dialogues.
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
