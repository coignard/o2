use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use eframe::egui::{self, StrokeKind};
use egui_ratatui::RataguiBackend;
use midir::{MidiOutput, MidiOutputConnection};
use o2_rs::core::app::{EditorState, InputMode};
use o2_rs::core::io::MidiMessage;
use o2_rs::editor::input::{handle_key, handle_paste};
use o2_rs::ui::render::draw;
use ratatui::Terminal;
use soft_ratatui::embedded_graphics_unicodefonts::{
    mono_8x13_atlas, mono_8x13_bold_atlas, mono_8x13_italic_atlas,
};
use soft_ratatui::{EmbeddedGraphics, SoftBackend};

const TERM_W: usize = 100;
const TERM_H: usize = 50;
const STATUS_ROWS: usize = 2;

struct SharedMidiOutput {
    out: Option<MidiOutputConnection>,
    device_name: String,
    output_index: i32,
}

impl SharedMidiOutput {
    fn new() -> Self {
        let mut output = Self {
            out: None,
            device_name: "No Midi Device".to_string(),
            output_index: -1,
        };
        output.select_next_output();
        output
    }

    fn select_next_output(&mut self) {
        if let Ok(midi) = MidiOutput::new("o2-multi-ctrl-n") {
            let ports = midi.ports();
            if ports.is_empty() {
                self.output_index = -1;
                self.device_name = "No Output Device".to_string();
                self.out = None;
                return;
            }
            self.output_index = (self.output_index + 1) % ports.len() as i32;
            let port = &ports[self.output_index as usize];
            self.device_name = midi
                .port_name(port)
                .unwrap_or_else(|_| "Unknown Device".to_string());
            self.out = midi.connect(port, "o2-multi-ctrl-n-output").ok();
        }
    }

    fn send(&mut self, msg: &[u8]) {
        if let Some(out) = self.out.as_mut() {
            let _ = out.send(msg);
        }
    }
}

struct EditorPane {
    id: usize,
    terminal: Terminal<RataguiBackend<EmbeddedGraphics>>,
    editor: EditorState,
    next_clock_tick: Instant,
    clock_counter: usize,
}

impl EditorPane {
    fn new(id: usize) -> Self {
        let font_regular = mono_8x13_atlas();
        let font_italic = mono_8x13_italic_atlas();
        let font_bold = mono_8x13_bold_atlas();
        let soft_backend = SoftBackend::<EmbeddedGraphics>::new(
            TERM_W as u16,
            TERM_H as u16,
            font_regular,
            Some(font_bold),
            Some(font_italic),
        );
        let backend = RataguiBackend::new(&format!("o2_multi_ctrl_n_{id}"), soft_backend);
        let mut editor = EditorState::new(TERM_W, TERM_H - STATUS_ROWS, id as u64 + 1, 100);
        editor.update_ports();
        editor.midi.out = None;
        editor.midi.output_index = -1;
        editor.midi.device_name = "Shared MIDI Output".to_string();

        Self {
            id,
            terminal: Terminal::new(backend).unwrap(),
            editor,
            next_clock_tick: Instant::now(),
            clock_counter: 0,
        }
    }

    fn tick(&mut self, shared_midi: &mut SharedMidiOutput) {
        let now = Instant::now();
        let tick_rate = Duration::from_millis(if self.editor.paused {
            100
        } else {
            60000 / self.editor.bpm.max(1) as u64 / 4
        });
        let clock_rate = tick_rate / 6;

        if now >= self.next_clock_tick {
            if self.clock_counter == 0 && !self.editor.paused {
                self.editor.operate();
                run_shared_midi(&mut self.editor, shared_midi);
                self.editor.engine.f += 1;
            }

            self.clock_counter = (self.clock_counter + 1) % 6;
            self.next_clock_tick += clock_rate;

            if now.duration_since(self.next_clock_tick) > clock_rate * 12 {
                self.next_clock_tick = now + clock_rate;
            }
        }
    }
}

fn run_shared_midi(editor: &mut EditorState, shared_midi: &mut SharedMidiOutput) {
    let mut to_send = Vec::new();

    editor.midi.stack.retain_mut(|note| {
        if !note.is_played {
            to_send.push(vec![0x90 + note.channel, note.note_id, note.velocity]);
            note.is_played = true;
        }
        if note.length < 1 {
            to_send.push(vec![0x80 + note.channel, note.note_id, 0]);
            false
        } else {
            note.length = note.length.saturating_sub(1);
            true
        }
    });

    for slot in editor.midi.mono_stack.iter_mut() {
        if let Some(note) = slot {
            if note.length < 1 {
                if note.is_played {
                    to_send.push(vec![0x80 + note.channel, note.note_id, 0]);
                }
                *slot = None;
                continue;
            }
            if !note.is_played {
                to_send.push(vec![0x90 + note.channel, note.note_id, note.velocity]);
                note.is_played = true;
            }
            note.length = note.length.saturating_sub(1);
        }
    }

    for msg in &editor.midi.cc_stack {
        match msg {
            MidiMessage::Cc(cc) => {
                let knob_val = editor.midi.cc_offset.saturating_add(cc.knob).min(127);
                to_send.push(vec![0xB0 + cc.channel, knob_val, cc.value]);
            }
            MidiMessage::Pb(pb) => {
                to_send.push(vec![0xE0 + pb.channel, pb.lsb, pb.msb]);
            }
        }
    }

    for msg in to_send {
        shared_midi.send(&msg);
    }

    editor
        .midi
        .osc
        .run(editor.midi.udp_socket.as_ref(), &editor.midi.ip);
    editor
        .midi
        .udp
        .run(editor.midi.udp_socket.as_ref(), &editor.midi.ip);
    editor.midi.cc_stack.clear();
}

struct P2TilesBehavior<'a> {
    events: &'a [egui::Event],
    ignore_pointer_input: bool,
    active_tile: &'a mut Option<egui_tiles::TileId>,
    shared_midi: &'a mut SharedMidiOutput,
}

impl egui_tiles::Behavior<EditorPane> for P2TilesBehavior<'_> {
    fn tab_title_for_pane(&mut self, pane: &EditorPane) -> egui::WidgetText {
        format!("Editor {}", pane.id + 1).into()
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        tile_id: egui_tiles::TileId,
        pane: &mut EditorPane,
    ) -> egui_tiles::UiResponse {
        pane.editor.midi.out = None;
        pane.editor.midi.output_index = self.shared_midi.output_index;
        pane.editor.midi.device_name = format!("Shared: {}", self.shared_midi.device_name);
        pane.tick(self.shared_midi);

        let size = pane.terminal.size().expect("terminal size");
        let term_w = size.width.max(1) as usize;
        let term_h = size.height.max(STATUS_ROWS as u16 + 1) as usize;
        pane.editor
            .update_scroll(term_w, term_h.saturating_sub(STATUS_ROWS));
        pane.terminal
            .draw(|frame| {
                draw(frame, &pane.editor);
            })
            .expect("epic fail");

        let response = ui.add(pane.terminal.backend_mut());
        let interaction = ui.interact(
            response.rect,
            ui.id().with(("o2_multi_ctrl_n_pane", pane.id)),
            egui::Sense::click_and_drag(),
        );
        let pointer_over = ui.input(|input| {
            input
                .pointer
                .latest_pos()
                .is_some_and(|pos| response.rect.contains(pos))
        });

        let pointer_event = self.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::PointerButton { .. }
                    | egui::Event::PointerMoved(_)
                    | egui::Event::MouseWheel { .. }
            )
        });
        if pointer_over || interaction.clicked() || interaction.dragged() || response.has_focus() {
            *self.active_tile = Some(tile_id);
        }
        if interaction.clicked() {
            interaction.request_focus();
        }

        let size = pane.terminal.size().expect("terminal size");
        let term_w = size.width.max(1) as usize;
        let term_h = size.height.max(STATUS_ROWS as u16 + 1) as usize;
        pane.editor.resize(
            term_w.max(pane.editor.engine.w),
            term_h.saturating_sub(STATUS_ROWS).max(pane.editor.engine.h),
        );

        let is_active = *self.active_tile == Some(tile_id);
        if is_active {
            handle_egui_input(
                &mut pane.editor,
                response.rect,
                term_w,
                term_h,
                self.ignore_pointer_input || (!pointer_over && pointer_event),
                self.events.to_vec(),
            );
        }

        if is_active {
            ui.painter().rect_stroke(
                response.rect.expand(1.0),
                egui::CornerRadius::same(3),
                egui::Stroke::new(2.0, egui::Color32::from_rgb(0, 150, 255)),
                StrokeKind::Outside,
            );
        }

        egui_tiles::UiResponse::None
    }
}

fn collect_example_files() -> Vec<PathBuf> {
    fn visit(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit(&path, out);
            } else if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("o2"))
            {
                out.push(path);
            }
        }
    }

    let mut files = Vec::new();
    visit(Path::new("examples"), &mut files);
    files.sort();
    files
}

fn load_example(app: &mut EditorState, path: &Path, viewport_w: usize, viewport_h: usize) {
    if let Ok(content) = std::fs::read_to_string(path) {
        app.load(&content, Some(path.to_path_buf()));
        app.resize(viewport_w.max(app.engine.w), viewport_h.max(app.engine.h));
        app.update_ports();
        app.history.saved_absolute_index = Some(app.history.offset + app.history.index);
        app.cx = 0;
        app.cy = 0;
        app.scroll_x = 0;
        app.scroll_y = 0;
    }
}

fn create_editor_tree() -> (egui_tiles::Tree<EditorPane>, Vec<egui_tiles::TileId>, usize) {
    let mut tiles = egui_tiles::Tiles::default();
    let first = tiles.insert_pane(EditorPane::new(0));
    let root = tiles.insert_grid_tile(vec![first]);
    (
        egui_tiles::Tree::new("o2_multi_ctrl_n_editors", root, tiles),
        vec![first],
        1,
    )
}

fn add_editor_tile(
    tree: &mut egui_tiles::Tree<EditorPane>,
    pane_ids: &mut Vec<egui_tiles::TileId>,
    next_editor_id: &mut usize,
) -> egui_tiles::TileId {
    let editor_id = *next_editor_id;
    *next_editor_id += 1;
    let pane_id = tree.tiles.insert_pane(EditorPane::new(editor_id));
    pane_ids.push(pane_id);
    tree.root = Some(tree.tiles.insert_grid_tile(pane_ids.clone()));
    pane_id
}

fn key_modifiers(modifiers: egui::Modifiers) -> KeyModifiers {
    let mut out = KeyModifiers::empty();
    if modifiers.alt {
        out |= KeyModifiers::ALT;
    }
    if modifiers.ctrl || modifiers.command {
        out |= KeyModifiers::CONTROL;
    }
    if modifiers.shift {
        out |= KeyModifiers::SHIFT;
    }
    if modifiers.mac_cmd {
        out |= KeyModifiers::META;
    }
    out
}

fn key_code(key: egui::Key) -> Option<KeyCode> {
    Some(match key {
        egui::Key::ArrowDown => KeyCode::Down,
        egui::Key::ArrowLeft => KeyCode::Left,
        egui::Key::ArrowRight => KeyCode::Right,
        egui::Key::ArrowUp => KeyCode::Up,
        egui::Key::Escape => KeyCode::Esc,
        egui::Key::Tab => KeyCode::Tab,
        egui::Key::Backspace => KeyCode::Backspace,
        egui::Key::Enter => KeyCode::Enter,
        egui::Key::Space => KeyCode::Char(' '),
        egui::Key::Insert => KeyCode::Insert,
        egui::Key::Delete => KeyCode::Delete,
        egui::Key::Home => KeyCode::Home,
        egui::Key::End => KeyCode::End,
        egui::Key::A => KeyCode::Char('a'),
        egui::Key::B => KeyCode::Char('b'),
        egui::Key::C => KeyCode::Char('c'),
        egui::Key::D => KeyCode::Char('d'),
        egui::Key::E => KeyCode::Char('e'),
        egui::Key::F => KeyCode::Char('f'),
        egui::Key::G => KeyCode::Char('g'),
        egui::Key::H => KeyCode::Char('h'),
        egui::Key::I => KeyCode::Char('i'),
        egui::Key::J => KeyCode::Char('j'),
        egui::Key::K => KeyCode::Char('k'),
        egui::Key::L => KeyCode::Char('l'),
        egui::Key::M => KeyCode::Char('m'),
        egui::Key::N => KeyCode::Char('n'),
        egui::Key::O => KeyCode::Char('o'),
        egui::Key::P => KeyCode::Char('p'),
        egui::Key::Q => KeyCode::Char('q'),
        egui::Key::R => KeyCode::Char('r'),
        egui::Key::S => KeyCode::Char('s'),
        egui::Key::T => KeyCode::Char('t'),
        egui::Key::U => KeyCode::Char('u'),
        egui::Key::V => KeyCode::Char('v'),
        egui::Key::W => KeyCode::Char('w'),
        egui::Key::X => KeyCode::Char('x'),
        egui::Key::Y => KeyCode::Char('y'),
        egui::Key::Z => KeyCode::Char('z'),
        egui::Key::Num0 => KeyCode::Char('0'),
        egui::Key::Num1 => KeyCode::Char('1'),
        egui::Key::Num2 => KeyCode::Char('2'),
        egui::Key::Num3 => KeyCode::Char('3'),
        egui::Key::Num4 => KeyCode::Char('4'),
        egui::Key::Num5 => KeyCode::Char('5'),
        egui::Key::Num6 => KeyCode::Char('6'),
        egui::Key::Num7 => KeyCode::Char('7'),
        egui::Key::Num8 => KeyCode::Char('8'),
        egui::Key::Num9 => KeyCode::Char('9'),
        egui::Key::Backtick => KeyCode::Char('`'),
        egui::Key::Minus => KeyCode::Char('-'),
        egui::Key::Equals => KeyCode::Char('='),
        egui::Key::OpenBracket => KeyCode::Char('['),
        egui::Key::CloseBracket => KeyCode::Char(']'),
        egui::Key::Backslash => KeyCode::Char('\\'),
        egui::Key::Semicolon => KeyCode::Char(';'),
        egui::Key::Colon => KeyCode::Char(':'),
        egui::Key::Comma => KeyCode::Char(','),
        egui::Key::Period => KeyCode::Char('.'),
        egui::Key::Slash => KeyCode::Char('/'),
        egui::Key::F1 => KeyCode::F(1),
        _ => return None,
    })
}

fn send_key(app: &mut EditorState, code: KeyCode, modifiers: KeyModifiers, repeat: bool) {
    handle_key(
        app,
        KeyEvent {
            code,
            modifiers,
            kind: if repeat {
                KeyEventKind::Repeat
            } else {
                KeyEventKind::Press
            },
            state: KeyEventState::empty(),
        },
    );
}

fn pointer_button(button: egui::PointerButton) -> Option<MouseButton> {
    Some(match button {
        egui::PointerButton::Primary => MouseButton::Left,
        egui::PointerButton::Secondary => MouseButton::Right,
        egui::PointerButton::Middle => MouseButton::Middle,
        _ => return None,
    })
}

fn pos_to_cell(
    pos: egui::Pos2,
    rect: egui::Rect,
    term_w: usize,
    term_h: usize,
) -> Option<(u16, u16)> {
    if !rect.contains(pos) {
        return None;
    }

    let x = ((pos.x - rect.left()) / rect.width() * term_w as f32).floor() as u16;
    let y = ((pos.y - rect.top()) / rect.height() * term_h as f32).floor() as u16;

    Some((x.min(term_w as u16 - 1), y.min(term_h as u16 - 1)))
}

fn handle_demo_mouse(app: &mut EditorState, event: MouseEvent, term_h: usize) {
    app.last_input_was_mouse = true;

    let col = event.column as usize;
    let row = event.row as usize;
    let viewport_h = term_h.saturating_sub(STATUS_ROWS);
    let grid_x = col + app.scroll_x;
    let grid_y = row + app.scroll_y;

    match event.kind {
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

fn handle_egui_input(
    app: &mut EditorState,
    rect: egui::Rect,
    term_w: usize,
    term_h: usize,
    ignore_pointer_input: bool,
    events: Vec<egui::Event>,
) {
    for event in events {
        match event {
            egui::Event::Key {
                key,
                pressed,
                repeat,
                modifiers,
                ..
            } if pressed => {
                if let Some(code) = key_code(key) {
                    let modifiers = key_modifiers(modifiers);
                    let is_plain_char = matches!(code, KeyCode::Char(_))
                        && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);

                    if !is_plain_char || code == KeyCode::Char(' ') {
                        send_key(app, code, modifiers, repeat);
                    }
                }
            }
            egui::Event::Text(text) => {
                for c in text.chars() {
                    if c == ' ' {
                        continue;
                    }
                    send_key(app, KeyCode::Char(c), KeyModifiers::empty(), false);
                }
            }
            egui::Event::Paste(text) => handle_paste(app, &text),
            egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers,
            } if !ignore_pointer_input => {
                if let (Some((column, row)), Some(button)) = (
                    pos_to_cell(pos, rect, term_w, term_h),
                    pointer_button(button),
                ) {
                    handle_demo_mouse(
                        app,
                        MouseEvent {
                            kind: if pressed {
                                MouseEventKind::Down(button)
                            } else {
                                MouseEventKind::Up(button)
                            },
                            column,
                            row,
                            modifiers: key_modifiers(modifiers),
                        },
                        term_h,
                    );
                }
            }
            egui::Event::PointerMoved(pos) if !ignore_pointer_input => {
                if let Some((column, row)) = pos_to_cell(pos, rect, term_w, term_h) {
                    handle_demo_mouse(
                        app,
                        MouseEvent {
                            kind: MouseEventKind::Drag(MouseButton::Left),
                            column,
                            row,
                            modifiers: KeyModifiers::empty(),
                        },
                        term_h,
                    );
                }
            }
            egui::Event::MouseWheel {
                delta, modifiers, ..
            } if !ignore_pointer_input => {
                let pos = rect.center();
                if let Some((column, row)) = pos_to_cell(pos, rect, term_w, term_h) {
                    let modifiers = key_modifiers(modifiers);
                    if delta.y > 0.0 {
                        handle_demo_mouse(
                            app,
                            MouseEvent {
                                kind: MouseEventKind::ScrollUp,
                                column,
                                row,
                                modifiers,
                            },
                            term_h,
                        );
                    } else if delta.y < 0.0 {
                        handle_demo_mouse(
                            app,
                            MouseEvent {
                                kind: MouseEventKind::ScrollDown,
                                column,
                                row,
                                modifiers,
                            },
                            term_h,
                        );
                    }
                    if delta.x > 0.0 {
                        handle_demo_mouse(
                            app,
                            MouseEvent {
                                kind: MouseEventKind::ScrollRight,
                                column,
                                row,
                                modifiers,
                            },
                            term_h,
                        );
                    } else if delta.x < 0.0 {
                        handle_demo_mouse(
                            app,
                            MouseEvent {
                                kind: MouseEventKind::ScrollLeft,
                                column,
                                row,
                                modifiers,
                            },
                            term_h,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 420.0]),
        ..Default::default()
    };

    let (mut editor_tree, mut pane_ids, mut next_editor_id) = create_editor_tree();
    let mut active_tile = pane_ids.first().copied();
    let example_files = collect_example_files();
    let mut example_index = example_files.len().saturating_sub(1);
    let mut shared_midi = SharedMidiOutput::new();

    eframe::run_ui_native("o2-multi-ctrl-n", options, move |ctx, _frame| {
        let events = ctx.input(|i| i.events.clone());
        let mut ignore_pointer_input = false;
        let mut add_editor = ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::N));

        egui::Area::new(egui::Id::new("o2_multi_ctrl_n_add"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::LEFT_TOP, egui::vec2(4.0, 2.0))
            .show(ctx, |ui| {
                ui.spacing_mut().button_padding = egui::vec2(4.0, 0.0);
                let response = ui
                    .add(egui::Button::new("+").small())
                    .on_hover_text("New editor (Ctrl+N)");
                ignore_pointer_input |= response.hovered() || response.clicked();
                add_editor |= response.clicked();
            });

        if add_editor {
            active_tile = Some(add_editor_tile(
                &mut editor_tree,
                &mut pane_ids,
                &mut next_editor_id,
            ));
        }

        egui::Area::new(egui::Id::new("example_switcher"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 2.0))
            .show(ctx, |ui| {
                ui.spacing_mut().button_padding = egui::vec2(2.0, 0.0);
                let next = ui
                    .add_enabled(!example_files.is_empty(), egui::Button::new(">").small())
                    .on_hover_text(
                        example_files
                            .get((example_index + 1) % example_files.len().max(1))
                            .and_then(|path| path.strip_prefix("examples").ok())
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "No examples found".to_string()),
                    );

                ignore_pointer_input |= next.hovered() || next.clicked();
                if next.clicked() && !example_files.is_empty() {
                    example_index = (example_index + 1) % example_files.len();
                    if let Some(tile_id) = active_tile
                        && let Some(egui_tiles::Tile::Pane(pane)) =
                            editor_tree.tiles.get_mut(tile_id)
                    {
                        let size = pane.terminal.size().expect("terminal size");
                        let term_w = size.width.max(1) as usize;
                        let term_h = size.height.max(STATUS_ROWS as u16 + 1) as usize;
                        load_example(
                            &mut pane.editor,
                            &example_files[example_index],
                            term_w,
                            term_h.saturating_sub(STATUS_ROWS),
                        );
                    }
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let mut behavior = P2TilesBehavior {
                events: &events,
                ignore_pointer_input,
                active_tile: &mut active_tile,
                shared_midi: &mut shared_midi,
            };
            editor_tree.ui(&mut behavior, ui);
        });

        ctx.request_repaint_after(Duration::from_millis(16));
    })
}
