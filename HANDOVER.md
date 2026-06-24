# JpoProducer 引継ぎ書

**最終更新:** 2026-06-24
**対象:** 次セッションの実装担当（AI / 人間どちらでも）

---

## 1. プロジェクト概要

| 項目 | 内容 |
|------|------|
| パス | `C:\Users\user\JpoProducer\jpo\` |
| GitHub | **https://github.com/OptimalNotes/JpoProducer** （公開・main ブランチ） |
| ソース | **単一ファイル** `src/main.rs`（約 2900 行） |
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

### 2026-06-24 セッション ✅
- **Solo / Mute / Volume** — 全 16ch、再生 + 編集プレビューに反映、Play 中も即時再構築
- **MIDI テンプレートジェネレータ** — `assets/patterns/`（Desktop `JPoP_MidiTemp` 由来、キー C）
  - Piano / Bass / Drum パターン ComboBox + **Syncopation fill**（短コードブロック検知 → 2 拍差し替え）
  - ドラムはコード変更ごとのクラッシュ廃止 → 8beat 等パターンで小節埋め
- **オニオン opacity** — `powf(2.2)` イージング + 彩度抑制（低スライダーで急に派手にならない）
- **Arrange モード骨格** — Sketch/Arrange 切替、スロット列、Export Full MIDI、王道 I-V-vi-IV ボタン
- 保存形式 **v3**（`TrackData.muted/solo/track_vol`、v1/v2 互換読込）
- `pack.ps1` — `patterns/*.mid` 同梱

### 2026-06-17 まで ✅

### Phase A ✅
- 三連符 Len `1/12` + Snap
- Undo/Redo（Ctrl+Z/Y）、Copy/Paste/Duplicate（Ctrl+C/V/D）、Quantize、Velocity
- `.jpo` JSON 保存/読込（serde）

### Phase B ✅
- ループ長 4 / 8 / 16 小節 + 🔁 区間ループ再生
- ループ境界オレンジ線 + Fit loop
- **Loop Bank**（右パネル: 切替 / 名前 / +New / Dup）
- **ループごとの Key**（ツールバー Key = アクティブループのキー）
- 保存形式 **v2**（`loop_bank`, `active_bank_idx`, `loop_bars`）

### UX 改善（直近セッション）✅
1. **Onion** — `scale_opacity` / `chord_opacity` 別スライダー（デフォルト薄め 0.22 / 0.28）
2. **ピアノロール罫線** — 縦横とも太く・明るく
3. **コード当たり判定** — ブロック外余白ヒット削除、リサイズは右端 ~14px のみ（連続ペイントしやすく）
4. **ピッチホイール** — 1ノッチ≈1半音、Pitch Scroll スライダーをズーム幅連動に修正

### その他 UI
- ツールバー圧縮: **Tools ▾** メニュー（Pencil/Eraser, Len, Undo, Save/Load, Grok 等）
- ComboBox ID 衝突修正（`from_id_salt("toolbar_key")` 等）
- ピアノロール: Shift+drag = 箱選択、通常ドラッグ空き = ノート長ペイント

---

## 4. 次にやること（優先順）

### 優先 1 — Arrange 仕上げ
- [ ] Arrange **通しプレビュー**のプレイヘッド / 画面スクロール（現状はイベント結合のみ）
- [ ] 転調補助テンプレ追加（小室系・丸サ系など、王道以外）
- [ ] Arrange スロットのドラッグ並べ替え（現状は ↑↓ ボタン）

### 優先 2 — スケッチ品質
- [ ] ジェネレータ **個別 Generate**（Piano のみ等）
- [ ] シンコペ検知の微調整（閾値 `dur < 1.5`、オフビート開始など）

### 後回し（Phase D）
- MIDI Import、日本語フォント同梱

---

## 5. 主要データ構造（`main.rs`）

```text
Project          … bpm（曲全体）, key_root, is_minor, tracks[16], chord_blocks
LoopSketch       … name, loop_bars, key_root, is_minor, tracks, chord_blocks
JpoApp           … UI 状態 + loop_bank + active_bank_idx + 編集ドラッグ状態

ChordBlock       … start, dur, degree, quality: String, octave
Note             … start, pitch, dur, vel
TrackData        … ch, notes, patch, muted, solo, track_vol
```

**ループの真実:**
- 編集中の中身 = `proj`（tracks + chord_blocks + key）
- スナップショット = `loop_bank[active_bank_idx]`
- 切替時 `snapshot_active_bank()` → `switch_loop_bank(idx)` でロード
- 編集終了時 `end_gesture_undo()` 内で `sync_active_bank_from_proj()`

**再生:**
- `PlaybackPlayer` — cpal コールバック、ループ時 `loop_end_sample` で巻き戻し + all_notes_off
- `PreviewEngine` — 編集時短い SF2 プレビュー（別ストリーム）

---

## 6. 重要な関数・描画箇所

| 領域 | 関数 |
|------|------|
| ツールバー | `update()` 内 TopBottomPanel `toolbar` |
| Tools メニュー | `show_tools_menu()` |
| Loop Bank | `show_loop_bank_panel()` |
| コード入力 | `draw_chord_timeline()` — `chord_hit_at()` |
| ピアノロール | `draw_piano_roll_grid()` — `scroll_pitch_view()` |
| Onion | `draw_piano_roll_grid()` 内 scale/chord レイヤー |
| 保存 | `save_project()` v3 / `load_project()` v1–v2 互換 |
| ジェネレータ | `generate_from_patterns()` / `assets/patterns/` |
| Arrange | `show_arrange_panel()` / `export_arrange_midi()` |
| 配布 | `pack.ps1` |

---

## 7. 操作クイック（最新）

### ツールバー（常時）
BPM | Key | Major/Minor | **Pencil ▾** | Len ▾ | Snap | Vol | **▶ Play**

### 下部バー
- **Loop:** 4/8/16 bars、🔁 Loop、Fit loop
- Zoom / Scroll（ループ長でクランプ）
- Pitch Zoom / Pitch Scroll（範囲は span に連動）
- **Onion:** Scale / Chord 別スライダー
- Generate range、Gen=Loop、Generate All

### コード（Ch1）
- 空き **ドラッグ** → 新規ブロック（長さペイント）
- 空き **クリック** → 1 ブロック
- ブロック **中央クリック** → パレット
- **右端のみ** ドラッグ → 伸縮（連続配置しやすい）
- ダブルクリック / Eraser → 削除

### ピアノロール（Ch2–16）
- 空きドラッグ → ノート長ペイント
- **Shift+ドラッグ** → 箱選択
- ホイール → 半音スクロール（±12 まで/フレーム）

### ショートカット
`Ctrl+Z/Y`, `C/V/D`, `Delete`

---

## 8. 既知の注意点・技術的負債

- **単一 `main.rs`** — 分割は意図的に後回し。Phase C でもしばらくこのまま可
- **PlaybackPlayer** — オーディオスレッドで `static mut SYNTH`（Send 回避のハック）
- **Chord パレット** — 毎フレーム degree/quality を proj に書き戻す（ボタン押下時のみ undo）
- **README 古い** — Onion 単一スライダー、Tools バー、Loop Bank 等の記載なし
- **ユーザ未検証の細部** — UX 4 点は実装済みだが、長時間使用での微調整はあり得る

---

## 9. 次セッション用プロンプト（コピペ可）

```text
JpoProducer の続きを実装して。
引継ぎ: C:\Users\user\JpoProducer\jpo\HANDOVER.md を読んでから着手。

優先:
1. トラック Solo / Mute / Volume
2. Phase C（Arrange モード → フル MIDI Export → 転調補助テンプレ）

開発フロー: 君は cargo check、僕が cargo run で確認。
ソースは src/main.rs 単体。README も必要なら更新。
```

---

## 10. ビルドコマンド早見

```powershell
cd "C:\Users\user\JpoProducer\jpo"
cargo check          # AI が毎回
cargo run            # 人間が GUI 確認
cargo build --release
pwsh -File pack.ps1 -Zip   # 別 PC 用 zip
```

---

*このファイルが次セッションの唯一の「状態の真実」。README は製品説明、本書は開発引継ぎ用。*