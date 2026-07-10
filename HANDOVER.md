# JpoProducer 引継ぎ書

**最終更新:** 2026-07-10  
**方針:** v2 凍結 → **v1 を 4タブ方式に再設計（v0.3.0）**  
**リポジトリ:** https://github.com/OptimalNotes/JpoProducer  
**ローカル:** `C:\Users\user\JpoProducer\`

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

### #1 Piano ノート重なり（生成）

- **症状:** コードトーンが時間的に重なって聴こえる / 見える。
- **現状:** `trim_piano_pitch_class_dupes` は **同一 onset・同一 pitch class** のみ間引き。テンプレ側の長い dur やオクターブ違いの C は残る。
- **次:** `Piano01.mid` を Domino で短 dur（≤1.5拍）に手編集 + 時間 overlap の post-process 検討。

### #2 Bass 帯域が E1〜D#2 に入っていない

- **症状:** ベースが C2 基準の平行移動のままに聞こえる。目標帯域 **MIDI 28〜51（E1〜D#2）**。
- **仕様（fold）:** パターン基準 C3(48)。C/C#/D/D# は上側、E〜B は -12 fold。`degree_root(degree, 2)` + clamp 28〜52。
- **参照:** `bass0710.png`（`OneDrive\画像\Screenshots\`）、Domino（`Desktop\Domino\` — UI/仕組みのお手本、自動化なし）。
- **次:** `Bass8beat01.mid` の contour を Domino 準拠で編集。生成後に Ch3 の min/max pitch を golden で assert。

### #3 Tab3 選択ハイライトが全ノートに付く

- **症状:** 1ノート選択で **画面上の全ノートがハイライト**。操作（移動・削除等）は先頭1件だけ。
- **原因（ほぼ確定）:** Tab2「Generate All」経由のノートがすべて **`NoteId(0)`** のまま `replace_notes_in_range` に入る。描画は `n.id == selected_id` で判定するため id=0 が全件マッチ。
- **修正方針:** Generate 適用時に `next_note_id()` でユニーク ID を付与。`generate_from_patterns` 出力にも同様。

---

## ピッチマッピング

| トラック | 方式 | chord_root | clamp |
|---------|------|------------|-------|
| **Bass Ch3** | `map_pattern_pitch`（fold） | `degree_root(degree, 2)` | 28〜52 |
| **Piano Ch2** | `melodic_pitch`（平行移動・ボイシング保持） | `degree_root(degree, 3)` | 36〜96 |
| **Drum Ch10** | 転調なし | — | — |

**Fold ルール（Bass）** — template_root = 48:

| 音名（基準 C から） | rel mod 12 | オクターブ |
|--------------------|------------|-----------|
| C, C#, D, D# | 0〜3 | **上側** |
| E, F, F#, G, G#, A, A#, B | 4〜11 | **下側**（-12） |

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

下部パネル **Grok import**:
- **Natural language** — Tab1 で進行配置
- **MIDI file** — Tab3 の選択トラックへ import（playhead 位置）

---

## ビルド

```powershell
cd "C:\Users\user\JpoProducer"
cargo run      # Tab2: Syncopation fill ON で G◆ 短ブロック確認
cargo test     # 18 tests（2026-07-10 時点）
cargo build --release
```

---

## F0 受け入れ

- [x] Tab1: 空きクリック即配置、伸張は右端のみ、ブロック非重複
- [x] Tab1: Ctrl+C/V・Shift 不要
- [x] Tab2: 3レーン preview、シンコ適応窓（手動確認済み）
- [ ] Tab2: ベース E1〜D#2、ピアノ重なりなし（**未達**）
- [x] Tab3: Select 空きクリック / タイムラインで playhead
- [x] Tab3: Ctrl+C/V で貼り付け位置が playhead
- [ ] Tab3: 選択ハイライトが選択ノートのみ（**NoteId バグ**）
- [x] Tab2/4: 編集ショートカット無効
- [x] 全タブ: Space 再生

---

## 次セッションの推奨順

1. **Tab3:** Generate 適用時の `NoteId` ユニーク化（即効・小変更）
2. **Bass:** 生成結果の pitch range assert + `Bass8beat01.mid` Domino 編集
3. **Piano:** overlap 検出テスト + `Piano01.mid` 短 dur 化
4. **Golden:** case03 に `fixed.mid` が揃ったら Ch2/Ch3 range/overlap assert 追加

---

## 将来（F 以降）

- **F1:** J-Pop プリセット進行
- **F3:** v2 EditEngine 取り込み
- **F4:** Arrange 仕上げ