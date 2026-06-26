//! JpoProducer v2 — M1 minimal piano roll with edit engine.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use jpo_edit::{EditEngine, EditResult};
use jpo_model::NoteId;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const GIT_HASH: &str = env!("JPO_GIT_HASH");

const PIANO_LOW: u8 = 48;
const PIANO_HIGH: u8 = 84;
const ROW_H: f32 = 14.0;
const BEAT_W: f32 = 48.0;

fn window_title() -> String {
    format!("JpoProducer v{VERSION} ({GIT_HASH})")
}

struct JpoApp {
    engine: EditEngine,
    current_beat: f64,
    note_len: f64,
    last_mouse_pitch: u8,
    piano_roll_focused: bool,
    toast: Option<(String, f32)>,
    box_select: Option<(f64, u8, f64, u8)>,
}

impl Default for JpoApp {
    fn default() -> Self {
        let mut engine = EditEngine::default();
        engine.set_active_track(4);
        Self {
            engine,
            current_beat: 0.0,
            note_len: 0.25,
            last_mouse_pitch: 60,
            piano_roll_focused: false,
            toast: None,
            box_select: None,
        }
    }
}

impl JpoApp {
    fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), 2.5));
    }

    fn tick_toast(&mut self, dt: f32) {
        if let Some((_, ref mut t)) = self.toast {
            *t -= dt;
            if *t <= 0.0 {
                self.toast = None;
            }
        }
    }

    fn snap_beat(&self, beat: f64) -> f64 {
        let grid = jpo_model::GridSettings {
            note_len: self.note_len,
            snap_enabled: self.engine.grid.snap_enabled,
        };
        jpo_model::snap_beat(beat, &grid)
    }

    fn handle_edit_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        if !self.piano_roll_focused {
            return false;
        }
        let ch = self.engine.active_track_ch();
        if ch == 1 {
            return false;
        }

        let mut acted = false;
        ctx.input(|i| {
            let ctrl = i.modifiers.ctrl || i.modifiers.command;

            if ctrl && i.key_pressed(egui::Key::C) {
                match self.engine.copy_selection() {
                    EditResult::Ok => {
                        self.show_toast(format!("Copied {} note(s)", self.engine.selection_count()));
                        acted = true;
                    }
                    EditResult::NoSelection => self.show_toast("Nothing to copy"),
                    _ => {}
                }
            }
            if ctrl && i.key_pressed(egui::Key::X) {
                match self.engine.cut_selection() {
                    EditResult::Ok => {
                        self.show_toast("Cut");
                        acted = true;
                    }
                    EditResult::NoSelection => self.show_toast("Nothing to cut"),
                    _ => {}
                }
            }
            if ctrl && i.key_pressed(egui::Key::V) {
                if self.engine.clipboard.is_some() {
                    match self.engine.paste_clipboard(self.current_beat, self.last_mouse_pitch) {
                        EditResult::Ok => {
                            self.show_toast(format!(
                                "Pasted at beat {:.2}",
                                self.current_beat
                            ));
                            acted = true;
                        }
                        _ => self.show_toast("Paste failed"),
                    }
                } else {
                    self.show_toast("Clipboard empty — Ctrl+C first");
                }
            }
            if ctrl && i.key_pressed(egui::Key::D) {
                match self.engine.duplicate_selection(self.current_beat, self.last_mouse_pitch) {
                    EditResult::Ok => {
                        self.show_toast("Duplicated");
                        acted = true;
                    }
                    EditResult::NoSelection => self.show_toast("Nothing to duplicate"),
                    _ => {}
                }
            }
            if ctrl && i.key_pressed(egui::Key::A) {
                self.engine.select_all_on_track();
                self.show_toast(format!("Selected {} note(s)", self.engine.selection_count()));
                acted = true;
            }
            if i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace) {
                match self.engine.delete_selection() {
                    EditResult::Ok => acted = true,
                    _ => {}
                }
            }
            if ctrl && i.key_pressed(egui::Key::Z) && !i.modifiers.shift {
                if self.engine.undo() {
                    self.show_toast("Undo");
                    acted = true;
                }
            }
            if ctrl && i.modifiers.shift && i.key_pressed(egui::Key::Z) {
                if self.engine.redo() {
                    self.show_toast("Redo");
                    acted = true;
                }
            }

            let step = if i.modifiers.shift {
                self.note_len
            } else {
                0.25
            };
            if i.key_pressed(egui::Key::ArrowLeft) {
                self.engine.nudge_selection(-step, 0);
                acted = true;
            }
            if i.key_pressed(egui::Key::ArrowRight) {
                self.engine.nudge_selection(step, 0);
                acted = true;
            }
            if i.key_pressed(egui::Key::ArrowUp) {
                self.engine.nudge_selection(0.0, 1);
                acted = true;
            }
            if i.key_pressed(egui::Key::ArrowDown) {
                self.engine.nudge_selection(0.0, -1);
                acted = true;
            }
        });
        acted
    }

    fn note_at(&self, ch: u8, beat: f64, pitch: u8) -> Option<NoteId> {
        let track = self.engine.project.track_by_ch(ch)?;
        let tol = self.note_len * 0.4;
        track
            .notes
            .iter()
            .filter(|n| n.pitch == pitch && (n.start - beat).abs() < tol)
            .min_by(|a, b| {
                (a.start - beat)
                    .abs()
                    .partial_cmp(&(b.start - beat).abs())
                    .unwrap()
            })
            .map(|n| n.id)
    }

    fn draw_piano_roll(&mut self, ui: &mut egui::Ui) {
        let ch = self.engine.active_track_ch();
        let beats = self.engine.project.beats();
        let roll_w = beats as f32 * BEAT_W;
        let roll_h = (PIANO_HIGH - PIANO_LOW + 1) as f32 * ROW_H;

        let (rect, resp) = ui.allocate_exact_size(Vec2::new(roll_w + 40.0, roll_h + 24.0), Sense::click_and_drag());
        if resp.clicked() || resp.dragged() {
            self.piano_roll_focused = true;
            ui.ctx().memory_mut(|m| m.request_focus(resp.id));
        }
        if resp.lost_focus() && !resp.has_focus() {
            // keep focus flag while interacting inside
        }

        let origin = rect.min + Vec2::new(40.0, 12.0);
        let painter = ui.painter_at(rect);

        painter.rect_filled(rect, 4.0, Color32::from_rgb(22, 24, 28));

        for b in 0..=beats as i32 {
            let x = origin.x + b as f32 * BEAT_W;
            let major = b % 4 == 0;
            painter.line_segment(
                [Pos2::new(x, origin.y), Pos2::new(x, origin.y + roll_h)],
                Stroke::new(1.0, if major { Color32::from_gray(70) } else { Color32::from_gray(40) }),
            );
        }

        for p in (PIANO_LOW..=PIANO_HIGH).rev() {
            let row = (PIANO_HIGH - p) as f32;
            let y = origin.y + row * ROW_H;
            let black = [1, 3, 6, 8, 10].contains(&(p % 12));
            if black {
                painter.rect_filled(
                    Rect::from_min_size(Pos2::new(origin.x, y), Vec2::new(roll_w, ROW_H)),
                    0.0,
                    Color32::from_rgb(30, 32, 38),
                );
            }
        }

        if let Some(track) = self.engine.project.track_by_ch(ch) {
            for n in &track.notes {
                if n.pitch < PIANO_LOW || n.pitch > PIANO_HIGH {
                    continue;
                }
                let row = (PIANO_HIGH - n.pitch) as f32;
                let x = origin.x + n.start as f32 * BEAT_W;
                let w = (n.dur as f32 * BEAT_W).max(4.0);
                let y = origin.y + row * ROW_H + 1.0;
                let selected = self.engine.selection.notes.contains(&n.id);
                let color = if selected {
                    Color32::from_rgb(255, 200, 60)
                } else {
                    Color32::from_rgb(230, 140, 50)
                };
                painter.rect_filled(
                    Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, ROW_H - 2.0)),
                    2.0,
                    color,
                );
            }
        }

        let play_x = origin.x + self.current_beat as f32 * BEAT_W;
        painter.line_segment(
            [Pos2::new(play_x, origin.y), Pos2::new(play_x, origin.y + roll_h)],
            Stroke::new(2.0, Color32::from_rgb(255, 80, 80)),
        );

        if let Some((b0, p0, b1, p1)) = self.box_select {
            let x0 = origin.x + b0 as f32 * BEAT_W;
            let x1 = origin.x + b1 as f32 * BEAT_W;
            let y0 = origin.y + (PIANO_HIGH - p1) as f32 * ROW_H;
            let y1 = origin.y + (PIANO_HIGH - p0) as f32 * ROW_H + ROW_H;
            painter.rect_stroke(
                Rect::from_two_pos(Pos2::new(x0.min(x1), y0.min(y1)), Pos2::new(x0.max(x1), y0.max(y1))),
                0.0,
                Stroke::new(1.0, Color32::from_rgb(120, 180, 255)),
            );
        }

        if let Some(pos) = resp.interact_pointer_pos() {
            if pos.x >= origin.x && pos.y >= origin.y {
                let beat = ((pos.x - origin.x) / BEAT_W) as f64;
                let row = ((pos.y - origin.y) / ROW_H).floor() as i32;
                let pitch = (PIANO_HIGH as i32 - row).clamp(0, 127) as u8;
                self.last_mouse_pitch = pitch;

                let shift = ui.ctx().input(|i| i.modifiers.shift);

                if resp.clicked() {
                    let snapped = self.snap_beat(beat);
                    if let Some(id) = self.note_at(ch, snapped, pitch) {
                        self.engine.select_note(id, shift);
                    } else if !shift {
                        self.engine.place_note(snapped, pitch, self.note_len, 100);
                        self.show_toast(format!("Placed {pitch} @ {snapped:.2}"));
                    }
                }

                if resp.drag_started() && shift {
                    let b = self.snap_beat(beat);
                    self.box_select = Some((b, pitch, b, pitch));
                }
                if resp.dragged() && shift {
                    let snapped = self.snap_beat(beat);
                    if let Some(ref mut bs) = self.box_select {
                        bs.2 = snapped;
                        bs.3 = pitch;
                    }
                }
                if resp.drag_stopped() && shift {
                    if let Some((b0, p0, b1, p1)) = self.box_select.take() {
                        let (beat_min, beat_max) = if b0 <= b1 { (b0, b1) } else { (b1, b0) };
                        let (pitch_min, pitch_max) = if p0 <= p1 { (p0, p1) } else { (p1, p0) };
                        self.engine.select_in_rect(beat_min, beat_max, pitch_min, pitch_max, true);
                    }
                }
            }
        }

        if resp.clicked() && resp.interact_pointer_pos().map(|p| p.x < origin.x).unwrap_or(false) {
            let row = ((resp.interact_pointer_pos().unwrap().y - origin.y) / ROW_H).floor() as i32;
            let pitch = (PIANO_HIGH as i32 - row).clamp(0, 127) as u8;
            self.last_mouse_pitch = pitch;
        }
    }
}

impl eframe::App for JpoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick_toast(ctx.input(|i| i.unstable_dt));
        self.engine.grid.note_len = self.note_len;
        self.handle_edit_shortcuts(ctx);

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("JpoProducer v2 (M1)");
                ui.separator();
                for (ch, label) in [
                    (4, "Ch4 Melody"),
                    (2, "Ch2 Piano"),
                    (3, "Ch3 Bass"),
                    (10, "Ch10 Drum"),
                ] {
                    let sel = self.engine.active_track_ch() == ch;
                    if ui.selectable_label(sel, label).clicked() {
                        self.engine.set_active_track(ch);
                    }
                }
                ui.separator();
                for (len, name) in [
                    (0.0625, "1/16"),
                    (0.125, "1/8"),
                    (0.25, "1/4"),
                    (0.5, "1/2"),
                    (1.0, "1"),
                ] {
                    if ui.selectable_label((self.note_len - len).abs() < 1e-9, name).clicked() {
                        self.note_len = len;
                    }
                }
                ui.separator();
                ui.label(format!("Playhead: {:.2}", self.current_beat));
                if ui.button("◀").clicked() {
                    self.current_beat = (self.current_beat - self.note_len).max(0.0);
                }
                if ui.button("▶").clicked() {
                    self.current_beat = (self.current_beat + self.note_len).min(self.engine.project.beats());
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(
                egui::RichText::new("M1: click place • Shift+click multi • Shift+drag box • Ctrl+C/V/X/D/A • arrows nudge")
                    .small()
                    .weak(),
            );
            ui.label(format!(
                "Track Ch{} — {} notes, {} selected",
                self.engine.active_track_ch(),
                self.engine.note_count_on_track(self.engine.active_track_ch()),
                self.engine.selection_count(),
            ));
            self.draw_piano_roll(ui);
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some((ref msg, _)) = self.toast {
                    ui.label(egui::RichText::new(msg).color(Color32::from_rgb(120, 220, 140)));
                } else {
                    ui.label(egui::RichText::new("Ready").weak());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{}/{} bars", self.engine.project.loop_bars, 16));
                });
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 700.0])
            .with_title(window_title()),
        ..Default::default()
    };
    eframe::run_native(
        &window_title(),
        options,
        Box::new(|_cc| Ok(Box::new(JpoApp::default()))),
    )
}