# JpoProducer 引継ぎ書

**最終更新:** 2026-06-26
**対象:** 次セッションの実装担当（AI / 人間どちらでも）

---

## 1. プロジェクト概要

| 項目 | 内容 |
|------|------|
| パス | `C:\Users\user\JpoProducer\jpo\` |
| GitHub | **https://github.com/OptimalNotes/JpoProducer** （公開・main ブランチ） |
| ソース | **単一ファイル** `src/main.rs`（約 4200 行） |
| 仕様の参考 | `C:\Users\user\OneDrive\Desktop\# JpoProducer.txt`（2026-06-15 改訂） |
| 製品 README | `jpo/README.md`（ロードマップ・操作説明） |
| SF2 | `FluidR3 GM.SF2`（`jpo/` または exe 同梱） |

**コンセプト:** J-Pop / J-Rock 向けループ・パズル型 MIDI スケッチツール。  
Ch1 = コードブロックが真実。Ch2–16 = ピアノロール。無限タイムラインは作らない。

---

## 2. 開発フロー（合意済み）

```text
AI  → cargo check / cargo build --release / pack.ps1 / git commit & push
人間 → cargo run（GUI の見た目・操作感の確認）
```

**Git:** `origin` = `https://github.com/OptimalNotes/JpoProducer.git`  
SF2 は `.gitignore` 済み（各自ローカル配置）。`target/` `dist/` も除外。

- **`cargo run` を AI に任せる専用ツールは不要**（GUI 確認は人間の目が必要）
- 別 PC 配布: `pwsh -File pack.ps1 -Zip` → `dist/JpoProducer-portable-YYYY-MM-DD.zip`
- Windows で exe ロック時: jpo.exe を閉じてから再ビルド

---

## 3. 完了済み

### 2026-06-26 セッション — バグ修正スプリント ✅（**人間未検証**）

ユーザー報告 8 件に対し、以下を **AI 実装済み・cargo check 通過**。GUI 確認は次セッション / ユーザー待ち。

| # | 報告 | 対応 |
|---|------|------|
| 1 | Ctrl+C/V が効かない | **ノート選択 > コード選択** の優先順位。`consume_key` 追加。ペースト先を **`current_beat`（再生ヘッド）** に統一。下部トースト表示 |
| 2 | コード伸縮ヒット判定が狭い | `chord_hit_at`: 24px / 幅50% / 右端外8pxスロップ |
| 3 | コードブロック重なり | `resolve_chord_overlaps()` — place / move / resize / paste 後に呼び出し（前ブロックをトリム or 完全包含を削除） |
| 4 | デフォルト 4 bar | `loop_bars` デフォルト **4**、`default_loop_bars()` も 4 |
| 5 | Generate ピッチが2オクターブ高い | `template_root` を MIDI 中央ピッチから自動推定 + `melodic_pitch()` で相対転調 |
| 6 | patch DragValue が再生に効かない | `PlaybackPlayer::render` 初回で `initial_patches` Program Change 送信。patch 変更時 `on_track_mix_changed()` で再生再構築 |
| 7 | シンコペ後に休符が残る | `pattern_tile_at()` で `fill_start` 基準の位相合わせ + `apply_melodic_block_range` 修正 |
| 8 | ノートペースト先 | `paste_clipboard` → `current_beat`（`last_mouse_beat` 廃止） |

**直近コミット:** `169714e` Bugfix sprint: Ctrl+C/V, stretch, overlaps, patch, generate pitch

### 2026-06-24 以前 ✅
- Chord Strip（タイムライン下固定パレット）
- 手動シンコペ（`ChordBlock.syncopation_fill` + ★）
- 保存 v4
- DAW 風再生ヘッド（赤線、クリック移動、Space 再生/停止、停止後位置保持）
- コード配置（空きクリック = Len 長ブロック）
- 複数選択（Shift+クリック、Alt+ドラッグ箱選択）
- Solo / Mute / Volume、Pattern ジェネレータ、Arrange 骨格、Loop Bank、Undo/Redo 等

---

## 4. 次にやること（優先順）

### 優先 0 — **ユーザー GUI 検証**（このスプリントの確認）
- [ ] 伸縮: 右端 24px ゾーンで掴めるか
- [ ] Ctrl+C/V: ノートとコードで期待どおりか、トースト表示
- [ ] コード重なり: move/paste 後にトリムされるか
- [ ] 新規プロジェクトが 4 bar か
- [ ] Generate ピッチが妥当か（Piano/Bass）
- [ ] トラック patch 変更が再生音色に反映されるか
- [ ] シンコペ ★ 後、2拍目以降に休符が残らないか

**検証で問題があれば** → 該当関数をピンポイント修正（一覧は §6）

### 優先 1 — UI 本格洗練（バグ確認後）
- Arrange 通しプレビューのプレイヘッド / スクロール
- ジェネレータ個別 Generate（Piano のみ等）
- README 更新（Chord Strip、Loop Bank、操作一覧）

### 後回し
- MIDI Import、日本語フォント同梱、main.rs 分割

---

## 5. 主要データ構造（`main.rs`）

```text
Project          … bpm, key_root, is_minor, tracks[16], chord_blocks
LoopSketch       … name, loop_bars, key_root, is_minor, tracks, chord_blocks
JpoApp           … UI 状態 + loop_bank + playback + status_toast
ChordBlock       … start, dur, degree, quality, octave, syncopation_fill
Note             … start, pitch, dur, vel
TrackData        … ch, notes, patch, muted, solo, track_vol
PlaybackPlayer   … patches_applied, initial_patches, events, loop
```

**ループの真実:** 編集中 = `proj`、スナップショット = `loop_bank[active_bank_idx]`

---

## 6. 重要な関数・今回触った箇所

| 領域 | 関数 / 箇所 |
|------|-------------|
| 伸縮ヒット | `chord_hit_at()` L~1505 |
| 重なり解消 | `resolve_chord_overlaps()` |
| Ctrl+C/V | `update()` 内 `input_mut` + `has_note_selection()` |
| ペースト | `paste_clipboard()` / `paste_chord_blocks()` → `current_beat` |
| トースト | `show_toast()` + bottom panel `status_toast` |
| 音色 | `PlaybackPlayer::render()` + patch `on_track_mix_changed()` |
| Generate | `parse_pattern_midi()` median `template_root`, `melodic_pitch()`, `pattern_tile_at()` |
| シンコペ | `refill_after_sync_windows()` / `apply_melodic_block_range()` |
| デフォルト長 | `JpoApp::default()` `loop_bars: 4` |

---

## 7. 操作クイック（最新）

### コード（Ch1 タイムライン）
- 空き **クリック** → Len 長ブロック配置
- ブロック **クリック** → 選択 + パレット（下部 Chord Strip）
- **右端ドラッグ** → 伸縮（24px ゾーン）
- **中央ドラッグ** → 移動
- Shift+クリック / Alt+ドラッグ → 複数選択
- Ctrl+C/V → コード（ノート未選択時）
- Space → 再生/停止

### ピアノロール（Ch2–16）
- Shift+ドラッグ → 箱選択
- Ctrl+C/V → ノート（選択あり時はコードより優先）
- ペースト先 = **再生ヘッド（赤線）**

### 下部バー
- Loop: **4**/8/16 bars、Generate range、Onion、Generate All

---

## 8. 既知の注意点・技術的負債

- **単一 `main.rs`** — 意図的に後回し分割
- **PlaybackPlayer** — オーディオスレッド `static mut SYNTH`（Send 回避ハック）
- **2026-06-26 修正は未検証** — 上記 §4 優先 0 を人間が `cargo run` で確認すること
- **README 古い** — Onion 別スライダー、Chord Strip、Loop Bank 等未記載
- **伸縮** — まだ厳しければ `resize_px` をさらに広げる（`chord_hit_at` のみ触ればよい）

---

## 9. 次セッション用プロンプト（コピペ可）

```text
JpoProducer の続きを実装して。
引継ぎ: C:\Users\user\JpoProducer\jpo\HANDOVER.md を読んでから着手。

状況: 2026-06-26 バグ修正スプリントを push 済み。ユーザーが cargo run で §4 優先0 を検証中。
問題報告があれば該当バグだけ直す。問題なければ UI 洗練（Arrange プレイヘッド等）へ。

開発フロー: 君は cargo check、僕が cargo run で確認。
ソースは src/main.rs 単体。
```

---

## 10. ビルドコマンド早見

```powershell
cd "C:\Users\user\JpoProducer\jpo"
cargo check
cargo run            # 人間が GUI 確認
cargo build --release
pwsh -File pack.ps1 -Zip
git status && git log -3 --oneline
```

---

## 11. Git 状態メモ（2026-06-26）

- ブランチ: `main`
- リモート: `origin` → `https://github.com/OptimalNotes/JpoProducer.git`
- 直前の確定コミット: `6a76a00` Chord timeline click-to-place
- **本セッション追加:** `169714e` バグ修正スプリント一式（HANDOVER 更新含む）

---

*このファイルが次セッションの唯一の「状態の真実」。README は製品説明、本書は開発引継ぎ用。*