#!/usr/bin/env python3
"""
JpoProducer - 軽量 J-Pop / J-Rock MIDI スケッチツール
Python + Dear PyGui  (Dominoより少し見やすいダークモード)
"""

import sys
import time
import threading
import math
from dataclasses import dataclass, field
from typing import List, Optional, Dict, Tuple
import os
import dearpygui.dearpygui as dpg
import mido
from mido import Message, MidiFile, MidiTrack, MetaMessage

# rtmidi is optional (playback). App works fully for editing + MIDI export without it.
try:
    import rtmidi
    HAS_RTMIDI = True
except Exception:
    rtmidi = None  # type: ignore
    HAS_RTMIDI = False

try:
    import pyperclip
    HAS_PYPERCLIP = True
except Exception:
    pyperclip = None  # type: ignore
    HAS_PYPERCLIP = False

# FluidSynth flag (actual import is lazy inside _init_fluidsynth so the app always starts even if the DLL is missing)
HAS_FLUIDSYNTH = False
fluidsynth = None  # type: ignore

# =============================================================================
# 定数 / 設定
# =============================================================================
PPQ = 480  # MIDI ticks per quarter note
DEFAULT_BPM = 128
GRID_SNAP = 0.5  # ブロック/ノート配置の基本スナップ（ビート単位 = 8分音符）
MIN_NOTE_DUR = 0.125  # 32分音符相当
VISIBLE_BEATS_DEFAULT = 8.0

NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]
ROMAN = ["I", "ii", "iii", "IV", "V", "vi", "vii°"]

# スケール半音（メジャー / ナチュラルマイナー）
MAJ_SEMITONES = [0, 2, 4, 5, 7, 9, 11]
MIN_SEMITONES = [0, 2, 3, 5, 7, 8, 10]

# コード品質ごとのインターバル（rootからの相対半音）
QUALITY_INTERVALS: Dict[str, List[int]] = {
    "": [0, 4, 7],          # maj triad
    "m": [0, 3, 7],         # min triad
    "7": [0, 4, 7, 10],     # dom7
    "maj7": [0, 4, 7, 11],  # maj7
    "m7": [0, 3, 7, 10],    # min7
    "dim": [0, 3, 6],       # dim
    "aug": [0, 4, 8],       # aug
    "sus4": [0, 5, 7],
    "6": [0, 4, 7, 9],
    "m6": [0, 3, 7, 9],
}

# よく使う借用 / セカンダリ（degreeは1-7基準、quality別）
EXTRA_CHORDS = [
    (0, "7"),      # bVII7 風
    (5, "7"),      # bII7? 簡易
    (2, "7"),      # V/V など
]

# 色（ダークで見やすいように調整）
COLOR_BG = (18, 18, 22)
COLOR_PANEL = (26, 26, 31)
COLOR_BORDER = (45, 45, 52)
COLOR_TEXT = (230, 230, 235)
COLOR_ACCENT = (249, 115, 22)      # 暖かみオレンジ（mb-composer風）
COLOR_CHORD_BLOCK = (65, 85, 115)
COLOR_CHORD_BLOCK_ACTIVE = (90, 115, 155)
COLOR_NOTE = (245, 120, 35)
COLOR_NOTE_SEL = (255, 215, 70)
COLOR_ONION = (100, 116, 139, 70)  # 半透明想定（drawではalpha別扱い）
COLOR_GRID = (40, 40, 48)
COLOR_BARLINE = (70, 70, 80)
COLOR_PITCH_BG = (22, 22, 27)

# =============================================================================
# データモデル
# =============================================================================
@dataclass
class ChordBlock:
    start: float
    dur: float
    degree: int = 1          # 1..7
    quality: str = ""        # "", "m", "7", "maj7" ...
    inv: int = 0
    octave: int = 4          # rootのオクターブ

    def end(self) -> float:
        return self.start + self.dur

@dataclass
class Note:
    start: float
    pitch: int
    dur: float
    vel: int = 96

    def end(self) -> float:
        return self.start + self.dur

@dataclass
class Track:
    ch: int  # 1-16
    name: str = ""
    notes: List[Note] = field(default_factory=list)
    patch: int = 0           # GM program (0-127), ch10は無視
    vol: float = 1.0         # 0..1
    muted: bool = False
    solo: bool = False

@dataclass
class Project:
    bpm: float = DEFAULT_BPM
    key_root: int = 0        # 0=C ... 11=B
    is_minor: bool = False
    tracks: List[Track] = field(default_factory=list)
    chord_blocks: List[ChordBlock] = field(default_factory=list)

    def __post_init__(self):
        if not self.tracks:
            for ch in range(1, 17):
                t = Track(ch=ch)
                if ch == 1:
                    t.name = "Chord Blocks (Ch1)"
                elif ch == 10:
                    t.name = "Drums"
                    t.patch = 0
                else:
                    t.name = f"Track {ch}"
                self.tracks.append(t)

    def get_scale_semitones(self) -> List[int]:
        return MIN_SEMITONES if self.is_minor else MAJ_SEMITONES

    def degree_to_root_note(self, degree: int, octave: int) -> int:
        """key_root + degree から root MIDIノートを計算"""
        semis = self.get_scale_semitones()
        idx = (degree - 1) % 7
        semitone_offset = semis[idx]
        # キーからの相対
        root = 60 + self.key_root + semitone_offset + (octave - 4) * 12
        return max(0, min(127, root))

    def chord_to_pitches(self, block: ChordBlock) -> List[int]:
        root = self.degree_to_root_note(block.degree, block.octave)
        intervals = QUALITY_INTERVALS.get(block.quality, [0, 4, 7])
        pitches = [root + i for i in intervals]
        # インバージョン簡易対応（底上げ）
        if block.inv > 0:
            for _ in range(block.inv):
                if pitches:
                    lowest = pitches.pop(0)
                    pitches.append(lowest + 12)
        return [max(0, min(127, p)) for p in pitches]

    def chord_symbol(self, block: ChordBlock) -> str:
        deg = ROMAN[(block.degree - 1) % 7]
        q = block.quality
        sym = deg + (q if q and q not in ("", "m") else "")
        if block.inv:
            sym += f"/{block.inv}"
        return sym

    def get_chord_at(self, beat: float) -> Optional[ChordBlock]:
        for b in self.chord_blocks:
            if b.start <= beat < b.end():
                return b
        return None

# =============================================================================
# ヘルパー
# =============================================================================
def beat_to_ticks(beat: float) -> int:
    return int(round(beat * PPQ))

def ticks_to_beat(ticks: int) -> float:
    return ticks / PPQ

def find_midi_output_port() -> Optional[str]:
    """Windows標準GMシンセや利用可能なポートを探す"""
    if not HAS_RTMIDI or rtmidi is None:
        return None
    try:
        midiout = rtmidi.MidiOut()
        ports = midiout.get_ports()
        for p in ports:
            if "Microsoft GS Wavetable" in p or "GS Wavetable" in p:
                return p
        # 最初の利用可能ポート（VirtualMIDISynth など）
        for p in ports:
            if "loop" not in p.lower() and "through" not in p.lower():
                return p
        return ports[0] if ports else None
    except Exception:
        return None

def expand_chord_blocks_to_notes(proj: Project) -> List[Note]:
    """Ch1のコードブロックを実際のノート列に展開（同時発音）"""
    out: List[Note] = []
    for blk in proj.chord_blocks:
        pitches = proj.chord_to_pitches(blk)
        vel = 80
        for p in pitches:
            out.append(Note(start=blk.start, pitch=p, dur=blk.dur, vel=vel))
    return out

def simple_generate_piano(proj: Project, start_b: float, end_b: float) -> List[Note]:
    """小学校の先生レベル伴奏：コードのタイミングに合わせてトライアドを繰り返し"""
    notes: List[Note] = []
    dur = 0.5
    for blk in proj.chord_blocks:
        if blk.end() <= start_b or blk.start >= end_b:
            continue
        b = max(blk.start, start_b)
        while b < min(blk.end(), end_b):
            pitches = proj.chord_to_pitches(blk)
            for i, p in enumerate(pitches[:3]):
                v = 78 - i * 4
                notes.append(Note(start=b, pitch=p, dur=min(dur, blk.end() - b), vel=v))
            b += dur
    return notes

def simple_generate_bass(proj: Project, start_b: float, end_b: float) -> List[Note]:
    """ルート弾き シンプル4パターン風"""
    notes: List[Note] = []
    for blk in proj.chord_blocks:
        if blk.end() <= start_b or blk.start >= end_b:
            continue
        root = proj.degree_to_root_note(blk.degree, 3)  # 低め
        b = blk.start
        # 1拍目 + 少し後
        notes.append(Note(start=max(b, start_b), pitch=root, dur=0.9, vel=92))
        if blk.dur > 1.0:
            notes.append(Note(start=b + 1.0, pitch=root + 12, dur=0.6, vel=70))
    return notes

def simple_generate_drums(proj: Project, start_b: float, end_b: float) -> List[Note]:
    """8ビート基本 + シンコペーション検知でクラッシュ"""
    notes: List[Note] = []
    kick = 36
    snare = 38
    hh = 42
    crash = 49
    b = math.floor(start_b)
    end = math.ceil(end_b)
    has_syncop = any(blk.dur < 1.5 for blk in proj.chord_blocks if start_b <= blk.start < end_b)

    while b < end:
        # 8ビート
        # Kick on 1 and 3 of each bar-ish
        if (b % 2) < 0.1:
            notes.append(Note(start=b, pitch=kick, dur=0.1, vel=110))
        if abs((b + 1.0) % 2) < 0.1:
            notes.append(Note(start=b + 1.0, pitch=snare, dur=0.1, vel=95))

        # 8分ハイハット
        for off in [0.0, 0.5, 1.0, 1.5]:
            t = b + off
            if start_b <= t < end_b:
                notes.append(Note(start=t, pitch=hh, dur=0.08, vel=70))

        # シンコペでクラッシュ
        if has_syncop and (b % 4) < 0.1:
            notes.append(Note(start=b, pitch=crash, dur=0.6, vel=85))
        b += 2.0
    return notes

def get_chord_name(proj: Project, blk: ChordBlock) -> str:
    root_idx = (proj.key_root + proj.get_scale_semitones()[(blk.degree-1) % 7]) % 12
    root_name = NOTE_NAMES[root_idx]
    qmap = {"": "", "m": "m", "7": "7", "maj7": "M7", "m7": "m7", "dim": "dim", "aug": "+", "sus4": "sus4"}
    return root_name + qmap.get(blk.quality, blk.quality)

# =============================================================================
# アプリケーション本体
# =============================================================================
class JpoProducerApp:
    def __init__(self):
        self.proj = Project()
        self.selected_track = 4          # デフォルトでメロディ想定
        self.current_beat = 0.0
        self.visible_start = 0.0
        self.visible_beats = VISIBLE_BEATS_DEFAULT
        self.is_playing = False
        self.play_thread: Optional[threading.Thread] = None
        self.stop_event = threading.Event()
        self.midi_out_port_name: Optional[str] = None
        self.midi_out: Optional[rtmidi.MidiOut] = None

        self.selected_block_idx: Optional[int] = None
        self.selected_note_idx: Optional[int] = None   # (track, note_index) 的に使う
        self.selected_notes: List[Tuple[int, int]] = []  # (track_ch, note_idx)

        self.paint_grid = GRID_SNAP
        self.range_start = 0.0
        self.range_end = 4.0

        # Edit tools & input settings
        self.edit_mode = "pencil"          # "pencil" or "eraser"
        self.note_length_beats = 0.5       # default 1/8

        # Playback engines
        self.fs = None          # fluidsynth.Synth
        self.sf2_id = None
        self.midi_out = None
        self.midi_out_port_name = None

        self._init_fluidsynth()   # preferred (user-provided FluidR3 GM.SF2)
        self._init_midi_out()     # rtmidi fallback
        self._init_dearpygui()

    def _init_midi_out(self):
        self.midi_out_port_name = None
        if not HAS_RTMIDI:
            print("[MIDI] rtmidi not available — playback disabled (editing + export still fully work).")
            return
        self.midi_out = None  # ensure clean if previous attempt failed
        try:
            self.midi_out = rtmidi.MidiOut()
            port = find_midi_output_port()
            if port:
                self.midi_out.open_port(self.midi_out.get_ports().index(port))
                self.midi_out_port_name = port
                print(f"[MIDI] Output: {port}")
            else:
                print("[MIDI] No output port found. Playback disabled.")
        except Exception as e:
            print(f"[MIDI] Failed to init rtmidi: {e}")
            self.midi_out = None

    def _setup_fonts(self):
        """Load a Japanese-capable font so labels don't turn into ????"""
        candidates = [
            r"C:\Windows\Fonts\meiryo.ttc",
            r"C:\Windows\Fonts\msgothic.ttc",
            r"C:\Windows\Fonts\YuGothM.ttc",
            r"C:\Windows\Fonts\YuGothR.ttc",
        ]
        for path in candidates:
            if os.path.exists(path):
                try:
                    with dpg.font_registry():
                        with dpg.font(path, 14, tag="jp_font") as font:
                            dpg.add_font_range_hint(dpg.mvFontRangeHint_Default)
                            dpg.add_font_range_hint(dpg.mvFontRangeHint_Japanese)
                            # extra CJK range for safety
                            dpg.add_font_range(0x3000, 0x9FFF)
                    dpg.bind_font("jp_font")
                    print(f"[Font] Japanese font loaded: {path}")
                    return
                except Exception as e:
                    print(f"[Font] Failed to load {path}: {e}")
        print("[Font] No CJK font found — some Japanese text may show as ???? (mojibake). Install Meiryo or Gothic and restart.")

    def _init_fluidsynth(self):
        """Initialize FluidSynth with the user-provided FluidR3 GM soundfont as default."""
        global HAS_FLUIDSYNTH, fluidsynth
        try:
            import fluidsynth as _fs
            fluidsynth = _fs
            HAS_FLUIDSYNTH = True
        except Exception as e:
            print(f"[FluidSynth] pyfluidsynth / FluidSynth library not available: {e}")
            HAS_FLUIDSYNTH = False
            return

        try:
            self.fs = fluidsynth.Synth(gain=0.75)
            # Try to start audio driver (Windows friendly first)
            started = False
            for drv in ["wasapi", "dsound", "portaudio", None]:
                try:
                    if drv:
                        self.fs.start(driver=drv)
                    else:
                        self.fs.start()
                    started = True
                    break
                except Exception:
                    continue
            if not started:
                print("[FluidSynth] Could not start audio driver.")
                self.fs = None
                return

            # Locate the soundfont next to main.py or in the project root
            base = os.path.dirname(__file__) or "."
            sf_path = os.path.abspath(os.path.join(base, "FluidR3 GM.SF2"))
            if not os.path.exists(sf_path):
                # fallback search
                sf_path = os.path.abspath(os.path.join(base, "FluidR3_GM.sf2"))
            if os.path.exists(sf_path):
                self.sf2_id = self.fs.sfload(sf_path)
                print(f"[FluidSynth] Soundfont loaded: {sf_path}")
                # GM defaults: set a basic piano on all channels for sketch (user can change per track later)
                for ch in range(16):
                    try:
                        self.fs.program_select(ch, self.sf2_id, 0, 0)
                    except Exception:
                        pass
            else:
                print(f"[FluidSynth] Soundfont not found at {sf_path}. Place 'FluidR3 GM.SF2' next to main.py.")
                self.fs = None
        except Exception as e:
            print(f"[FluidSynth] Failed to initialize: {e}")
            self.fs = None

    def _init_dearpygui(self):
        dpg.create_context()
        self._setup_fonts()
        dpg.create_viewport(title="JpoProducer - J-Pop/J-Rock Sketch Tool", width=1280, height=820)
        dpg.setup_dearpygui()

        # ダークテーマ（Dominoより少し見やすく）
        with dpg.theme() as global_theme:
            with dpg.theme_component(dpg.mvAll):
                dpg.add_theme_color(dpg.mvThemeCol_WindowBg, COLOR_BG)
                dpg.add_theme_color(dpg.mvThemeCol_ChildBg, COLOR_PANEL)
                dpg.add_theme_color(dpg.mvThemeCol_Border, COLOR_BORDER)
                dpg.add_theme_color(dpg.mvThemeCol_Text, COLOR_TEXT)
                dpg.add_theme_color(dpg.mvThemeCol_Button, (45, 45, 55))
                dpg.add_theme_color(dpg.mvThemeCol_ButtonHovered, (70, 70, 85))
                dpg.add_theme_color(dpg.mvThemeCol_ButtonActive, COLOR_ACCENT)
                dpg.add_theme_color(dpg.mvThemeCol_FrameBg, (30, 30, 36))
                dpg.add_theme_color(dpg.mvThemeCol_Header, (55, 65, 80))
                dpg.add_theme_color(dpg.mvThemeCol_HeaderHovered, (80, 90, 110))
                dpg.add_theme_color(dpg.mvThemeCol_CheckMark, COLOR_ACCENT)
            with dpg.theme_component(dpg.mvInputInt):
                dpg.add_theme_color(dpg.mvThemeCol_FrameBg, (35, 35, 42))
        dpg.bind_theme(global_theme)

        # メインメニュー
        with dpg.viewport_menu_bar():
            with dpg.menu(label="File"):
                dpg.add_menu_item(label="New Project", callback=self.new_project)
                dpg.add_menu_item(label="Open MIDI...", callback=self.open_midi)
                dpg.add_menu_item(label="Save MIDI As...", callback=self.save_midi)
                dpg.add_separator()
                dpg.add_menu_item(label="Exit", callback=lambda: dpg.stop_dearpygui())
            with dpg.menu(label="Edit"):
                dpg.add_menu_item(label="Clear Current Track Notes", callback=self.clear_current_track)
                dpg.add_menu_item(label="Clear All Chord Blocks", callback=self.clear_chord_blocks)
            with dpg.menu(label="Help"):
                dpg.add_menu_item(label="About JpoProducer", callback=self.show_about)

        # メインウィンドウ
        with dpg.window(tag="main_window", label="JpoProducer", no_title_bar=True, no_move=True, no_resize=True, no_close=True):
            # ツールバー
            with dpg.group(horizontal=True):
                dpg.add_text("BPM")
                dpg.add_input_float(tag="bpm_input", default_value=self.proj.bpm, width=70,
                                    callback=self.on_bpm_changed, step=1.0, format="%.0f")
                dpg.add_spacer(width=12)

                dpg.add_text("Key")
                dpg.add_combo(items=[NOTE_NAMES[i] for i in range(12)], default_value=NOTE_NAMES[self.proj.key_root],
                              width=55, tag="key_combo", callback=self.on_key_changed)
                dpg.add_combo(items=["Major", "Minor"], default_value="Minor" if self.proj.is_minor else "Major",
                              width=70, tag="mode_combo", callback=self.on_key_changed)
                dpg.add_spacer(width=12)

                # Edit tool + note length (user request: not just fixed input, have delete tool)
                dpg.add_text("Tool")
                dpg.add_button(label="Pencil", width=60, callback=self.set_edit_mode, user_data="pencil")
                dpg.add_button(label="Eraser", width=60, callback=self.set_edit_mode, user_data="eraser")
                dpg.add_spacer(width=8)
                dpg.add_text("Len")
                dpg.add_combo(items=["1/16", "1/8", "1/4", "1/2", "1", "2"], default_value="1/8",
                              width=55, tag="note_len_combo", callback=self.on_note_len_changed)

                dpg.add_spacer(width=12)

                # Transport
                dpg.add_button(label="▶ Play", tag="btn_play", callback=self.toggle_play, width=80)
                dpg.add_button(label="■ Stop", callback=self.stop_playback, width=70)
                dpg.add_text("Pos:", tag="pos_text")
                dpg.add_spacer(width=8)
                dpg.add_slider_float(tag="time_slider", default_value=0.0, min_value=0.0, max_value=64.0,
                                     width=200, callback=self.on_time_slider)

                dpg.add_spacer(width=16)
                dpg.add_button(label="Grok", callback=self.open_grok_dialog, width=70)
                dpg.add_text("(chord ideas)", color=(140,145,155))

            dpg.add_separator()

            # 本体レイアウト
            with dpg.group(horizontal=True):
                # === 左：トラック一覧 ===
                with dpg.child_window(width=190, height=520, border=True, tag="track_panel"):
                    dpg.add_text("TRACKS (click to select)")
                    dpg.add_separator()
                    for i in range(16):
                        ch = i + 1
                        is_ch1 = (ch == 1)
                        label = f"Ch{ch:02d} " + ("[Chord]" if is_ch1 else self.proj.tracks[i].name)
                        dpg.add_selectable(label=label, tag=f"track_sel_{ch}",
                                           callback=self.select_track, user_data=ch)
                    dpg.add_spacer(height=6)
                    dpg.add_text("Synth / Out:")
                    synth_label = "FluidSynth (FluidR3 GM)" if self.fs else (self.midi_out_port_name or "None (no playback)")
                    dpg.add_text(synth_label, color=(120, 200, 160) if self.fs else (180,180,190), wrap=170)

                # === 中央：エディタエリア ===
                with dpg.group():
                    # Chord Timeline（常時表示 + オニオンソース）
                    dpg.add_text("CHORD TIMELINE (Ch1)  —  drag to paint blocks • click block for palette", color=(190,200,215))
                    with dpg.drawlist(width=980, height=92, tag="chord_drawlist"):
                        pass

                    dpg.add_spacer(height=2)

                    # Piano Roll
                    dpg.add_text("PIANO ROLL  —  edit selected track (onion = Ch1 chords visible in background)", color=(190,200,215))
                    with dpg.drawlist(width=980, height=380, tag="piano_drawlist"):
                        pass

                    # ズーム・スクロール
                    with dpg.group(horizontal=True):
                        dpg.add_text("Time Zoom")
                        dpg.add_slider_float(tag="zoom_slider", default_value=self.visible_beats,
                                             min_value=2.0, max_value=32.0, width=160,
                                             callback=self.on_zoom)
                        dpg.add_text("Scroll")
                        dpg.add_slider_float(tag="scroll_slider", default_value=self.visible_start,
                                             min_value=0.0, max_value=64.0, width=300,
                                             callback=self.on_scroll)
                        dpg.add_button(label="Fit 16 bars", callback=lambda: self.set_visible(0, 16))
                        dpg.add_button(label="Fit 8 bars", callback=lambda: self.set_visible(0, 8))

                    # 範囲選択 + Generate
                    with dpg.group(horizontal=True):
                        dpg.add_text("Generate Range (beats)")
                        dpg.add_input_float(tag="range_start", default_value=self.range_start, width=70,
                                            callback=self.on_range_changed, step=0.5)
                        dpg.add_text("→")
                        dpg.add_input_float(tag="range_end", default_value=self.range_end, width=70,
                                            callback=self.on_range_changed, step=0.5)
                        dpg.add_button(label="Generate All (Piano+Bass+Drum)", callback=self.do_generate_all, width=220)
                        dpg.add_button(label="Clear Generated (Ch2,3,10)", callback=self.clear_generated, width=170)
                        dpg.add_spacer(width=12)
                        dpg.add_button(label="Delete Selected Note", callback=self.delete_selected_note, width=145)
                        dpg.add_button(label="Delete Sel. Chord Block", callback=self.delete_selected_block, width=160)

            dpg.add_separator()

            # ステータスバー風
            with dpg.group(horizontal=True):
                dpg.add_text(tag="status_text", default_value="Ready. Paint chords on the top timeline. Select a track below to edit melody etc.")
                dpg.add_spacer(width=40)
                dpg.add_text("Tip: Right-click = delete • Drag in top timeline to paint chords • Onion always shows Ch1 progress")

        # コードパレット用ウィンドウ（モーダル風）
        with dpg.window(label="Chord Palette", modal=True, show=False, tag="chord_palette_win",
                        width=520, height=340, pos=(300, 180)):
            dpg.add_text("Select chord for the block (degree + quality). Changes apply immediately.")
            dpg.add_separator()
            with dpg.group(horizontal=True):
                for deg in range(1, 8):
                    dpg.add_button(label=ROMAN[deg-1], width=60, height=32,
                                   callback=self.apply_chord_choice, user_data=(deg, ""))
            dpg.add_spacer(height=4)
            dpg.add_text("7ths / variations")
            with dpg.group(horizontal=True):
                for q in ["7", "maj7", "m7", "dim", "aug", "sus4", "6", "m6"]:
                    dpg.add_button(label=q, width=52, height=26,
                                   callback=self.apply_chord_choice, user_data=(None, q))
            dpg.add_spacer(height=4)
            dpg.add_text("Borrowed / Secondary (common in J-Rock/J-Pop)")
            with dpg.group(horizontal=True):
                borrowed = [(1, "m"), (4, "m"), (5, "7"), (6, "m"), (7, "7"), (2, "7")]
                for deg, q in borrowed:
                    dpg.add_button(label=f"{ROMAN[deg-1]}{q or ''}", width=58,
                                   callback=self.apply_chord_choice, user_data=(deg, q))
            dpg.add_spacer(height=8)
            dpg.add_button(label="Close", callback=lambda: dpg.hide_item("chord_palette_win"))

        # Grokダイアログ
        with dpg.window(label="Grok Chord Suggestion Prompt", modal=True, show=False, tag="grok_win",
                        width=620, height=280, pos=(280, 200)):
            dpg.add_text("このプロンプトをコピーしてGrokやLLMに投げてください。", color=(200,210,220))
            dpg.add_input_text(tag="grok_prompt", multiline=True, height=120, width=580, readonly=True)
            with dpg.group(horizontal=True):
                dpg.add_button(label="Copy to Clipboard", callback=self.copy_grok_prompt, width=160)
                dpg.add_button(label="Close", callback=lambda: dpg.hide_item("grok_win"))

        # マウスハンドラ（handler_registry に入れてグローバル化 + 各コールバック内で hovered 判定）
        with dpg.handler_registry(tag="global_mouse_handlers"):
            dpg.add_mouse_down_handler(callback=self.on_mouse_down)
            dpg.add_mouse_release_handler(callback=self.on_mouse_up)
            dpg.add_mouse_drag_handler(callback=self.on_mouse_drag)

        # 定期再描画（再生中など）
        dpg.set_frame_callback(0, self.on_frame)

        dpg.show_viewport()
        self.refresh_ui()

    # -------------------------------------------------------------------------
    # UI 更新 / 描画
    # -------------------------------------------------------------------------
    def refresh_ui(self):
        # トラック選択表示更新
        for ch in range(1, 17):
            sel = dpg.get_value(f"track_sel_{ch}") or False
            dpg.configure_item(f"track_sel_{ch}", default_value=(ch == self.selected_track))

        dpg.set_value("bpm_input", self.proj.bpm)
        dpg.set_value("key_combo", NOTE_NAMES[self.proj.key_root])
        dpg.set_value("mode_combo", "Minor" if self.proj.is_minor else "Major")
        dpg.set_value("time_slider", self.current_beat)
        dpg.set_value("zoom_slider", self.visible_beats)
        dpg.set_value("scroll_slider", self.visible_start)
        dpg.set_value("range_start", self.range_start)
        dpg.set_value("range_end", self.range_end)
        dpg.set_value("pos_text", f" {self.current_beat:.2f} beats")
        # keep note length combo in sync
        rev = {0.25:"1/16", 0.5:"1/8", 1.0:"1/4", 2.0:"1/2", 4.0:"1", 8.0:"2"}
        dpg.set_value("note_len_combo", rev.get(self.note_length_beats, "1/8"))

        self.draw_chord_timeline()
        self.draw_piano_roll()
        self.update_status()

    def update_status(self):
        ch = self.selected_track
        blk_count = len(self.proj.chord_blocks)
        note_count = len(self.proj.tracks[ch-1].notes)
        tool = self.edit_mode.upper()
        ln = {0.25:"1/16", 0.5:"1/8", 1:"1/4", 2:"1/2", 4:"1", 8:"2"}.get(self.note_length_beats, "?")
        dpg.set_value("status_text",
                      f"Ch{ch:02d} | Chord blocks: {blk_count} | Notes: {note_count} | Tool:{tool} Len:{ln} | "
                      f"Key: {NOTE_NAMES[self.proj.key_root]}{'m' if self.proj.is_minor else ''} @ {self.proj.bpm} BPM")

    def set_visible(self, start: float, beats: float):
        self.visible_start = max(0.0, start)
        self.visible_beats = max(2.0, min(32.0, beats))
        self.refresh_ui()

    def on_zoom(self, sender, app_data):
        self.visible_beats = float(app_data)
        self.refresh_ui()

    def on_scroll(self, sender, app_data):
        self.visible_start = float(app_data)
        self.refresh_ui()

    def on_time_slider(self, sender, app_data):
        self.current_beat = float(app_data)
        dpg.set_value("pos_text", f" {self.current_beat:.2f} beats")
        self.refresh_ui()

    def on_range_changed(self, sender, app_data):
        self.range_start = dpg.get_value("range_start")
        self.range_end = dpg.get_value("range_end")

    def on_bpm_changed(self, sender, app_data):
        self.proj.bpm = max(40.0, min(240.0, float(app_data)))
        self.update_status()

    def on_key_changed(self, sender, app_data):
        root = NOTE_NAMES.index(dpg.get_value("key_combo"))
        mode = dpg.get_value("mode_combo")
        self.proj.key_root = root
        self.proj.is_minor = (mode == "Minor")
        self.draw_chord_timeline()
        self.draw_piano_roll()
        self.update_status()

    def set_edit_mode(self, sender, app_data, user_data):
        self.edit_mode = str(user_data)
        self.update_status()

    def on_note_len_changed(self, sender, app_data):
        val = dpg.get_value("note_len_combo")
        mapping = {"1/16": 0.25, "1/8": 0.5, "1/4": 1.0, "1/2": 2.0, "1": 4.0, "2": 8.0}
        self.note_length_beats = mapping.get(val, 0.5)
        self.paint_grid = self.note_length_beats   # also affect block snap min size
        self.update_status()

    def select_track(self, sender, app_data, user_data):
        self.selected_track = int(user_data)
        self.selected_note_idx = None
        self.selected_notes.clear()
        if self.selected_track == 1:
            dpg.set_value("status_text", "Ch1 selected: Use the top CHORD TIMELINE to drag-paint blocks (variable length). They become the actual MIDI notes.")
        self.refresh_ui()

    # -------------------------------------------------------------------------
    # 描画：Chord Timeline（ブロック塗り + オニオンソース）
    # -------------------------------------------------------------------------
    def draw_chord_timeline(self):
        dl = "chord_drawlist"
        dpg.delete_item(dl, children_only=True)

        w = dpg.get_item_width(dl) or 980
        h = dpg.get_item_height(dl) or 78

        # 背景
        dpg.draw_rectangle((0, 0), (w, h), color=COLOR_BORDER, fill=COLOR_PANEL, parent=dl)

        # 空のときのヒント（Ch1が生きてることを視覚的に伝える）
        if not self.proj.chord_blocks:
            dpg.draw_text((18, 28), "← drag in this area to paint chord blocks", color=(85, 90, 105), size=12, parent=dl)

        # グリッド
        pixels_per_beat = w / self.visible_beats
        start_b = self.visible_start
        end_b = start_b + self.visible_beats

        # 拍線
        b = math.floor(start_b)
        while b <= end_b + 0.01:
            x = (b - start_b) * pixels_per_beat
            is_bar = (b % 4) < 0.01
            thick = 2.0 if is_bar else 1.0
            dpg.draw_line((x, 4), (x, h-4), color=(100, 100, 115) if is_bar else (55, 55, 65), thickness=thick, parent=dl)
            # 小節番号
            if (b % 4) < 0.01:
                dpg.draw_text((x+2, h-16), f"{int(b/4)+1}", color=(140,140,150), size=11, parent=dl)
            b += self.paint_grid

        # コードブロック描画
        for idx, blk in enumerate(self.proj.chord_blocks):
            if blk.end() < start_b or blk.start > end_b:
                continue
            x0 = max(0, (blk.start - start_b) * pixels_per_beat)
            x1 = min(w, (blk.end() - start_b) * pixels_per_beat)
            if x1 - x0 < 3:
                continue
            color = COLOR_CHORD_BLOCK_ACTIVE if idx == self.selected_block_idx else COLOR_CHORD_BLOCK
            dpg.draw_rectangle((x0, 8), (x1, h-8), color=color, fill=color, parent=dl, rounding=3.0)

            # ラベル
            sym = self.proj.chord_symbol(blk)
            actual = get_chord_name(self.proj, blk)
            label = f"{sym} ({actual})"
            dpg.draw_text((x0 + 5, 18), label, color=(235, 240, 250), size=13, parent=dl)

            # 選択枠
            if idx == self.selected_block_idx:
                dpg.draw_rectangle((x0, 8), (x1, h-8), color=COLOR_ACCENT, thickness=2.0, parent=dl)

    # -------------------------------------------------------------------------
    # 描画：Piano Roll + オニオン（超重要）
    # -------------------------------------------------------------------------
    def draw_piano_roll(self):
        dl = "piano_drawlist"
        dpg.delete_item(dl, children_only=True)

        w = dpg.get_item_width(dl) or 980
        h = dpg.get_item_height(dl) or 380
        pixels_per_beat = w / self.visible_beats
        start_b = self.visible_start
        end_b = start_b + self.visible_beats

        track = self.proj.tracks[self.selected_track - 1]

        # 背景
        dpg.draw_rectangle((0, 0), (w, h), color=COLOR_BORDER, fill=COLOR_PITCH_BG, parent=dl)

        # === オニオン表示（Ch1のコード + スケール）===
        if self.selected_track != 1:
            # スケール行ハイライト（薄く）
            scale_semis = set(self.proj.get_scale_semitones())
            base_pitch = 48  # C3 くらいから
            for pitch in range(36, 84):
                rel = (pitch - self.proj.key_root) % 12
                is_scale = rel in scale_semis
                y_center = self.pitch_to_y(pitch, h)
                if is_scale:
                    dpg.draw_rectangle((0, y_center - 7), (w, y_center + 7),
                                       fill=(55, 70, 95, 35), color=(0,0,0,0), parent=dl)

            # コードブロックを半透明で描く（フルハイト薄帯 + ラベル）
            for blk in self.proj.chord_blocks:
                if blk.end() <= start_b or blk.start >= end_b:
                    continue
                x0 = max(0, (blk.start - start_b) * pixels_per_beat)
                x1 = min(w, (blk.end() - start_b) * pixels_per_beat)
                # 薄い帯 (onion / ghost chords)
                dpg.draw_rectangle((x0, 0), (x1, h), fill=(85, 115, 155, 48), color=(0,0,0,0), parent=dl)
                # ラベル
                label = self.proj.chord_symbol(blk) + " " + get_chord_name(self.proj, blk)
                dpg.draw_text((x0 + 3, 6), label, color=(160, 175, 195), size=11, parent=dl)

        # グリッド線（時間）
        b = math.floor(start_b)
        while b <= end_b + 0.01:
            x = (b - start_b) * pixels_per_beat
            is_bar = (b % 4) < 0.01
            dpg.draw_line((x, 0), (x, h), color=(95, 95, 110) if is_bar else (55, 55, 65),
                          thickness=2.0 if is_bar else 1.0, parent=dl)
            b += 0.5

        # 横線（オクターブ強調）
        for pitch in range(36, 85, 12):
            y = self.pitch_to_y(pitch, h)
            dpg.draw_line((0, y), (w, y), color=(60, 60, 70), thickness=1.0, parent=dl)

        # Piano key labels on the left (only C notes for clarity)
        for pitch in range(36, 85, 12):  # every C
            y = self.pitch_to_y(pitch, h)
            if 2 < y < h - 10:
                octave = (pitch // 12) - 1
                dpg.draw_text((4, y - 6), f"C{octave}", size=10,
                              color=(180, 190, 205), parent=dl)
                # subtle key line
                dpg.draw_line((0, y), (w, y), color=(55, 55, 65), thickness=1.0, parent=dl)

        # ノート描画
        is_ch1 = (self.selected_track == 1)
        notes = track.notes
        if is_ch1:
            # Ch1は展開したコードノートをゴースト表示
            notes = expand_chord_blocks_to_notes(self.proj)

        for idx, note in enumerate(notes):
            if note.end() < start_b or note.start > end_b:
                continue
            x0 = (note.start - start_b) * pixels_per_beat
            x1 = (note.end() - start_b) * pixels_per_beat
            y0 = self.pitch_to_y(note.pitch + 0.5, h)   # 少し太めに
            y1 = self.pitch_to_y(note.pitch - 0.5, h)

            if x1 - x0 < 2:
                x1 = x0 + 2

            sel = (self.selected_track, idx) in self.selected_notes or idx == self.selected_note_idx
            fill = COLOR_NOTE_SEL if sel else (COLOR_ACCENT if is_ch1 else COLOR_NOTE)
            dpg.draw_rectangle((x0, y0), (x1, y1), fill=fill, color=(255,255,255,120) if sel else (40,40,40),
                               parent=dl, rounding=2.0)

            # 短いノートにはラベル出さない（見にくくなるので）
            if x1 - x0 > 28:
                dpg.draw_text((x0 + 2, (y0 + y1) / 2 - 5), NOTE_NAMES[note.pitch % 12], size=9,
                              color=(20, 20, 25), parent=dl)

        # 再生位置インジケータ
        if start_b <= self.current_beat <= end_b:
            x = (self.current_beat - start_b) * pixels_per_beat
            dpg.draw_line((x, 0), (x, h), color=(250, 250, 120), thickness=2.0, parent=dl)

    def pitch_to_y(self, pitch: float, height: float) -> float:
        # 36(C2)〜84(C6)くらいをフル表示
        min_p = 36
        max_p = 84
        norm = (max_p - pitch) / (max_p - min_p)
        return norm * height

    def y_to_pitch(self, y: float, height: float) -> int:
        min_p = 36
        max_p = 84
        norm = 1.0 - (y / height)
        p = min_p + norm * (max_p - min_p)
        return max(min_p, min(max_p, int(round(p))))

    def _get_local_mouse_for_drawlist(self, tag: str):
        """Reliable local coordinates for a drawlist using global mouse + item rect.
        Returns (x, y) local or None if not hovered/inside.
        This fixes the 'notes appear far from mouse click' problem.
        """
        if not dpg.does_item_exist(tag):
            return None
        if not dpg.is_item_hovered(tag):
            return None
        try:
            mx, my = dpg.get_mouse_pos(local=False)  # global screen coords
            ix, iy = dpg.get_item_rect_min(tag)
            w = dpg.get_item_width(tag) or 980
            h = dpg.get_item_height(tag) or 380
            lx = mx - ix
            ly = my - iy
            if 0 <= lx <= w and 0 <= ly <= h:
                return (lx, ly)
        except Exception:
            pass
        return None

    # -------------------------------------------------------------------------
    # マウス操作（ブロック / ノート） — robust local coord version
    # -------------------------------------------------------------------------
    def on_mouse_down(self, sender, app_data):
        # Try chord timeline first
        pos = self._get_local_mouse_for_drawlist("chord_drawlist")
        if pos is not None:
            is_right = (app_data == 1)
            if is_right:
                self._delete_at_chord_mouse(pos)
            else:
                self._handle_chord_mouse("down", pos)
            return

        # Then piano roll
        pos = self._get_local_mouse_for_drawlist("piano_drawlist")
        if pos is not None:
            is_right = (app_data == 1)
            if is_right:
                self._delete_at_piano_mouse(pos)
            else:
                self._handle_piano_mouse("down", pos)
            return

    def on_mouse_up(self, sender, app_data):
        # Use the same robust getter (may be None if mouse left the area)
        pos = self._get_local_mouse_for_drawlist("chord_drawlist")
        if pos is not None:
            self._handle_chord_mouse("up", pos)
        else:
            self._handle_chord_mouse("up", (0, 0))

        pos = self._get_local_mouse_for_drawlist("piano_drawlist")
        if pos is not None:
            self._handle_piano_mouse("up", pos)
        else:
            self._handle_piano_mouse("up", (0, 0))
        self.refresh_ui()

    def on_mouse_drag(self, sender, app_data):
        pos = self._get_local_mouse_for_drawlist("chord_drawlist")
        if pos is not None:
            self._handle_chord_mouse("drag", pos)
            return
        pos = self._get_local_mouse_for_drawlist("piano_drawlist")
        if pos is not None:
            self._handle_piano_mouse("drag", pos)

    def _handle_chord_mouse(self, phase: str, pos: Tuple[float, float]):
        dl = "chord_drawlist"
        w = dpg.get_item_width(dl) or 980
        h = dpg.get_item_height(dl) or 78
        pixels_per_beat = w / self.visible_beats
        start_b = self.visible_start

        beat = start_b + (pos[0] / pixels_per_beat)
        beat = round(beat / self.paint_grid) * self.paint_grid   # snap

        if phase == "down":
            # 既存ブロックをクリックしたか？
            hit_idx = None
            for idx, blk in enumerate(self.proj.chord_blocks):
                if blk.start - 0.1 <= beat < blk.end() + 0.1:
                    hit_idx = idx
                    break

            if hit_idx is not None:
                if self.edit_mode == "eraser":
                    # eraserでブロック削除
                    del self.proj.chord_blocks[hit_idx]
                    self.selected_block_idx = None
                    self.draw_chord_timeline()
                    self.draw_piano_roll()
                    self.update_status()
                    return
                self.selected_block_idx = hit_idx
                dpg.show_item("chord_palette_win")
                self.draw_chord_timeline()
                return

            if self.edit_mode == "eraser":
                return  # 何もないところでeraserは無視

            # 新規ブロック作成（ドラッグで長さ決定開始） — Ch1のメイン入力ツール
            new_blk = ChordBlock(start=beat, dur=max(self.paint_grid, self.note_length_beats), degree=1, quality="")
            self.proj.chord_blocks.append(new_blk)
            self.proj.chord_blocks.sort(key=lambda b: b.start)
            self.selected_block_idx = len(self.proj.chord_blocks) - 1
            self._chord_drag_start = beat
            self.draw_chord_timeline()

        elif phase == "drag" and hasattr(self, "_chord_drag_start") and self.selected_block_idx is not None:
            blk = self.proj.chord_blocks[self.selected_block_idx]
            new_end = max(blk.start + self.paint_grid, beat)
            blk.dur = max(self.paint_grid, round((new_end - blk.start) / self.paint_grid) * self.paint_grid)
            self.draw_chord_timeline()

        elif phase == "up":
            if hasattr(self, "_chord_drag_start"):
                del self._chord_drag_start
            self.draw_chord_timeline()
            self.update_status()

    def _handle_piano_mouse(self, phase: str, pos: Tuple[float, float]):
        dl = "piano_drawlist"
        w = dpg.get_item_width(dl) or 980
        h = dpg.get_item_height(dl) or 380
        pixels_per_beat = w / self.visible_beats
        start_b = self.visible_start
        track_idx = self.selected_track - 1
        track = self.proj.tracks[track_idx]

        beat = start_b + (pos[0] / pixels_per_beat)
        pitch = self.y_to_pitch(pos[1], h)

        # Ch1はブロック入力専用（トップのChord Timelineで塗る）。ピアノロールは結果表示。
        is_ch1 = (self.selected_track == 1)

        if phase == "down":
            self.selected_note_idx = None
            self.selected_notes.clear()

            # 既存ノートをヒットしたか？
            hit_idx = None
            for idx, n in enumerate(track.notes):
                if n.start - 0.05 <= beat <= n.end() + 0.05 and n.pitch - 1 <= pitch <= n.pitch + 1:
                    hit_idx = idx
                    break

            if hit_idx is not None:
                # ヒット → 選択してドラッグ準備（eraserなら即削除）
                if self.edit_mode == "eraser":
                    del track.notes[hit_idx]
                    self.draw_piano_roll()
                    self.update_status()
                    return
                self.selected_note_idx = hit_idx
                n = track.notes[hit_idx]
                self._drag_note_original = (n.start, n.pitch, n.dur)
                self._drag_start_mouse_beat = beat
                self._drag_start_mouse_pitch = pitch
                self._drag_mode = "resize" if beat > n.end() - 0.25 else "move"
                self.draw_piano_roll()
                return

            # 何もない場所をクリック
            if self.edit_mode == "eraser":
                # eraserは何もしない（または範囲削除は将来）
                return

            if is_ch1:
                # Ch1はトップタイムラインでコードブロックを入力してください
                dpg.set_value("status_text", "Ch1 input: drag in the CHORD TIMELINE above to paint blocks (they turn into notes). Piano roll here is preview only.")
                self.draw_piano_roll()
                return

            # Pencil: 新規ノート作成（現在のLenで開始、ドラッグで長さを伸ばせる）
            dur = max(MIN_NOTE_DUR, self.note_length_beats)
            new_n = Note(start=round(beat / GRID_SNAP) * GRID_SNAP, pitch=pitch, dur=dur, vel=92)
            track.notes.append(new_n)
            track.notes.sort(key=lambda n: (n.start, n.pitch))
            self.selected_note_idx = len(track.notes) - 1
            self._drag_note_original = (new_n.start, new_n.pitch, new_n.dur)
            self._drag_mode = "resize"          # すぐ右ドラッグで長さ調整可能
            self._is_creating_new = True        # フラグで区別
            self.draw_piano_roll()

        elif phase == "drag" and self.selected_note_idx is not None:
            n = track.notes[self.selected_note_idx]
            orig_start, orig_pitch, orig_dur = self._drag_note_original
            db = beat - self._drag_start_mouse_beat
            dp = pitch - self._drag_start_mouse_pitch

            if getattr(self, "_drag_mode", "move") == "move":
                n.start = max(0.0, round((orig_start + db) / GRID_SNAP) * GRID_SNAP)
                n.pitch = max(36, min(84, orig_pitch + dp))
            else:
                # resize / create dragで長さ変更
                new_dur = max(MIN_NOTE_DUR, round((orig_dur + db) / GRID_SNAP) * GRID_SNAP)
                n.dur = new_dur

            self.draw_piano_roll()

        elif phase == "up":
            if getattr(self, "_is_creating_new", False):
                self._is_creating_new = False
            self.draw_piano_roll()
            self.update_status()

    def _delete_at_piano_mouse(self, pos: Tuple[float, float]):
        dl = "piano_drawlist"
        w = dpg.get_item_width(dl) or 980
        h = dpg.get_item_height(dl) or 380
        pixels_per_beat = w / self.visible_beats
        start_b = self.visible_start
        track = self.proj.tracks[self.selected_track - 1]

        beat = start_b + (pos[0] / pixels_per_beat)
        pitch = self.y_to_pitch(pos[1], h)

        for idx, n in enumerate(track.notes):
            if n.start - 0.08 <= beat <= n.end() + 0.08 and n.pitch - 1 <= pitch <= n.pitch + 1:
                del track.notes[idx]
                if self.selected_note_idx == idx:
                    self.selected_note_idx = None
                self.draw_piano_roll()
                self.update_status()
                return

    def _delete_at_chord_mouse(self, pos: Tuple[float, float]):
        dl = "chord_drawlist"
        w = dpg.get_item_width(dl) or 980
        pixels_per_beat = w / self.visible_beats
        start_b = self.visible_start
        beat = start_b + (pos[0] / pixels_per_beat)

        for idx, blk in enumerate(self.proj.chord_blocks):
            if blk.start - 0.15 <= beat < blk.end() + 0.15:
                del self.proj.chord_blocks[idx]
                if self.selected_block_idx == idx:
                    self.selected_block_idx = None
                self.draw_chord_timeline()
                self.draw_piano_roll()
                self.update_status()
                return

    def on_mouse_right_click(self):  # 現在未使用、右クリックはdpgで直接取るのが面倒なので簡易削除
        pass

    # 右クリック削除はシンプルに「選択中ノートを削除」ボタン推奨だが、簡単のためマウスアップ時に右ボタン判定を追加
    # 簡易対応：選択ノートがある状態で右クリック領域で削除
    # 実際は on_mouse_up 内でボタンをチェック
    # ここでは on_frame や他の方法で対応。右クリックは「選択削除」で代用（UIにボタン追加済みではないがマウスで十分）

    def apply_chord_choice(self, sender, app_data, user_data):
        if self.selected_block_idx is None:
            return
        blk = self.proj.chord_blocks[self.selected_block_idx]
        deg, qual = user_data
        if deg is not None:
            blk.degree = deg
        if qual is not None:
            blk.quality = qual
        dpg.hide_item("chord_palette_win")
        self.draw_chord_timeline()
        self.draw_piano_roll()
        self.update_status()

    # -------------------------------------------------------------------------
    # 再生（別スレッド）
    # -------------------------------------------------------------------------
    def toggle_play(self):
        if self.is_playing:
            self.stop_playback()
        else:
            self.start_playback()

    def start_playback(self):
        has_playback = self.fs is not None or self.midi_out is not None
        if self.is_playing or not has_playback:
            if not has_playback:
                dpg.set_value("status_text", "Playback unavailable (no FluidSynth soundfont or rtmidi). MIDI export still works!")
            return
        self.is_playing = True
        self.stop_event.clear()
        dpg.configure_item("btn_play", label="⏸ Pause")

        self.play_thread = threading.Thread(target=self._playback_worker, daemon=True)
        self.play_thread.start()

    def stop_playback(self):
        self.stop_event.set()
        self.is_playing = False
        dpg.configure_item("btn_play", label="▶ Play")
        self._all_notes_off()
        self.refresh_ui()

    def _send_note(self, ch0: int, pitch: int, vel: int, on: bool):
        """Send note on/off to the active synth (FluidSynth preferred)."""
        if self.fs:
            try:
                if on:
                    self.fs.noteon(ch0, pitch, vel)
                else:
                    self.fs.noteoff(ch0, pitch)
            except Exception:
                pass
        elif self.midi_out:
            status = (0x90 | ch0) if on else (0x80 | ch0)
            try:
                self.midi_out.send_message([status, pitch, vel])
            except Exception:
                pass

    def _send_program(self, ch0: int, program: int):
        if self.fs and self.sf2_id is not None:
            try:
                self.fs.program_select(ch0, self.sf2_id, 0, program % 128)
            except Exception:
                pass
        elif self.midi_out:
            try:
                self.midi_out.send_message([0xC0 | ch0, program % 128])
            except Exception:
                pass

    def _all_notes_off(self):
        if self.fs:
            try:
                for ch in range(16):
                    self.fs.all_notes_off(ch)
            except Exception:
                pass
        if self.midi_out:
            for ch in range(16):
                try:
                    self.midi_out.send_message([0xB0 | ch, 123, 0])
                except Exception:
                    pass

    def _playback_worker(self):
        """シンプルなスケジューラー（FluidSynth優先）"""
        proj = self.proj
        bpm = proj.bpm
        beat_dur = 60.0 / bpm

        # 最初に各トラックのプログラムを送信（GM音色）
        for ti, tr in enumerate(proj.tracks):
            if tr.ch == 10:
                continue  # drums use channel 10, no program usually
            self._send_program(ti, tr.patch)

        # 全ノートを収集（Ch1展開 + 他トラック）
        all_events: List[Tuple[float, str, int, int, int]] = []  # (beat, 'on'/'off', pitch, vel, ch0)
        ch1_notes = expand_chord_blocks_to_notes(proj)
        for n in ch1_notes:
            ch0 = 0
            all_events.append((n.start, "on", n.pitch, n.vel, ch0))
            all_events.append((n.end(), "off", n.pitch, 0, ch0))

        for ti, tr in enumerate(proj.tracks):
            if tr.muted or (any(t.solo for t in proj.tracks) and not tr.solo):
                continue
            ch0 = ti
            for n in tr.notes:
                all_events.append((n.start, "on", n.pitch, n.vel, ch0))
                all_events.append((n.end(), "off", n.pitch, 0, ch0))

        all_events.sort(key=lambda e: (e[0], 0 if e[1] == "on" else 1))

        start_beat = self.current_beat
        t0 = time.perf_counter()
        last_beat = start_beat

        try:
            for ev in all_events:
                if self.stop_event.is_set():
                    break
                ev_beat = ev[0]
                if ev_beat < start_beat:
                    continue

                # 待つ
                target_real = t0 + (ev_beat - start_beat) * beat_dur
                now = time.perf_counter()
                if target_real > now:
                    time.sleep(target_real - now)

                if self.stop_event.is_set():
                    break

                # シンセに送信（FluidSynth or rtmidi）
                _, typ, pitch, vel, ch0 = ev
                self._send_note(ch0, pitch, vel, on=(typ == "on"))

                self.current_beat = ev_beat
                last_beat = ev_beat

            # 最後まで行ったら止める
            if not self.stop_event.is_set():
                self.current_beat = last_beat
        finally:
            self.is_playing = False
            dpg.configure_item("btn_play", label="▶ Play")
            self._all_notes_off()
            self.refresh_ui()

    # -------------------------------------------------------------------------
    # Generate
    # -------------------------------------------------------------------------
    def do_generate_all(self):
        s = self.range_start
        e = self.range_end
        if e <= s:
            e = s + 4.0

        piano_notes = simple_generate_piano(self.proj, s, e)
        bass_notes = simple_generate_bass(self.proj, s, e)
        drum_notes = simple_generate_drums(self.proj, s, e)

        self.proj.tracks[1].notes.extend(piano_notes)   # Ch2
        self.proj.tracks[2].notes.extend(bass_notes)    # Ch3
        self.proj.tracks[9].notes.extend(drum_notes)    # Ch10

        # ソート
        for t in [1, 2, 9]:
            self.proj.tracks[t].notes.sort(key=lambda n: (n.start, n.pitch))

        self.update_status()
        self.draw_piano_roll()
        dpg.set_value("status_text", "Generated Piano (Ch2), Bass (Ch3), Drums (Ch10). All editable in piano roll!")

    def clear_generated(self):
        for t_idx in [1, 2, 9]:
            self.proj.tracks[t_idx].notes.clear()
        self.refresh_ui()

    # -------------------------------------------------------------------------
    # Grok連携
    # -------------------------------------------------------------------------
    def open_grok_dialog(self):
        chords = []
        for blk in self.proj.chord_blocks[:12]:  # 先頭12個まで
            chords.append(f"{self.proj.chord_symbol(blk)}({get_chord_name(self.proj, blk)})")
        chord_str = " | ".join(chords) if chords else "(no chords yet)"

        prompt = (
            f"J-Pop / J-Rockの曲のコード進行アイデアをください。\n"
            f"現在のキー: {NOTE_NAMES[self.proj.key_root]}{' minor' if self.proj.is_minor else ' major'}\n"
            f"BPM: {int(self.proj.bpm)}\n"
            f"現在のコード進行（先頭）: {chord_str}\n\n"
            f"この部分（{self.range_start:.1f}〜{self.range_end:.1f}ビート）のコード進行を、"
            f"半拍前や裏拍を使ったJ-Rock/J-Popらしい自然な進行で4〜8小節提案してください。"
            f"ローマ数字と実際のコード名（C, Am7など）の両方を書いて。"
        )
        dpg.set_value("grok_prompt", prompt)
        dpg.show_item("grok_win")

    def copy_grok_prompt(self):
        text = dpg.get_value("grok_prompt")
        if HAS_PYPERCLIP and pyperclip is not None:
            try:
                pyperclip.copy(text)
                dpg.set_value("status_text", "Prompt copied to clipboard! Paste into Grok / ChatGPT / Claude etc.")
                return
            except Exception:
                pass
        dpg.set_value("status_text", "Copy failed (or pyperclip not available). Select the text in the box and Ctrl+C manually.")

    # -------------------------------------------------------------------------
    # MIDI 入出力
    # -------------------------------------------------------------------------
    def new_project(self):
        self.stop_playback()
        self.proj = Project()
        self.selected_track = 4
        self.current_beat = 0.0
        self.visible_start = 0.0
        self.selected_block_idx = None
        self.selected_note_idx = None
        self.refresh_ui()

    def save_midi(self):
        self.stop_playback()
        try:
            mid = MidiFile(ticks_per_beat=PPQ)
            # テンポトラック
            tempo_track = MidiTrack()
            tempo_track.append(MetaMessage('set_tempo', tempo=mido.bpm2tempo(self.proj.bpm)))
            tempo_track.append(MetaMessage('time_signature', numerator=4, denominator=4))
            mid.tracks.append(tempo_track)

            for ti, tr in enumerate(self.proj.tracks):
                mtrk = MidiTrack()
                mid.tracks.append(mtrk)

                # プログラムチェンジ（Ch10=ドラムは bank select など省略）
                if tr.ch != 10:
                    mtrk.append(Message('program_change', program=tr.patch, channel=tr.ch-1, time=0))

                # イベント収集
                events: List[Tuple[int, Message]] = []
                # Ch1はブロックを展開
                if tr.ch == 1:
                    for n in expand_chord_blocks_to_notes(self.proj):
                        t_on = beat_to_ticks(n.start)
                        t_off = beat_to_ticks(n.end())
                        events.append((t_on, Message('note_on', note=n.pitch, velocity=n.vel, channel=0, time=0)))
                        events.append((t_off, Message('note_off', note=n.pitch, velocity=0, channel=0, time=0)))
                else:
                    for n in tr.notes:
                        t_on = beat_to_ticks(n.start)
                        t_off = beat_to_ticks(n.end())
                        ch = tr.ch - 1
                        events.append((t_on, Message('note_on', note=n.pitch, velocity=n.vel, channel=ch, time=0)))
                        events.append((t_off, Message('note_off', note=n.pitch, velocity=0, channel=ch, time=0)))

                events.sort(key=lambda x: x[0])

                prev_t = 0
                for t, msg in events:
                    delta = t - prev_t
                    msg.time = delta
                    mtrk.append(msg)
                    prev_t = t

            # 保存ダイアログ（Dear PyGui native file dialogはシンプルに固定名で）
            path = "JpoProducer_output.mid"
            mid.save(path)
            dpg.set_value("status_text", f"Saved: {path}  (16ch, Ch1 expanded from blocks)")
        except Exception as ex:
            dpg.set_value("status_text", f"Save MIDI failed: {ex}")

    def open_midi(self):
        self.stop_playback()
        # 超シンプル：固定ファイル名 "import.mid" を読む（本格はファイルダイアログ追加）
        path = "import.mid"
        try:
            mid = mido.MidiFile(path)
            new_proj = Project()
            new_proj.bpm = DEFAULT_BPM
            new_proj.key_root = self.proj.key_root
            new_proj.is_minor = self.proj.is_minor

            # 超簡易インポート（noteのみ、複数トラック）
            for i, mtrk in enumerate(mid.tracks):
                if i >= 16:
                    break
                abs_tick = 0
                notes_on: Dict[int, Tuple[int, int]] = {}  # pitch -> (start_tick, vel)
                for msg in mtrk:
                    abs_tick += msg.time
                    if msg.type == 'set_tempo':
                        new_proj.bpm = mido.tempo2bpm(msg.tempo)
                    if msg.type == 'note_on' and msg.velocity > 0:
                        notes_on[msg.note] = (abs_tick, msg.velocity)
                    elif msg.type in ('note_off', 'note_on') and msg.note in notes_on:
                        start_t, vel = notes_on.pop(msg.note)
                        dur_t = abs_tick - start_t
                        beat_start = ticks_to_beat(start_t)
                        dur_b = ticks_to_beat(dur_t)
                        ch = (i if i < 16 else 15)
                        new_proj.tracks[ch].notes.append(Note(beat_start, msg.note, max(0.05, dur_b), vel))

            self.proj = new_proj
            self.refresh_ui()
            dpg.set_value("status_text", f"Loaded basic notes from {path}. Chord blocks are empty (import as free notes).")
        except FileNotFoundError:
            dpg.set_value("status_text", "import.mid が見つかりません。保存したMIDIを import.mid にリネームして読み込んでください。")
        except Exception as ex:
            dpg.set_value("status_text", f"Open MIDI error: {ex}")

    # -------------------------------------------------------------------------
    # その他
    # -------------------------------------------------------------------------
    def clear_current_track(self):
        self.proj.tracks[self.selected_track - 1].notes.clear()
        self.refresh_ui()

    def clear_chord_blocks(self):
        self.proj.chord_blocks.clear()
        self.selected_block_idx = None
        self.refresh_ui()

    def delete_selected_note(self):
        if self.selected_note_idx is None:
            return
        tr = self.proj.tracks[self.selected_track - 1]
        if 0 <= self.selected_note_idx < len(tr.notes):
            del tr.notes[self.selected_note_idx]
        self.selected_note_idx = None
        self.selected_notes.clear()
        self.refresh_ui()

    def delete_selected_block(self):
        if self.selected_block_idx is None:
            return
        if 0 <= self.selected_block_idx < len(self.proj.chord_blocks):
            del self.proj.chord_blocks[self.selected_block_idx]
        self.selected_block_idx = None
        self.refresh_ui()

    def show_about(self):
        dpg.set_value("status_text",
                      "JpoProducer v0.1 — Python/DearPyGui | Ch1 block chords + onion + generators + MIDI | Personal sketch tool")

    def on_frame(self, app_data):
        # 再生中は定期的にUI更新（位置インジケータ用）
        if self.is_playing:
            self.draw_chord_timeline()
            self.draw_piano_roll()
            dpg.set_value("time_slider", self.current_beat)
            dpg.set_value("pos_text", f" {self.current_beat:.2f} beats")

    def run(self):
        dpg.start_dearpygui()
        dpg.destroy_context()
        self.stop_playback()

# =============================================================================
# エントリポイント
# =============================================================================
if __name__ == "__main__":
    app = JpoProducerApp()
    app.run()
