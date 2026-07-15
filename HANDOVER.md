# JpoProducer 引継ぎ書

**最終更新:** 2026-07-10  
**方針:** v2 凍結 → **v1 を 4タブ方式に再設計（v0.3.0）**  
**リポジトリ:** https://github.com/OptimalNotes/JpoProducer  
**開発環境:** [`ENV.md`](ENV.md) — **WSL2 `~/JpoProducer` がメイン**、Windows は配布用  
**ローカル (Win):** `C:\Users\user\JpoProducer\`

> **実装リマインド:** 下記 Phase 表・未解決バグ・golden ケースを順に潰す。  
> コード内マーカー: `src/main.rs` の `// UPDATE-ROADMAP:` を検索。

---

## 4タブ = 4つの編集モデル

各タブは別アプリとして設計する。旧1画面の共有ショートカット・ジェスチャは廃止済み。

| Tab | 何のエディタか | 標準操作 | やらないこと |
|-----|--------------|---------|-------------|
| **1 Chord** | 進行エディタ（ブロックタイムライン） | 空きクリック配置、右端伸長、ドラッグ移動 | Ctrl+C/V、ピアノロール編集 |
| **2 Generate** | 生成プレビュー | ボタンで生成、タイムラインは閲覧 | 編集ショートカット、ノート直接編集 |
| **3 Edit** | 標準 MIDI ピアノロール（Ch2–16） | Select/Draw/Erase、Ctrl+C/X/V/D、**playhead で貼り付け** | Ch1 コード編集 |
| **4 Arrange** | ループ並べ | Loop Bank、通し再生 | ノート編集 |

**Tab3「標準 MIDI エディタ」MVP（Phase E で完成）**

1. **Playhead** — クリックで挿入位置（貼り付け・MIDI import の基準）
2. **Select** — 空きクリックで playhead + 選択解除、マーキー、Ctrl+複選、移動・リサイズ
3. **Draw / Erase** — 配置・削除（キー割り当ては Phase F）
4. **Clipboard** — Ctrl+C/X/V/D（Tab3 + Ch2–16 のみ）
5. **Snap + Len** — グリッドとデフォルト音価

**全タブ共通:** SF2 再生（Space）、Loop 4/8/16、再生ヘッド表示

---

## 実装フェーズ

実装順: **A → B → D → E → (B' | C) → F**

| Phase | 状態 | 内容 |
|-------|------|------|
| **A** | done | 入力分離: Ctrl+C/V は Tab3 のみ、`switch_tab` で状態クリア |
| **B** | done | Tab1: クリック配置、右端伸長、`enforce_chord_timeline_no_overlap` |
| **D** | done | Tab3: Select/Draw/Erase、marquee、Ctrl+click 複選 |
| **E** | done | Tab3 MVP: Select 空きクリック + タイムラインで playhead → paste 可能に |
| **B'** | done | Tab1: ドラッグ中クランプ + enforce（押し出し・dur 切り詰め、ブロック削除なし） |
| **C-1** | done | Tab2 生成: 1-pass、BPM gate、beat-grid BPM 非依存 |
| **C-2** | done | Tab2 UI: Piano / Bass / Drum 3レーン preview + Preview ボタン |
| **C-3** | partial | シンコ適応窓・bass fold・piano pitch-class trim（下記） |
| **F** | pending | キーバインド、UI 微調整、Arrange/EditEngine |

### C-3 で入れたもの（2026-07-10）

| 項目 | 状態 | 内容 |
|------|------|------|
| シンコ適応窓 | **done** | 短ブロックで `win_len = min(2.0, block_len - 1.5)`。2.5拍 G◆ → 窓1拍 + refill1.5拍 |
| Bass `map_pattern_pitch` | **partial** | コード実装済みだが **帯域が goal と一致せず**（未解決 #2） |
| Piano pitch-class trim | **partial** | 同一 onset の重複 PC を vel 最大で1つに。重なり **まだ残る**（未解決 #1） |
| Golden case02 | done | sync ON + 短 G◆ ブロック regression |
| Golden case03 | added | `Miditest_broken01.mid` 取り込み（2026-07-10 ユーザー報告） |

**シンコペーション:** ユーザー確認済み — **大幅改善・OK**。

---

## 未解決バグ（優先度順）

共同作業: ケースごとに `broken.mid` + `fixed.mid` + スクショ（Ch2/3/10 入り1ファイル）。

### #1 Piano ノート重なり（生成）— **partial (2026-07-11)**

- **症状:** コードトーンが時間的に重なって聴こえる / 見える。
- **コード対応:** `trim_same_pitch_temporal_overlap` — 同一 pitch の時間 overlap を前ノート短縮で解消。テスト `generate_piano_no_same_pitch_temporal_overlap`。
- **残り:** テンプレ長 Gate による **異音高** の聴感重なり → `Piano01.mid` Domino 短縮（明日以降）。

### #1b Piano コード品質（Em 等）— **done (2026-07-11)**

- **症状:** Em が E G# D… になる（正: **E G B**）。`melodic_pitch` が quality 無視。
- **修正:** `piano_pitch_from_pattern` — テンプレ interval → `chord_pitches(block)` スロット（quality 反映）。
- **テスト:** `piano_em_block_uses_minor_third_not_g_sharp`, `piano_quality_matrix_pitch_classes`。
- **他 quality:** maj / m / 7 / m7 を PC 集合で回帰。dim・sus は未テスト（必要時追加）。

### #2 Bass 帯域 — **register map (2026-07-11)**

- **目標:** エレキベース正規チューニング **E1..=D#2 = MIDI 28..=51**（C2 基準・octave=2 は使わない）。
- **モデル:**
  - テンプレから取る: **リズム** (start/dur/vel) + **レジスタ slot**（template_root より ≥12 上なら high）
  - コードから取る: `root_pitch_class(degree)` → 帯域内の **low/high 2音**（各 PC ちょうど2つ）
  - `bass_pitch_from_pattern` = (相対 pitch class) + slot → 最終 MIDI
- **オクターブ交互:** 潰さない。`BassDance01` の 48/60 → C なら 36/48 を維持（4分ルート化しない）。
- **V が上に逃げる問題:** low slot の G は **G1(31)**（旧 C2 基準では G2=43）。
- **テスト:** `bass_register_*`, `bass_c_major_v_is_g1_*`, `generate_bass_dance_keeps_octave_slots`, `generate_bass_stays_in_e1_ds2_band`

### #3 Tab3 選択ハイライトが全ノートに付く — **done (2026-07-11)**

- **症状:** 1ノート選択で **画面上の全ノートがハイライト**。操作（移動・削除等）は先頭1件だけ。
- **原因:** Tab2「Generate All」経由のノートがすべて **`NoteId(0)`** のまま project tracks に入っていた。描画が id 一致でハイライトするため全件マッチ。
- **修正:** Project のノート（Tab3 の編集 SoT）へ書くときだけユニーク ID を採番。
  - `assign_unique_note_ids` + `replace_notes_in_range(..., next_id)`
  - Generate All / MIDI import で採番
  - ハイライト判定を `(track_idx, id)` 一致に修正
  - テスト: `assign_unique_note_ids_rewrites_zeros`, `generate_then_replace_yields_unique_ids_across_tracks`
- Preview（dry-run）は id=0 のままでよい（project に書かない）。

---

## ピッチマッピング

| トラック | 方式 | chord_root | clamp |
|---------|------|------------|-------|
| **Bass Ch3** | `bass_pitch_from_pattern`（register + slot） | `root_pitch_class` | 28〜51 |
| **Piano Ch2** | `piano_pitch_from_pattern`（quality 反映 + オクターブ層） | `chord_pitches(block)` | 36〜96 |
| **Drum Ch10** | 転調なし | — | — |

**Bass レジスタ:** 各 pitch class は [28,51] に low/high の2音。slot0=low、pattern≥template+12 で slot1=high。

**BPM:** beat-grid の `start` / `dur` は BPM 非依存（`pattern_time_scale = 1.0`）。

---

## Golden テスト

```powershell
cd C:\Users\user\JpoProducer
cargo test
```

| ケース | 内容 |
|--------|------|
| `case01/` | MidiTest 回帰: broken vs fixed、Ch2 87%+ match（trim 正規化後） |
| `case02/` | sync ON + 2.5拍 G◆: 窓6.5・refill 6.5–8.0 にノートあり |
| `case03/` | **2026-07-10** `Miditest_broken01.mid` — piano overlap / bass range / Tab3 報告時点の broken |

各 `scenario.md` / `README.txt` 参照。goal MIDI はユーザー手編集で随時追加。

---

## 参考パス

| パス | 内容 |
|------|------|
| `src/main.rs` | 現行アプリ（単一ファイル、生成器・UI） |
| `assets/patterns/` | ジェネレータ用パターン MIDI |
| `tests/golden/` | broken / fixed / .jpo / scenario |
| `C:\Users\user\OneDrive\Desktop\Domino\` | テンプレ編集・UI のお手本 |
| `C:\Users\user\OneDrive\Desktop\Miditest_broken01.mid` | 最新 broken（case03 にコピー済み） |
| `archive/jpo-v2/` | 凍結 v2（EditEngine 参考） |

---

## Grok 連携

**エージェント:** プロジェクトスキル `/jpo-producer`（`.grok/skills/jpo-producer/`）+ ルート `AGENTS.md`。

| 参照 | 内容 |
|------|------|
| `references/bug-reduction-plan.md` | バグ削減ロードマップ（#3→cleanup→分割） |
| `references/domino-lessons.md` | デスクトップ Domino の UX/設定から得た基準 |
| `references/midi-industry.md` | 編集モデル vs SMF、ID、重なり |
| `references/invariants.md` | データ不変条件 |

**Domino（お手本・パターン手編集）:** `C:\Users\user\OneDrive\Desktop\Domino\`  
自動操作しない。生成テンプレの Gate/帯域掃除に使う。

下部パネル **Grok import**:
- **Natural language** — Tab1 で進行配置
- **MIDI file** — Tab3 の選択トラックへ import（playhead 位置）

---

## ビルド

```powershell
cd "C:\Users\user\JpoProducer"
cargo run      # Tab2: Syncopation fill ON で G◆ 短ブロック確認
cargo test     # 28 tests（2026-07-11 時点）
cargo build --release
```

---

## F0 受け入れ

- [x] Tab1: 空きクリック即配置、伸張は右端のみ、ブロック非重複
- [x] Tab1: Ctrl+C/V・Shift 不要
- [x] Tab2: 3レーン preview、シンコ適応窓（手動確認済み）
- [x] Tab2: ベース E1〜E2 帯域 clamp + 回帰（**2026-07-11**；walking contour はパターン次第）
- [x] Tab2: Em 等 minor の 3 度（**#1b done 2026-07-11**）
- [ ] Tab2: ピアノ重なり聴感（**#1 partial** — same-pitch overlap done、テンプレ Gate 残）
- [x] Tab3: Select 空きクリック / タイムラインで playhead
- [x] Tab3: Ctrl+C/V で貼り付け位置が playhead
- [x] Tab3: 選択ハイライトが選択ノートのみ（**NoteId ユニーク化 2026-07-11**）
- [x] Tab2/4: 編集ショートカット無効
- [x] 全タブ: Space 再生

---

## 次セッションの推奨順

1. **テンプレ（Domino）:** `Piano01.mid` Gate 短縮、`Bass8beat01.mid` contour
2. **Golden case03:** `fixed.mid` 到着後、Em PC + overlap + bass band assert
3. **品質拡張:** dim / sus4 piano テスト（必要時）
4. **Phase F:** キーバインド、Tab3 snap 分離、薄い Undo
5. **`main.rs` 分割:** pitch/cleanup モジュール（テスト緑のまま）

## 完成までの行程（全体）

| 段階 | 内容 | 状態 |
|------|------|------|
| **0 止血** | NoteId ユニーク、選択ハイライト | done |
| **1 生成 cleanup** | bass 帯域、sync、piano same-pitch overlap、**piano quality** | **ほぼ done**（テンプレ残） |
| **2 テンプレ品質** | Domino Piano/Bass 手編集 + golden | **次（明日以降）** |
| **3 Tab3 仕上げ** | Gate/Vel、snap 分離、Undo | pending |
| **4 構造** | main.rs 分割 | pending |
| **5 プロダクト** | Arrange、プリセット、配布 | pending |

---

## 将来（F 以降）

- **F1:** J-Pop プリセット進行
- **F3:** v2 EditEngine 取り込み
- **F4:** Arrange 仕上げ