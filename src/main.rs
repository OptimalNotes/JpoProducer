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
    ) -> Self {
        Self {
            sf_path,
            events,
            position,
            sample_rate,
            gain,
            initial_patches,
            event_idx: 0,
            current_sample: 0,
            loop_enabled,
            loop_end_sample,
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
        for &(id, category, root) in specs {
            if let Some(bytes) = find_pattern_bytes(id) {
                match parse_pattern_midi(&bytes, id, category, root) {
                    Ok(p) => lib.patterns.push(p),
                    Err(e) => eprintln!("[Pattern] {e}"),
                }
            } else {
                eprintln!("[Pattern] missing: {id}.mid");
            }
        }
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

fn find_pattern_bytes(id: &str) -> Option<Vec<u8>> {
    let filename = format!("{id}.mid");
    let mut candidates: Vec<std::path::PathBuf> = vec![
        std::path::PathBuf::from("assets/patterns").join(&filename),
        std::path::PathBuf::from("patterns").join(&filename),
    ];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("patterns").join(&filename));
        }
    }
    for path in candidates {
        if path.exists() {
            return std::fs::read(path).ok();
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
        .filter(|b| b.dur < 1.5 && b.end() > s && b.start < e)
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
        PatternCategory::Piano => 4,
        PatternCategory::Bass => 3,
        PatternCategory::Drum => 3,
    };

    for block in &proj.chord_blocks {
        if block.end() <= range_start || block.start >= range_end {
            continue;
        }
        let transpose = proj.degree_root(block.degree, ref_octave) as i32
            - pattern.template_root as i32;
        let block_end = block.end().min(range_end);
        let mut tile = block.start;
        while tile < block_end {
            for pn in &pattern.notes {
                let t = tile + pn.start_beats;
                if t >= range_start && t < range_end && t < block_end {
                    notes.push(Note {
                        start: t,
                        pitch: (pn.pitch as i32 + transpose).clamp(0, 127) as u8,
                        dur: pn.dur_beats,
                        vel: pn.vel,
                    });
                }
            }
            tile += pattern.length_beats;
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
            .any(|(s, e)| n.start >= *s - 0.001 && n.start < *e - 0.001)
    });
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
            PatternCategory::Piano => 4,
            PatternCategory::Bass => 3,
            _ => 3,
        };
        let transpose = block.map(|b| {
            proj.degree_root(b.degree, ref_octave) as i32 - sync_pattern.template_root as i32
        }).unwrap_or(0);

        for pn in &sync_pattern.notes {
            let t = win_start + pn.start_beats;
            if t >= win_end {
                continue;
            }
            let pitch = match sync_pattern.category {
                PatternCategory::Drum => pn.pitch,
                _ => (pn.pitch as i32 + transpose).clamp(0, 127) as u8,
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
            }
            if let Some(p) = lib.get("Bass_syncopation") {
                remove_notes_in_windows(&mut bass, &windows);
                apply_syncopation_splice(&mut bass, p, proj, &windows);
            }
            if let Some(p) = lib.get("Drum_syncopation") {
                remove_notes_in_windows(&mut drums, &windows);
                apply_syncopation_splice(&mut drums, p, proj, &windows);
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
enum UiMode { Sketch, Arrange }

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
enum ChordDragKind { None, Create, Move, Resize }

#[derive(Clone, Debug)]
struct ClipboardNotes {
    notes: Vec<Note>,
    min_start: f64,
    anchor_pitch: u8,
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
    // Old single for backward compat during transition; new multi-select is the future
    selected_block: Option<usize>,
    selected_note: Option<(usize, usize)>, // (track_idx, note_idx)

    // New multi-selection (priority #2 editing foundation)
    selection: Selection,

    drag_start_beat: f64,
    drag_start_pitch: u8,
    drag_orig: (f64, u8, f64), // start, pitch, dur for the dragged item
    is_creating: bool,
    note_drag_kind: NoteDragKind,
    chord_drag_kind: ChordDragKind,
    block_drag_orig: (f64, f64), // start, dur for chord block drags

    // Box selection state for range tool (editing base)
    box_select_start_beat: Option<f64>,
    box_select_start_pitch: Option<u8>,

    // Chord palette state
    show_chord_palette: bool,
    palette_block_idx: Option<usize>,

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
    ui_mode: UiMode,
    arrange_sequence: Vec<ArrangeSlot>,
}

impl Default for JpoApp {
    fn default() -> Self {
        Self {
            proj: Project::default(),
            selected_ch: 4,
            visible_start: 0.0,
            visible_beats: 8.0,
            current_beat: 0.0,
            edit_mode: EditMode::Pencil,
            note_len: 0.5,
            snap_enabled: true,
            selected_block: None,
            selected_note: None,
            selection: Selection::default(),
            drag_start_beat: 0.0,
            drag_start_pitch: 60,
            drag_orig: (0.0, 60, 0.5),
            is_creating: false,
            note_drag_kind: NoteDragKind::None,
            chord_drag_kind: ChordDragKind::None,
            block_drag_orig: (0.0, 0.5),
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
            show_chord_palette: false,
            palette_block_idx: None,
            gen_start: 0.0,
            gen_end: 32.0,
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
            loop_bars: 8,
            loop_playback: true,
            loop_bank: vec![LoopSketch::new_empty("Loop 1", 8)],
            active_bank_idx: 0,
            loop_name_counter: 2,
            pattern_lib: PatternLibrary::load(),
            piano_pattern_id: "Piano01".to_string(),
            bass_pattern_id: "Bass8beat01".to_string(),
            drum_pattern_id: "Drum8beat_01".to_string(),
            syncopation_fill: true,
            ui_mode: UiMode::Sketch,
            arrange_sequence: vec![ArrangeSlot { bank_idx: 0, repeats: 1 }],
        }
    }
}

impl JpoApp {
    fn track_idx(&self) -> usize { (self.selected_ch - 1) as usize }

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

    fn chord_hit_at(beat: f64, blk: &ChordBlock, px_per_beat: f64) -> ChordDragKind {
        if beat < blk.start || beat > blk.end() {
            return ChordDragKind::None;
        }
        let resize_beats = (14.0 / px_per_beat).clamp(0.06, 0.2);
        if beat >= blk.end() - resize_beats {
            ChordDragKind::Resize
        } else {
            ChordDragKind::Move
        }
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
        self.selected_block = None;
        self.show_chord_palette = false;
        self.palette_block_idx = None;
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
            ChordBlock { start: 0.0, dur: bar, degree: 1, quality: "".into(), octave: 4 },
            ChordBlock { start: bar, dur: bar, degree: 5, quality: "".into(), octave: 4 },
            ChordBlock { start: bar * 2.0, dur: bar, degree: 6, quality: "m".into(), octave: 4 },
            ChordBlock { start: bar * 3.0, dur: bar, degree: 4, quality: "".into(), octave: 4 },
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
                egui::RichText::new("Shift+drag = box select • Ctrl+Z/Y, C/V/D")
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
            self.selected_block = None;
            self.gesture_undo_saved = false;
        }
    }

    fn do_redo(&mut self) {
        if let Some(next) = self.undo.redo(&self.proj) {
            self.proj = next;
            self.selection.notes.clear();
            self.selection.blocks.clear();
            self.selected_note = None;
            self.selected_block = None;
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

    fn copy_selection(&mut self) {
        let t_idx = self.track_idx();
        if self.selected_ch == 1 {
            return;
        }
        let indices = self.selected_note_indices(t_idx);
        if indices.is_empty() {
            return;
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
            return;
        }
        self.clipboard = Some(ClipboardNotes {
            notes,
            min_start,
            anchor_pitch,
        });
    }

    fn paste_clipboard(&mut self) {
        let Some(cb) = self.clipboard.clone() else {
            return;
        };
        if self.selected_ch == 1 {
            return;
        }
        let t_idx = self.track_idx();
        self.begin_gesture_undo();
        let paste_at = self.snap_beat(self.last_mouse_beat.max(0.0));
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
    }

    fn duplicate_selection(&mut self) {
        self.copy_selection();
        self.paste_clipboard();
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
            version: 3,
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
            8
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
        self.selected_block = None;
        self.project_path = Some(path.to_path_buf());
        self.gesture_undo_saved = false;
        Ok(())
    }

    fn place_piano_note(&mut self, track_idx: usize, beat: f64, pitch: u8, drag_kind: NoteDragKind) {
        self.begin_gesture_undo();
        let dur = self.note_len;
        let new_n = Note {
            start: self.snap_beat(beat),
            pitch: pitch.clamp(0, 127),
            dur,
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
        self.selected_note = Some((track_idx, new_idx));
        self.drag_orig = (new_n.start, new_n.pitch, new_n.dur);
        self.drag_start_beat = beat;
        self.drag_start_pitch = pitch;
        self.is_creating = drag_kind == NoteDragKind::Create;
        self.note_drag_kind = drag_kind;
        self.drag_sel_offsets = vec![(new_idx, 0.0, 0)];
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
        ctx.input(|i| {
            let ctrl = i.modifiers.ctrl;
            if i.key_pressed(egui::Key::Delete) {
                self.delete_selected_notes();
            }
            if ctrl && i.key_pressed(egui::Key::Z) && !i.modifiers.shift {
                self.do_undo();
            }
            if ctrl && (i.key_pressed(egui::Key::Y) || (i.key_pressed(egui::Key::Z) && i.modifiers.shift)) {
                self.do_redo();
            }
            if ctrl && i.key_pressed(egui::Key::C) {
                self.copy_selection();
            }
            if ctrl && i.key_pressed(egui::Key::V) {
                self.paste_clipboard();
            }
            if ctrl && i.key_pressed(egui::Key::D) {
                self.duplicate_selection();
            }
        });

        // Top toolbar — compact: play always visible; extras live in Tools menu
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            let roots = ["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"];
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(self.ui_mode == UiMode::Sketch, "Sketch")
                    .clicked()
                {
                    self.ui_mode = UiMode::Sketch;
                }
                if ui
                    .selectable_label(self.ui_mode == UiMode::Arrange, "Arrange")
                    .clicked()
                {
                    self.ui_mode = UiMode::Arrange;
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
                    .clicked()
                {
                    if self.audio_stream.is_some() {
                        self.stop_playback();
                    } else {
                        self.start_playback();
                    }
                }
                ui.monospace(format!("{:.2}", self.current_beat));
            });
        });

        // Bottom controls — always pinned so Zoom/Generate never clip off-screen
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

            ui.horizontal(|ui| {
                ui.label("Pitch Zoom");
                if ui.add(egui::Slider::new(&mut self.visible_pitch_span, 12.0..=72.0)).changed() {
                    self.clamp_pitch_center();
                }
                ui.label("Pitch Scroll");
                let (p_lo, p_hi) = self.pitch_scroll_range();
                if ui
                    .add(egui::Slider::new(&mut self.visible_pitch_center, p_lo..=p_hi))
                    .changed()
                {
                    self.clamp_pitch_center();
                }
                if ui.button("Fit C2-C6").clicked() {
                    self.visible_pitch_center = 60.0;
                    self.visible_pitch_span = 48.0;
                }
                if ui.button("Fit C3-C5").clicked() {
                    self.visible_pitch_center = 60.0;
                    self.visible_pitch_span = 24.0;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Onion");
                ui.label("Scale");
                ui.add(egui::Slider::new(&mut self.scale_opacity, 0.0..=1.0).step_by(0.02));
                ui.label("Chord");
                ui.add(egui::Slider::new(&mut self.chord_opacity, 0.0..=1.0).step_by(0.02));
            });

            ui.horizontal(|ui| {
                ui.label("Generate range (beats)");
                ui.add(egui::DragValue::new(&mut self.gen_start).speed(0.5).range(0.0..=128.0));
                ui.label("→");
                ui.add(egui::DragValue::new(&mut self.gen_end).speed(0.5).range(0.0..=128.0));
                if ui.button("Gen=Visible").clicked() {
                    self.gen_start = self.visible_start;
                    self.gen_end = self.visible_start + self.visible_beats;
                }
                if ui.button("Gen=Loop").clicked() {
                    self.gen_start = 0.0;
                    self.gen_end = self.loop_beats();
                }
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
                if ui.button("Generate All (Piano+Bass+Drum)").clicked() {
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
                    self.proj.tracks[1].notes.extend(p);
                    self.proj.tracks[2].notes.extend(b);
                    self.proj.tracks[9].notes.extend(d);
                    self.end_gesture_undo();
                }
                if ui.button("Clear Generated (Ch2,3,10)").clicked() {
                    self.proj.tracks[1].notes.clear();
                    self.proj.tracks[2].notes.clear();
                    self.proj.tracks[9].notes.clear();
                }
            });

            ui.label("Tip: Loop 4/8/16 bars • Shift+drag = box select • Ctrl+Z/Y, C/V/D");
        });

        // Main area
        egui::CentralPanel::default().show(ctx, |ui| {
            // Track list (left)
            egui::SidePanel::left("tracks")
                .resizable(false)
                .default_width(220.0)
                .max_width(240.0)
                .show_inside(ui, |ui| {
                ui.label("TRACKS  (M=mute S=solo)");
                for ch in 1..=16 {
                    let t_idx = (ch - 1) as usize;
                    let mut mix_changed = false;
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(
                            Vec2::new(40.0, 18.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                if ui
                                    .selectable_label(self.selected_ch == ch, track_short_label(ch))
                                    .clicked()
                                {
                                    self.selected_ch = ch;
                                    self.selected_note = None;
                                    self.selected_block = None;
                                }
                            },
                        );
                        let t = &mut self.proj.tracks[t_idx];
                        let m_label = if t.muted { "M" } else { "m" };
                        let s_label = if t.solo { "S" } else { "s" };
                        if ui
                            .add(egui::Button::new(m_label).min_size(Vec2::new(18.0, 16.0)))
                            .on_hover_text("Mute")
                            .clicked()
                        {
                            t.muted = !t.muted;
                            mix_changed = true;
                        }
                        if ui
                            .add(egui::Button::new(s_label).min_size(Vec2::new(18.0, 16.0)))
                            .on_hover_text("Solo")
                            .clicked()
                        {
                            t.solo = !t.solo;
                            mix_changed = true;
                        }
                        if ui
                            .add(
                                egui::Slider::new(&mut t.track_vol, 0.0..=1.0)
                                    .show_value(false)
                                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                            )
                            .on_hover_text("Track volume")
                            .changed()
                        {
                            mix_changed = true;
                        }
                    });
                    if mix_changed {
                        self.on_track_mix_changed();
                    }
                    if ch != 1 {
                        let t = &mut self.proj.tracks[t_idx];
                        let patch_name =
                            truncate_ascii(gm_instrument_name(t.patch), 12);
                        ui.horizontal(|ui| {
                            ui.add_space(42.0);
                            ui.add(
                                egui::DragValue::new(&mut t.patch)
                                    .speed(1.0)
                                    .range(0..=127),
                            );
                            ui.label(egui::RichText::new(patch_name).size(10.0));
                        });
                    }
                }
                ui.separator();
                ui.label("Synth: MIDI Out (port or softsynth) + rustysynth ready");
                if let Some(ref p) = self.soundfont_path {
                    ui.label(format!("SF2: found ({})", p.display()));
                } else {
                    ui.label(egui::RichText::new("SF2: NOT FOUND — copy FluidR3 GM.SF2 next to jpo.exe (or into jpo/ for dev)").color(Color32::from_rgb(255, 180, 80)));
                }
            });

            egui::SidePanel::right("loop_bank")
                .resizable(false)
                .default_width(132.0)
                .max_width(160.0)
                .show_inside(ui, |ui| {
                    self.show_loop_bank_panel(ui);
                });

            if self.ui_mode == UiMode::Arrange {
                self.show_arrange_panel(ui);
            } else {
                // Chord Timeline (always visible - onion source + Ch1 input)
                ui.label(egui::RichText::new("CHORD TIMELINE (Ch1) — drag to paint blocks, click for palette, Eraser to delete").strong());
                let _chord_response = self.draw_chord_timeline(ui);

                ui.add_space(4.0);

                // Piano Roll
                let roll_label = if self.selected_ch == 1 {
                    "PIANO ROLL (Ch1 preview — actual input is in the Chord Timeline above)"
                } else {
                    "PIANO ROLL — pink = key scale, blue = chord tones in block range"
                };
                ui.label(roll_label);
                let roll_h = ui.available_height().clamp(180.0, 450.0);
                let _roll_response = self.draw_piano_roll_with_keyboard(ui, roll_h);
            }
        });

        // Chord Palette window - extracted to separate method to avoid borrow issues with egui Window + self mutation
        self.show_chord_palette_window(ctx);

        // Drive playhead from the real audio thread's sample counter (when playback is active).
        if self.audio_stream.is_some() {
            let samples = self.play_position_samples.load(Ordering::Relaxed);
            if self.synth_sample_rate > 0 {
                let beat = samples as f64 * self.proj.bpm / (60.0 * self.synth_sample_rate as f64);
                self.current_beat = if self.loop_playback {
                    beat % self.loop_beats()
                } else {
                    beat
                };
            }
            ctx.request_repaint();
        }
    }
}

impl JpoApp {
    // ===== Chord Timeline drawing + interaction =====
    fn draw_chord_timeline(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let desired = Vec2::new(ui.available_width().min(980.0), 92.0);
        let (resp, painter) = ui.allocate_painter(desired, Sense::click_and_drag());

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

        // blocks
        for (i, blk) in self.proj.chord_blocks.iter().enumerate() {
            if blk.end() < start_b || blk.start > end_b { continue; }
            let x0 = rect.min.x + ((blk.start - start_b) * px_per_beat) as f32;
            let x1 = rect.min.x + ((blk.end() - start_b) * px_per_beat) as f32;
            let y0 = rect.min.y + 8.0;
            let y1 = rect.max.y - 8.0;

            let col = if Some(i) == self.selected_block {
                Color32::from_rgb(95, 130, 175)
            } else {
                Color32::from_rgb(70, 95, 125)
            };
            painter.rect_filled(Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)), 3.0, col);

            let name = self.proj.chord_name(blk);
            painter.text(Pos2::new(x0 + 6.0, y0 + 6.0), egui::Align2::LEFT_TOP, name, egui::FontId::proportional(13.0), Color32::WHITE);

            if Some(i) == self.selected_block {
                painter.rect_stroke(Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)), 3.0, Stroke::new(2.0, Color32::from_rgb(255, 180, 80)));
                // small resize handle on right edge to hint that you can drag to stretch length
                let hx = x1 - 3.0;
                painter.rect_filled(Rect::from_min_max(Pos2::new(hx-2.0, y0+4.0), Pos2::new(hx+4.0, y1-4.0)), 1.0, Color32::from_rgb(255, 200, 120));
            }
        }

        // empty hint
        if self.proj.chord_blocks.is_empty() {
            painter.text(rect.min + Vec2::new(16.0, 28.0), egui::Align2::LEFT_TOP, "drag here to paint chord blocks (Ch1)", egui::FontId::proportional(12.0), Color32::from_rgb(90,95,105));
        }

        // interaction
        if resp.clicked() || resp.dragged() || resp.double_clicked() {
            if let Some(ptr) = resp.interact_pointer_pos() {
                let beat = start_b + ((ptr.x - rect.min.x) as f64 / px_per_beat);
                let snapped = self.snap_beat(beat);

                if resp.drag_started() {
                    self.begin_gesture_undo();
                }

                if resp.drag_started() || resp.clicked() || resp.double_clicked() {
                    let mut hit = None;
                    let mut hit_kind = ChordDragKind::None;
                    for (i, blk) in self.proj.chord_blocks.iter().enumerate() {
                        let kind = Self::chord_hit_at(beat, blk, px_per_beat);
                        if kind != ChordDragKind::None {
                            hit = Some(i);
                            hit_kind = kind;
                            break;
                        }
                    }

                    if let Some(i) = hit {
                        let should_delete = self.edit_mode == EditMode::Eraser
                            || (self.edit_mode == EditMode::Pencil && resp.double_clicked());
                        if should_delete {
                            self.begin_gesture_undo();
                            self.proj.chord_blocks.remove(i);
                            self.selected_block = None;
                            self.show_chord_palette = false;
                            self.palette_block_idx = None;
                            self.chord_drag_kind = ChordDragKind::None;
                            self.end_gesture_undo();
                        } else if self.edit_mode == EditMode::Pencil {
                            let blk = &self.proj.chord_blocks[i];
                            self.selected_block = Some(i);
                            self.block_drag_orig = (blk.start, blk.dur);
                            self.drag_start_beat = beat;
                            self.chord_drag_kind = hit_kind;
                            self.is_creating = false;
                            // Palette only on click inside block body (not resize edge / not drag-paint).
                            if resp.clicked()
                                && !resp.dragged()
                                && hit_kind == ChordDragKind::Move
                            {
                                self.show_chord_palette = true;
                                self.palette_block_idx = Some(i);
                                let preview = blk.clone();
                                self.preview_chord_block(&preview);
                            }
                        }
                    } else if self.edit_mode == EditMode::Pencil {
                        // Tap = one block; drag = paint length (no palette popup on create).
                        let should_create =
                            (resp.clicked() && !resp.dragged()) || resp.drag_started();
                        if should_create && self.chord_drag_kind == ChordDragKind::None {
                            self.begin_gesture_undo();
                            let q = self.proj.default_quality(1).to_string();
                            let new = ChordBlock {
                                start: snapped,
                                dur: self.note_len.max(0.5),
                                degree: 1,
                                quality: q,
                                octave: 4,
                            };
                            self.block_drag_orig = (new.start, new.dur);
                            self.proj.chord_blocks.push(new);
                            self.proj.chord_blocks.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
                            let new_idx = self.proj.chord_blocks.len() - 1;
                            self.selected_block = Some(new_idx);
                            self.drag_start_beat = beat;
                            self.chord_drag_kind = ChordDragKind::Create;
                            self.is_creating = true;
                            if resp.clicked() && !resp.dragged() {
                                let preview = self.proj.chord_blocks[new_idx].clone();
                                self.preview_chord_block(&preview);
                            }
                        }
                    }
                }

                if resp.dragged() {
                    if let Some(i) = self.selected_block {
                        if i < self.proj.chord_blocks.len() {
                            let (orig_start, _orig_dur) = self.block_drag_orig;
                            let db = beat - self.drag_start_beat;
                            match self.chord_drag_kind {
                                ChordDragKind::Create | ChordDragKind::Resize => {
                                    let new_end = beat.max(orig_start + self.note_len * 0.5);
                                    let new_dur = self.snap_dur(new_end - orig_start);
                                    self.proj.chord_blocks[i].dur = new_dur;
                                }
                                ChordDragKind::Move => {
                                    let new_start = self.snap_beat((orig_start + db).max(0.0));
                                    self.proj.chord_blocks[i].start = new_start;
                                }
                                ChordDragKind::None => {}
                            }
                        }
                    }
                }
            }
        }

        if resp.drag_stopped() {
            self.is_creating = false;
            self.chord_drag_kind = ChordDragKind::None;
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

        // playhead
        if start_b <= self.current_beat && self.current_beat <= end_b {
            let x = rect.min.x + ((self.current_beat - start_b) * px_per_beat) as f32;
            painter.line_segment([Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)], Stroke::new(2.0, Color32::from_rgb(255, 230, 120)));
        }

        // mouse interaction (Ch1 roll is preview-only)
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
                            self.box_select_start_beat = None;
                            self.box_select_start_pitch = None;
                            self.selected_note = Some((track_idx, i));
                            let n = &notes[i];
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
                                self.selection.notes.clear();
                                self.selection.notes.insert((track_idx, i));
                                self.drag_sel_offsets = vec![(i, 0.0, 0)];
                            }
                        }
                    } else if self.edit_mode == EditMode::Pencil {
                        if resp.drag_started() && shift {
                            self.box_select_start_beat = Some(beat);
                            self.box_select_start_pitch = Some(pitch);
                            self.selection.notes.clear();
                        } else if resp.drag_started() && self.note_drag_kind == NoteDragKind::None {
                            self.place_piano_note(track_idx, beat, pitch, NoteDragKind::Create);
                        } else if resp.clicked() && !resp.dragged() && self.note_drag_kind == NoteDragKind::None {
                            self.place_piano_note(track_idx, beat, pitch, NoteDragKind::None);
                            self.note_drag_kind = NoteDragKind::None;
                            self.end_gesture_undo();
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

    /// Chord palette as a separate method.
    /// We snapshot block data to locals so the .show closure does *not* borrow &mut self.
    /// This resolves the E0499 "cannot borrow `open` as mutable more than once" caused by
    /// .open(&mut open) + closure that previously did self.proj mutations + open= inside.
    fn show_chord_palette_window(&mut self, ctx: &egui::Context) {
        if !self.show_chord_palette {
            return;
        }
        let Some(idx) = self.palette_block_idx else { return; };
        if idx >= self.proj.chord_blocks.len() {
            self.show_chord_palette = false;
            self.palette_block_idx = None;
            return;
        }

        // Snapshot the current block so we can edit locals inside the closure.
        let current = self.proj.chord_blocks[idx].clone();
        let mut chosen_degree = current.degree;
        let mut chosen_quality = current.quality.clone();

        let mut open = self.show_chord_palette;
        let mut close_requested = false;
        let mut preview_requested = false;

        let screen = ctx.screen_rect();
        let default_pos = egui::pos2(
            (screen.max.x - 310.0).max(screen.min.x + 170.0),
            screen.min.y + 165.0,
        );

        egui::Window::new("Chord Palette")
            .open(&mut open)
            .resizable(false)
            .default_pos(default_pos)
            .default_size(egui::vec2(300.0, 255.0))
            .show(ctx, |ui| {
                ui.label("Choose chord for the selected block");
                ui.separator();

                let qualities = ["", "m", "7", "maj7", "m7", "dim", "sus4"];
                let borrowed = [(1, "m"), (4, "m"), (5, "7"), (6, "m"), (7, "7"), (2, "7")];

                ui.horizontal_wrapped(|ui| {
                    let romans = ["Ⅰ", "Ⅱ", "Ⅲ", "Ⅳ", "Ⅴ", "Ⅵ", "Ⅶ"];
                    for (i, &rom) in romans.iter().enumerate() {
                        let deg = (i + 1) as u8;
                        if ui.button(rom).clicked() {
                            chosen_degree = deg;
                            chosen_quality = self.proj.default_quality(deg).to_string();
                            preview_requested = true;
                        }
                    }
                });

                ui.label("Qualities");
                ui.horizontal_wrapped(|ui| {
                    for &q in &qualities {
                        let label = if q.is_empty() { "maj" } else { q };
                        if ui.button(label).clicked() {
                            chosen_quality = q.to_string();
                            preview_requested = true;
                        }
                    }
                });

                ui.label("Common borrowed / secondary (J-Rock/J-Pop)");
                ui.horizontal_wrapped(|ui| {
                    for (deg, q) in borrowed {
                        let label = format!("{}{}", ["Ⅰ","Ⅱ","Ⅲ","Ⅳ","Ⅴ","Ⅵ","Ⅶ"][(deg-1) as usize], q);
                        if ui.button(label).clicked() {
                            chosen_degree = deg;
                            chosen_quality = q.to_string();
                            preview_requested = true;
                        }
                    }
                });

                ui.separator();
                if ui.button("Close").clicked() {
                    close_requested = true;
                }
            });

        if close_requested {
            open = false;
        }

        // Apply changes back (outside the window closure, no overlapping borrow with `open`).
        if preview_requested {
            self.begin_gesture_undo();
        }
        if let Some(b) = self.proj.chord_blocks.get_mut(idx) {
            b.degree = chosen_degree;
            b.quality = chosen_quality;
        }
        if preview_requested {
            self.end_gesture_undo();
            let preview = self.proj.chord_blocks[idx].clone();
            self.preview_chord_block(&preview);
        }

        self.show_chord_palette = open;
        if !open {
            self.palette_block_idx = None;
        }
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

        if self.ui_mode == UiMode::Arrange && !self.arrange_sequence.is_empty() {
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
            let loop_beats = if self.ui_mode == UiMode::Arrange {
                self.arrange_total_beats().max(0.25)
            } else {
                self.loop_beats()
            };
            (loop_beats * samples_per_beat) as u64
        } else {
            0
        };

        // Reset position
        self.play_position_samples.store(0, Ordering::Relaxed);

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
        self.is_playing = false;
        self.play_position_samples.store(0, Ordering::Relaxed);
        self.current_beat = 0.0;
    }
}
