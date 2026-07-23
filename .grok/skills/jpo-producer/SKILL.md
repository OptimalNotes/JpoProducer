---
name: jpo-producer
description: >
  JpoProducer (Rust + egui J-Pop MIDI sketch tool): dense chord progressions,
  simple bed generate, piano-roll edit, loop arrange, Grok part import,
  NoteId/invariants, golden regressions. Use for JpoProducer, /jpo-producer,
  Tab/Sketch UX, generate cleanup, or Domino pattern handoff.
metadata:
  short-description: "JpoProducer SPEC-v1 pillars + MIDI rules"
---

# JpoProducer 開発スキル

## 必読（優先順）

1. **`SPEC-v2.md`** — 仕様の真実（五本柱・契約・Reshape・DoD）  
2. **`HANDOVER.md`** — 実装ギャップ  
3. このスキル + 参照:
   - `references/invariants.md`（v2 と矛盾時は SPEC-v2 が勝つ）
   - `references/domino-lessons.md`
   - `references/midi-industry.md`
   - `references/bug-reduction-plan.md`（分割手順の参考。優先は SPEC-v2）
4. `ENV.md` — WSL メイン / Windows 配布  
5. `tests/golden/ACCEPTANCE.md` + `case*/scenario.md`
6. `SPEC-v1.md` — 履歴のみ

ローカル Domino: `C:\Users\user\OneDrive\Desktop\Domino\`（**自動操作しない**）。

---

## 五本柱（優先の北極星）

| 柱 | 内容 |
|----|------|
| P1 | 密な J-Pop 進行（細かい切り替え・シンコ）— 市場の 1 小節 1 コードより細かく |
| P2 | 単純伴奏ベッド（メロが書ける土台。アレンジしない） |
| P3 | 普通の MIDI 編集（ピアノロール） |
| P4 | 4/8/16 ループを繋いで曲の骨格 |
| P5 | Grok に進行を渡し MIDI パートを import |

**ターゲット UI:** Progress / Sketch / Arrange（3 ワークスペース）。  
**現行:** 4 タブは入力隔離の暫定。隔離ルールを守ればタブ数は可変。

### 入力隔離（タブ名よりこちらが本質）

| 領域 | 許可 |
|------|------|
| Chord strip | ブロックのみ |
| Piano roll | ノート編集・Ctrl+C/X/V/D・Undo |
| Grok dock | テキスト / MIDI import（ロールのショートカットを奪わない） |
| Arrange | Bank / スロットのみ |

共通: Space 再生、Loop 4/8/16、playhead。

### P3 を Domino 目線で（ロールのみ）

| Domino | Jpo で守る |
|--------|------------|
| ペン / 選択 / 消しゴム | ツール排他 |
| ヘッダー再生位置 | playhead = 貼付・import の挿入点 |
| 同一音 | 生成後 **same-pitch 時間重なり禁止** |
| Undo | 状態破壊バグを優先。薄い Undo で可 |

P1 の進行 UI と P2 のベッドは **Domino 化しない**（Jpo の核心）。

---

## プロダクト境界

| やる | やらない |
|------|----------|
| 密なコードブロック進行 | フル CC / 音源マクロ |
| Simple Bed（P+B+D） | 本番アレンジ自動化 |
| 標準ピアノロール Ch2–16 | Domino 全機能 |
| Loop → Arrange → MIDI | 無限タイムライン DAW |
| Grok **context コピー + MIDI import** | v1 で API 課金直結必須化 |
| SF2 試聴 | プラグインホスト |

---

## データ不変条件（要約）

詳細は `references/invariants.md`。

1. 編集モデル `{id, start, dur, pitch, vel}`。SMF は I/O のみ  
2. **NoteId ユニーク**（生成・import・paste）  
3. Ch1=進行 SoT、Ch2=Piano、Ch3=Bass、Ch10=Drum  
4. 拍グリッド・**BPM 非依存**  
5. Bass 生成 28–51  
6. Piano: quality 反映 + same-pitch overlap 0。異 pitch 和音は仕様  
7. コードブロック非重複（enforce で削除しない）  
8. `replace_notes_in_range` は範囲外を消さない  

```text
pattern → time map → pitch map → cleanup(ids, range, overlap) → replace_in_range
```

---

## 変更プロセス

```
1. SPEC-v2 に反しないか確認（反するなら先に SPEC-v2 更新 + ユーザー合意）
2. HANDOVER のギャップ ID or 新ケース名
3. 再現 / 失敗テスト（契約を先に赤で固定）
4. 最小修正（隔離・不変条件・Reshape）
5. cargo test
6. HANDOVER 更新
```

### 禁止

- テストなしの generate / pitch / sync / trim 変更  
- バグに合わせて golden を改ざん  
- 挙動修正と大きな分割の同時実施  
- フルリライト / archive/jpo-v2 のメイン化  
- SPEC-v2 DoD 外のスコープ追加（明示依頼なし）  
- シンコの「窓に何かあれば OK」への後退（**連続被覆**が契約）

### 残ギャップの目安（HANDOVER / SPEC-v2 Phase と同期）

- H1 シンコ連続被覆  
- H2 複数選択一括移動  
- H3 スタンプ規約どおりの保存 UX  
- Loop flush / TimelineLayout / InputFocus  
- gen pipeline 切り出し  


---

## 主要シンボル（`src/main.rs`）

| 領域 | 名前 |
|------|------|
| 生成 | `generate_from_patterns`, `replace_notes_in_range` |
| Bass | `bass_pitch_from_pattern` |
| Piano | `piano_pitch_from_pattern`, `trim_same_pitch_temporal_overlap` |
| ID | `assign_unique_note_ids` |
| 編集 | `build_pasted_notes`, selection, `UndoHistory` |
| コード | `enforce_chord_timeline_no_overlap` |
| Grok | `show_grok_panel`, `GrokImportMode` |

---

## テスト

```bash
cd ~/JpoProducer && cargo test
```

手動: `tests/golden/ACCEPTANCE.md`（v1.0 ゲート）。

---

## セッション開始

- [ ] cwd = project root  
- [ ] SPEC-v2 五本柱 + 契約（sync 被覆 / selection / loop flush）+ HANDOVER  
- [ ] 触る領域の InputFocus  
- [ ] 終了時 `cargo test` + HANDOVER（仕様変更時は SPEC-v2 も）  
