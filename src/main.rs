// JpoProducer - Rust + egui version
// Goal: easy to run (single exe after `cargo build --release`), clean dark UI,
// good Japanese support, proper tools + variable note lengths, Ch1 block painting,
// real SF2 playback with the user-provided FluidR3 GM.SF2 (rustysynth + cpal).
// Follows the revised # JpoProducer.txt instruction (desktop). Playback is priority #1.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use midly::{Format, MetaMessage, MidiMessage, Smf, Track, TrackEvent, TrackEventKind};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::rc::Rc;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

// Real-time audio playback (priority #1 per revised instruction)
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::StreamConfig;
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

const PPQ: u16 = 480;
const SYNCOPATION_WINDOW_BEATS: f64 = 2.0;

const PITCH_CLASS_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Smooth onion fade — avoids a harsh jump from invisible to vivid at low slider values.
fn onion_alpha(slider: f32) -> u8 {
    let t = slider.clamp(0.0, 1.0);
    let eased = t.powf(2.2);
    (eased * 88.0).round() as u8
}

fn default_track_vol() -> f32 {
    1.0
}

fn pitch_class_name(pitch: u8) -> &'static str {
    PITCH_CLASS_NAMES[(pitch % 12) as usize]
}

fn track_short_label(ch: u8) -> String {
    match ch {
        1 => "Chord".to_string(),
        10 => "Drum".to_string(),
        n => format!("Ch{}", n),
    }
}

fn truncate_ascii(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if s.len() <= max_chars {
        return s.to_string();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    format!("{}…", &s[..max_chars - 1])
}

fn gm_instrument_name(patch: u8) -> &'static str {
    const NAMES: [&str; 128] = [
        "Acoustic Grand Piano", "Bright Acoustic Piano", "Electric Grand Piano", "Honky-tonk Piano",
        "Electric Piano 1", "Electric Piano 2", "Harpsichord", "Clavinet",
        "Celesta", "Glockenspiel", "Music Box", "Vibraphone",
        "Marimba", "Xylophone", "Tubular Bells", "Dulcimer",
        "Drawbar Organ", "Percussive Organ", "Rock Organ", "Church Organ",
        "Reed Organ", "Accordion", "Harmonica", "Tango Accordion",
        "Acoustic Guitar (nylon)", "Acoustic Guitar (steel)", "Electric Guitar (jazz)", "Electric Guitar (clean)",
        "Electric Guitar (muted)", "Overdriven Guitar", "Distortion Guitar", "Guitar harmonics",
        "Acoustic Bass", "Electric Bass (finger)", "Electric Bass (pick)", "Fretless Bass",
        "Slap Bass 1", "Slap Bass 2", "Synth Bass 1", "Synth Bass 2",
        "Violin", "Viola", "Cello", "Contrabass",
        "Tremolo Strings", "Pizzicato Strings", "Orchestral Harp", "Timpani",
        "String Ensemble 1", "String Ensemble 2", "Synth Strings 1", "Synth Strings 2",
        "Choir Aahs", "Voice Oohs", "Synth Voice", "Orchestra Hit",
        "Trumpet", "Trombone", "Tuba", "Muted Trumpet",
        "French Horn", "Brass Section", "Synth Brass 1", "Synth Brass 2",
        "Soprano Sax", "Alto Sax", "Tenor Sax", "Baritone Sax",
        "Oboe", "English Horn", "Bassoon", "Clarinet",
        "Piccolo", "Flute", "Recorder", "Pan Flute",
        "Blown Bottle", "Shakuhachi", "Whistle", "Ocarina",
        "Square Lead", "Sawtooth Lead", "Calliope Lead", "Chiff Lead",
        "Charang Lead", "Voice Lead", "Fifths Lead", "Bass + Lead",
        "New Age Pad", "Warm Pad", "Polysynth Pad", "Choir Pad",
        "Bowed Pad", "Metallic Pad", "Halo Pad", "Sweep Pad",
        "Rain FX", "Soundtrack FX", "Crystal FX", "Atmosphere FX",
        "Brightness FX", "Goblins FX", "Echoes FX", "Sci-Fi FX",
        "Sitar", "Banjo", "Shamisen", "Koto",
        "Kalimba", "Bagpipe", "Fiddle", "Shanai",
        "Tinkle Bell", "Agogo", "Steel Drums", "Woodblock",
        "Taiko Drum", "Melodic Tom", "Synth Drum", "Reverse Cymbal",
        "Guitar Fret Noise", "Breath Noise", "Seashore", "Bird Tweet",
        "Telephone Ring", "Helicopter", "Applause", "Gunshot",
    ];
    NAMES.get(patch as usize).copied().unwrap_or("Unknown")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ChordBlock {
    start: f64,
    dur: f64,
    degree: u8,   // 1-7
    quality: String, // "", "m", "7", ...
    octave: u8,
    /// Per-block manual syncopation splice (★ in Chord Strip).
    #[serde(default)]
    syncopation_fill: bool,
}

impl ChordBlock {
    fn end(&self) -> f64 { self.start + self.dur }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Note {
    start: f64,
    pitch: u8,
    dur: f64,
    vel: u8,
}

impl Note {
    fn end(&self) -> f64 { self.start + self.dur }
}

/// Playback state that is Send (for moving into cpal closure).
/// Owns the Send parts + the advancing cursor for an efficient scheduler.
/// The actual Synthesizer (which internally uses !Send Rc for samples) is
/// created on the audio thread using a small pragmatic static (only the heavy part).
#[allow(dead_code)]
struct PlaybackPlayer {
    sf_path: std::path::PathBuf,
    events: Vec<(u64, u8, bool, u8, u8)>,
    position: Arc<AtomicU64>,
    sample_rate: u32,
    gain: f32,
    initial_patches: Vec<(u8, u8)>, // (ch 0-15, program) -- planned for future live program changes in synth

    // These are Send and persist in the closure environment
    event_idx: usize,
    current_sample: u64,
    loop_enabled: bool,
    loop_end_sample: u64,
    patches_applied: bool,
}

impl PlaybackPlayer {
    fn new(
        sf_path: std::path::PathBuf,
        events: Vec<(u64, u8, bool, u8, u8)>,
        position: Arc<AtomicU64>,
        sample_rate: u32,
        gain: f32,
        initial_patches: Vec<(u8, u8)>,
        loop_enabled: bool,
        loop_end_sample: u64,
        start_sample_offset: u64,
    ) -> Self {
        let mut event_idx = 0;
        while event_idx < events.len() && events[event_idx].0 < start_sample_offset {
            event_idx += 1;
        }
        Self {
            sf_path,
            events,
            position,
            sample_rate,
            gain,
            initial_patches,
            event_idx,
            current_sample: start_sample_offset,
            loop_enabled,
            loop_end_sample,
            patches_applied: false,
        }
    }

    fn apply_patches(synth: &mut Synthesizer, patches: &[(u8, u8)]) {
        for &(ch, program) in patches {
            synth.process_midi_message(ch as i32, 0xC0, program as i32, 0);
        }
    }

    fn all_notes_off(synth: &mut Synthesizer) {
        for ch in 0..16 {
            for pitch in 0..128 {
                synth.note_off(ch, pitch);
            }
        }
    }

    /// Called from the audio callback. Uses advancing cursor (no full re-scan).
    /// Synth is managed via a small static (created on this thread).
    fn render(&mut self, data: &mut [f32]) {
        // Lazy synth creation on audio thread only (Rc never leaves this thread)
        // NOTE: The mutable static here is a pragmatic desktop-only hack to load the SoundFont
        // on the audio thread (to avoid Send issues with Rc inside SoundFont).
        // This triggers static_mut_refs warnings but is contained and works.
        #[allow(static_mut_refs)]
        let synth = unsafe {
            static mut SYNTH: Option<Synthesizer> = None;
            if SYNTH.is_none() {
                if let Ok(f) = std::fs::File::open(&self.sf_path) {
                    let mut r = std::io::BufReader::new(f);
                    if let Ok(sf) = SoundFont::new(&mut r) {
                        let sf = Rc::new(sf);
                        if let Ok(s) = Synthesizer::new(&sf, &SynthesizerSettings::new(self.sample_rate as i32)) {
                            SYNTH = Some(s);
                            eprintln!("[Playback] SF2 loaded on audio thread");
                        }
                    }
                }
            }
            SYNTH.as_mut()
        };

        let synth = match synth {
            Some(s) => s,
            None => {
                for s in data.iter_mut() { *s = 0.0; }
                return;
            }
        };

        if !self.patches_applied {
            Self::apply_patches(synth, &self.initial_patches);
            self.patches_applied = true;
        }

        // Efficient scheduler with persistent cursor
        for frame in data.chunks_mut(2) {
            while self.event_idx < self.events.len() {
                let (t, ch, is_on, pitch, vel) = self.events[self.event_idx];
                if t > self.current_sample { break; }
                if is_on {
                    synth.note_on(ch as i32, pitch as i32, vel as i32);
                } else {
                    synth.note_off(ch as i32, pitch as i32);
                }
                self.event_idx += 1;
            }

            let mut left = [0.0f32; 1];
            let mut right = [0.0f32; 1];
            synth.render(&mut left, &mut right);

            // Apply volume (simple post-gain for now)
            left[0] *= self.gain;
            right[0] *= self.gain;

            if frame.len() >= 2 {
                frame[0] = left[0];
                frame[1] = right[0];
            } else if !frame.is_empty() {
                frame[0] = left[0];
            }

            self.current_sample += 1;

            if self.loop_enabled
                && self.loop_end_sample > 0
                && self.current_sample >= self.loop_end_sample
            {
                Self::all_notes_off(synth);
                self.current_sample = 0;
                self.event_idx = 0;
            }
        }

        self.position.store(self.current_sample, Ordering::Relaxed);
    }
}

/// Short audition notes (editing feedback) on a dedicated low-latency stream.
#[derive(Clone)]
struct PreviewTrigger {
    synth_ch: u8,
    program: u8,
    pitches: Vec<u8>,
    velocity: u8,
    duration_samples: u32,
    gain: f32,
}

struct PreviewVoice {
    synth_ch: u8,
    pitch: u8,
    release_at: u64,
}

struct PreviewEngine {
    sf_path: std::path::PathBuf,
    sample_rate: u32,
    triggers: Arc<Mutex<Vec<PreviewTrigger>>>,
    voices: Vec<PreviewVoice>,
    clock: u64,
    preview_gain: f32,
}

impl PreviewEngine {
    fn new(sf_path: std::path::PathBuf, sample_rate: u32, triggers: Arc<Mutex<Vec<PreviewTrigger>>>) -> Self {
        Self {
            sf_path,
            sample_rate,
            triggers,
            voices: Vec::new(),
            clock: 0,
            preview_gain: 0.75,
        }
    }

    fn render(&mut self, data: &mut [f32]) {
        #[allow(static_mut_refs)]
        let synth = unsafe {
            static mut PREVIEW_SYNTH: Option<Synthesizer> = None;
            if PREVIEW_SYNTH.is_none() {
                if let Ok(f) = std::fs::File::open(&self.sf_path) {
                    let mut r = std::io::BufReader::new(f);
                    if let Ok(sf) = SoundFont::new(&mut r) {
                        let sf = Rc::new(sf);
                        if let Ok(s) =
                            Synthesizer::new(&sf, &SynthesizerSettings::new(self.sample_rate as i32))
                        {
                            PREVIEW_SYNTH = Some(s);
                        }
                    }
                }
            }
            PREVIEW_SYNTH.as_mut()
        };

        let synth = match synth {
            Some(s) => s,
            None => {
                for s in data.iter_mut() {
                    *s = 0.0;
                }
                return;
            }
        };

        if let Ok(mut pending) = self.triggers.lock() {
            for t in pending.drain(..) {
                let ch = t.synth_ch as i32;
                synth.process_midi_message(ch, 0xC0, t.program as i32, 0);
                for &p in &t.pitches {
                    synth.note_on(ch, p as i32, t.velocity as i32);
                    self.voices.push(PreviewVoice {
                        synth_ch: t.synth_ch,
                        pitch: p,
                        release_at: self.clock + t.duration_samples as u64,
                    });
                }
                self.preview_gain = t.gain;
            }
        }

        for frame in data.chunks_mut(2) {
            while let Some(idx) = self
                .voices
                .iter()
                .position(|v| v.release_at <= self.clock)
            {
                let v = self.voices.remove(idx);
                synth.note_off(v.synth_ch as i32, v.pitch as i32);
            }

            let mut left = [0.0f32; 1];
            let mut right = [0.0f32; 1];
            synth.render(&mut left, &mut right);
            let g = self.preview_gain;
            left[0] *= g;
            right[0] *= g;

            if frame.len() >= 2 {
                frame[0] = left[0];
                frame[1] = right[0];
            } else if !frame.is_empty() {
                frame[0] = left[0];
            }

            self.clock += 1;
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TrackData {
    ch: u8,
    notes: Vec<Note>,
    patch: u8,
    #[serde(default)]
    muted: bool,
    #[serde(default)]
    solo: bool,
    #[serde(default = "default_track_vol")]
    track_vol: f32,
}

fn track_is_audible(tracks: &[TrackData], idx: usize) -> bool {
    let t = &tracks[idx];
    if t.muted {
        return false;
    }
    let any_solo = tracks.iter().any(|tr| tr.solo);
    if any_solo {
        return t.solo;
    }
    true
}

fn scaled_velocity(vel: u8, track_vol: f32) -> u8 {
    ((vel as f32) * track_vol.clamp(0.0, 1.0))
        .round()
        .clamp(1.0, 127.0) as u8
}

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
struct Selection {
    // (track_idx, note_idx) for regular notes
    notes: HashSet<(usize, usize)>,
    // block indices for Ch1
    blocks: HashSet<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Project {
    bpm: f64,
    key_root: u8,     // 0=C ... 11=B
    is_minor: bool,
    tracks: Vec<TrackData>,
    chord_blocks: Vec<ChordBlock>,
}

/// One loop sketch in the Loop Bank (Phase B). Key is per-loop; BPM stays project-wide.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct LoopSketch {
    name: String,
    loop_bars: u8,
    key_root: u8,
    is_minor: bool,
    tracks: Vec<TrackData>,
    chord_blocks: Vec<ChordBlock>,
}

impl LoopSketch {
    fn beats(&self) -> f64 {
        self.loop_bars as f64 * 4.0
    }

    fn new_empty(name: impl Into<String>, loop_bars: u8) -> Self {
        Self {
            name: name.into(),
            loop_bars,
            key_root: 0,
            is_minor: false,
            tracks: Project::default().tracks,
            chord_blocks: vec![],
        }
    }

    fn capture_from(proj: &Project, name: String, loop_bars: u8) -> Self {
        Self {
            name,
            loop_bars,
            key_root: proj.key_root,
            is_minor: proj.is_minor,
            tracks: proj.tracks.clone(),
            chord_blocks: proj.chord_blocks.clone(),
        }
    }

    fn apply_to(&self, proj: &mut Project) {
        proj.key_root = self.key_root;
        proj.is_minor = self.is_minor;
        proj.tracks = self.tracks.clone();
        proj.chord_blocks = self.chord_blocks.clone();
    }

    fn key_short_label(&self) -> String {
        let roots = ["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"];
        let root = roots.get(self.key_root as usize).copied().unwrap_or("?");
        let mode = if self.is_minor { "m" } else { "" };
        format!("{root}{mode}")
    }
}

impl Default for Project {
    fn default() -> Self {
        let mut tracks = Vec::new();
        for ch in 1..=16 {
            let default_patch = match ch {
                3 => 33, // Finger Bass for track 3 (Root Basist)
                _ => 0,
            };
            tracks.push(TrackData {
                ch,
                notes: vec![],
                patch: default_patch,
                muted: false,
                solo: false,
                track_vol: 1.0,
            });
        }
        Self {
            bpm: 128.0,
            key_root: 0,
            is_minor: false,
            tracks,
            chord_blocks: vec![],
        }
    }
}

impl Project {
    fn scale_semitones(&self) -> [i32; 7] {
        if self.is_minor {
            [0, 2, 3, 5, 7, 8, 10]
        } else {
            [0, 2, 4, 5, 7, 9, 11]
        }
    }

    fn degree_root(&self, degree: u8, octave: u8) -> u8 {
        let semis = self.scale_semitones();
        let idx = ((degree - 1) % 7) as usize;
        let offset = semis[idx];
        let root = 60i32 + self.key_root as i32 + offset + (octave as i32 - 4) * 12;
        root.clamp(0, 127) as u8
    }

    fn chord_pitches(&self, blk: &ChordBlock) -> Vec<u8> {
        let root = self.degree_root(blk.degree, blk.octave);
        let intervals: &[i32] = match blk.quality.as_str() {
            "m" => &[0, 3, 7],
            "7" => &[0, 4, 7, 10],
            "maj7" => &[0, 4, 7, 11],
            "m7" => &[0, 3, 7, 10],
            "dim" => &[0, 3, 6],
            "aug" => &[0, 4, 8],
            "sus4" => &[0, 5, 7],
            _ => &[0, 4, 7], // major triad default
        };
        intervals.iter().map(|&i| (root as i32 + i).clamp(0, 127) as u8).collect()
    }

    fn chord_name(&self, blk: &ChordBlock) -> String {
        let semis = self.scale_semitones();
        let idx = ((blk.degree - 1) % 7) as usize;
        let root_idx = ((self.key_root as i32 + semis[idx]) % 12) as usize;
        let roots = ["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"];
        let q = match blk.quality.as_str() {
            "m" => "m",
            "7" => "7",
            "maj7" => "M7",
            "m7" => "m7",
            "dim" => "dim",
            "aug" => "+",
            "sus4" => "sus4",
            _ => "",
        };
        format!("{}{}", roots[root_idx], q)
    }

    /// Return the "natural" quality for a degree in the current key.
    /// Matches common J-Pop usage: for major: Ⅰ Ⅱm Ⅲm Ⅳ Ⅴ Ⅵm
    /// Diatonic pitch classes (0-11) for the current key — used for scale highlighting.
    fn scale_pitch_classes(&self) -> [u8; 7] {
        let semis = self.scale_semitones();
        semis.map(|s| ((self.key_root as i32 + s).rem_euclid(12)) as u8)
    }

    fn default_quality(&self, degree: u8) -> &'static str {
        let d = ((degree - 1) % 7) + 1;
        if self.is_minor {
            match d {
                1 => "",
                2 => "m",
                3 => "",
                4 => "m",
                5 => "m",
                6 => "",
                7 => "",
                _ => "",
            }
        } else {
            match d {
                1 => "",
                2 => "m",
                3 => "m",
                4 => "",
                5 => "",
                6 => "m",
                7 => "",
                _ => "",
            }
        }
    }
}

fn expand_chords(proj: &Project) -> Vec<Note> {
    let mut out = vec![];
    for b in &proj.chord_blocks {
        for p in proj.chord_pitches(b) {
            out.push(Note { start: b.start, pitch: p, dur: b.dur, vel: 78 });
        }
    }
    out
}

// === GROK / PROGRESSION PARSING ===

fn normalize_roman_token(token: &str) -> String {
    token
        .replace('Ⅰ', "I")
        .replace('ⅰ', "i")
        .replace('Ⅱ', "II")
        .replace('ⅱ', "ii")
        .replace('Ⅲ', "III")
        .replace('ⅲ', "iii")
        .replace('Ⅳ', "IV")
        .replace('ⅳ', "iv")
        .replace('Ⅴ', "V")
        .replace('ⅴ', "v")
        .replace('Ⅵ', "VI")
        .replace('ⅵ', "vi")
        .replace('Ⅶ', "VII")
        .replace('ⅶ', "vii")
}

fn split_progression_tokens(s: &str) -> Vec<String> {
    let mut buf = String::new();
    for c in s.chars() {
        match c {
            '|' | ',' | '/' | '\n' | '\t' | '–' | '—' | '-' => buf.push(' '),
            _ => buf.push(c),
        }
    }
    buf.split_whitespace()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

fn parse_literal_chord(proj: &Project, token: &str) -> Option<(u8, String)> {
    let t = token.trim();
    if t.is_empty() {
        return None;
    }
    let roots = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let (root_str, quality): (&str, &str) = if let Some(rest) = t.strip_prefix("Bb") {
        ("A#", rest)
    } else if let Some(rest) = t.strip_prefix("Db") {
        ("C#", rest)
    } else if let Some(rest) = t.strip_prefix("Eb") {
        ("D#", rest)
    } else if let Some(rest) = t.strip_prefix("Gb") {
        ("F#", rest)
    } else if let Some(rest) = t.strip_prefix("Ab") {
        ("G#", rest)
    } else {
        let mut matched: Option<(&str, &str)> = None;
        for r in roots {
            if t.starts_with(r) {
                matched = Some((r, &t[r.len()..]));
                break;
            }
        }
        matched?
    };
    let root_pc = roots.iter().position(|&r| r == root_str)? as u8;
    let semis = proj.scale_semitones();
    let mut degree = None;
    for (i, &s) in semis.iter().enumerate() {
        let pc = ((proj.key_root as i32 + s).rem_euclid(12)) as u8;
        if pc == root_pc {
            degree = Some((i as u8) + 1);
            break;
        }
    }
    let degree = degree?;
    let quality = match quality.to_lowercase().as_str() {
        "" | "maj" | "major" => proj.default_quality(degree).to_string(),
        "m" | "min" | "minor" => "m".to_string(),
        "7" => "7".to_string(),
        "maj7" | "m7" | "dim" | "aug" | "sus4" => quality.to_lowercase(),
        _ => proj.default_quality(degree).to_string(),
    };
    Some((degree, quality))
}

fn parse_roman_chord(proj: &Project, token: &str) -> Option<(u8, String)> {
    let t = token.trim();
    if t.is_empty() {
        return None;
    }
    if t.starts_with('b') || t.starts_with('B') {
        return parse_literal_chord(proj, t);
    }
    let t = normalize_roman_token(t);
    let roman_part: String = t
        .chars()
        .take_while(|c| *c == 'I' || *c == 'V' || *c == 'i' || *c == 'v')
        .collect();
    if roman_part.is_empty() {
        return None;
    }
    let qual_tail = &t[roman_part.len()..];
    let roman = roman_part;
    let degree = match roman.to_uppercase().as_str() {
        "I" => 1,
        "II" => 2,
        "III" => 3,
        "IV" => 4,
        "V" => 5,
        "VI" => 6,
        "VII" => 7,
        _ => return None,
    };
    let lower = roman.chars().any(|c| c.is_ascii_lowercase());
    let mut quality = if lower { "m".to_string() } else { proj.default_quality(degree).to_string() };
    let tail = qual_tail.to_lowercase();
    if tail.contains("maj7") {
        quality = "maj7".to_string();
    } else if tail.contains("m7") {
        quality = "m7".to_string();
    } else if tail.contains('7') {
        quality = "7".to_string();
    } else if tail.contains("dim") {
        quality = "dim".to_string();
    } else if tail.contains("aug") || tail.contains('+') {
        quality = "aug".to_string();
    } else if tail.contains("sus4") {
        quality = "sus4".to_string();
    }
    Some((degree, quality))
}

fn parse_chord_token(proj: &Project, token: &str) -> Option<(u8, String)> {
    parse_roman_chord(proj, token).or_else(|| parse_literal_chord(proj, token))
}

fn parse_progression_text(proj: &Project, text: &str) -> Vec<(u8, String)> {
    split_progression_tokens(text)
        .iter()
        .filter_map(|tok| parse_chord_token(proj, tok))
        .collect()
}

fn parse_midi_track_notes(data: &[u8], paste_at: f64) -> Result<Vec<Note>, String> {
    let smf = Smf::parse(data).map_err(|e| format!("MIDI parse: {e}"))?;
    let ppq = match smf.header.timing {
        midly::Timing::Metrical(t) => t.as_int() as f64,
        _ => PPQ as f64,
    };
    let track = smf
        .tracks
        .iter()
        .max_by_key(|t| t.len())
        .ok_or_else(|| "MIDI: no tracks".to_string())?;

    let mut abs_tick = 0u32;
    let mut active: std::collections::HashMap<u8, (u32, u8)> = std::collections::HashMap::new();
    let mut pairs: Vec<(u32, u32, u8, u8)> = Vec::new();

    for ev in track {
        abs_tick = abs_tick.saturating_add(ev.delta.as_int());
        if let TrackEventKind::Midi { message, .. } = &ev.kind {
            match message {
                MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                    active.insert(key.as_int(), (abs_tick, vel.as_int()));
                }
                MidiMessage::NoteOn { key, .. } | MidiMessage::NoteOff { key, .. } => {
                    if let Some((on_tick, vel)) = active.remove(&key.as_int()) {
                        pairs.push((on_tick, abs_tick, key.as_int(), vel));
                    }
                }
                _ => {}
            }
        }
    }

    if pairs.is_empty() {
        return Err("MIDI: no notes found".to_string());
    }

    let min_tick = pairs.iter().map(|p| p.0).min().unwrap_or(0);
    let mut notes = Vec::new();
    for (on, off, pitch, vel) in pairs {
        let rel_start = (on.saturating_sub(min_tick)) as f64 / ppq;
        let dur = ((off.saturating_sub(on)) as f64 / ppq).max(0.0625);
        notes.push(Note {
            start: paste_at + rel_start,
            pitch,
            dur,
            vel,
        });
    }
    notes.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap().then(a.pitch.cmp(&b.pitch)));
    Ok(notes)
}

// === PATTERN ENGINE ===
// Key-C MIDI templates from assets/patterns — tiled per chord block (melodic) or range (drums).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PatternCategory {
    Piano,
    Bass,
    Drum,
}

#[derive(Clone, Debug)]
struct PatternNote {
    start_beats: f64,
    dur_beats: f64,
    pitch: u8,
    vel: u8,
}

#[derive(Clone, Debug)]
struct LoadedPattern {
    id: String,
    category: PatternCategory,
    length_beats: f64,
    template_root: u8,
    notes: Vec<PatternNote>,
}

#[derive(Clone, Debug, Default)]
struct PatternLibrary {
    patterns: Vec<LoadedPattern>,
}

impl PatternLibrary {
    fn load() -> Self {
        let mut lib = Self::default();
        let specs: &[(&str, PatternCategory, u8)] = &[
            ("Piano01", PatternCategory::Piano, 60),
            ("Piano02", PatternCategory::Piano, 60),
            ("Piano03", PatternCategory::Piano, 60),
            ("Piano04", PatternCategory::Piano, 60),
            ("Piano_syncopation", PatternCategory::Piano, 60),
            ("Bass8beat01", PatternCategory::Bass, 48),
            ("Bass8beat02", PatternCategory::Bass, 48),
            ("BassDance01", PatternCategory::Bass, 48),
            ("BassDance02", PatternCategory::Bass, 48),
            ("Bass_syncopation", PatternCategory::Bass, 48),
            ("Drum4beat_01", PatternCategory::Drum, 0),
            ("Drum8beat_01", PatternCategory::Drum, 0),
            ("Drum8beat_02", PatternCategory::Drum, 0),
            ("Drum16beat_01", PatternCategory::Drum, 0),
            ("DrumHipHopbeat_01", PatternCategory::Drum, 0),
            ("Drum_syncopation", PatternCategory::Drum, 0),
        ];
        let Some(dir) = find_patterns_dir() else {
            eprintln!(
                "[Pattern] directory not found — place patterns in jpo/assets/patterns/ or next to jpo.exe"
            );
            return lib;
        };
        eprintln!("[Pattern] loading from {}", dir.display());
        for &(id, category, root) in specs {
            let path = dir.join(format!("{id}.mid"));
            match std::fs::read(&path) {
                Ok(bytes) => match parse_pattern_midi(&bytes, id, category, root) {
                    Ok(p) => lib.patterns.push(p),
                    Err(e) => eprintln!("[Pattern] {e}"),
                },
                Err(_) => eprintln!("[Pattern] missing: {}", path.display()),
            }
        }
        eprintln!("[Pattern] loaded {} pattern(s)", lib.patterns.len());
        lib
    }

    fn ids_for(&self, category: PatternCategory, syncopation: bool) -> Vec<&str> {
        self.patterns
            .iter()
            .filter(|p| {
                p.category == category
                    && p.id.contains("syncopation") == syncopation
            })
            .map(|p| p.id.as_str())
            .collect()
    }

    fn get(&self, id: &str) -> Option<&LoadedPattern> {
        self.patterns.iter().find(|p| p.id == id)
    }
}

/// Locate the patterns folder (mirrors SF2 search — works under `cargo run` → target/debug).
fn find_patterns_dir() -> Option<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = vec![
        std::path::PathBuf::from("assets/patterns"),
        std::path::PathBuf::from("patterns"),
    ];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("patterns"));
            candidates.push(parent.join("assets/patterns"));
            candidates.push(parent.join("../assets/patterns"));
            candidates.push(parent.join("../../assets/patterns"));
            candidates.push(parent.join("../../../assets/patterns"));
        }
    }
    for path in candidates {
        if path.is_dir() {
            return path.canonicalize().ok().or(Some(path));
        }
    }
    None
}

fn parse_pattern_midi(
    data: &[u8],
    id: &str,
    category: PatternCategory,
    template_root: u8,
) -> Result<LoadedPattern, String> {
    let smf = Smf::parse(data).map_err(|e| format!("parse {id}: {e}"))?;
    let ppq = match smf.header.timing {
        midly::Timing::Metrical(t) => t.as_int() as f64,
        _ => PPQ as f64,
    };
    let track = smf
        .tracks
        .iter()
        .max_by_key(|t| t.len())
        .ok_or_else(|| format!("{id}: no tracks"))?;

    let mut abs_tick = 0u32;
    let mut active: std::collections::HashMap<u8, (u32, u8)> = std::collections::HashMap::new();
    let mut pairs: Vec<(u32, u32, u8, u8)> = Vec::new();

    for ev in track {
        abs_tick = abs_tick.saturating_add(ev.delta.as_int());
        if let TrackEventKind::Midi { message, .. } = &ev.kind {
            match message {
                MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                    active.insert(key.as_int(), (abs_tick, vel.as_int()));
                }
                MidiMessage::NoteOn { key, .. } | MidiMessage::NoteOff { key, .. } => {
                    if let Some((on_tick, vel)) = active.remove(&key.as_int()) {
                        pairs.push((on_tick, abs_tick, key.as_int(), vel));
                    }
                }
                _ => {}
            }
        }
    }

    let mut notes = Vec::new();
    let mut span_end = 0.0f64;
    for (on, off, pitch, vel) in pairs {
        let start_beats = on as f64 / ppq;
        let dur_beats = ((off.saturating_sub(on)) as f64 / ppq).max(0.05);
        span_end = span_end.max(start_beats + dur_beats);
        notes.push(PatternNote {
            start_beats,
            dur_beats,
            pitch,
            vel,
        });
    }

    let length_beats = if notes.is_empty() {
        8.0
    } else if id.contains("syncopation") {
        SYNCOPATION_WINDOW_BEATS
    } else {
        ((span_end / 4.0).ceil() * 4.0).max(2.0)
    };

    Ok(LoadedPattern {
        id: id.to_string(),
        category,
        length_beats,
        template_root,
        notes,
    })
}

fn melodic_pitch(pattern_pitch: u8, pattern_template: u8, target_root: u8) -> u8 {
    (pattern_pitch as i32 - pattern_template as i32 + target_root as i32)
        .clamp(0, 127) as u8
}

fn pattern_tile_at(fill_start: f64, origin: f64, pat_len: f64) -> f64 {
    let pat_len = pat_len.max(0.25);
    if fill_start <= origin + 0.001 {
        return origin;
    }
    let k = ((fill_start - origin) / pat_len).floor();
    origin + k * pat_len
}

fn note_overlaps_range(n: &Note, range_start: f64, range_end: f64) -> bool {
    n.start < range_end - 0.001 && n.end() > range_start + 0.001
}

fn dedupe_notes(notes: &mut Vec<Note>) {
    notes.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap()
            .then(a.pitch.cmp(&b.pitch))
    });
    notes.dedup_by(|a, b| (a.start - b.start).abs() < 0.02 && a.pitch == b.pitch);
}

fn replace_notes_in_range(notes: &mut Vec<Note>, range_start: f64, range_end: f64, mut new_notes: Vec<Note>) {
    notes.retain(|n| !note_overlaps_range(n, range_start, range_end));
    dedupe_notes(&mut new_notes);
    notes.extend(new_notes);
    dedupe_notes(notes);
}

fn chord_block_at<'a>(blocks: &'a [ChordBlock], beat: f64) -> Option<&'a ChordBlock> {
    blocks
        .iter()
        .find(|b| beat >= b.start && beat < b.end())
        .or_else(|| {
            blocks
                .iter()
                .filter(|b| b.start <= beat)
                .max_by(|a, b| a.start.partial_cmp(&b.start).unwrap())
        })
}

fn syncopation_windows(proj: &Project, s: f64, e: f64) -> Vec<(f64, f64)> {
    proj.chord_blocks
        .iter()
        .filter(|b| b.syncopation_fill && b.end() > s && b.start < e)
        .map(|b| (b.start, (b.start + SYNCOPATION_WINDOW_BEATS).min(e)))
        .collect()
}

fn apply_melodic_pattern(
    pattern: &LoadedPattern,
    proj: &Project,
    range_start: f64,
    range_end: f64,
) -> Vec<Note> {
    let mut notes = Vec::new();
    let ref_octave = match pattern.category {
        PatternCategory::Piano => 3,
        PatternCategory::Bass => 3,
        PatternCategory::Drum => 3,
    };

    for block in &proj.chord_blocks {
        if block.end() <= range_start || block.start >= range_end {
            continue;
        }
        let target_root = proj.degree_root(block.degree, ref_octave);
        let block_end = block.end().min(range_end);
        let pat_len = pattern.length_beats.max(0.25);
        let mut tile = pattern_tile_at(range_start, block.start, pat_len);
        while tile < block_end {
            for pn in &pattern.notes {
                let t = tile + pn.start_beats;
                if t >= range_start && t < range_end && t < block_end {
                    notes.push(Note {
                        start: t,
                        pitch: melodic_pitch(pn.pitch, pattern.template_root, target_root),
                        dur: pn.dur_beats,
                        vel: pn.vel,
                    });
                }
            }
            tile += pat_len;
        }
    }
    notes
}

fn apply_drum_pattern(pattern: &LoadedPattern, range_start: f64, range_end: f64) -> Vec<Note> {
    let mut notes = Vec::new();
    let len = pattern.length_beats.max(0.25);
    let mut tile = (range_start / len).floor() * len;
    while tile < range_end {
        for pn in &pattern.notes {
            let t = tile + pn.start_beats;
            if t >= range_start && t < range_end {
                notes.push(Note {
                    start: t,
                    pitch: pn.pitch,
                    dur: pn.dur_beats,
                    vel: pn.vel,
                });
            }
        }
        tile += len;
    }
    notes
}

fn remove_notes_in_windows(notes: &mut Vec<Note>, windows: &[(f64, f64)]) {
    if windows.is_empty() {
        return;
    }
    notes.retain(|n| {
        !windows
            .iter()
            .any(|(s, e)| n.start < *e - 0.001 && n.end() > *s + 0.001)
    });
}

fn apply_melodic_block_range(
    pattern: &LoadedPattern,
    proj: &Project,
    block: &ChordBlock,
    range_start: f64,
    range_end: f64,
) -> Vec<Note> {
    let mut notes = Vec::new();
    let ref_octave = match pattern.category {
        PatternCategory::Piano => 3,
        PatternCategory::Bass => 3,
        PatternCategory::Drum => 3,
    };
    let target_root = proj.degree_root(block.degree, ref_octave);
    let block_end = block.end().min(range_end);
    let pat_len = pattern.length_beats.max(0.25);
    let mut tile = pattern_tile_at(range_start, block.start, pat_len);
    while tile < block_end {
        for pn in &pattern.notes {
            let t = tile + pn.start_beats;
            if t >= range_start && t < range_end && t < block_end {
                notes.push(Note {
                    start: t,
                    pitch: melodic_pitch(pn.pitch, pattern.template_root, target_root),
                    dur: pn.dur_beats,
                    vel: pn.vel,
                });
            }
        }
        tile += pat_len;
    }
    notes
}

/// After syncopation splice, refill the remainder of each chord block so no rest gap follows.
fn refill_after_sync_windows(
    notes: &mut Vec<Note>,
    pattern: &LoadedPattern,
    proj: &Project,
    windows: &[(f64, f64)],
    _range_start: f64,
    range_end: f64,
) {
    for &(win_start, win_end) in windows {
        let Some(block) = chord_block_at(&proj.chord_blocks, win_start) else {
            continue;
        };
        let fill_start = win_end;
        let fill_end = block.end().min(range_end);
        if fill_start >= fill_end - 0.001 {
            continue;
        }
        notes.retain(|n| !note_overlaps_range(n, fill_start, fill_end));
        let added = match pattern.category {
            PatternCategory::Drum => apply_drum_pattern(pattern, fill_start, fill_end),
            _ => apply_melodic_block_range(pattern, proj, block, fill_start, fill_end),
        };
        notes.extend(added);
    }
}

fn apply_syncopation_splice(
    notes: &mut Vec<Note>,
    sync_pattern: &LoadedPattern,
    proj: &Project,
    windows: &[(f64, f64)],
) {
    for &(win_start, win_end) in windows {
        let block = chord_block_at(&proj.chord_blocks, win_start);
        let ref_octave = match sync_pattern.category {
            PatternCategory::Piano => 3,
            PatternCategory::Bass => 3,
            _ => 3,
        };
        let target_root = block.map(|b| proj.degree_root(b.degree, ref_octave));

        for pn in &sync_pattern.notes {
            let t = win_start + pn.start_beats;
            if t >= win_end {
                continue;
            }
            let pitch = match (sync_pattern.category, target_root) {
                (PatternCategory::Drum, _) => pn.pitch,
                (_, Some(root)) => melodic_pitch(pn.pitch, sync_pattern.template_root, root),
                _ => pn.pitch,
            };
            notes.push(Note {
                start: t,
                pitch,
                dur: pn.dur_beats.min(win_end - t).max(0.05),
                vel: pn.vel,
            });
        }
    }
}

fn generate_from_patterns(
    lib: &PatternLibrary,
    proj: &Project,
    range_start: f64,
    range_end: f64,
    piano_id: &str,
    bass_id: &str,
    drum_id: &str,
    syncopation_fill: bool,
) -> (Vec<Note>, Vec<Note>, Vec<Note>) {
    let piano_pat = lib.get(piano_id);
    let bass_pat = lib.get(bass_id);
    let drum_pat = lib.get(drum_id);

    let mut piano = piano_pat
        .map(|p| apply_melodic_pattern(p, proj, range_start, range_end))
        .unwrap_or_default();
    let mut bass = bass_pat
        .map(|p| apply_melodic_pattern(p, proj, range_start, range_end))
        .unwrap_or_default();
    let mut drums = drum_pat
        .map(|p| apply_drum_pattern(p, range_start, range_end))
        .unwrap_or_default();

    if syncopation_fill {
        let windows = syncopation_windows(proj, range_start, range_end);
        if !windows.is_empty() {
            if let Some(p) = lib.get("Piano_syncopation") {
                remove_notes_in_windows(&mut piano, &windows);
                apply_syncopation_splice(&mut piano, p, proj, &windows);
                if let Some(base) = piano_pat {
                    refill_after_sync_windows(
                        &mut piano,
                        base,
                        proj,
                        &windows,
                        range_start,
                        range_end,
                    );
                }
            }
            if let Some(p) = lib.get("Bass_syncopation") {
                remove_notes_in_windows(&mut bass, &windows);
                apply_syncopation_splice(&mut bass, p, proj, &windows);
                if let Some(base) = bass_pat {
                    refill_after_sync_windows(
                        &mut bass,
                        base,
                        proj,
                        &windows,
                        range_start,
                        range_end,
                    );
                }
            }
            if let Some(p) = lib.get("Drum_syncopation") {
                remove_notes_in_windows(&mut drums, &windows);
                apply_syncopation_splice(&mut drums, p, proj, &windows);
                if let Some(base) = drum_pat {
                    refill_after_sync_windows(
                        &mut drums,
                        base,
                        proj,
                        &windows,
                        range_start,
                        range_end,
                    );
                }
            }
        }
    }

    (piano, bass, drums)
}

/// Locate the user's FluidR3 GM.SF2.
/// Priority (per revised instruction):
/// 1. Next to the executable (release/distribution: copy SF2 + exe into same folder)
/// 2. Sibling to the jpo/ dir or inside jpo/ (for `cargo run`)
/// 3. Original location in the JpoProducer/ project root
fn find_soundfont() -> Option<std::path::PathBuf> {
    let candidates: Vec<std::path::PathBuf> = vec![
        // Next to running exe (the most important case for "just works" distribution)
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.join("FluidR3 GM.SF2")))
            .unwrap_or_default(),
        // Inside jpo/ (copy placed here for development)
        std::path::PathBuf::from("FluidR3 GM.SF2"),
        // From jpo/ looking at project root
        std::path::PathBuf::from("../FluidR3 GM.SF2"),
        // Extra fallback if cwd is deeper
        std::path::PathBuf::from("../../FluidR3 GM.SF2"),
    ];

    for p in candidates {
        if !p.as_os_str().is_empty() && p.exists() {
            return Some(p);
        }
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditMode { Pencil, Eraser }

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppTab {
    Chord,
    Generate,
    Edit,
    Arrange,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GrokImportMode {
    NaturalLanguage,
    MidiFile,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ArrangeSlot {
    bank_idx: usize,
    #[serde(default = "default_arrange_repeats")]
    repeats: u8,
}

fn default_arrange_repeats() -> u8 {
    1
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NoteDragKind { None, Create, Move, Resize }

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChordDragKind { None, Move, Resize }

#[derive(Clone, Debug)]
struct ClipboardNotes {
    notes: Vec<Note>,
    min_start: f64,
    anchor_pitch: u8,
}

#[derive(Clone, Debug)]
struct ChordClipboard {
    blocks: Vec<ChordBlock>,
    anchor_start: f64,
}

struct UndoHistory {
    past: Vec<Project>,
    future: Vec<Project>,
}

impl UndoHistory {
    fn new() -> Self {
        Self {
            past: Vec::new(),
            future: Vec::new(),
        }
    }

    fn push(&mut self, proj: &Project) {
        self.past.push(proj.clone());
        if self.past.len() > 80 {
            self.past.remove(0);
        }
        self.future.clear();
    }

    fn undo(&mut self, current: &Project) -> Option<Project> {
        let prev = self.past.pop()?;
        self.future.push(current.clone());
        Some(prev)
    }

    fn redo(&mut self, current: &Project) -> Option<Project> {
        let next = self.future.pop()?;
        self.past.push(current.clone());
        Some(next)
    }

    fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
}

struct JpoApp {
    proj: Project,
    selected_ch: u8,           // 1-16
    visible_start: f64,
    visible_beats: f64,
    current_beat: f64,

    edit_mode: EditMode,
    note_len: f64,             // 0.25, 0.5, 1.0 ...
    snap_enabled: bool,

    // interaction state
    /// Stable chord selection keyed by block start beat (survives sort_by).
    active_chord_beat: Option<f64>,
    selected_note: Option<(usize, usize)>, // (track_idx, note_idx)

    // New multi-selection (priority #2 editing foundation)
    selection: Selection,

    drag_start_beat: f64,
    drag_start_pitch: u8,
    drag_orig: (f64, u8, f64), // start, pitch, dur for the dragged item
    is_creating: bool,
    note_drag_kind: NoteDragKind,
    chord_drag_kind: ChordDragKind,
    chord_drag_block_idx: Option<usize>,
    block_drag_orig: (f64, f64), // start, dur for chord block drags
    chord_timeline_mouse_beat: f64,

    // Box selection state for range tool (editing base)
    box_select_start_beat: Option<f64>,
    box_select_start_pitch: Option<u8>,

    chord_clipboard: Option<ChordClipboard>,
    chord_box_select_start: Option<f64>,

    // Generate range (user visible)
    gen_start: f64,
    gen_end: f64,

    // Real playback (wired to rustysynth + cpal per revised instruction)
    is_playing: bool,

    // Playback volume (0.0 - 1.0), applied in the audio thread
    playback_volume: f32,

    // Vertical piano-roll view (mirrors horizontal scroll + zoom)
    visible_pitch_center: f32,
    visible_pitch_span: f32,

    /// 0.0–1.0 onion highlight strength (scale pink / chord blue separately).
    scale_opacity: f32,
    chord_opacity: f32,

    // Last mouse position in beat/pitch for live overlays like box selection
    last_mouse_beat: f64,
    last_mouse_pitch: u8,

    // SoundFont location discovered at startup (used for future rustysynth init)
    soundfont_path: Option<std::path::PathBuf>,

    // === Real playback state (priority #1 in revised # JpoProducer.txt) ===
    audio_stream: Option<cpal::Stream>,
    /// Shared atomic sample counter written by the audio thread, read by UI for playhead.
    play_position_samples: Arc<AtomicU64>,
    synth_sample_rate: u32,

    // Short SF2 preview blips while editing
    preview_triggers: Arc<Mutex<Vec<PreviewTrigger>>>,
    preview_stream: Option<cpal::Stream>,
    preview_sample_rate: u32,
    last_move_preview_pitch: Option<u8>,

    // Phase A editing
    undo: UndoHistory,
    gesture_undo_saved: bool,
    clipboard: Option<ClipboardNotes>,
    default_velocity: u8,
    /// Per-note offsets during multi-note move (idx, start_delta, pitch_delta).
    drag_sel_offsets: Vec<(usize, f64, i32)>,
    project_path: Option<std::path::PathBuf>,

    // Phase B — loop sketch model
    loop_bars: u8,
    loop_playback: bool,
    loop_bank: Vec<LoopSketch>,
    active_bank_idx: usize,
    loop_name_counter: u32,

    // Pattern-based generator (JPoP_MidiTemp)
    pattern_lib: PatternLibrary,
    piano_pattern_id: String,
    bass_pattern_id: String,
    drum_pattern_id: String,
    syncopation_fill: bool,

    // Phase C — arrange
    active_tab: AppTab,
    arrange_sequence: Vec<ArrangeSlot>,

    /// Short UI feedback (copy/paste etc.)
    status_toast: Option<String>,
    status_toast_until: f64,

    /// True after interacting with the piano roll (Edit tab only).
    piano_roll_focused: bool,

    /// Grok: natural-language progression paste, or MIDI file import.
    grok_import_mode: GrokImportMode,
    grok_paste_text: String,
}

impl Default for JpoApp {
    fn default() -> Self {
        Self {
            proj: Project::default(),
            selected_ch: 4,
            visible_start: 0.0,
            visible_beats: 16.0,
            current_beat: 0.0,
            edit_mode: EditMode::Pencil,
            note_len: 0.5, // 1/8 note default (beat units)
            snap_enabled: true,
            active_chord_beat: None,
            selected_note: None,
            selection: Selection::default(),
            drag_start_beat: 0.0,
            drag_start_pitch: 60,
            drag_orig: (0.0, 60, 0.5),
            is_creating: false,
            note_drag_kind: NoteDragKind::None,
            chord_drag_kind: ChordDragKind::None,
            chord_drag_block_idx: None,
            block_drag_orig: (0.0, 0.5),
            chord_box_select_start: None,
            chord_timeline_mouse_beat: 0.0,
            is_playing: false,
            box_select_start_beat: None,
            box_select_start_pitch: None,
            playback_volume: 0.8,
            visible_pitch_center: 60.0,
            visible_pitch_span: 48.0,
            scale_opacity: 0.22,
            chord_opacity: 0.28,
            last_mouse_beat: 0.0,
            last_mouse_pitch: 60,
            chord_clipboard: None,
            gen_start: 0.0,
            gen_end: 16.0,
            soundfont_path: find_soundfont(),
            audio_stream: None,
            play_position_samples: Arc::new(AtomicU64::new(0)),
            synth_sample_rate: 44100,
            preview_triggers: Arc::new(Mutex::new(Vec::new())),
            preview_stream: None,
            preview_sample_rate: 44100,
            last_move_preview_pitch: None,
            undo: UndoHistory::new(),
            gesture_undo_saved: false,
            clipboard: None,
            default_velocity: 90,
            drag_sel_offsets: Vec::new(),
            project_path: None,
            loop_bars: 4,
            loop_playback: true,
            loop_bank: vec![LoopSketch::new_empty("Loop 1", 4)],
            active_bank_idx: 0,
            loop_name_counter: 2,
            pattern_lib: PatternLibrary::load(),
            piano_pattern_id: "Piano01".to_string(),
            bass_pattern_id: "Bass8beat01".to_string(),
            drum_pattern_id: "Drum8beat_01".to_string(),
            syncopation_fill: true,
            active_tab: AppTab::Chord,
            arrange_sequence: vec![ArrangeSlot { bank_idx: 0, repeats: 1 }],
            status_toast: None,
            status_toast_until: 0.0,
            piano_roll_focused: false,
            grok_import_mode: GrokImportMode::NaturalLanguage,
            grok_paste_text: String::new(),
        }
    }
}

impl JpoApp {
    fn track_idx(&self) -> usize { (self.selected_ch - 1) as usize }

    fn switch_tab(&mut self, tab: AppTab) {
        self.active_tab = tab;
        self.piano_roll_focused = false;
        if tab != AppTab::Edit {
            self.selected_note = None;
            self.selection.notes.clear();
        }
        if tab != AppTab::Chord {
            self.clear_chord_selection();
        }
    }

    fn apply_grok_natural_language(&mut self, ctx: &egui::Context) {
        let chords = parse_progression_text(&self.proj, &self.grok_paste_text);
        if chords.is_empty() {
            self.show_toast(ctx, "Could not parse progression — try C | Am | F | G or I-vi-IV-V");
            return;
        }
        let count = chords.len();
        self.begin_gesture_undo();
        let mut beat = self.snap_beat(self.current_beat.max(0.0));
        let dur = self.snap_dur(self.note_len);
        for (degree, quality) in chords {
            self.proj.chord_blocks.push(ChordBlock {
                start: beat,
                dur,
                degree,
                quality,
                octave: 4,
                syncopation_fill: false,
            });
            beat += dur;
        }
        self.proj.chord_blocks.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
        self.resolve_chord_overlaps();
        self.end_gesture_undo();
        self.show_toast(ctx, &format!("Placed {count} chord block(s) at playhead"));
    }

    fn import_midi_to_selected_track(&mut self, path: &std::path::Path, ctx: &egui::Context) {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                self.show_toast(ctx, &format!("Read failed: {e}"));
                return;
            }
        };
        let paste_at = self.snap_beat(self.current_beat.max(0.0));
        match parse_midi_track_notes(&data, paste_at) {
            Ok(notes) => {
                let count = notes.len();
                self.begin_gesture_undo();
                let t_idx = self.track_idx();
                if self.active_tab == AppTab::Chord {
                    // Grok chord MIDI: replace blocks in loop range with best-effort single blocks
                    self.show_toast(ctx, "Chord tab: use Natural Language for progressions; switch to Edit for MIDI parts");
                    self.end_gesture_undo();
                    return;
                }
                self.proj.tracks[t_idx].notes.extend(notes);
                self.proj.tracks[t_idx]
                    .notes
                    .sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap().then(a.pitch.cmp(&b.pitch)));
                self.end_gesture_undo();
                self.show_toast(ctx, &format!("Imported {count} notes to Ch{}", self.selected_ch));
            }
            Err(e) => self.show_toast(ctx, &e),
        }
    }

    fn show_grok_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label(egui::RichText::new("Grok import").strong());
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.grok_import_mode,
                GrokImportMode::NaturalLanguage,
                "Natural language",
            );
            ui.selectable_value(&mut self.grok_import_mode, GrokImportMode::MidiFile, "MIDI file");
        });
        match self.grok_import_mode {
            GrokImportMode::NaturalLanguage => {
                ui.label(
                    egui::RichText::new("Paste Grok response (C | Am | F | G or I-vi-IV-V)")
                        .small()
                        .weak(),
                );
                ui.add(
                    egui::TextEdit::multiline(&mut self.grok_paste_text)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal(|ui| {
                    if ui.button("Apply at playhead").clicked() {
                        self.apply_grok_natural_language(ctx);
                    }
                    if ui.button("Copy Grok prompt").clicked() {
                        let roots = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
                        let prompt = format!(
                            "J-Popコード進行提案\nKey: {} {}\nBPM: {}\nLoop: {} bars\n細かいコード割り（1/8拍単位可）で4-8小節の進行を、ローマ数字と実コード名で提案して。例: C | Am | F | G",
                            roots[self.proj.key_root as usize],
                            if self.proj.is_minor { "minor" } else { "major" },
                            self.proj.bpm as i32,
                            self.loop_bars,
                        );
                        ui.ctx().output_mut(|o| o.copied_text = prompt);
                        self.show_toast(ctx, "Prompt copied");
                    }
                });
            }
            GrokImportMode::MidiFile => {
                ui.label(
                    egui::RichText::new("Import Grok-exported .mid to selected track at playhead (Edit tab)")
                        .small()
                        .weak(),
                );
                if ui.button("Import MIDI file…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("MIDI", &["mid", "midi"])
                        .pick_file()
                    {
                        self.import_midi_to_selected_track(&path, ctx);
                    }
                }
            }
        }
    }

    fn visible_pitch_min(&self) -> u8 {
        (self.visible_pitch_center - self.visible_pitch_span / 2.0)
            .ceil()
            .clamp(0.0, 127.0) as u8
    }

    fn visible_pitch_max(&self) -> u8 {
        (self.visible_pitch_center + self.visible_pitch_span / 2.0)
            .floor()
            .clamp(0.0, 127.0) as u8
    }

    fn pitch_scroll_range(&self) -> (f32, f32) {
        let half = self.visible_pitch_span / 2.0;
        (half, (127.0 - half).max(half))
    }

    fn clamp_pitch_center(&mut self) {
        let (lo, hi) = self.pitch_scroll_range();
        self.visible_pitch_center = self.visible_pitch_center.clamp(lo, hi);
    }

    fn scroll_pitch_view(&mut self, scroll_y: f32) {
        if scroll_y.abs() < 0.01 {
            return;
        }
        // One wheel notch ≈ 1 semitone; fast flick ≈ 1 octave max per frame.
        let delta = (scroll_y / 6.0).round().clamp(-12.0, 12.0) as i32;
        if delta != 0 {
            self.visible_pitch_center =
                (self.visible_pitch_center + delta as f32).clamp(0.0, 127.0);
            self.clamp_pitch_center();
        }
    }

    fn active_chord_idx(&self) -> Option<usize> {
        let beat = self.active_chord_beat?;
        self.proj
            .chord_blocks
            .iter()
            .position(|b| (b.start - beat).abs() < 0.01)
    }

    fn set_active_chord_idx(&mut self, idx: usize) {
        if let Some(b) = self.proj.chord_blocks.get(idx) {
            self.active_chord_beat = Some(b.start);
        }
    }

    fn clear_chord_selection(&mut self) {
        self.active_chord_beat = None;
        self.selection.blocks.clear();
    }

    fn selected_chord_indices(&self) -> Vec<usize> {
        if !self.selection.blocks.is_empty() {
            let mut v: Vec<_> = self.selection.blocks.iter().copied().collect();
            v.sort_unstable();
            v
        } else if let Some(i) = self.active_chord_idx() {
            vec![i]
        } else {
            vec![]
        }
    }

    fn select_note_toggle(&mut self, track_idx: usize, note_idx: usize, shift: bool) {
        if shift {
            if self.selection.notes.contains(&(track_idx, note_idx)) {
                self.selection.notes.remove(&(track_idx, note_idx));
                if self.selected_note == Some((track_idx, note_idx)) {
                    self.selected_note = self
                        .selection
                        .notes
                        .iter()
                        .filter(|&&(t, _)| t == track_idx)
                        .min_by_key(|&&(_, i)| i)
                        .copied();
                }
            } else {
                self.selection.notes.insert((track_idx, note_idx));
                self.selected_note = Some((track_idx, note_idx));
            }
        } else {
            self.selection.notes.clear();
            self.selection.notes.insert((track_idx, note_idx));
            self.selected_note = Some((track_idx, note_idx));
        }
    }

    fn select_all_notes_in_track(&mut self, track_idx: usize) {
        self.selection.notes.clear();
        for (i, _) in self.proj.tracks[track_idx].notes.iter().enumerate() {
            self.selection.notes.insert((track_idx, i));
        }
        self.selected_note = self
            .selection
            .notes
            .iter()
            .filter(|&&(t, _)| t == track_idx)
            .min_by_key(|&&(_, i)| i)
            .copied();
    }

    fn cut_selection(&mut self) -> bool {
        if self.copy_selection() {
            self.delete_selected_notes();
            true
        } else {
            false
        }
    }

    fn nudge_selection(&mut self, beat_delta: f64, pitch_delta: i32) {
        let t_idx = self.track_idx();
        if self.selected_ch == 1 {
            return;
        }
        let indices = self.selected_note_indices(t_idx);
        if indices.is_empty() {
            return;
        }
        self.begin_gesture_undo();
        let mut updates: Vec<(usize, f64, u8)> = Vec::new();
        for i in indices {
            if let Some(n) = self.proj.tracks[t_idx].notes.get(i) {
                let start = if beat_delta.abs() > 0.0 {
                    self.snap_beat((n.start + beat_delta).max(0.0))
                } else {
                    n.start
                };
                let pitch = if pitch_delta != 0 {
                    (n.pitch as i32 + pitch_delta).clamp(0, 127) as u8
                } else {
                    n.pitch
                };
                updates.push((i, start, pitch));
            }
        }
        for (i, start, pitch) in updates {
            if let Some(n) = self.proj.tracks[t_idx].notes.get_mut(i) {
                n.start = start;
                n.pitch = pitch;
            }
        }
        self.end_gesture_undo();
    }

    fn handle_edit_shortcuts(&mut self, ctx: &egui::Context, i: &mut egui::InputState) {
        let ctrl = i.modifiers.ctrl;
        let on_piano_track = self.active_tab == AppTab::Edit && self.selected_ch != 1;

        if ctrl && i.key_pressed(egui::Key::A) && on_piano_track {
            self.select_all_notes_in_track(self.track_idx());
            i.consume_key(egui::Modifiers::CTRL, egui::Key::A);
            return;
        }

        if ctrl && i.key_pressed(egui::Key::C) {
            if self.active_tab == AppTab::Chord && self.has_chord_selection() {
                if self.copy_chord_blocks() {
                    self.show_toast(ctx, "Copied chords");
                }
            } else if on_piano_track && self.has_note_selection() {
                if self.copy_selection() {
                    self.show_toast(ctx, "Copied notes");
                } else {
                    self.show_toast(ctx, "Nothing to copy");
                }
            } else if on_piano_track {
                self.show_toast(ctx, "Select notes first (Shift+click)");
            } else if self.active_tab == AppTab::Chord {
                self.show_toast(ctx, "Select chord blocks first");
            }
            i.consume_key(egui::Modifiers::CTRL, egui::Key::C);
            return;
        }

        if ctrl && i.key_pressed(egui::Key::X) && on_piano_track {
            if self.cut_selection() {
                self.show_toast(ctx, "Cut notes");
            } else {
                self.show_toast(ctx, "Nothing to cut");
            }
            i.consume_key(egui::Modifiers::CTRL, egui::Key::X);
            return;
        }

        if ctrl && i.key_pressed(egui::Key::V) {
            if self.active_tab == AppTab::Chord && self.chord_clipboard.is_some() {
                self.paste_chord_blocks();
                self.show_toast(ctx, "Pasted chords at playhead");
            } else if on_piano_track && self.clipboard.is_some() {
                if self.paste_clipboard() {
                    self.show_toast(ctx, "Pasted notes at playhead");
                }
            } else if self.active_tab == AppTab::Chord {
                self.show_toast(ctx, "Chord clipboard empty — Ctrl+C first");
            } else if on_piano_track {
                self.show_toast(ctx, "Note clipboard empty — Ctrl+C first");
            }
            i.consume_key(egui::Modifiers::CTRL, egui::Key::V);
            return;
        }

        if ctrl && i.key_pressed(egui::Key::D) && on_piano_track {
            if self.duplicate_selection() {
                self.show_toast(ctx, "Duplicated notes");
            }
            i.consume_key(egui::Modifiers::CTRL, egui::Key::D);
            return;
        }

        if on_piano_track && self.has_note_selection() {
            let step = if i.modifiers.shift { self.note_len } else { 0.25 };
            if i.key_pressed(egui::Key::ArrowLeft) {
                self.nudge_selection(-step, 0);
            } else if i.key_pressed(egui::Key::ArrowRight) {
                self.nudge_selection(step, 0);
            } else if i.key_pressed(egui::Key::ArrowUp) {
                self.nudge_selection(0.0, 1);
            } else if i.key_pressed(egui::Key::ArrowDown) {
                self.nudge_selection(0.0, -1);
            }
        }
    }

    fn select_chord_block(&mut self, idx: usize, shift: bool) {
        if shift {
            if self.selection.blocks.contains(&idx) {
                self.selection.blocks.remove(&idx);
                if self.active_chord_idx() == Some(idx) {
                    if let Some(&next) = self.selection.blocks.iter().max() {
                        self.set_active_chord_idx(next);
                    } else {
                        self.active_chord_beat = None;
                    }
                }
            } else {
                self.selection.blocks.insert(idx);
                self.set_active_chord_idx(idx);
            }
        } else {
            self.selection.blocks.clear();
            self.selection.blocks.insert(idx);
            self.set_active_chord_idx(idx);
        }
    }

    fn set_playhead(&mut self, beat: f64) {
        let beat = self.snap_beat(beat.max(0.0));
        self.current_beat = if self.loop_playback {
            beat % self.loop_beats()
        } else {
            beat
        };
        if self.audio_stream.is_some() {
            let seek_to = self.current_beat;
            self.stop_playback();
            self.current_beat = seek_to;
            self.start_playback();
        }
    }

    fn toggle_playback(&mut self) {
        if self.audio_stream.is_some() {
            self.stop_playback();
        } else {
            self.start_playback();
        }
    }

    fn draw_playhead_line(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        start_b: f64,
        px_per_beat: f64,
    ) {
        if self.current_beat >= start_b - 0.01 && self.current_beat <= start_b + self.visible_beats + 0.1
        {
            let x = rect.min.x + ((self.current_beat - start_b) * px_per_beat) as f32;
            painter.line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(2.5, Color32::from_rgb(255, 90, 90)),
            );
            painter.circle_filled(
                Pos2::new(x, rect.min.y + 5.0),
                4.0,
                Color32::from_rgb(255, 90, 90),
            );
        }
    }

    /// Right-edge pixel zone = stretch; everything else on the block = move. No modifier key.
    /// resize_px: min(32, width*0.4), floor 16px. Slop: 12px outside right edge still counts as resize.
    fn chord_hit_at(ptr_x: f32, block_x0: f32, block_x1: f32, beat: f64, blk: &ChordBlock) -> ChordDragKind {
        if beat < blk.start || beat > blk.end() {
            return ChordDragKind::None;
        }
        let width = (block_x1 - block_x0).max(1.0);
        let resize_px = 32.0f32.min(width * 0.4).max(16.0);
        let dist_right = block_x1 - ptr_x;
        if dist_right >= -12.0 && dist_right <= resize_px {
            ChordDragKind::Resize
        } else {
            ChordDragKind::Move
        }
    }

    fn resolve_chord_overlaps(&mut self) {
        if self.proj.chord_blocks.len() < 2 {
            return;
        }
        let selected_starts: Vec<f64> = self
            .selected_chord_indices()
            .iter()
            .filter_map(|&i| self.proj.chord_blocks.get(i).map(|b| b.start))
            .collect();
        let active_start = self.active_chord_beat;
        self.proj
            .chord_blocks
            .sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
        let mut i = 0;
        while i < self.proj.chord_blocks.len() {
            let start_i = self.proj.chord_blocks[i].start;
            let end_i = self.proj.chord_blocks[i].end();
            let j = i + 1;
            while j < self.proj.chord_blocks.len() {
                let next_start = self.proj.chord_blocks[j].start;
                if next_start >= end_i - 0.001 {
                    break;
                }
                if next_start <= start_i + 0.001 {
                    self.proj.chord_blocks.remove(j);
                    continue;
                }
                let new_dur = (next_start - start_i).max(0.25);
                self.proj.chord_blocks[i].dur = new_dur;
                break;
            }
            i += 1;
        }
        self.selection.blocks.clear();
        for start in selected_starts {
            if let Some(idx) = self
                .proj
                .chord_blocks
                .iter()
                .position(|b| (b.start - start).abs() < 0.001)
            {
                self.selection.blocks.insert(idx);
            }
        }
        if let Some(start) = active_start {
            if let Some(idx) = self
                .proj
                .chord_blocks
                .iter()
                .position(|b| (b.start - start).abs() < 0.001)
            {
                self.set_active_chord_idx(idx);
            }
        } else if let Some(&last) = self.selection.blocks.iter().max() {
            self.set_active_chord_idx(last);
        }
    }

    fn has_note_selection(&self) -> bool {
        if self.selected_ch == 1 {
            return false;
        }
        !self.selected_note_indices(self.track_idx()).is_empty()
    }

    fn has_chord_selection(&self) -> bool {
        !self.selection.blocks.is_empty() || self.active_chord_beat.is_some()
    }

    fn show_toast(&mut self, ctx: &egui::Context, msg: impl Into<String>) {
        self.status_toast = Some(msg.into());
        self.status_toast_until = ctx.input(|i| i.time) + 2.0;
        ctx.request_repaint();
    }

    fn place_chord_block_at(&mut self, snapped: f64) {
        if self
            .proj
            .chord_blocks
            .iter()
            .any(|b| snapped >= b.start - 0.001 && snapped < b.end() - 0.001)
        {
            return;
        }
        self.begin_gesture_undo();
        let q = self.proj.default_quality(1).to_string();
        let new = ChordBlock {
            start: snapped,
            dur: self.note_len.max(0.5),
            degree: 1,
            quality: q,
            octave: 4,
            syncopation_fill: false,
        };
        self.proj.chord_blocks.push(new);
        self.proj.chord_blocks.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
        self.resolve_chord_overlaps();
        if let Some(idx) = self
            .proj
            .chord_blocks
            .iter()
            .position(|b| (b.start - snapped).abs() < 0.001)
        {
            self.selection.blocks.clear();
            self.selection.blocks.insert(idx);
            self.set_active_chord_idx(idx);
            let preview = self.proj.chord_blocks[idx].clone();
            self.preview_chord_block(&preview);
        }
        self.end_gesture_undo();
    }

    fn copy_chord_blocks(&mut self) -> bool {
        if self.selection.blocks.is_empty() {
            if let Some(i) = self.active_chord_idx() {
                self.selection.blocks.insert(i);
            }
        }
        let indices = self.selected_chord_indices();
        if indices.is_empty() {
            return false;
        }
        let blocks: Vec<ChordBlock> = indices
            .iter()
            .filter_map(|&i| self.proj.chord_blocks.get(i).cloned())
            .collect();
        if blocks.is_empty() {
            return false;
        }
        let anchor_start = blocks.iter().map(|b| b.start).fold(f64::INFINITY, f64::min);
        self.chord_clipboard = Some(ChordClipboard {
            blocks,
            anchor_start,
        });
        true
    }

    fn paste_chord_blocks(&mut self) {
        let Some(cb) = self.chord_clipboard.clone() else {
            return;
        };
        let paste_at = self.snap_beat(self.current_beat.max(0.0));
        let delta = paste_at - cb.anchor_start;
        self.begin_gesture_undo();
        self.selection.blocks.clear();
        for b in &cb.blocks {
            let mut new = b.clone();
            new.start = self.snap_beat((b.start + delta).max(0.0));
            self.proj.chord_blocks.push(new);
        }
        self.proj.chord_blocks.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
        self.resolve_chord_overlaps();
        for b in &cb.blocks {
            let target = self.snap_beat((b.start + delta).max(0.0));
            if let Some(idx) = self
                .proj
                .chord_blocks
                .iter()
                .position(|blk| (blk.start - target).abs() < 0.001)
            {
                self.selection.blocks.insert(idx);
            }
        }
        if let Some(&last) = self.selection.blocks.iter().max() {
            self.set_active_chord_idx(last);
            let preview = self.proj.chord_blocks[last].clone();
            self.preview_chord_block(&preview);
        }
        self.end_gesture_undo();
    }

    fn delete_selected_chords(&mut self) {
        let indices = self.selected_chord_indices();
        if indices.is_empty() {
            return;
        }
        self.begin_gesture_undo();
        let mut sorted = indices;
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        for i in sorted {
            if i < self.proj.chord_blocks.len() {
                self.proj.chord_blocks.remove(i);
            }
        }
        self.clear_chord_selection();
        self.end_gesture_undo();
    }

    fn ensure_preview_stream(&mut self) {
        if self.preview_stream.is_some() {
            return;
        }
        let Some(ref sf_path) = self.soundfont_path else {
            return;
        };
        let host = cpal::default_host();
        let Some(device) = host.default_output_device() else {
            return;
        };
        let Ok(supported) = device.default_output_config() else {
            return;
        };
        let sample_rate = supported.sample_rate().0;
        self.preview_sample_rate = sample_rate;
        let config: StreamConfig = supported.into();
        let sf = sf_path.clone();
        let triggers = Arc::clone(&self.preview_triggers);
        let mut engine = PreviewEngine::new(sf, sample_rate, triggers);
        let stream_result = device.build_output_stream(
            &config,
            move |data: &mut [f32], _| engine.render(data),
            |err| eprintln!("[Preview] stream error: {}", err),
            None,
        );
        if let Ok(stream) = stream_result {
            if stream.play().is_ok() {
                self.preview_stream = Some(stream);
            }
        }
    }

    fn preview_pitches(&mut self, track_ch: u8, patch: u8, pitches: &[u8], velocity: u8) {
        if pitches.is_empty() {
            return;
        }
        let t_idx = (track_ch.saturating_sub(1)) as usize;
        if t_idx >= self.proj.tracks.len() || !track_is_audible(&self.proj.tracks, t_idx) {
            return;
        }
        let tr = &self.proj.tracks[t_idx];
        let velocity = scaled_velocity(velocity, tr.track_vol);
        self.ensure_preview_stream();
        let synth_ch = track_ch.saturating_sub(1);
        let duration_samples =
            (self.preview_sample_rate as f64 * 0.22).max(1.0) as u32;
        let trigger = PreviewTrigger {
            synth_ch,
            program: patch,
            pitches: pitches.to_vec(),
            velocity,
            duration_samples,
            gain: self.playback_volume * 0.85,
        };
        if let Ok(mut q) = self.preview_triggers.lock() {
            q.retain(|t| t.synth_ch != synth_ch);
            q.push(trigger);
        }
    }

    fn preview_note(&mut self, track_ch: u8, patch: u8, pitch: u8, velocity: u8) {
        self.preview_pitches(track_ch, patch, &[pitch], velocity);
    }

    fn preview_chord_block(&mut self, blk: &ChordBlock) {
        let pitches = self.proj.chord_pitches(blk);
        let patch = self.proj.tracks[0].patch;
        self.preview_pitches(1, patch, &pitches, 78);
    }

    fn on_track_mix_changed(&mut self) {
        if self.audio_stream.is_some() {
            self.stop_playback();
            self.start_playback();
        }
    }

    fn finalize_box_selection(&mut self, min_b: f64, max_b: f64, min_p: u8, max_p: u8) {
        let t_idx = self.track_idx();
        for (i, n) in self.proj.tracks[t_idx].notes.iter().enumerate() {
            let in_time = n.start <= max_b && n.end() >= min_b;
            let in_pitch = n.pitch >= min_p && n.pitch <= max_p;
            if in_time && in_pitch {
                self.selection.notes.insert((t_idx, i));
            }
        }
        self.selected_note = self
            .selection
            .notes
            .iter()
            .filter(|&&(t, _)| t == t_idx)
            .min_by_key(|&&(_, i)| i)
            .copied();
    }

    const NOTE_LENS: [(&'static str, f64); 7] = [
        ("1/16", 0.25),
        ("1/12", 1.0 / 3.0),
        ("1/8", 0.5),
        ("1/4", 1.0),
        ("1/2", 2.0),
        ("1", 4.0),
        ("2", 8.0),
    ];

    fn set_len(&mut self, beats: f64) {
        self.note_len = beats;
    }

    fn loop_beats(&self) -> f64 {
        self.loop_bank
            .get(self.active_bank_idx)
            .map(LoopSketch::beats)
            .unwrap_or_else(|| self.loop_bars as f64 * 4.0)
    }

    fn set_loop_bars(&mut self, bars: u8) {
        self.loop_bars = match bars {
            4 | 8 | 16 => bars,
            _ => 8,
        };
        self.gen_start = 0.0;
        self.gen_end = self.loop_beats();
        self.fit_loop_view();
        self.sync_active_bank_from_proj();
    }

    fn fit_loop_view(&mut self) {
        self.visible_start = 0.0;
        self.visible_beats = self.loop_beats();
    }

    fn sync_active_bank_from_proj(&mut self) {
        if let Some(slot) = self.loop_bank.get_mut(self.active_bank_idx) {
            slot.loop_bars = self.loop_bars;
            slot.key_root = self.proj.key_root;
            slot.is_minor = self.proj.is_minor;
            slot.tracks = self.proj.tracks.clone();
            slot.chord_blocks = self.proj.chord_blocks.clone();
        }
    }

    fn snapshot_active_bank(&mut self) {
        if let Some(slot) = self.loop_bank.get_mut(self.active_bank_idx) {
            *slot = LoopSketch::capture_from(
                &self.proj,
                slot.name.clone(),
                self.loop_bars,
            );
        }
    }

    fn switch_loop_bank(&mut self, idx: usize) {
        if idx >= self.loop_bank.len() {
            return;
        }
        if self.audio_stream.is_some() {
            self.stop_playback();
        }
        self.snapshot_active_bank();
        self.active_bank_idx = idx;
        let slot = self.loop_bank[idx].clone();
        self.loop_bars = slot.loop_bars;
        slot.apply_to(&mut self.proj);
        self.gen_start = 0.0;
        self.gen_end = self.loop_beats();
        self.fit_loop_view();
        self.selection.notes.clear();
        self.selection.blocks.clear();
        self.selected_note = None;
        self.clear_chord_selection();
        self.gesture_undo_saved = false;
    }

    fn new_loop_bank_slot(&mut self) {
        self.snapshot_active_bank();
        let name = format!("Loop {}", self.loop_name_counter);
        self.loop_name_counter += 1;
        let slot = LoopSketch::new_empty(name, self.loop_bars);
        self.loop_bank.push(slot);
        let idx = self.loop_bank.len() - 1;
        self.switch_loop_bank(idx);
    }

    fn duplicate_loop_bank_slot(&mut self) {
        self.snapshot_active_bank();
        let src = self.loop_bank[self.active_bank_idx].clone();
        let name = format!("{} copy", src.name);
        self.loop_bank.push(src);
        let idx = self.loop_bank.len() - 1;
        self.loop_bank[idx].name = name;
        self.switch_loop_bank(idx);
    }

    fn on_proj_key_changed(&mut self) {
        self.sync_active_bank_from_proj();
    }

    fn show_loop_bank_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("LOOP BANK").strong());
        ui.label(
            egui::RichText::new(format!("{} bars • per-loop key", self.loop_bars))
                .small()
                .weak(),
        );
        let active = self.active_bank_idx;
        let mut switch_to = None;
        for (i, slot) in self.loop_bank.iter().enumerate() {
            let label = format!(
                "{}  {}  {}b",
                slot.name,
                slot.key_short_label(),
                slot.loop_bars
            );
            if ui.selectable_label(i == active, label).clicked() && i != active {
                switch_to = Some(i);
            }
        }
        if let Some(i) = switch_to {
            self.switch_loop_bank(i);
        }
        ui.horizontal(|ui| {
            if ui.button("+ New").on_hover_text("New empty loop slot").clicked() {
                self.new_loop_bank_slot();
            }
            if ui
                .button("Dup")
                .on_hover_text("Duplicate current loop")
                .clicked()
            {
                self.duplicate_loop_bank_slot();
            }
        });
        if let Some(slot) = self.loop_bank.get_mut(self.active_bank_idx) {
            ui.label("Name");
            ui.text_edit_singleline(&mut slot.name);
        }
        ui.separator();
        ui.label(
            egui::RichText::new("Toolbar Key = this loop")
                .small()
                .weak(),
        );
    }

    fn arrange_total_beats(&self) -> f64 {
        self.arrange_sequence
            .iter()
            .filter_map(|slot| {
                self.loop_bank
                    .get(slot.bank_idx)
                    .map(|sk| sk.beats() * slot.repeats.max(1) as f64)
            })
            .sum()
    }

    fn show_arrange_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("ARRANGE — sequence loops left to right, then Export Full MIDI")
                .strong(),
        );
        ui.label(
            egui::RichText::new(format!(
                "Total: {:.0} beats ({:.1} bars) • Play uses this timeline in Arrange mode",
                self.arrange_total_beats(),
                self.arrange_total_beats() / 4.0
            ))
            .small()
            .weak(),
        );

        let mut remove_idx: Option<usize> = None;
        let mut move_up: Option<usize> = None;
        let mut move_down: Option<usize> = None;
        let mut add_from_bank: Option<usize> = None;
        let bank_names: Vec<String> = self
            .loop_bank
            .iter()
            .enumerate()
            .map(|(bi, s)| format!("{bi}: {}", s.name))
            .collect();
        let seq_len = self.arrange_sequence.len();

        egui::ScrollArea::vertical()
            .max_height(280.0)
            .show(ui, |ui| {
                for (i, slot) in self.arrange_sequence.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}.", i + 1));
                        let selected = bank_names
                            .get(slot.bank_idx)
                            .cloned()
                            .unwrap_or_else(|| "?".to_string());
                        egui::ComboBox::from_id_salt(format!("arr_slot_{i}"))
                            .selected_text(selected)
                            .show_ui(ui, |ui| {
                                for (bi, name) in bank_names.iter().enumerate() {
                                    if ui.selectable_label(slot.bank_idx == bi, name).clicked() {
                                        slot.bank_idx = bi;
                                    }
                                }
                            });
                        ui.label("×");
                        ui.add(egui::DragValue::new(&mut slot.repeats).range(1..=8));
                        if ui.button("↑").clicked() && i > 0 {
                            move_up = Some(i);
                        }
                        if ui.button("↓").clicked() && i + 1 < seq_len {
                            move_down = Some(i);
                        }
                        if ui.button("✕").clicked() {
                            remove_idx = Some(i);
                        }
                    });
                }
            });

        ui.horizontal(|ui| {
            if ui.button("+ Add slot").clicked() {
                self.arrange_sequence.push(ArrangeSlot {
                    bank_idx: self.active_bank_idx,
                    repeats: 1,
                });
            }
            if ui.button("+ From bank").clicked() {
                add_from_bank = Some(self.active_bank_idx);
            }
            if ui.button("Export Full MIDI…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("arrangement.mid")
                    .add_filter("MIDI", &["mid"])
                    .save_file()
                {
                    if let Err(e) = self.export_arrange_midi(&path.to_string_lossy()) {
                        eprintln!("Arrange MIDI export error: {}", e);
                    }
                }
            }
            if ui.button("王道 I-V-vi-IV (8b)").clicked() {
                self.apply_chord_progression_odori();
            }
        });

        if let Some(i) = remove_idx {
            if self.arrange_sequence.len() > 1 {
                self.arrange_sequence.remove(i);
            }
        }
        if let Some(i) = move_up {
            self.arrange_sequence.swap(i, i - 1);
        }
        if let Some(i) = move_down {
            self.arrange_sequence.swap(i, i + 1);
        }
        if let Some(bi) = add_from_bank {
            self.arrange_sequence.push(ArrangeSlot {
                bank_idx: bi,
                repeats: 1,
            });
        }

        ui.add_space(8.0);
        ui.label("Switch to Sketch to edit the active loop slot.");
    }

    fn apply_chord_progression_odori(&mut self) {
        self.begin_gesture_undo();
        let bar = 4.0;
        self.proj.chord_blocks = vec![
            ChordBlock { start: 0.0, dur: bar, degree: 1, quality: "".into(), octave: 4, syncopation_fill: false },
            ChordBlock { start: bar, dur: bar, degree: 5, quality: "".into(), octave: 4, syncopation_fill: false },
            ChordBlock { start: bar * 2.0, dur: bar, degree: 6, quality: "m".into(), octave: 4, syncopation_fill: false },
            ChordBlock { start: bar * 3.0, dur: bar, degree: 4, quality: "".into(), octave: 4, syncopation_fill: false },
        ];
        self.end_gesture_undo();
        self.sync_active_bank_from_proj();
    }

    fn export_arrange_midi(&self, path: &str) -> Result<(), String> {
        let header = midly::Header::new(Format::Parallel, midly::Timing::Metrical(PPQ.into()));
        let mut smf = Smf::new(header);

        let mut tempo = Track::new();
        let tempo_us = (60_000_000.0 / self.proj.bpm) as u32;
        tempo.push(TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Meta(MetaMessage::Tempo(tempo_us.into())),
        });
        tempo.push(TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });
        smf.tracks.push(tempo);

        let mut beat_offset = 0.0f64;
        let mut track_notes: Vec<Vec<Note>> = vec![vec![]; 16];

        for slot in &self.arrange_sequence {
            let Some(sketch) = self.loop_bank.get(slot.bank_idx) else {
                continue;
            };
            for _ in 0..slot.repeats.max(1) {
                let chords = Self::expand_sketch_chords(sketch);
                for n in chords {
                    track_notes[0].push(Note {
                        start: beat_offset + n.start,
                        pitch: n.pitch,
                        dur: n.dur,
                        vel: n.vel,
                    });
                }
                for tr in sketch.tracks.iter().skip(1) {
                    let ti = (tr.ch.saturating_sub(1)) as usize;
                    for n in &tr.notes {
                        track_notes[ti].push(Note {
                            start: beat_offset + n.start,
                            pitch: n.pitch,
                            dur: n.dur,
                            vel: n.vel,
                        });
                    }
                }
                beat_offset += sketch.beats();
            }
        }

        for (ti, notes) in track_notes.iter().enumerate() {
            if notes.is_empty() {
                continue;
            }
            let ch = (ti + 1) as u8;
            let patch = self.proj.tracks[ti].patch;
            let mut track = Track::new();
            if ch != 10 {
                track.push(TrackEvent {
                    delta: 0.into(),
                    kind: TrackEventKind::Midi {
                        channel: (ch - 1).into(),
                        message: MidiMessage::ProgramChange { program: patch.into() },
                    },
                });
            }
            let mut events: Vec<(u32, bool, u8, u8)> = vec![];
            for n in notes {
                let on_tick = (n.start * PPQ as f64) as u32;
                let off_tick = (n.end() * PPQ as f64) as u32;
                events.push((on_tick, true, n.pitch, n.vel));
                events.push((off_tick, false, n.pitch, 0));
            }
            events.sort_by_key(|e| e.0);
            let mut prev = 0u32;
            for (tick, on, pitch, vel) in events {
                let delta = tick - prev;
                let msg = if on {
                    MidiMessage::NoteOn {
                        key: pitch.into(),
                        vel: vel.into(),
                    }
                } else {
                    MidiMessage::NoteOff {
                        key: pitch.into(),
                        vel: 0.into(),
                    }
                };
                track.push(TrackEvent {
                    delta: delta.into(),
                    kind: TrackEventKind::Midi {
                        channel: (ch - 1).into(),
                        message: msg,
                    },
                });
                prev = tick;
            }
            track.push(TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
            });
            smf.tracks.push(track);
        }

        let mut f = File::create(path).map_err(|e| e.to_string())?;
        smf.write(&mut midly::io::IoWrap(&mut f))
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn draw_loop_boundaries(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        start_b: f64,
        px_per_beat: f64,
        full_height: bool,
    ) {
        let loop_end = self.loop_beats();
        let y0 = if full_height { rect.min.y } else { rect.min.y + 4.0 };
        let y1 = if full_height { rect.max.y } else { rect.max.y - 4.0 };

        if start_b <= 0.0 && 0.0 <= start_b + self.visible_beats {
            let x = rect.min.x + ((0.0 - start_b) * px_per_beat) as f32;
            painter.line_segment(
                [Pos2::new(x, y0), Pos2::new(x, y1)],
                Stroke::new(3.0, Color32::from_rgb(255, 170, 70)),
            );
        }

        if start_b <= loop_end && loop_end <= start_b + self.visible_beats {
            let x = rect.min.x + ((loop_end - start_b) * px_per_beat) as f32;
            painter.line_segment(
                [Pos2::new(x, y0), Pos2::new(x, y1)],
                Stroke::new(3.0, Color32::from_rgb(255, 120, 60)),
            );
        }

        let x_loop_end = rect.min.x + ((loop_end - start_b) * px_per_beat) as f32;
        if x_loop_end < rect.max.x {
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(x_loop_end, rect.min.y),
                    Pos2::new(rect.max.x, rect.max.y),
                ),
                0.0,
                Color32::from_rgba_unmultiplied(0, 0, 0, 50),
            );
        }
    }

    fn note_len_label(&self) -> &'static str {
        for (label, val) in Self::NOTE_LENS {
            if (self.note_len - val).abs() < 0.01 {
                return label;
            }
        }
        "?"
    }

    fn edit_tool_label(&self) -> &'static str {
        match self.edit_mode {
            EditMode::Pencil => "Pencil",
            EditMode::Eraser => "Eraser",
        }
    }

    /// Compact overflow menu: edit extras, velocity, undo, file I/O, Grok.
    fn show_tools_menu(&mut self, ui: &mut egui::Ui, roots: &[&str]) {
        let menu_title = format!("{} ▾", self.edit_tool_label());
        ui.menu_button(menu_title, |ui| {
            ui.label(egui::RichText::new("Edit tool").strong());
            if ui.selectable_label(self.edit_mode == EditMode::Pencil, "Pencil").clicked() {
                self.edit_mode = EditMode::Pencil;
                ui.close_menu();
            }
            if ui.selectable_label(self.edit_mode == EditMode::Eraser, "Eraser").clicked() {
                self.edit_mode = EditMode::Eraser;
                ui.close_menu();
            }

            ui.separator();
            ui.label(egui::RichText::new("Note length").strong());
            for (label, val) in Self::NOTE_LENS {
                if ui.selectable_label((self.note_len - val).abs() < 0.01, label).clicked() {
                    self.set_len(val);
                }
            }

            ui.separator();
            let snap_label = if self.snap_enabled { "✓ Snap to grid" } else { "Snap to grid (off)" };
            if ui.selectable_label(self.snap_enabled, snap_label).clicked() {
                self.snap_enabled = !self.snap_enabled;
            }

            ui.separator();
            ui.label(egui::RichText::new("Velocity").strong());
            ui.horizontal(|ui| {
                ui.label("Default");
                ui.add(egui::DragValue::new(&mut self.default_velocity).speed(1).range(1..=127));
            });
            let sel_count = self.selected_note_indices(self.track_idx()).len();
            if sel_count > 0 && self.selected_ch != 1 {
                let mut v = self.default_velocity;
                ui.horizontal(|ui| {
                    ui.label(format!("Selection ({sel_count})"));
                    if ui.add(egui::DragValue::new(&mut v).speed(1).range(1..=127)).changed() {
                        self.apply_velocity_to_selection(v);
                        self.default_velocity = v;
                    }
                });
            }

            ui.separator();
            ui.label(egui::RichText::new("Edit").strong());
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(self.undo.can_undo(), egui::Button::new("Undo"))
                    .on_hover_text("Ctrl+Z")
                    .clicked()
                {
                    self.do_undo();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(self.undo.can_redo(), egui::Button::new("Redo"))
                    .on_hover_text("Ctrl+Y")
                    .clicked()
                {
                    self.do_redo();
                    ui.close_menu();
                }
            });
            if ui.button("Quantize selection").on_hover_text("Snap selected notes to grid").clicked() {
                self.quantize_selection();
            }

            ui.separator();
            ui.label(egui::RichText::new("Project").strong());
            if ui.button("Save…").clicked() {
                let default_name = self
                    .project_path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("sketch.jpo");
                let mut dlg = rfd::FileDialog::new()
                    .set_file_name(default_name)
                    .add_filter("JpoProducer", &["jpo"]);
                if let Some(ref p) = self.project_path {
                    if let Some(parent) = p.parent() {
                        dlg = dlg.set_directory(parent);
                    }
                }
                if let Some(path) = dlg.save_file() {
                    if let Err(e) = self.save_project(&path) {
                        eprintln!("Save error: {}", e);
                    }
                }
                ui.close_menu();
            }
            if ui.button("Load…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JpoProducer", &["jpo"])
                    .pick_file()
                {
                    if let Err(e) = self.load_project(&path) {
                        eprintln!("Load error: {}", e);
                    }
                }
                ui.close_menu();
            }
            if ui.button("Export MIDI…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("sketch.mid")
                    .add_filter("MIDI", &["mid"])
                    .save_file()
                {
                    if let Err(e) = self.export_midi(&path.to_string_lossy()) {
                        eprintln!("MIDI export error: {}", e);
                    }
                }
                ui.close_menu();
            }

            ui.separator();
            if ui.button("Grok (ideas) — copy prompt").clicked() {
                let prompt = format!(
                    "J-Pop/J-Rockコード進行提案\nKey: {} {}\nBPM: {}\n現在のブロック数: {}\n範囲: {:.1}-{:.1}\nこの部分の良い進行をローマ数字と実際のコード名で4-8小節提案して",
                    roots[self.proj.key_root as usize],
                    if self.proj.is_minor { "minor" } else { "major" },
                    self.proj.bpm as i32,
                    self.proj.chord_blocks.len(),
                    self.gen_start,
                    self.gen_end
                );
                ui.ctx().output_mut(|o| o.copied_text = prompt);
                ui.close_menu();
            }

            ui.separator();
            ui.label(
                egui::RichText::new("click empty = place • Shift+click multi • Shift+drag box • Ctrl+C/V/X/D")
                    .small()
                    .weak(),
            );
        });
    }

    fn snap_beat(&self, beat: f64) -> f64 {
        if !self.snap_enabled {
            return beat.max(0.0);
        }
        let step = self.note_len.max(0.0625);
        (beat / step).round() * step
    }

    fn snap_dur(&self, dur: f64) -> f64 {
        let min = 0.0625;
        if !self.snap_enabled {
            return dur.max(min);
        }
        let step = self.note_len.max(min);
        ((dur / step).round() * step).max(step)
    }

    fn begin_gesture_undo(&mut self) {
        if !self.gesture_undo_saved {
            self.undo.push(&self.proj);
            self.gesture_undo_saved = true;
        }
    }

    fn end_gesture_undo(&mut self) {
        self.gesture_undo_saved = false;
        self.sync_active_bank_from_proj();
    }

    fn do_undo(&mut self) {
        if let Some(prev) = self.undo.undo(&self.proj) {
            self.proj = prev;
            self.selection.notes.clear();
            self.selection.blocks.clear();
            self.selected_note = None;
            self.clear_chord_selection();
            self.gesture_undo_saved = false;
        }
    }

    fn do_redo(&mut self) {
        if let Some(next) = self.undo.redo(&self.proj) {
            self.proj = next;
            self.selection.notes.clear();
            self.selection.blocks.clear();
            self.selected_note = None;
            self.clear_chord_selection();
            self.gesture_undo_saved = false;
        }
    }

    fn selected_note_indices(&self, track_idx: usize) -> Vec<usize> {
        if !self.selection.notes.is_empty() {
            self.selection
                .notes
                .iter()
                .filter(|&&(t, _)| t == track_idx)
                .map(|&(_, i)| i)
                .collect()
        } else if let Some((t, i)) = self.selected_note {
            if t == track_idx {
                vec![i]
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    }

    fn copy_selection(&mut self) -> bool {
        let t_idx = self.track_idx();
        if self.selected_ch == 1 {
            return false;
        }
        let indices = self.selected_note_indices(t_idx);
        if indices.is_empty() {
            return false;
        }
        let mut notes = Vec::new();
        let mut min_start = f64::MAX;
        let mut anchor_pitch = 60u8;
        for &i in &indices {
            if let Some(n) = self.proj.tracks[t_idx].notes.get(i) {
                min_start = min_start.min(n.start);
                anchor_pitch = n.pitch;
                notes.push(*n);
            }
        }
        if notes.is_empty() {
            return false;
        }
        self.clipboard = Some(ClipboardNotes {
            notes,
            min_start,
            anchor_pitch,
        });
        true
    }

    fn paste_clipboard(&mut self) -> bool {
        let Some(cb) = self.clipboard.clone() else {
            return false;
        };
        if self.selected_ch == 1 {
            return false;
        }
        let t_idx = self.track_idx();
        self.begin_gesture_undo();
        let paste_at = self.snap_beat(self.current_beat.max(0.0));
        let pitch_base = self.last_mouse_pitch;
        let pitch_shift = pitch_base as i32 - cb.anchor_pitch as i32;
        self.selection.notes.clear();
        for n in &cb.notes {
            let rel = n.start - cb.min_start;
            let new_n = Note {
                start: self.snap_beat(paste_at + rel),
                pitch: (n.pitch as i32 + pitch_shift).clamp(0, 127) as u8,
                dur: n.dur,
                vel: n.vel,
            };
            self.proj.tracks[t_idx].notes.push(new_n);
        }
        self.proj.tracks[t_idx].notes.sort_by(|a, b| {
            a.start
                .partial_cmp(&b.start)
                .unwrap()
                .then(a.pitch.cmp(&b.pitch))
        });
        let start_len = self.proj.tracks[t_idx].notes.len() - cb.notes.len();
        for i in start_len..self.proj.tracks[t_idx].notes.len() {
            self.selection.notes.insert((t_idx, i));
        }
        self.selected_note = self.selection.notes.iter().find(|&&(t, _)| t == t_idx).copied();
        self.end_gesture_undo();
        true
    }

    fn duplicate_selection(&mut self) -> bool {
        if self.copy_selection() {
            self.paste_clipboard()
        } else {
            false
        }
    }

    fn quantize_selection(&mut self) {
        let t_idx = self.track_idx();
        if self.selected_ch == 1 {
            return;
        }
        let indices = self.selected_note_indices(t_idx);
        if indices.is_empty() {
            return;
        }
        self.begin_gesture_undo();
        for i in indices {
            if let Some(n) = self.proj.tracks[t_idx].notes.get(i) {
                let snapped_start = self.snap_beat(n.start);
                let snapped_dur = self.snap_dur(n.dur);
                if let Some(nn) = self.proj.tracks[t_idx].notes.get_mut(i) {
                    nn.start = snapped_start;
                    nn.dur = snapped_dur;
                }
            }
        }
        self.end_gesture_undo();
    }

    fn apply_velocity_to_selection(&mut self, vel: u8) {
        let t_idx = self.track_idx();
        let indices = self.selected_note_indices(t_idx);
        if indices.is_empty() {
            return;
        }
        self.begin_gesture_undo();
        for i in indices {
            if let Some(n) = self.proj.tracks[t_idx].notes.get_mut(i) {
                n.vel = vel;
            }
        }
        self.end_gesture_undo();
    }

    fn note_fill_color(&self, vel: u8, selected: bool, is_ch1: bool) -> Color32 {
        let t = (vel as f32 / 127.0).clamp(0.2, 1.0);
        if selected {
            Color32::from_rgb(
                (50.0 + 205.0 * t) as u8,
                (220.0 + 35.0 * t) as u8,
                (120.0 + 55.0 * t) as u8,
            )
        } else if is_ch1 {
            Color32::from_rgb(
                (30.0 + 175.0 * t) as u8,
                (150.0 + 60.0 * t) as u8,
                (100.0 + 45.0 * t) as u8,
            )
        } else {
            Color32::from_rgb(
                (20.0 + 165.0 * t) as u8,
                (140.0 + 55.0 * t) as u8,
                (90.0 + 40.0 * t) as u8,
            )
        }
    }

    fn save_project(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.snapshot_active_bank();
        #[derive(Serialize)]
        struct JpoFile<'a> {
            format: &'a str,
            version: u32,
            project: &'a Project,
            loop_bars: u8,
            loop_playback: bool,
            loop_bank: &'a [LoopSketch],
            active_bank_idx: usize,
        }
        let file = JpoFile {
            format: "jpo",
            version: 4,
            project: &self.proj,
            loop_bars: self.loop_bars,
            loop_playback: self.loop_playback,
            loop_bank: &self.loop_bank,
            active_bank_idx: self.active_bank_idx,
        };
        let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())?;
        self.project_path = Some(path.to_path_buf());
        Ok(())
    }

    fn load_project(&mut self, path: &std::path::Path) -> Result<(), String> {
        #[derive(Deserialize)]
        struct JpoFileV2 {
            #[serde(default)]
            version: u32,
            project: Project,
            #[serde(default = "default_loop_bars")]
            loop_bars: u8,
            #[serde(default = "default_loop_playback")]
            loop_playback: bool,
            #[serde(default)]
            loop_bank: Vec<LoopSketch>,
            #[serde(default)]
            active_bank_idx: usize,
        }
        fn default_loop_bars() -> u8 {
            4
        }
        fn default_loop_playback() -> bool {
            true
        }
        let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let file: JpoFileV2 = serde_json::from_str(&data).map_err(|e| e.to_string())?;
        self.undo.push(&self.proj);
        self.proj = file.project;
        self.loop_bars = match file.loop_bars {
            4 | 8 | 16 => file.loop_bars,
            _ => 8,
        };
        self.loop_playback = file.loop_playback;
        if file.loop_bank.is_empty() {
            if file.version < 2 {
                eprintln!("[Load] Upgrading v1 project to loop bank");
            }
            self.loop_bank = vec![LoopSketch::capture_from(
                &self.proj,
                "Loop 1".to_string(),
                self.loop_bars,
            )];
            self.active_bank_idx = 0;
        } else {
            self.loop_bank = file.loop_bank;
            self.active_bank_idx = file.active_bank_idx.min(self.loop_bank.len().saturating_sub(1));
        }
        self.loop_name_counter = (self.loop_bank.len() as u32).saturating_add(1);
        self.gen_start = 0.0;
        self.gen_end = self.loop_beats();
        self.fit_loop_view();
        self.selection.notes.clear();
        self.selection.blocks.clear();
        self.selected_note = None;
        self.clear_chord_selection();
        self.project_path = Some(path.to_path_buf());
        self.gesture_undo_saved = false;
        Ok(())
    }

    /// Click empty grid = place note at Len (same UX as chord blocks).
    fn place_piano_note_at(&mut self, track_idx: usize, beat: f64, pitch: u8) {
        self.begin_gesture_undo();
        let new_n = Note {
            start: self.snap_beat(beat),
            pitch: pitch.clamp(0, 127),
            dur: self.note_len,
            vel: self.default_velocity,
        };
        self.proj.tracks[track_idx].notes.push(new_n);
        self.proj.tracks[track_idx].notes.sort_by(|a, b| {
            a.start
                .partial_cmp(&b.start)
                .unwrap()
                .then(a.pitch.cmp(&b.pitch))
        });
        let new_idx = self.proj.tracks[track_idx].notes.len() - 1;
        self.selection.notes.clear();
        self.selection.notes.insert((track_idx, new_idx));
        self.selected_note = Some((track_idx, new_idx));
        self.note_drag_kind = NoteDragKind::None;
        self.end_gesture_undo();
        self.preview_note(
            self.selected_ch,
            self.proj.tracks[track_idx].patch,
            new_n.pitch,
            new_n.vel,
        );
    }

    fn delete_selected_notes(&mut self) {
        self.begin_gesture_undo();
        if !self.selection.notes.is_empty() {
            let mut by_track: std::collections::HashMap<usize, Vec<usize>> =
                std::collections::HashMap::new();
            for &(t, i) in &self.selection.notes {
                by_track.entry(t).or_default().push(i);
            }
            for (t, mut indices) in by_track {
                indices.sort_unstable_by(|a, b| b.cmp(a));
                for i in indices {
                    if t < self.proj.tracks.len() && i < self.proj.tracks[t].notes.len() {
                        self.proj.tracks[t].notes.remove(i);
                    }
                }
            }
            self.selection.notes.clear();
            self.selected_note = None;
        } else if let Some((t, i)) = self.selected_note {
            if t < self.proj.tracks.len()
                && i < self.proj.tracks[t].notes.len()
                && self.proj.tracks[t].ch != 1
            {
                self.proj.tracks[t].notes.remove(i);
                self.selected_note = None;
            }
        }
        self.end_gesture_undo();
    }

    fn export_midi(&self, path: &str) -> Result<(), String> {
        let header = midly::Header::new(Format::Parallel, midly::Timing::Metrical(PPQ.into()));
        let mut smf = Smf::new(header);

        // tempo track
        let mut tempo = Track::new();
        let tempo_us = (60_000_000.0 / self.proj.bpm) as u32;
        tempo.push(TrackEvent { delta: 0.into(), kind: TrackEventKind::Meta(MetaMessage::Tempo(tempo_us.into())) });
        tempo.push(TrackEvent { delta: 0.into(), kind: TrackEventKind::Meta(MetaMessage::EndOfTrack) });
        smf.tracks.push(tempo);

        for (ti, tr) in self.proj.tracks.iter().enumerate() {
            let mut track = Track::new();
            if tr.ch != 10 {
                track.push(TrackEvent { delta: 0.into(), kind: TrackEventKind::Midi {
                    channel: (tr.ch - 1).into(),
                    message: MidiMessage::ProgramChange { program: tr.patch.into() },
                }});
            }

            let mut events: Vec<(u32, bool, u8, u8)> = vec![]; // tick, on, pitch, vel

            let notes = if ti == 0 {
                expand_chords(&self.proj)
            } else {
                tr.notes.clone()
            };

            for n in &notes {
                let on_tick = (n.start * PPQ as f64) as u32;
                let off_tick = (n.end() * PPQ as f64) as u32;
                events.push((on_tick, true, n.pitch, n.vel));
                events.push((off_tick, false, n.pitch, 0));
            }
            events.sort_by_key(|e| e.0);

            let mut prev = 0u32;
            for (tick, on, pitch, vel) in events {
                let delta = tick - prev;
                let msg = if on {
                    MidiMessage::NoteOn { key: pitch.into(), vel: vel.into() }
                } else {
                    MidiMessage::NoteOff { key: pitch.into(), vel: 0.into() }
                };
                track.push(TrackEvent {
                    delta: delta.into(),
                    kind: TrackEventKind::Midi { channel: (tr.ch-1).into(), message: msg },
                });
                prev = tick;
            }
            track.push(TrackEvent { delta: 0.into(), kind: TrackEventKind::Meta(MetaMessage::EndOfTrack) });
            smf.tracks.push(track);
        }

        let mut f = File::create(path).map_err(|e| e.to_string())?;
        smf.write(&mut midly::io::IoWrap(&mut f)).map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_title("JpoProducer (Rust+egui) - J-Pop/J-Rock Sketch Tool"),
        ..Default::default()
    };

    eframe::run_native(
        "JpoProducer",
        options,
        Box::new(|cc| {
            // Try to load a decent Japanese font
            let mut fonts = egui::FontDefinitions::default();
            // Attempt common Windows CJK fonts (user can also drop a .ttf next to the exe)
            let font_paths = [
                "C:/Windows/Fonts/meiryo.ttc",
                "C:/Windows/Fonts/msgothic.ttc",
                "C:/Windows/Fonts/YuGothM.ttc",
            ];
            for p in font_paths {
                if let Ok(bytes) = std::fs::read(p) {
                    fonts.font_data.insert("jp".to_owned(), egui::FontData::from_owned(bytes).into());
                    if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                        fam.insert(0, "jp".to_owned());
                    }
                    println!("Loaded Japanese font: {}", p);
                    break;
                }
            }
            cc.egui_ctx.set_fonts(fonts);

            // Report SF2 discovery at startup (helps follow the revised instruction)
            match find_soundfont() {
                Some(p) => println!("[SF2] Found: {}", p.display()),
                None => println!("[SF2] NOT FOUND. Per # JpoProducer.txt: copy FluidR3 GM.SF2 next to the exe (or place a copy inside jpo/ for `cargo run`)."),
            }

            // Dark theme
            let mut style = (*cc.egui_ctx.style()).clone();
            style.visuals.dark_mode = true;
            style.visuals.panel_fill = Color32::from_rgb(22, 22, 26);
            style.visuals.window_fill = Color32::from_rgb(26, 26, 30);
            cc.egui_ctx.set_style(style);

            Ok(Box::new(JpoApp::default()))
        }),
    )
}

impl eframe::App for JpoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let wants_kb = ctx.wants_keyboard_input();
        if !wants_kb {
            ctx.input(|i| {
                if i.key_pressed(egui::Key::Space) {
                    self.toggle_playback();
                }
            });
        }
        ctx.input_mut(|i| {
            if i.key_pressed(egui::Key::Delete) {
                if self.active_tab == AppTab::Chord && !self.selected_chord_indices().is_empty() {
                    self.delete_selected_chords();
                } else if self.active_tab == AppTab::Edit {
                    self.delete_selected_notes();
                }
            }
            let ctrl = i.modifiers.ctrl;
            if ctrl && i.key_pressed(egui::Key::Z) && !i.modifiers.shift {
                self.do_undo();
                i.consume_key(egui::Modifiers::CTRL, egui::Key::Z);
            }
            if ctrl && i.key_pressed(egui::Key::Y) {
                self.do_redo();
                i.consume_key(egui::Modifiers::CTRL, egui::Key::Y);
            }
            if ctrl && i.modifiers.shift && i.key_pressed(egui::Key::Z) {
                self.do_redo();
                i.consume_key(egui::Modifiers::CTRL.plus(egui::Modifiers::SHIFT), egui::Key::Z);
            }
            self.handle_edit_shortcuts(ctx, i);
        });

        if let Some(ref msg) = self.status_toast {
            let now = ctx.input(|i| i.time);
            if now < self.status_toast_until {
                egui::TopBottomPanel::bottom("status_toast")
                    .exact_height(22.0)
                    .show(ctx, |ui| {
                        ui.horizontal_centered(|ui| {
                            ui.label(egui::RichText::new(msg).size(12.0).color(Color32::from_rgb(140, 200, 255)));
                        });
                    });
            } else {
                self.status_toast = None;
            }
        }

        // Top toolbar — compact: play always visible; extras live in Tools menu
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            let roots = ["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"];
            ui.horizontal(|ui| {
                for (tab, label) in [
                    (AppTab::Chord, "1 Chord"),
                    (AppTab::Generate, "2 Generate"),
                    (AppTab::Edit, "3 Edit"),
                    (AppTab::Arrange, "4 Arrange"),
                ] {
                    if ui
                        .selectable_label(self.active_tab == tab, label)
                        .clicked()
                    {
                        self.switch_tab(tab);
                    }
                }

                ui.separator();
                ui.label("BPM");
                ui.add(egui::DragValue::new(&mut self.proj.bpm).speed(1.0).range(40.0..=240.0));

                ui.separator();
                ui.label("Key");
                egui::ComboBox::from_id_salt("toolbar_key")
                    .selected_text(roots[self.proj.key_root as usize])
                    .width(52.0)
                    .show_ui(ui, |ui| {
                        for (i, r) in roots.iter().enumerate() {
                            if ui.selectable_label(self.proj.key_root as usize == i, *r).clicked() {
                                self.proj.key_root = i as u8;
                                self.on_proj_key_changed();
                            }
                        }
                    });
                let mode_label = if self.proj.is_minor { "Minor" } else { "Major" };
                if ui.button(mode_label).clicked() {
                    self.proj.is_minor = !self.proj.is_minor;
                    self.on_proj_key_changed();
                }

                ui.separator();
                self.show_tools_menu(ui, &roots);

                ui.separator();
                ui.label("Len");
                egui::ComboBox::from_id_salt("toolbar_note_len")
                    .selected_text(self.note_len_label())
                    .width(48.0)
                    .show_ui(ui, |ui| {
                        for (label, val) in Self::NOTE_LENS {
                            if ui.selectable_label((self.note_len - val).abs() < 0.01, label).clicked() {
                                self.set_len(val);
                            }
                        }
                    });

                ui.separator();
                let snap_label = if self.snap_enabled { "Snap" } else { "Snap off" };
                if ui
                    .selectable_label(self.snap_enabled, snap_label)
                    .on_hover_text("Toggle grid snap (also in Tools menu)")
                    .clicked()
                {
                    self.snap_enabled = !self.snap_enabled;
                }

                ui.separator();
                ui.label("Vol");
                ui.add(
                    egui::Slider::new(&mut self.playback_volume, 0.0..=1.0)
                        .step_by(0.05)
                        .show_value(false),
                );

                ui.separator();
                let play_label = if self.audio_stream.is_some() { "■ Stop" } else { "▶ Play" };
                if ui
                    .add(egui::Button::new(play_label).min_size(egui::vec2(72.0, 0.0)))
                    .on_hover_text("Space")
                    .clicked()
                {
                    self.toggle_playback();
                }
                ui.monospace(format!("▸ {:.2}", self.current_beat));
            });
        });

        // Bottom controls — tab-specific (reduces input overlap)
        egui::TopBottomPanel::bottom("controls").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Loop").strong());
                for bars in [4u8, 8, 16] {
                    let label = format!("{bars} bars");
                    if ui.selectable_label(self.loop_bars == bars, label).clicked() {
                        self.set_loop_bars(bars);
                    }
                }
                let loop_label = if self.loop_playback { "🔁 Loop" } else { "Play once" };
                if ui
                    .selectable_label(self.loop_playback, loop_label)
                    .on_hover_text("Loop playback within loop region")
                    .clicked()
                {
                    self.loop_playback = !self.loop_playback;
                }
                if ui.button("Fit loop").clicked() {
                    self.fit_loop_view();
                }
            });

            ui.horizontal(|ui| {
                ui.label("Zoom");
                let max_visible = self.loop_beats().max(8.0);
                ui.add(egui::Slider::new(&mut self.visible_beats, 4.0..=max_visible));
                ui.label("Scroll");
                let max_scroll = (self.loop_beats() - 4.0).max(0.0);
                ui.add(egui::Slider::new(&mut self.visible_start, 0.0..=max_scroll));
            });

            match self.active_tab {
                AppTab::Chord => {
                    self.show_grok_panel(ui, ctx);
                    ui.label("Tip: Tab1 — place chords (Len 1/8 default) • Shift+multi • Ctrl+C/V • Space=play");
                }
                AppTab::Generate => {
                    ui.horizontal(|ui| {
                        ui.label("Generate range (beats)");
                        ui.add(egui::DragValue::new(&mut self.gen_start).speed(0.5).range(0.0..=128.0));
                        ui.label("→");
                        ui.add(egui::DragValue::new(&mut self.gen_end).speed(0.5).range(0.0..=128.0));
                        if ui.button("Gen=Loop").clicked() {
                            self.gen_start = 0.0;
                            self.gen_end = self.loop_beats();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Piano");
                        egui::ComboBox::from_id_salt("gen_piano_pat")
                            .selected_text(&self.piano_pattern_id)
                            .width(88.0)
                            .show_ui(ui, |ui| {
                                for id in self.pattern_lib.ids_for(PatternCategory::Piano, false) {
                                    if ui.selectable_label(self.piano_pattern_id == id, id).clicked() {
                                        self.piano_pattern_id = id.to_string();
                                    }
                                }
                            });
                        ui.label("Bass");
                        egui::ComboBox::from_id_salt("gen_bass_pat")
                            .selected_text(&self.bass_pattern_id)
                            .width(96.0)
                            .show_ui(ui, |ui| {
                                for id in self.pattern_lib.ids_for(PatternCategory::Bass, false) {
                                    if ui.selectable_label(self.bass_pattern_id == id, id).clicked() {
                                        self.bass_pattern_id = id.to_string();
                                    }
                                }
                            });
                        ui.label("Drum");
                        egui::ComboBox::from_id_salt("gen_drum_pat")
                            .selected_text(&self.drum_pattern_id)
                            .width(108.0)
                            .show_ui(ui, |ui| {
                                for id in self.pattern_lib.ids_for(PatternCategory::Drum, false) {
                                    if ui.selectable_label(self.drum_pattern_id == id, id).clicked() {
                                        self.drum_pattern_id = id.to_string();
                                    }
                                }
                            });
                        ui.checkbox(&mut self.syncopation_fill, "Syncopation fill");
                        if ui.button("Generate All").clicked() {
                            self.begin_gesture_undo();
                            let s = self.gen_start;
                            let e = self.gen_end.max(s + 0.25);
                            let (p, b, d) = generate_from_patterns(
                                &self.pattern_lib,
                                &self.proj,
                                s,
                                e,
                                &self.piano_pattern_id,
                                &self.bass_pattern_id,
                                &self.drum_pattern_id,
                                self.syncopation_fill,
                            );
                            replace_notes_in_range(&mut self.proj.tracks[1].notes, s, e, p);
                            replace_notes_in_range(&mut self.proj.tracks[2].notes, s, e, b);
                            replace_notes_in_range(&mut self.proj.tracks[9].notes, s, e, d);
                            self.end_gesture_undo();
                            self.show_toast(ctx, "Generated Ch2/3/10");
                        }
                        if ui.button("Clear Ch2,3,10").clicked() {
                            self.proj.tracks[1].notes.clear();
                            self.proj.tracks[2].notes.clear();
                            self.proj.tracks[9].notes.clear();
                        }
                    });
                }
                AppTab::Edit => {
                    ui.horizontal(|ui| {
                        ui.label("Pitch Zoom");
                        if ui.add(egui::Slider::new(&mut self.visible_pitch_span, 12.0..=72.0)).changed() {
                            self.clamp_pitch_center();
                        }
                        ui.label("Onion Chord");
                        ui.add(egui::Slider::new(&mut self.chord_opacity, 0.0..=1.0).step_by(0.02));
                        if ui.button("Import MIDI…").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("MIDI", &["mid", "midi"])
                                .pick_file()
                            {
                                self.import_midi_to_selected_track(&path, ctx);
                            }
                        }
                    });
                    self.show_grok_panel(ui, ctx);
                    ui.label("Tip: Tab3 — piano roll only • Ctrl+C/V/X/D • Grok MIDI import");
                }
                AppTab::Arrange => {
                    ui.label("Tip: Tab4 — sequence loops • Space plays full arrange timeline");
                }
            }
        });

        // Main area — one tab = one primary interaction surface
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::SidePanel::right("loop_bank")
                .resizable(false)
                .default_width(132.0)
                .max_width(160.0)
                .show_inside(ui, |ui| {
                    self.show_loop_bank_panel(ui);
                });

            match self.active_tab {
                AppTab::Chord => {
                    ui.label(
                        egui::RichText::new("TAB 1 CHORD — fine grid placement • Shift+click multi • Ctrl+C/V")
                            .strong(),
                    );
                    let _chord_response = self.draw_chord_timeline(ui);
                    self.show_chord_strip(ui);
                }
                AppTab::Generate => {
                    ui.label(
                        egui::RichText::new("TAB 2 GENERATE — preview chords + pattern accompaniment")
                            .strong(),
                    );
                    let _chord_response = self.draw_chord_timeline(ui);
                    ui.add_space(8.0);
                    ui.label(format!(
                        "Ch2 Piano: {} notes • Ch3 Bass: {} • Ch10 Drum: {}",
                        self.proj.tracks[1].notes.len(),
                        self.proj.tracks[2].notes.len(),
                        self.proj.tracks[9].notes.len(),
                    ));
                    ui.label(
                        egui::RichText::new("Use bottom panel to Generate All. Edit notes in Tab 3.")
                            .weak(),
                    );
                }
                AppTab::Edit => {
                    egui::SidePanel::left("tracks")
                        .resizable(false)
                        .default_width(168.0)
                        .max_width(180.0)
                        .show_inside(ui, |ui| {
                            ui.label("TRACKS  (M=mute S=solo Vol%)");
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    for ch in 1..=16 {
                                        let t_idx = (ch - 1) as usize;
                                        let mut mix_changed = false;
                                        ui.horizontal(|ui| {
                                            ui.allocate_ui_with_layout(
                                                Vec2::new(40.0, 18.0),
                                                egui::Layout::left_to_right(egui::Align::Center),
                                                |ui| {
                                                    if ui
                                                        .selectable_label(
                                                            self.selected_ch == ch,
                                                            track_short_label(ch),
                                                        )
                                                        .clicked()
                                                    {
                                                        self.selected_ch = ch;
                                                        self.selected_note = None;
                                                        self.piano_roll_focused = ch != 1;
                                                        if ch != 1 {
                                                            self.clear_chord_selection();
                                                        }
                                                    }
                                                },
                                            );
                                            let t = &mut self.proj.tracks[t_idx];
                                            let m_label = if t.muted { "M" } else { "m" };
                                            let s_label = if t.solo { "S" } else { "s" };
                                            if ui
                                                .add(egui::Button::new(m_label).min_size(Vec2::new(18.0, 16.0)))
                                                .clicked()
                                            {
                                                t.muted = !t.muted;
                                                mix_changed = true;
                                            }
                                            if ui
                                                .add(egui::Button::new(s_label).min_size(Vec2::new(18.0, 16.0)))
                                                .clicked()
                                            {
                                                t.solo = !t.solo;
                                                mix_changed = true;
                                            }
                                            let mut vol_pct = (t.track_vol * 100.0).round() as i32;
                                            if ui
                                                .add(
                                                    egui::DragValue::new(&mut vol_pct)
                                                        .speed(1.0)
                                                        .range(0..=100)
                                                        .prefix("V"),
                                                )
                                                .changed()
                                            {
                                                t.track_vol =
                                                    (vol_pct as f32 / 100.0).clamp(0.0, 1.0);
                                                mix_changed = true;
                                            }
                                        });
                                        if mix_changed {
                                            self.on_track_mix_changed();
                                        }
                                    }
                                });
                        });

                    ui.label(
                        egui::RichText::new("TAB 3 EDIT — piano roll • chord onion background • Ctrl+C/V")
                            .strong(),
                    );
                    if self.selected_ch == 1 {
                        ui.label("Select Ch2–16 in the track list to edit generated/hand parts.");
                    }
                    let roll_h = ui.available_height().clamp(220.0, 520.0);
                    let _roll_response = self.draw_piano_roll_with_keyboard(ui, roll_h);
                }
                AppTab::Arrange => {
                    self.show_arrange_panel(ui);
                }
            }
        });

        // Drive playhead from the audio thread while playing.
        if self.audio_stream.is_some() && self.synth_sample_rate > 0 {
            let samples = self.play_position_samples.load(Ordering::Relaxed);
            let beat = samples as f64 * self.proj.bpm / (60.0 * self.synth_sample_rate as f64);
            self.current_beat = if self.loop_playback {
                beat % self.loop_beats()
            } else {
                beat
            };
            ctx.request_repaint();
        }
    }
}

impl JpoApp {
    // ===== Chord Timeline drawing + interaction =====
    fn draw_chord_timeline(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let editable = self.active_tab == AppTab::Chord;
        let desired = Vec2::new(ui.available_width().min(980.0), 92.0);
        let sense = if editable {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        };
        let (resp, painter) = ui.allocate_painter(desired, sense);
        if editable && (resp.clicked() || resp.dragged() || resp.drag_started()) {
            self.piano_roll_focused = false;
        }

        let rect = resp.rect;
        let w = rect.width();
        let _h = rect.height();
        let px_per_beat = w as f64 / self.visible_beats;

        // bg
        painter.rect_filled(rect, 4.0, Color32::from_rgb(28, 28, 34));

        let start_b = self.visible_start;
        let end_b = start_b + self.visible_beats;

        // grid + bar numbers - improved subdivisions
        let mut b = start_b.floor();
        while b <= end_b + 0.1 {
            let x = rect.min.x + ((b - start_b) * px_per_beat) as f32;
            let is_bar = (b % 4.0).abs() < 0.01;
            let is_beat = (b % 1.0).abs() < 0.01;
            let width = if is_bar { 2.8 } else if is_beat { 1.7 } else { 0.9 };
            let color = if is_bar { Color32::from_rgb(100,100,115) } else if is_beat { Color32::from_rgb(70,70,85) } else { Color32::from_rgb(48,48,60) };
            painter.line_segment(
                [Pos2::new(x, rect.min.y + 4.0), Pos2::new(x, rect.max.y - 4.0)],
                Stroke::new(width, color),
            );
            if is_bar {
                painter.text(Pos2::new(x + 3.0, rect.max.y - 16.0), egui::Align2::LEFT_BOTTOM, format!("{}", (b/4.0).floor() as i32 + 1), egui::FontId::proportional(11.0), Color32::from_rgb(140,145,155));
            }
            b += 0.25;
        }

        self.draw_loop_boundaries(&painter, rect, start_b, px_per_beat, false);

        if let Some(ptr) = resp.hover_pos() {
            let beat = start_b + ((ptr.x - rect.min.x) as f64 / px_per_beat);
            self.chord_timeline_mouse_beat = beat;
        }

        // blocks
        for (i, blk) in self.proj.chord_blocks.iter().enumerate() {
            if blk.end() < start_b || blk.start > end_b { continue; }
            let x0 = rect.min.x + ((blk.start - start_b) * px_per_beat) as f32;
            let x1 = rect.min.x + ((blk.end() - start_b) * px_per_beat) as f32;
            let y0 = rect.min.y + 8.0;
            let y1 = rect.max.y - 8.0;

            let primary = self
                .active_chord_beat
                .map(|b| (b - blk.start).abs() < 0.001)
                .unwrap_or(false);
            let selected = self.selection.blocks.contains(&i) || primary;
            let col = if selected {
                Color32::from_rgb(95, 130, 175)
            } else {
                Color32::from_rgb(70, 95, 125)
            };
            painter.rect_filled(Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)), 3.0, col);

            let mut label = self.proj.chord_name(blk);
            if blk.syncopation_fill {
                label.push(' ');
                label.push('◆');
            }
            painter.text(Pos2::new(x0 + 6.0, y0 + 6.0), egui::Align2::LEFT_TOP, label, egui::FontId::proportional(13.0), Color32::WHITE);

            if primary {
                painter.rect_stroke(Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)), 3.0, Stroke::new(2.0, Color32::from_rgb(255, 180, 80)));
                let width = (x1 - x0).max(1.0);
                let resize_px = 16.0f32.min(width * 0.35).max(8.0);
                let hx0 = x1 - resize_px;
                painter.rect_filled(
                    Rect::from_min_max(Pos2::new(hx0, y0 + 3.0), Pos2::new(x1, y1 - 3.0)),
                    1.0,
                    Color32::from_rgba_unmultiplied(255, 200, 120, 70),
                );
                painter.line_segment(
                    [Pos2::new(x1 - 1.0, y0 + 4.0), Pos2::new(x1 - 1.0, y1 - 4.0)],
                    Stroke::new(2.0, Color32::from_rgb(255, 200, 120)),
                );
            } else if self.selection.blocks.contains(&i) {
                painter.rect_stroke(
                    Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)),
                    3.0,
                    Stroke::new(1.5, Color32::from_rgb(180, 200, 255)),
                );
            }
        }

        self.draw_playhead_line(&painter, rect, start_b, px_per_beat);

        let box_active = self.chord_box_select_start.is_some();
        if box_active {
            if let (Some(sb), Some(ptr)) = (self.chord_box_select_start, resp.interact_pointer_pos()) {
                let beat = start_b + ((ptr.x - rect.min.x) as f64 / px_per_beat);
                let x0 = rect.min.x + ((sb.min(beat) - start_b) * px_per_beat) as f32;
                let x1 = rect.min.x + ((sb.max(beat) - start_b) * px_per_beat) as f32;
                painter.rect_filled(
                    Rect::from_min_max(Pos2::new(x0, rect.min.y + 6.0), Pos2::new(x1, rect.max.y - 6.0)),
                    0.0,
                    Color32::from_rgba_unmultiplied(70, 130, 255, 55),
                );
                painter.rect_stroke(
                    Rect::from_min_max(Pos2::new(x0, rect.min.y + 6.0), Pos2::new(x1, rect.max.y - 6.0)),
                    0.0,
                    Stroke::new(1.5, Color32::from_rgb(90, 150, 255)),
                );
            }
        }

        // empty hint
        if self.proj.chord_blocks.is_empty() {
            painter.text(
                rect.min + Vec2::new(16.0, 28.0),
                egui::Align2::LEFT_TOP,
                "click empty = place chord block (Len setting)",
                egui::FontId::proportional(12.0),
                Color32::from_rgb(90, 95, 105),
            );
        }

        // interaction (Chord tab only)
        if editable && (resp.clicked() || resp.dragged() || resp.double_clicked()) {
            if let Some(ptr) = resp.interact_pointer_pos() {
                let beat = start_b + ((ptr.x - rect.min.x) as f64 / px_per_beat);
                let snapped = self.snap_beat(beat);
                let shift = ui.ctx().input(|i| i.modifiers.shift);
                let alt = ui.ctx().input(|i| i.modifiers.alt);

                if alt && resp.drag_started() {
                    self.chord_box_select_start = Some(beat);
                }

                let mut hit = None;
                let mut hit_kind = ChordDragKind::None;
                if !alt {
                    for (i, blk) in self.proj.chord_blocks.iter().enumerate().rev() {
                        if beat < blk.start || beat > blk.end() {
                            continue;
                        }
                        let x0 = rect.min.x + ((blk.start - start_b) * px_per_beat) as f32;
                        let x1 = rect.min.x + ((blk.end() - start_b) * px_per_beat) as f32;
                        let kind = Self::chord_hit_at(ptr.x, x0, x1, beat, blk);
                        if kind != ChordDragKind::None {
                            hit = Some(i);
                            hit_kind = kind;
                            break;
                        }
                    }
                }

                if !alt && (resp.drag_started() || resp.clicked() || resp.double_clicked()) {
                    if let Some(i) = hit {
                        let should_delete = self.edit_mode == EditMode::Eraser
                            || (self.edit_mode == EditMode::Pencil && resp.double_clicked());
                        if should_delete {
                            self.begin_gesture_undo();
                            self.proj.chord_blocks.remove(i);
                            self.selection.blocks.remove(&i);
                            self.clear_chord_selection();
                            self.chord_drag_kind = ChordDragKind::None;
                            self.chord_drag_block_idx = None;
                            self.end_gesture_undo();
                        } else if self.edit_mode == EditMode::Pencil {
                            let blk = self.proj.chord_blocks[i].clone();
                            if resp.clicked() && !resp.dragged() {
                                self.select_chord_block(i, shift);
                                self.set_playhead(beat);
                                self.chord_drag_kind = ChordDragKind::None;
                                self.chord_drag_block_idx = None;
                                self.preview_chord_block(&blk);
                            } else if resp.drag_started() {
                                self.begin_gesture_undo();
                                if !shift {
                                    self.select_chord_block(i, false);
                                }
                                self.block_drag_orig = (blk.start, blk.dur);
                                self.drag_start_beat = beat;
                                self.chord_drag_kind = hit_kind;
                                self.chord_drag_block_idx = Some(i);
                            }
                        }
                    } else if self.edit_mode == EditMode::Pencil
                        && resp.clicked()
                        && !resp.dragged()
                        && !shift
                    {
                        self.set_playhead(snapped);
                        self.clear_chord_selection();
                        self.place_chord_block_at(snapped);
                    } else if self.edit_mode == EditMode::Pencil
                        && resp.clicked()
                        && !resp.dragged()
                        && shift
                    {
                        self.set_playhead(snapped);
                    }
                }

                if !alt && resp.dragged() {
                    if let Some(i) = self.chord_drag_block_idx {
                        if i < self.proj.chord_blocks.len() {
                            let (orig_start, _orig_dur) = self.block_drag_orig;
                            let db = beat - self.drag_start_beat;
                            match self.chord_drag_kind {
                                ChordDragKind::Resize => {
                                    let snapped = self.snap_beat(beat);
                                    let new_end = snapped.max(orig_start + self.note_len * 0.5);
                                    let new_dur = self.snap_dur(new_end - orig_start);
                                    self.proj.chord_blocks[i].dur = new_dur;
                                }
                                ChordDragKind::Move => {
                                    let new_start = self.snap_beat((orig_start + db).max(0.0));
                                    self.proj.chord_blocks[i].start = new_start;
                                    self.active_chord_beat = Some(new_start);
                                }
                                ChordDragKind::None => {}
                            }
                        }
                    }
                }
            }
        }

        if editable && resp.drag_stopped() {
            if let Some(sb) = self.chord_box_select_start {
                if let Some(ptr) = resp.interact_pointer_pos() {
                    let beat = start_b + ((ptr.x - rect.min.x) as f64 / px_per_beat);
                    let (lo, hi) = (sb.min(beat), sb.max(beat));
                    if !ui.ctx().input(|i| i.modifiers.shift) {
                        self.selection.blocks.clear();
                    }
                    for (i, blk) in self.proj.chord_blocks.iter().enumerate() {
                        if blk.end() > lo && blk.start < hi {
                            self.selection.blocks.insert(i);
                        }
                    }
                    if let Some(&last) = self.selection.blocks.iter().max() {
                        self.set_active_chord_idx(last);
                    }
                }
                self.chord_box_select_start = None;
            }
            if self.chord_drag_kind == ChordDragKind::Move || self.chord_drag_kind == ChordDragKind::Resize {
                self.proj.chord_blocks.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
                self.resolve_chord_overlaps();
                if let Some(beat) = self.active_chord_beat {
                    if let Some(idx) = self
                        .proj
                        .chord_blocks
                        .iter()
                        .position(|b| (b.start - beat).abs() < 0.001)
                    {
                        self.set_active_chord_idx(idx);
                        self.selection.blocks.clear();
                        self.selection.blocks.insert(idx);
                    }
                }
            }
            self.is_creating = false;
            self.chord_drag_kind = ChordDragKind::None;
            self.chord_drag_block_idx = None;
            self.end_gesture_undo();
        }

        resp
    }

    const PIANO_KEY_WIDTH: f32 = 44.0;

    fn pitch_row_y(min_p: u8, max_p: u8, pitch: u8, h: f64, rect: Rect) -> (f32, f32) {
        let norm_top = (max_p as f64 - pitch as f64 - 0.5) / (max_p - min_p) as f64;
        let norm_bot = (max_p as f64 - pitch as f64 + 0.5) / (max_p - min_p) as f64;
        let y0 = rect.min.y + (norm_top * h) as f32;
        let y1 = rect.min.y + (norm_bot * h) as f32;
        (y0, y1)
    }

    /// Vertical piano keyboard strip (rotated / "sideways" piano) aligned to roll rows.
    fn draw_piano_keyboard(&self, ui: &mut egui::Ui, height: f32) {
        let desired = Vec2::new(Self::PIANO_KEY_WIDTH, height);
        let (_resp, painter) = ui.allocate_painter(desired, Sense::hover());

        let rect = _resp.rect;
        let h = rect.height() as f64;
        painter.rect_filled(rect, 0.0, Color32::from_rgb(32, 32, 38));

        let min_p = self.visible_pitch_min();
        let max_p = self.visible_pitch_max().max(min_p.saturating_add(1));

        for p in min_p..=max_p {
            let pc = p % 12;
            let is_black = [1, 3, 6, 8, 10].contains(&pc);
            if is_black {
                continue;
            }
            let (y0, y1) = Self::pitch_row_y(min_p, max_p, p, h, rect);
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(rect.min.x, y0), Pos2::new(rect.max.x, y1)),
                0.0,
                Color32::from_rgb(228, 230, 238),
            );
            painter.rect_stroke(
                Rect::from_min_max(Pos2::new(rect.min.x, y0), Pos2::new(rect.max.x, y1)),
                0.0,
                Stroke::new(0.6, Color32::from_rgb(150, 155, 168)),
            );
            if pc == 0 {
                let octave = (p / 12) as i32 - 1;
                painter.text(
                    Pos2::new(rect.min.x + 3.0, (y0 + y1) * 0.5),
                    egui::Align2::LEFT_CENTER,
                    format!("{}{}", pitch_class_name(p), octave),
                    egui::FontId::proportional(8.5),
                    Color32::from_rgb(70, 74, 88),
                );
            }
        }

        for p in min_p..=max_p {
            let pc = p % 12;
            if ![1, 3, 6, 8, 10].contains(&pc) {
                continue;
            }
            let (y0, y1) = Self::pitch_row_y(min_p, max_p, p, h, rect);
            let key_h = (y1 - y0).max(2.0);
            let black_w = rect.width() * 0.62;
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(rect.max.x - black_w, y0),
                    Pos2::new(rect.max.x, y1),
                ),
                1.0,
                Color32::from_rgb(42, 44, 54),
            );
            if key_h >= 10.0 {
                painter.text(
                    Pos2::new(rect.max.x - black_w * 0.5, (y0 + y1) * 0.5),
                    egui::Align2::CENTER_CENTER,
                    pitch_class_name(p),
                    egui::FontId::proportional(7.0),
                    Color32::from_rgb(175, 178, 190),
                );
            }
        }
    }

    fn draw_piano_roll_with_keyboard(&mut self, ui: &mut egui::Ui, height: f32) -> egui::Response {
        let mut roll_response = ui.allocate_response(Vec2::ZERO, Sense::click_and_drag());
        ui.horizontal(|ui| {
            self.draw_piano_keyboard(ui, height);
            roll_response = self.draw_piano_roll_grid(ui, ui.available_width().min(940.0), height);
        });
        roll_response
    }

    // ===== Piano Roll + harmonic highlight layers =====
    fn draw_piano_roll_grid(&mut self, ui: &mut egui::Ui, width: f32, height: f32) -> egui::Response {
        let desired = Vec2::new(width, height);
        let (resp, painter) = ui.allocate_painter(desired, Sense::click_and_drag());
        if resp.clicked() || resp.dragged() || resp.drag_started() {
            self.piano_roll_focused = true;
        }

        let rect = resp.rect;
        let w = rect.width() as f64;
        let h = rect.height() as f64;
        let px_per_beat = w / self.visible_beats;
        let start_b = self.visible_start;
        let end_b = start_b + self.visible_beats;

        let track_idx = self.track_idx();
        let is_ch1 = self.selected_ch == 1;
        let notes = if is_ch1 { expand_chords(&self.proj) } else { self.proj.tracks[track_idx].notes.clone() };

        // background
        painter.rect_filled(rect, 0.0, Color32::from_rgb(24, 24, 28));

        // Mouse wheel scrolls pitch view while hovering the roll
        if resp.hovered() {
            let scroll = ui.ctx().input(|i| i.raw_scroll_delta.y + i.smooth_scroll_delta.y);
            self.scroll_pitch_view(scroll);
        }

        let min_p = self.visible_pitch_min();
        let max_p = self.visible_pitch_max().max(min_p.saturating_add(1));

        // time grid (vertical lines) - base on current note_len to avoid being too fine (user feedback).
        // Weighted: bar strong, beat medium, main snap, light subs only when zoomed in.
        let main_step = self.note_len.max(0.25);
        let sub_step = if self.visible_beats < 12.0 { main_step / 2.0 } else { main_step };
        let mut b = start_b.floor();
        while b <= end_b + 0.1 {
            let x = rect.min.x + ((b - start_b) * px_per_beat) as f32;
            let is_bar = (b % 4.0).abs() < 0.01;
            let is_beat = (b % 1.0).abs() < 0.01;
            let is_main = ((b / main_step) % 1.0).abs() < 0.02;
            let width = if is_bar { 3.4 } else if is_beat { 2.4 } else if is_main { 1.8 } else { 1.2 };
            let color = if is_bar {
                Color32::from_rgb(135, 135, 158)
            } else if is_beat {
                Color32::from_rgb(105, 105, 125)
            } else if is_main {
                Color32::from_rgb(82, 82, 100)
            } else {
                Color32::from_rgb(65, 65, 80)
            };
            painter.line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(width, color),
            );
            b += sub_step;
        }

        self.draw_loop_boundaries(&painter, rect, start_b, px_per_beat, true);

        // pitch grid - clear semitone lines so you can see C vs C# etc. (critical for editing)
        // uses the visible range from above for zoom
        for p in min_p..=max_p {
            let (_, y1) = Self::pitch_row_y(min_p, max_p, p, h, rect);
            let is_c = (p % 12) == 0;
            let is_black = [1, 3, 6, 8, 10].contains(&(p % 12));
            let width = if is_c { 2.6 } else if is_black { 1.6 } else { 1.3 };
            let color = if is_c {
                Color32::from_rgb(125, 125, 148)
            } else if is_black {
                Color32::from_rgb(92, 92, 110)
            } else {
                Color32::from_rgb(72, 72, 88)
            };
            painter.line_segment(
                [Pos2::new(rect.min.x, y1), Pos2::new(rect.max.x, y1)],
                Stroke::new(width, color),
            );
        }

        // Harmonic highlight layers (scale pink full-width, chord blue per block — chord wins)
        if !is_ch1 {
            let scale_a = onion_alpha(self.scale_opacity);
            let chord_a = onion_alpha(self.chord_opacity);
            let scale_pcs = self.proj.scale_pitch_classes();
            if scale_a > 0 {
                for p in min_p..=max_p {
                    if !scale_pcs.contains(&(p % 12)) {
                        continue;
                    }
                    let norm_top = (max_p as f64 - p as f64 - 0.5) / (max_p - min_p) as f64;
                    let norm_bot = (max_p as f64 - p as f64 + 0.5) / (max_p - min_p) as f64;
                    let y0 = rect.min.y + (norm_top * h) as f32;
                    let y1 = rect.min.y + (norm_bot * h) as f32;
                    painter.rect_filled(
                        Rect::from_min_max(Pos2::new(rect.min.x, y0), Pos2::new(rect.max.x, y1)),
                        0.0,
                        Color32::from_rgba_unmultiplied(175, 108, 132, scale_a),
                    );
                }
            }

            if chord_a > 0 {
                for blk in &self.proj.chord_blocks {
                    if blk.end() <= start_b || blk.start >= end_b {
                        continue;
                    }
                    let x0 = rect.min.x + ((blk.start - start_b) * px_per_beat) as f32;
                    let x1 = rect.min.x + ((blk.end() - start_b) * px_per_beat) as f32;
                    let mut chord_pcs = std::collections::HashSet::new();
                    for p in self.proj.chord_pitches(blk) {
                        chord_pcs.insert(p % 12);
                    }
                    for p in min_p..=max_p {
                        if !chord_pcs.contains(&(p % 12)) {
                            continue;
                        }
                        let norm_top = (max_p as f64 - p as f64 - 0.5) / (max_p - min_p) as f64;
                        let norm_bot = (max_p as f64 - p as f64 + 0.5) / (max_p - min_p) as f64;
                        let y0 = rect.min.y + (norm_top * h) as f32;
                        let y1 = rect.min.y + (norm_bot * h) as f32;
                        painter.rect_filled(
                            Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)),
                            0.0,
                            Color32::from_rgba_unmultiplied(58, 108, 168, chord_a),
                        );
                    }
                }
            }
        }

        // notes
        for (i, n) in notes.iter().enumerate() {
            if n.end() < start_b || n.start > end_b { continue; }
            let x0 = rect.min.x + ((n.start - start_b) * px_per_beat) as f32;
            let x1 = rect.min.x + ((n.end() - start_b) * px_per_beat) as f32;
            let norm_top = (max_p as f64 - n.pitch as f64 - 0.5) / (max_p - min_p) as f64;
            let norm_bot = (max_p as f64 - n.pitch as f64 + 0.5) / (max_p - min_p) as f64;
            let y0 = rect.min.y + (norm_top * h) as f32;
            let y1 = rect.min.y + (norm_bot * h) as f32;

            let is_single_sel = self.selected_note.map_or(false, |(_, idx)| idx == i);
            let is_multi_sel = self.selection.notes.contains(&(track_idx, i));
            let sel = is_single_sel || is_multi_sel;
            let col = self.note_fill_color(n.vel, sel, is_ch1);
            painter.rect_filled(Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)), 2.0, col);
            let border = if sel {
                Color32::from_rgb(190, 255, 220)
            } else {
                Color32::from_rgb(12, 55, 38)
            };
            painter.rect_stroke(Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)), 1.0, Stroke::new(0.8, border));

            if x1 - x0 > 20.0 {
                painter.text(
                    Pos2::new(x0 + 3.0, (y0 + y1) * 0.5 - 4.0),
                    egui::Align2::LEFT_CENTER,
                    pitch_class_name(n.pitch),
                    egui::FontId::proportional(8.0),
                    Color32::from_rgb(8, 28, 18),
                );
            }
        }

        self.draw_playhead_line(&painter, rect, start_b, px_per_beat);

        // Ch1 roll: click sets playhead only (chords edited in timeline).
        if is_ch1 && resp.clicked() && !resp.dragged() {
            if let Some(ptr) = resp.interact_pointer_pos() {
                let beat = start_b + ((ptr.x - rect.min.x) as f64 / px_per_beat);
                self.set_playhead(beat);
            }
        }

        // mouse interaction (Ch2–16)
        if !is_ch1 && (resp.clicked() || resp.dragged() || resp.double_clicked()) {
            if let Some(ptr) = resp.interact_pointer_pos() {
                let beat = start_b + ((ptr.x - rect.min.x) as f64 / px_per_beat);
                let norm = ((ptr.y - rect.min.y) as f64 / h).clamp(0.0, 1.0);
                let pitch = (max_p as f64 - norm * (max_p - min_p) as f64).round() as u8;

                self.last_mouse_beat = beat;
                self.last_mouse_pitch = pitch;

                let shift = ui.ctx().input(|i| i.modifiers.shift);

                // Box-select overlay (Shift+drag on empty area)
                let box_active = self.box_select_start_beat.is_some()
                    && self.note_drag_kind == NoteDragKind::None;
                if box_active {
                    if let (Some(sb), Some(sp)) = (self.box_select_start_beat, self.box_select_start_pitch) {
                        let p_lo = sp.min(pitch);
                        let p_hi = sp.max(pitch);
                        let x0 = rect.min.x + ((sb.min(beat) - start_b) * px_per_beat) as f32;
                        let x1 = rect.min.x + ((sb.max(beat) - start_b) * px_per_beat) as f32;
                        let norm_lo = (max_p as f64 - p_hi as f64 - 0.5) / (max_p - min_p) as f64;
                        let norm_hi = (max_p as f64 - p_lo as f64 + 0.5) / (max_p - min_p) as f64;
                        let y0 = rect.min.y + (norm_lo * h) as f32;
                        let y1 = rect.min.y + (norm_hi * h) as f32;
                        painter.rect_filled(
                            Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)),
                            0.0,
                            Color32::from_rgba_unmultiplied(70, 130, 255, 55),
                        );
                        painter.rect_stroke(
                            Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)),
                            0.0,
                            Stroke::new(1.5, Color32::from_rgb(90, 150, 255)),
                        );
                    }
                }

                if resp.drag_started() {
                    self.begin_gesture_undo();
                }

                if resp.drag_started() || resp.clicked() || resp.double_clicked() {
                    let mut hit = None;
                    for (i, n) in notes.iter().enumerate() {
                        if n.start - 0.06 <= beat && beat <= n.end() + 0.06 && (n.pitch as i32 - pitch as i32).abs() <= 1 {
                            hit = Some(i);
                            break;
                        }
                    }

                    if let Some(i) = hit {
                        let should_delete = self.edit_mode == EditMode::Eraser
                            || (self.edit_mode == EditMode::Pencil && resp.double_clicked());
                        if should_delete && i < self.proj.tracks[track_idx].notes.len() {
                            self.begin_gesture_undo();
                            self.proj.tracks[track_idx].notes.remove(i);
                            self.selected_note = None;
                            self.selection.notes.retain(|&(t, _)| t != track_idx);
                            self.end_gesture_undo();
                        } else if !should_delete && self.edit_mode == EditMode::Pencil {
                            if resp.clicked() && !resp.dragged() && shift {
                                self.select_note_toggle(track_idx, i, true);
                            } else {
                                self.box_select_start_beat = None;
                                self.box_select_start_pitch = None;
                                if !(resp.drag_started() && self.selection.notes.contains(&(track_idx, i))) {
                                    self.select_note_toggle(track_idx, i, false);
                                }
                                let n = &self.proj.tracks[track_idx].notes[i];
                                self.drag_orig = (n.start, n.pitch, n.dur);
                                self.drag_start_beat = beat;
                                self.drag_start_pitch = pitch;
                                self.is_creating = false;
                                let grab_resize = beat >= n.end() - 0.18;
                                self.note_drag_kind = if grab_resize {
                                    NoteDragKind::Resize
                                } else {
                                    NoteDragKind::Move
                                };
                                let is_multi = self.selection.notes.contains(&(track_idx, i));
                                if is_multi {
                                    self.drag_sel_offsets = self
                                        .selection
                                        .notes
                                        .iter()
                                        .filter(|&&(t, _)| t == track_idx)
                                        .map(|&(_, idx)| {
                                            let nn = &self.proj.tracks[track_idx].notes[idx];
                                            (
                                                idx,
                                                nn.start - n.start,
                                                nn.pitch as i32 - n.pitch as i32,
                                            )
                                        })
                                        .collect();
                                } else {
                                    self.drag_sel_offsets = vec![(i, 0.0, 0)];
                                }
                            }
                        }
                    } else if self.edit_mode == EditMode::Pencil {
                        if resp.drag_started() && shift {
                            self.box_select_start_beat = Some(beat);
                            self.box_select_start_pitch = Some(pitch);
                        } else if resp.clicked() && !resp.dragged() && self.note_drag_kind == NoteDragKind::None {
                            self.place_piano_note_at(track_idx, beat, pitch);
                            self.set_playhead(beat);
                        }
                    }
                }

                if resp.dragged() {
                    if let Some((ti, ni)) = self.selected_note {
                        if ti == track_idx {
                            let db = beat - self.drag_start_beat;
                            let dp = pitch as i32 - self.drag_start_pitch as i32;

                            match self.note_drag_kind {
                                NoteDragKind::Create | NoteDragKind::Resize => {
                                    let new_dur = self.snap_dur((self.drag_orig.2 + db).max(0.0625));
                                    for &(sni, _, _) in &self.drag_sel_offsets {
                                        if sni < self.proj.tracks[track_idx].notes.len() {
                                            self.proj.tracks[track_idx].notes[sni].dur = new_dur;
                                        }
                                    }
                                }
                                NoteDragKind::Move => {
                                    let new_start = self.snap_beat((self.drag_orig.0 + db).max(0.0));
                                    let new_pitch = (self.drag_orig.1 as i32 + dp).clamp(0, 127) as u8;
                                    for &(sni, ds, dpp) in &self.drag_sel_offsets {
                                        if sni < self.proj.tracks[track_idx].notes.len() {
                                            let snapped = self.snap_beat((new_start + ds).max(0.0));
                                            let p = (new_pitch as i32 + dpp).clamp(0, 127) as u8;
                                            self.proj.tracks[track_idx].notes[sni].start = snapped;
                                            self.proj.tracks[track_idx].notes[sni].pitch = p;
                                        }
                                    }
                                    if self.last_move_preview_pitch != Some(new_pitch) {
                                        self.last_move_preview_pitch = Some(new_pitch);
                                        let patch = self.proj.tracks[track_idx].patch;
                                        let vel = self.proj.tracks[track_idx].notes[ni].vel;
                                        self.preview_note(self.selected_ch, patch, new_pitch, vel);
                                    }
                                }
                                NoteDragKind::None => {}
                            }
                        }
                    }
                }
            }
        }

        if resp.drag_stopped() {
            if let (Some(sb), Some(sp)) = (self.box_select_start_beat, self.box_select_start_pitch) {
                let cur_b = self.last_mouse_beat;
                let cur_p = self.last_mouse_pitch;
                let min_b = sb.min(cur_b);
                let max_b = sb.max(cur_b);
                let min_p_sel = sp.min(cur_p);
                let max_p_sel = sp.max(cur_p);
                if (max_b - min_b).abs() > 0.04 || max_p_sel.abs_diff(min_p_sel) > 0 {
                    self.begin_gesture_undo();
                    if !ui.ctx().input(|i| i.modifiers.shift) {
                        self.selection.notes.clear();
                    }
                    self.finalize_box_selection(min_b, max_b, min_p_sel, max_p_sel);
                    self.end_gesture_undo();
                }
                self.box_select_start_beat = None;
                self.box_select_start_pitch = None;
            }
            self.is_creating = false;
            if self.note_drag_kind != NoteDragKind::None {
                self.note_drag_kind = NoteDragKind::None;
            }
            self.last_move_preview_pitch = None;
            self.drag_sel_offsets.clear();
            self.end_gesture_undo();
        }

        resp
    }

    /// Fixed chord editor under the timeline (replaces floating palette).
    fn show_chord_strip(&mut self, ui: &mut egui::Ui) {
        ui.add_space(2.0);
        ui.label(egui::RichText::new("CHORD STRIP").strong());

        let Some(idx) = self.active_chord_idx() else {
            let mut hint = format!(
                "▸ playhead {:.1} — click empty = place • Shift+click multi • Alt+drag box • Ctrl+C/V at playhead",
                self.current_beat
            );
            if self.chord_clipboard.is_some() {
                hint.push_str(" • clipboard ready");
            }
            ui.label(egui::RichText::new(hint).small().weak());
            return;
        };
        if idx >= self.proj.chord_blocks.len() {
            self.clear_chord_selection();
            return;
        }

        let block_label = format!(
            "beat {:.1} — {}",
            self.proj.chord_blocks[idx].start,
            self.proj.chord_name(&self.proj.chord_blocks[idx])
        );
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(block_label).strong());
            let sel_n = self.selection.blocks.len();
            if sel_n > 1 {
                ui.label(
                    egui::RichText::new(format!("{sel_n} selected"))
                        .small()
                        .color(Color32::from_rgb(180, 200, 255)),
                );
            }
            ui.label(
                egui::RichText::new(format!("▸ {:.1}", self.current_beat))
                    .small()
                    .color(Color32::from_rgb(255, 120, 120)),
            );
            if self.chord_clipboard.is_some() {
                ui.label(egui::RichText::new("clipboard ready").small().weak());
            }
            ui.separator();
            let mut sync = self.proj.chord_blocks[idx].syncopation_fill;
            if ui
                .checkbox(&mut sync, "★ Syncopation fill here")
                .changed()
            {
                self.begin_gesture_undo();
                self.proj.chord_blocks[idx].syncopation_fill = sync;
                self.end_gesture_undo();
            }
        });

        let qualities = ["", "m", "7", "maj7", "m7", "dim", "sus4"];
        let borrowed = [(1, "m"), (4, "m"), (5, "7"), (6, "m"), (7, "7"), (2, "7")];
        let romans = ["Ⅰ", "Ⅱ", "Ⅲ", "Ⅳ", "Ⅴ", "Ⅵ", "Ⅶ"];

        ui.horizontal_wrapped(|ui| {
            for (i, &rom) in romans.iter().enumerate() {
                let deg = (i + 1) as u8;
                if ui.button(rom).clicked() {
                    let q = self.proj.default_quality(deg).to_string();
                    self.begin_gesture_undo();
                    if let Some(b) = self.proj.chord_blocks.get_mut(idx) {
                        b.degree = deg;
                        b.quality = q;
                    }
                    self.end_gesture_undo();
                    let preview = self.proj.chord_blocks[idx].clone();
                    self.preview_chord_block(&preview);
                }
            }
        });

        ui.label("Qualities");
        ui.horizontal_wrapped(|ui| {
            for &q in &qualities {
                let label = if q.is_empty() { "maj" } else { q };
                if ui.button(label).clicked() {
                    self.begin_gesture_undo();
                    if let Some(b) = self.proj.chord_blocks.get_mut(idx) {
                        b.quality = q.to_string();
                    }
                    self.end_gesture_undo();
                    let preview = self.proj.chord_blocks[idx].clone();
                    self.preview_chord_block(&preview);
                }
            }
        });

        ui.label("Common borrowed / secondary (J-Rock/J-Pop)");
        ui.horizontal_wrapped(|ui| {
            for (deg, q) in borrowed {
                let label = format!("{}{}", romans[(deg - 1) as usize], q);
                if ui.button(label).clicked() {
                    self.begin_gesture_undo();
                    if let Some(b) = self.proj.chord_blocks.get_mut(idx) {
                        b.degree = deg;
                        b.quality = q.to_string();
                    }
                    self.end_gesture_undo();
                    let preview = self.proj.chord_blocks[idx].clone();
                    self.preview_chord_block(&preview);
                }
            }
        });
    }

    // ====================== PLAYBACK (real SF2 via rustysynth + cpal) ======================
    // Follows the revised instruction: #1 priority is working integrated playback using the
    // user-provided FluidR3 GM.SF2 next to the exe (or in jpo/ for dev). The finder is already
    // in place and reported in the UI.

    fn build_playback_events(&self, sample_rate: u32) -> Vec<(u64, u8, bool, u8, u8)> {
        let bpm = self.proj.bpm.max(1.0);
        let secs_per_beat = 60.0 / bpm;
        let samples_per_beat = sample_rate as f64 * secs_per_beat;

        let mut events: Vec<(u64, u8, bool, u8, u8)> = Vec::new();

        if self.active_tab == AppTab::Arrange && !self.arrange_sequence.is_empty() {
            let mut beat_offset = 0.0f64;
            for slot in &self.arrange_sequence {
                let Some(sketch) = self.loop_bank.get(slot.bank_idx) else {
                    continue;
                };
                let loop_len = sketch.beats();
                for _ in 0..slot.repeats.max(1) {
                    for n in Self::expand_sketch_chords(sketch) {
                        let start = beat_offset + n.start;
                        let end = beat_offset + n.end();
                        let start_samp = (start * samples_per_beat) as u64;
                        let end_samp = (end * samples_per_beat) as u64;
                        let ch = 0u8;
                        let vol = self.proj.tracks[0].track_vol;
                        let vel = scaled_velocity(n.vel, vol);
                        events.push((start_samp, ch, true, n.pitch, vel));
                        events.push((end_samp, ch, false, n.pitch, 0));
                    }
                    for (ti, tr) in sketch.tracks.iter().enumerate().skip(1) {
                        if !track_is_audible(&sketch.tracks, ti) {
                            continue;
                        }
                        let ch = (tr.ch.saturating_sub(1)) as u8;
                        for n in &tr.notes {
                            let start = beat_offset + n.start;
                            let end = beat_offset + n.end();
                            let start_samp = (start * samples_per_beat) as u64;
                            let end_samp = (end * samples_per_beat) as u64;
                            let vel = scaled_velocity(n.vel, tr.track_vol);
                            events.push((start_samp, ch, true, n.pitch, vel));
                            events.push((end_samp, ch, false, n.pitch, 0));
                        }
                    }
                    beat_offset += loop_len;
                }
            }
        } else {
            // Ch1: expand blocks to notes (source of truth)
            if track_is_audible(&self.proj.tracks, 0) {
                let vol = self.proj.tracks[0].track_vol;
                for n in expand_chords(&self.proj) {
                    let start_samp = (n.start * samples_per_beat) as u64;
                    let end_samp = (n.end() * samples_per_beat) as u64;
                    let ch = 0u8;
                    let vel = scaled_velocity(n.vel, vol);
                    events.push((start_samp, ch, true, n.pitch, vel));
                    events.push((end_samp, ch, false, n.pitch, 0));
                }
            }

            for (ti, tr) in self.proj.tracks.iter().enumerate().skip(1) {
                if !track_is_audible(&self.proj.tracks, ti) {
                    continue;
                }
                let ch = (tr.ch.saturating_sub(1)) as u8;
                for n in &tr.notes {
                    let start_samp = (n.start * samples_per_beat) as u64;
                    let end_samp = (n.end() * samples_per_beat) as u64;
                    let vel = scaled_velocity(n.vel, tr.track_vol);
                    events.push((start_samp, ch, true, n.pitch, vel));
                    events.push((end_samp, ch, false, n.pitch, 0));
                }
            }
        }

        events.sort_by_key(|e| e.0);
        events
    }

    fn expand_sketch_chords(sketch: &LoopSketch) -> Vec<Note> {
        let tmp = Project {
            bpm: 120.0,
            key_root: sketch.key_root,
            is_minor: sketch.is_minor,
            tracks: sketch.tracks.clone(),
            chord_blocks: sketch.chord_blocks.clone(),
        };
        expand_chords(&tmp)
    }

    fn start_playback(&mut self) {
        if self.audio_stream.is_some() {
            return;
        }
        let Some(ref sf_path) = self.soundfont_path else {
            eprintln!("[Playback] No SF2 path - copy FluidR3 GM.SF2 next to exe or in jpo/");
            return;
        };

        // Audio device + rate
        let host = cpal::default_host();
        let device = match host.default_output_device() {
            Some(d) => d,
            None => { eprintln!("[Playback] No output device"); return; }
        };
        let supported = match device.default_output_config() {
            Ok(c) => c,
            Err(e) => { eprintln!("[Playback] No default config: {}", e); return; }
        };
        let sample_rate = supported.sample_rate().0;
        self.synth_sample_rate = sample_rate;

        let config: StreamConfig = supported.into();

        // Build events (Ch1 expanded via expand_chords + regular tracks)
        let events = self.build_playback_events(sample_rate);
        let bpm = self.proj.bpm.max(1.0);
        let samples_per_beat = sample_rate as f64 * (60.0 / bpm);
        let loop_end_sample = if self.loop_playback {
            let loop_beats = if self.active_tab == AppTab::Arrange {
                self.arrange_total_beats().max(0.25)
            } else {
                self.loop_beats()
            };
            (loop_beats * samples_per_beat) as u64
        } else {
            0
        };

        let mut start_sample = (self.current_beat * samples_per_beat) as u64;
        if self.loop_playback && loop_end_sample > 0 {
            start_sample %= loop_end_sample;
        }
        self.play_position_samples
            .store(start_sample, Ordering::Relaxed);

        // Snapshot patches for program change support
        let initial_patches: Vec<(u8, u8)> = self.proj.tracks.iter()
            .map(|t| (t.ch.saturating_sub(1), t.patch))
            .collect();

        // Create clean player (will lazy-load SF2 + Synthesizer on the audio thread)
        let mut player = PlaybackPlayer::new(
            sf_path.clone(),
            events,
            Arc::clone(&self.play_position_samples),
            sample_rate,
            self.playback_volume,
            initial_patches,
            self.loop_playback,
            loop_end_sample,
            start_sample,
        );

        // Build stream — player owns everything and has a proper advancing cursor
        let stream_result = device.build_output_stream(
            &config,
            move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                player.render(data);
            },
            |err| eprintln!("[Playback] stream error: {}", err),
            None,
        );

        match stream_result {
            Ok(stream) => {
                if let Err(e) = stream.play() {
                    eprintln!("[Playback] stream.play() failed: {}", e);
                    return;
                }
                self.audio_stream = Some(stream);
                self.is_playing = true;
                println!("[Playback] Started real SF2 playback (SR: {})", sample_rate);
            }
            Err(e) => {
                eprintln!("[Playback] build_output_stream failed: {}", e);
            }
        }
    }

    fn stop_playback(&mut self) {
        if let Some(stream) = self.audio_stream.take() {
            drop(stream);
            println!("[Playback] Stopped.");
        }
        if self.synth_sample_rate > 0 {
            let samples = self.play_position_samples.load(Ordering::Relaxed);
            let beat = samples as f64 * self.proj.bpm / (60.0 * self.synth_sample_rate as f64);
            self.current_beat = if self.loop_playback {
                beat % self.loop_beats()
            } else {
                beat
            };
        }
        self.is_playing = false;
    }
}

#[cfg(test)]
mod generate_tests {
    use super::*;

    #[test]
    fn melodic_pitch_c_major_degree_i_is_near_c4() {
        let pitch = melodic_pitch(72, 60, 60);
        assert!(pitch >= 48 && pitch <= 84, "pitch {pitch} out of expected range");
    }

    #[test]
    fn melodic_pitch_transposes_by_degree() {
        let proj = Project::default();
        let root_iv = proj.degree_root(4, 4);
        let pitch = melodic_pitch(72, 60, root_iv);
        assert_eq!(pitch as i32, 72 + (root_iv as i32 - 60));
    }

    #[test]
    fn replace_notes_in_range_removes_overlap_only() {
        let mut notes = vec![
            Note { start: 0.0, pitch: 60, dur: 1.0, vel: 90 },
            Note { start: 8.0, pitch: 62, dur: 1.0, vel: 90 },
        ];
        replace_notes_in_range(
            &mut notes,
            0.0,
            4.0,
            vec![Note { start: 0.0, pitch: 64, dur: 1.0, vel: 90 }],
        );
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].pitch, 64);
        assert_eq!(notes[1].pitch, 62);
    }

    #[test]
    fn pattern_tile_at_keeps_phase_for_mid_bar_fill() {
        let tile = pattern_tile_at(2.0, 0.0, 8.0);
        assert!((tile - 0.0).abs() < 0.001);
    }
}
