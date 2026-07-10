case01 — golden 3-pack (sync OFF 推奨)

置くファイル:
  broken.mid     … Generate All 直後のぐちゃぐちゃ出力（Ch2+Ch3+Ch10 入り1ファイル）
  fixed.mid      … 手で直した正解（同じ進行・同じパターン・同じ gen 範囲）
  screenshot.png … broken と同じ画面のスクショ
  scenario.md    … 任意だがあると自動テストが完全比較できる

scenario.md 例:
  bpm: 128
  key: C major
  gen_start: 0
  gen_end: 16
  piano_pattern: Piano01
  bass_pattern: Bass8beat01
  drum_pattern: Drum8beat_01
  syncopation_fill: false
  chord_blocks: （Tab1 のコード配置をそのまま書く）

MIDI は Export MIDI… で出した Type1（Ch2/3/10）でOK。