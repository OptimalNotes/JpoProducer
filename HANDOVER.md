# JpoProducer 引継ぎ書

**最終更新:** 2026-07-02  
**方針:** v2 凍結 → **v1 を 4タブ方式に再設計（v0.3.0）**  
**リポジトリルート:** `C:\Users\user\JpoProducer\`

> **実装リマインド:** 下記「目指すアップデート」のチェックボックスを順に潰す。  
> コード内マーカー: `src/main.rs` の `// UPDATE-ROADMAP:` を検索。

---

## 目指すアップデート（4タブ操作体系 + 生成）

実装順: **A → B → C-1 → C-2 → D**（ピッチマッピングは C-1 で必ず入れる）

| Phase | 状態 | 内容 |
|-------|------|------|
| **A** | done | 入力分離: Ctrl+C/V は Tab3 のみ、ツールバー・`switch_tab` 整理 |
| **B** | done | Tab1: クリック配置のみ、右端ハンドル伸長、`enforce_chord_timeline_no_overlap` |
| **C-1** | pending | Tab2 生成ルール: 1-pass、BPM gate、**ベース/ピアノ pitch fold**（下記） |
| **C-2** | pending | Tab2 UI: Piano / Bass / Drum 3レーン preview |
| **D** | done | Tab3: Select/Draw/Erase、marquee（Shift不要）、Ctrl+click 複選、Ctrl+C/V Tab3のみ |

### ピッチマッピング（C-1 で実装 — 忘れないこと）

`melodic_pitch` の単純平行移動を廃止し、`map_pattern_pitch` に置き換える。

**共通 fold（Bass / Piano）** — パターン基準 `template_root = 48`（3弦3フレット C3）:

| 音名（基準 C から） | rel mod 12 | オクターブ |
|--------------------|------------|-----------|
| C, C#, D, D# | 0〜3 | **上側**（+0 fold、必要なら上へ） |
| E, F, F#, G, G#, A, A#, B | 4〜11 | **下側**（-12 fold → 開放 E など） |

例: コード root=C3(48) でパターン E(+4) → 52 ではなく **40(E2)**。

**着地点の違い:**

| トラック | chord_root | clamp |
|---------|------------|-------|
| Bass Ch3 | `degree_root(degree, 2)` | 28〜52 (E1〜E3) |
| Piano Ch2 | `degree_root(degree, 3)` | 48〜72 (低めメロディ確認) |
| Drum Ch10 | 転調なし | — |

**BPM:** パターン `reference_bpm`（既定 120）に対し `dur *= ref_bpm / proj.bpm`。

**テスト:** `cargo test pitch_map` — E→40付近、D→50、BPM180でdur短縮。

### F0 受け入れ（完了時）

- [ ] Tab1: 空きクリック即配置、伸張は右端のみ、**ブロック非重複**
- [ ] Tab1: Ctrl+C/V・Shift 不要
- [ ] Tab2: 3レーン preview、ベース低音域、BPMでノート長変化、重なりなし
- [ ] Tab3: 範囲選択 + Ctrl+C/V のみここ
- [ ] Tab2/4: 編集ショートカット無効
- [ ] 全タブ: Space 再生

---

## 現行アーキテクチャ（4タブ）

| Tab | 役割 | 入力 |
|-----|------|------|
| **1 Chord** | 細かいコード割り（Len デフォルト **1/8**、**1/16** 選択可） | タイムライン + Chord Strip |
| **2 Generate** | Piano/Bass/Drum 生成 | ボタンのみ（タイムラインは閲覧） |
| **3 Edit** | ピアノロール編集、Grok MIDI import | Ctrl+C/V はこのタブだけ |
| **4 Arrange** | Loop Bank + 通しプレビュー + export | 並べ替え UI |

**全タブ共通:** SF2 再生（Space）、Loop 4/8/16、再生ヘッド

---

## Grok 連携

下部パネル **Grok import**（Tab1）:
- **Natural language** — `C | Am | F | G` / `I-vi-IV-V` 等
- **MIDI file** — Tab3 の選択トラックへ import

---

## ビルド

```powershell
cd "C:\Users\user\JpoProducer"
cargo run
cargo build
cargo test
```

---

## フォルダ

| パス | 内容 |
|------|------|
| `src/main.rs` | 現行アプリ |
| `assets/patterns/` | ジェネレータ用パターン MIDI |
| `archive/jpo-v2/` | 凍結 v2（EditEngine 参考） |

---

## 将来（本アップデート後）

- **F1:** J-Pop プリセット進行
- **F3:** v2 EditEngine 取り込み
- **F4:** Arrange 仕上げ