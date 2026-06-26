//! JpoProducer v2 edit engine — commands, NoteId selection, clipboard, undo.

use jpo_model::{snap_beat, snap_dur, GridSettings, Note, NoteId, Project};
use std::collections::HashSet;

#[derive(Clone, Debug, Default)]
pub struct Selection {
    pub track_ch: u8,
    pub notes: HashSet<NoteId>,
}

impl Selection {
    pub fn clear(&mut self) {
        self.notes.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    pub fn toggle(&mut self, id: NoteId) {
        if self.notes.contains(&id) {
            self.notes.remove(&id);
        } else {
            self.notes.insert(id);
        }
    }

    pub fn set_single(&mut self, id: NoteId) {
        self.notes.clear();
        self.notes.insert(id);
    }
}

#[derive(Clone, Debug)]
pub struct ClipboardNote {
    pub rel_start: f64,
    pub pitch: u8,
    pub dur: f64,
    pub vel: u8,
}

#[derive(Clone, Debug)]
pub struct Clipboard {
    pub source_ch: u8,
    pub anchor_pitch: u8,
    pub notes: Vec<ClipboardNote>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditResult {
    Ok,
    NoSelection,
    ClipboardEmpty,
    TrackNotFound,
    NothingToDo,
}

#[derive(Clone)]
struct HistoryEntry {
    project: Project,
    selection: Selection,
}

pub struct EditEngine {
    pub project: Project,
    pub selection: Selection,
    pub clipboard: Option<Clipboard>,
    pub grid: GridSettings,
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
    gesture_saved: bool,
}

impl Default for EditEngine {
    fn default() -> Self {
        Self::new(Project::default())
    }
}

impl EditEngine {
    pub fn new(project: Project) -> Self {
        Self {
            selection: Selection {
                track_ch: 4,
                ..Default::default()
            },
            project,
            clipboard: None,
            grid: GridSettings::default(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            gesture_saved: false,
        }
    }

    pub fn active_track_ch(&self) -> u8 {
        self.selection.track_ch
    }

    pub fn set_active_track(&mut self, ch: u8) {
        self.selection.track_ch = ch;
        self.selection.clear();
    }

    fn push_undo(&mut self) {
        self.undo_stack.push(HistoryEntry {
            project: self.project.clone(),
            selection: self.selection.clone(),
        });
        self.redo_stack.clear();
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
    }

    pub fn begin_gesture(&mut self) {
        if !self.gesture_saved {
            self.push_undo();
            self.gesture_saved = true;
        }
    }

    pub fn end_gesture(&mut self) {
        self.gesture_saved = false;
    }

    pub fn undo(&mut self) -> bool {
        let Some(entry) = self.undo_stack.pop() else {
            return false;
        };
        self.redo_stack.push(HistoryEntry {
            project: self.project.clone(),
            selection: self.selection.clone(),
        });
        self.project = entry.project;
        self.selection = entry.selection;
        self.gesture_saved = false;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(entry) = self.redo_stack.pop() else {
            return false;
        };
        self.undo_stack.push(HistoryEntry {
            project: self.project.clone(),
            selection: self.selection.clone(),
        });
        self.project = entry.project;
        self.selection = entry.selection;
        self.gesture_saved = false;
        true
    }

    fn selected_ids_on_active_track(&self) -> Vec<NoteId> {
        self.selection.notes.iter().copied().collect()
    }

    pub fn select_note(&mut self, id: NoteId, shift: bool) {
        if shift {
            self.selection.toggle(id);
        } else {
            self.selection.set_single(id);
        }
    }

    pub fn select_all_on_track(&mut self) {
        let ch = self.selection.track_ch;
        self.selection.clear();
        if let Some(track) = self.project.track_by_ch(ch) {
            for n in &track.notes {
                self.selection.notes.insert(n.id);
            }
        }
    }

    pub fn select_in_rect(
        &mut self,
        beat_min: f64,
        beat_max: f64,
        pitch_min: u8,
        pitch_max: u8,
        additive: bool,
    ) {
        if !additive {
            self.selection.clear();
        }
        let ch = self.selection.track_ch;
        if let Some(track) = self.project.track_by_ch(ch) {
            for n in &track.notes {
                if n.start >= beat_min
                    && n.start <= beat_max
                    && n.pitch >= pitch_min
                    && n.pitch <= pitch_max
                {
                    self.selection.notes.insert(n.id);
                }
            }
        }
    }

    pub fn place_note(&mut self, beat: f64, pitch: u8, dur: f64, vel: u8) -> NoteId {
        self.begin_gesture();
        let ch = self.selection.track_ch;
        let start = snap_beat(beat, &self.grid);
        let dur = snap_dur(dur, &self.grid);
        let id = self.project.alloc_note_id();
        let note = Note {
            id,
            start,
            pitch,
            dur,
            vel,
        };
        if let Some(track) = self.project.track_by_ch_mut(ch) {
            track.notes.push(note);
        }
        self.project.sort_track_notes(ch);
        self.selection.set_single(id);
        self.end_gesture();
        id
    }

    pub fn delete_selection(&mut self) -> EditResult {
        let ids = self.selected_ids_on_active_track();
        if ids.is_empty() {
            return EditResult::NoSelection;
        }
        self.begin_gesture();
        let ch = self.selection.track_ch;
        if let Some(track) = self.project.track_by_ch_mut(ch) {
            track.notes.retain(|n| !ids.contains(&n.id));
        }
        self.selection.clear();
        self.end_gesture();
        EditResult::Ok
    }

    pub fn copy_selection(&mut self) -> EditResult {
        let ch = self.selection.track_ch;
        let ids = self.selected_ids_on_active_track();
        if ids.is_empty() {
            return EditResult::NoSelection;
        }
        let Some(track) = self.project.track_by_ch(ch) else {
            return EditResult::TrackNotFound;
        };

        let mut min_start = f64::MAX;
        let mut anchor_pitch = 60u8;
        let mut picked = Vec::new();
        for n in &track.notes {
            if ids.contains(&n.id) {
                min_start = min_start.min(n.start);
                anchor_pitch = n.pitch;
                picked.push(n.clone());
            }
        }
        if picked.is_empty() {
            return EditResult::NoSelection;
        }

        let notes = picked
            .iter()
            .map(|n| ClipboardNote {
                rel_start: n.start - min_start,
                pitch: n.pitch,
                dur: n.dur,
                vel: n.vel,
            })
            .collect();

        self.clipboard = Some(Clipboard {
            source_ch: ch,
            anchor_pitch,
            notes,
        });
        EditResult::Ok
    }

    pub fn cut_selection(&mut self) -> EditResult {
        match self.copy_selection() {
            EditResult::Ok => self.delete_selection(),
            other => other,
        }
    }

    pub fn paste_clipboard(&mut self, paste_at: f64, anchor_pitch: u8) -> EditResult {
        let Some(cb) = self.clipboard.clone() else {
            return EditResult::ClipboardEmpty;
        };
        let ch = self.selection.track_ch;
        self.begin_gesture();
        let paste_at = snap_beat(paste_at.max(0.0), &self.grid);
        let pitch_shift = anchor_pitch as i32 - cb.anchor_pitch as i32;

        let mut new_ids = Vec::new();
        for cn in &cb.notes {
            let id = self.project.alloc_note_id();
            let note = Note {
                id,
                start: snap_beat(paste_at + cn.rel_start, &self.grid),
                pitch: (cn.pitch as i32 + pitch_shift).clamp(0, 127) as u8,
                dur: cn.dur,
                vel: cn.vel,
            };
            if let Some(track) = self.project.track_by_ch_mut(ch) {
                track.notes.push(note);
            }
            new_ids.push(id);
        }
        self.project.sort_track_notes(ch);
        self.selection.clear();
        for id in new_ids {
            self.selection.notes.insert(id);
        }
        self.end_gesture();
        EditResult::Ok
    }

    pub fn duplicate_selection(&mut self, paste_at: f64, anchor_pitch: u8) -> EditResult {
        if self.copy_selection() != EditResult::Ok {
            return EditResult::NoSelection;
        }
        self.paste_clipboard(paste_at, anchor_pitch)
    }

    pub fn nudge_selection(&mut self, beat_delta: f64, pitch_delta: i32) -> EditResult {
        let ids = self.selected_ids_on_active_track();
        if ids.is_empty() {
            return EditResult::NoSelection;
        }
        self.begin_gesture();
        let ch = self.selection.track_ch;
        if let Some(track) = self.project.track_by_ch_mut(ch) {
            for n in &mut track.notes {
                if ids.contains(&n.id) {
                    n.start = snap_beat((n.start + beat_delta).max(0.0), &self.grid);
                    n.pitch = (n.pitch as i32 + pitch_delta).clamp(0, 127) as u8;
                }
            }
        }
        self.project.sort_track_notes(ch);
        self.end_gesture();
        EditResult::Ok
    }

    pub fn note_count_on_track(&self, ch: u8) -> usize {
        self.project.track_by_ch(ch).map(|t| t.notes.len()).unwrap_or(0)
    }

    pub fn selection_count(&self) -> usize {
        self.selection.notes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine_with_notes() -> EditEngine {
        let mut e = EditEngine::default();
        e.place_note(0.0, 60, 0.25, 100);
        e.place_note(0.25, 64, 0.25, 100);
        e.place_note(0.5, 67, 0.25, 100);
        e
    }

    #[test]
    fn place_note_assigns_unique_ids() {
        let mut e = EditEngine::default();
        let a = e.place_note(0.0, 60, 0.25, 100);
        let b = e.place_note(0.25, 62, 0.25, 100);
        assert_ne!(a, b);
        assert_eq!(e.note_count_on_track(4), 2);
    }

    #[test]
    fn shift_toggle_multi_select() {
        let mut e = engine_with_notes();
        let track = e.project.track_by_ch(4).unwrap();
        let ids: Vec<NoteId> = track.notes.iter().map(|n| n.id).collect();
        e.selection.clear();
        e.select_note(ids[0], false);
        assert_eq!(e.selection_count(), 1);
        e.select_note(ids[1], true);
        assert_eq!(e.selection_count(), 2);
        e.select_note(ids[0], true);
        assert_eq!(e.selection_count(), 1);
    }

    #[test]
    fn copy_fails_without_selection() {
        let mut e = EditEngine::default();
        assert_eq!(e.copy_selection(), EditResult::NoSelection);
    }

    #[test]
    fn copy_preserves_relative_timing() {
        let mut e = engine_with_notes();
        e.select_all_on_track();
        assert_eq!(e.copy_selection(), EditResult::Ok);
        let cb = e.clipboard.as_ref().unwrap();
        assert_eq!(cb.notes.len(), 3);
        assert!((cb.notes[0].rel_start - 0.0).abs() < 1e-9);
        assert!((cb.notes[1].rel_start - 0.25).abs() < 1e-9);
        assert!((cb.notes[2].rel_start - 0.5).abs() < 1e-9);
    }

    #[test]
    fn paste_creates_new_ids() {
        let mut e = engine_with_notes();
        e.select_all_on_track();
        e.copy_selection();
        let before_ids: HashSet<NoteId> = e
            .project
            .track_by_ch(4)
            .unwrap()
            .notes
            .iter()
            .map(|n| n.id)
            .collect();
        e.paste_clipboard(2.0, 60);
        let after: Vec<NoteId> = e
            .project
            .track_by_ch(4)
            .unwrap()
            .notes
            .iter()
            .map(|n| n.id)
            .collect();
        assert_eq!(after.len(), 6);
        for id in &after[3..] {
            assert!(!before_ids.contains(id));
        }
    }

    #[test]
    fn paste_twice_does_not_duplicate_originals() {
        let mut e = engine_with_notes();
        e.select_all_on_track();
        e.copy_selection();
        let original_count = e.note_count_on_track(4);
        e.paste_clipboard(2.0, 60);
        e.paste_clipboard(4.0, 60);
        assert_eq!(e.note_count_on_track(4), original_count + 6);
    }

    #[test]
    fn cut_removes_original() {
        let mut e = engine_with_notes();
        e.select_all_on_track();
        assert_eq!(e.cut_selection(), EditResult::Ok);
        assert_eq!(e.note_count_on_track(4), 0);
        assert!(e.clipboard.is_some());
    }

    #[test]
    fn duplicate_is_copy_then_paste() {
        let mut e = engine_with_notes();
        e.select_all_on_track();
        let before = e.note_count_on_track(4);
        e.duplicate_selection(2.0, 60);
        assert_eq!(e.note_count_on_track(4), before * 2);
    }

    #[test]
    fn delete_selection_removes_notes() {
        let mut e = engine_with_notes();
        let id = e.project.track_by_ch(4).unwrap().notes[0].id;
        e.select_note(id, false);
        assert_eq!(e.delete_selection(), EditResult::Ok);
        assert_eq!(e.note_count_on_track(4), 2);
        assert!(e.selection.is_empty());
    }

    #[test]
    fn undo_redo_place() {
        let mut e = EditEngine::default();
        e.place_note(0.0, 60, 0.25, 100);
        assert_eq!(e.note_count_on_track(4), 1);
        assert!(e.undo());
        assert_eq!(e.note_count_on_track(4), 0);
        assert!(e.redo());
        assert_eq!(e.note_count_on_track(4), 1);
    }

    #[test]
    fn undo_redo_paste() {
        let mut e = engine_with_notes();
        e.select_all_on_track();
        e.copy_selection();
        e.paste_clipboard(2.0, 60);
        assert_eq!(e.note_count_on_track(4), 6);
        assert!(e.undo());
        assert_eq!(e.note_count_on_track(4), 3);
    }

    #[test]
    fn nudge_moves_beat_and_pitch() {
        let mut e = engine_with_notes();
        let id = e.project.track_by_ch(4).unwrap().notes[0].id;
        e.select_note(id, false);
        e.nudge_selection(0.25, 2);
        let n = e.project.note_by_id(4, id).unwrap();
        assert!((n.start - 0.25).abs() < 1e-9);
        assert_eq!(n.pitch, 62);
    }

    #[test]
    fn select_all_on_track() {
        let mut e = engine_with_notes();
        e.select_all_on_track();
        assert_eq!(e.selection_count(), 3);
    }

    #[test]
    fn selection_survives_sort_after_nudge() {
        let mut e = engine_with_notes();
        let id = e.project.track_by_ch(4).unwrap().notes[2].id;
        e.select_note(id, false);
        e.nudge_selection(-0.5, 0);
        assert!(e.selection.notes.contains(&id));
        assert!(e.project.note_by_id(4, id).is_some());
    }

    #[test]
    fn paste_empty_clipboard_fails() {
        let mut e = EditEngine::default();
        assert_eq!(e.paste_clipboard(0.0, 60), EditResult::ClipboardEmpty);
    }
}