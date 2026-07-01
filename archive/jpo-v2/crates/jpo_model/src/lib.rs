//! JpoProducer v2 core data model — stable IDs, track roles, loop sketch.

use serde::{Deserialize, Serialize};

/// Stable note identity (never reuse after delete).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NoteId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: NoteId,
    pub start: f64,
    pub pitch: u8,
    pub dur: f64,
    pub vel: u8,
}

impl Note {
    pub fn end(&self) -> f64 {
        self.start + self.dur
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackRole {
    Chord,
    GenPiano,
    GenBass,
    Melody,
    GenDrum,
    Generic,
}

impl TrackRole {
    pub fn from_ch(ch: u8) -> Self {
        match ch {
            1 => Self::Chord,
            2 => Self::GenPiano,
            3 => Self::GenBass,
            4 => Self::Melody,
            10 => Self::GenDrum,
            _ => Self::Generic,
        }
    }

    pub fn default_ch(role: Self) -> u8 {
        match role {
            Self::Chord => 1,
            Self::GenPiano => 2,
            Self::GenBass => 3,
            Self::Melody => 4,
            Self::GenDrum => 10,
            Self::Generic => 5,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    pub ch: u8,
    pub role: TrackRole,
    pub name: String,
    pub notes: Vec<Note>,
    pub patch: u8,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub solo: bool,
    #[serde(default = "default_track_vol")]
    pub track_vol: f32,
}

fn default_track_vol() -> f32 {
    1.0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChordBlock {
    pub start: f64,
    pub dur: f64,
    pub degree: u8,
    pub quality: String,
    pub octave: u8,
    #[serde(default)]
    pub syncopation_fill: bool,
}

impl ChordBlock {
    pub fn end(&self) -> f64 {
        self.start + self.dur
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub bpm: f64,
    pub key_root: u8,
    pub is_minor: bool,
    pub loop_bars: u8,
    pub tracks: Vec<Track>,
    pub chord_blocks: Vec<ChordBlock>,
    #[serde(skip)]
    next_note_id: u64,
}

impl Default for Project {
    fn default() -> Self {
        let mut tracks = Vec::new();
        for ch in 1..=16 {
            let role = TrackRole::from_ch(ch);
            let patch = match ch {
                3 => 33,
                10 => 0,
                _ => 0,
            };
            let name = match role {
                TrackRole::Chord => "コード進行".to_string(),
                TrackRole::GenPiano => "Gen Piano".to_string(),
                TrackRole::GenBass => "Gen Bass".to_string(),
                TrackRole::Melody => "Melody".to_string(),
                TrackRole::GenDrum => "Drum".to_string(),
                TrackRole::Generic => format!("Ch{ch}"),
            };
            tracks.push(Track {
                ch,
                role,
                name,
                notes: vec![],
                patch,
                muted: false,
                solo: false,
                track_vol: 1.0,
            });
        }
        Self {
            bpm: 128.0,
            key_root: 0,
            is_minor: false,
            loop_bars: 4,
            tracks,
            chord_blocks: vec![],
            next_note_id: 1,
        }
    }
}

impl Project {
    pub fn beats(&self) -> f64 {
        self.loop_bars as f64 * 4.0
    }

    pub fn alloc_note_id(&mut self) -> NoteId {
        let id = NoteId(self.next_note_id);
        self.next_note_id += 1;
        id
    }

    pub fn track_index_by_ch(&self, ch: u8) -> Option<usize> {
        self.tracks.iter().position(|t| t.ch == ch)
    }

    pub fn track_by_ch_mut(&mut self, ch: u8) -> Option<&mut Track> {
        let idx = self.track_index_by_ch(ch)?;
        Some(&mut self.tracks[idx])
    }

    pub fn track_by_ch(&self, ch: u8) -> Option<&Track> {
        let idx = self.track_index_by_ch(ch)?;
        Some(&self.tracks[idx])
    }

    pub fn note_by_id(&self, ch: u8, id: NoteId) -> Option<&Note> {
        self.track_by_ch(ch)?.notes.iter().find(|n| n.id == id)
    }

    pub fn sort_track_notes(&mut self, ch: u8) {
        if let Some(track) = self.track_by_ch_mut(ch) {
            track.notes.sort_by(|a, b| {
                a.start
                    .partial_cmp(&b.start)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.pitch.cmp(&b.pitch))
                    .then(a.id.0.cmp(&b.id.0))
            });
        }
    }
}

/// One loop sketch in the Loop Bank (M4).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoopSketch {
    pub name: String,
    pub loop_bars: u8,
    pub key_root: u8,
    pub is_minor: bool,
    pub tracks: Vec<Track>,
    pub chord_blocks: Vec<ChordBlock>,
}

impl LoopSketch {
    pub fn beats(&self) -> f64 {
        self.loop_bars as f64 * 4.0
    }

    pub fn new_empty(name: impl Into<String>, loop_bars: u8) -> Self {
        let mut proj = Project::default();
        proj.loop_bars = loop_bars;
        Self {
            name: name.into(),
            loop_bars,
            key_root: proj.key_root,
            is_minor: proj.is_minor,
            tracks: proj.tracks,
            chord_blocks: vec![],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GridSettings {
    pub note_len: f64,
    pub snap_enabled: bool,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            note_len: 0.25,
            snap_enabled: true,
        }
    }
}

pub fn snap_beat(beat: f64, grid: &GridSettings) -> f64 {
    if !grid.snap_enabled {
        return beat.max(0.0);
    }
    let step = grid.note_len.max(0.0625);
    (beat / step).round() * step
}

pub fn snap_dur(dur: f64, grid: &GridSettings) -> f64 {
    let min = 0.0625;
    if !grid.snap_enabled {
        return dur.max(min);
    }
    let step = grid.note_len.max(min);
    ((dur / step).round() * step).max(step)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_ids_are_unique() {
        let mut p = Project::default();
        let a = p.alloc_note_id();
        let b = p.alloc_note_id();
        assert_ne!(a, b);
    }

    #[test]
    fn snap_beat_quarter() {
        let grid = GridSettings {
            note_len: 0.25,
            snap_enabled: true,
        };
        assert!((snap_beat(1.12, &grid) - 1.0).abs() < 1e-9);
    }
}