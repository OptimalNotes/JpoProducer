# JpoProducer 不変条件（詳細）

仕様変更時は **テスト → コード → HANDOVER** の順。Domino / 業界背景は  
`domino-lessons.md` / `midi-industry.md`。

---

## 編集モデル vs SMF

- UI・生成の正本: `Note { id, start, dur, pitch, vel, ... }`（拍単位）  
- SMF の NoteOn/Off + delta-tick は **import/export/再生スケジュール境界のみ**  
- 編集中に On/Off ペアで状態を持たない

---

## NoteId

- プロジェクト内で **ユニーク**（最低限: 同時に存在する全ノート）  
- 選択・ハイライト・ドラッグ対象は **id**（index や座標だけに依存しない）  
- 禁止: 生成結果をすべて `NoteId(0)` のまま `replace_notes_in_range`  
- 採番: `next_note_id()` または paste base から連番  
- **既知 #3:** id=0 重複 → 全ノートハイライト

検証例:

```rust
// 擬似
assert!(notes の id がすべて異なる);
```

---

## チャンネル

| Ch | 意味 | 制約 |
|----|------|------|
| 1 | コードブロック（進行 SoT） | Tab3 でノート編集しない |
| 2 | Piano | 生成 + 手編集 |
| 3 | Bass | fold + 帯域 |
| 10 | Drum | ピッチ転調しない（キット） |
| 2–16 | Tab3 手編集 | |

---

## 時間・BPM

- `start` / `dur` は **拍**  
- パターン展開は BPM 非依存（120 vs 180 でグリッド一致のテストを維持）  
- スナップ: 位置（tick 相当）と長さ（gate）を概念上分離（Domino 準拠）

---

## Bass（register map — エレキベース E1..=D#2）

- **帯域:** MIDI **28..=51**（E1..=D#2）。C2 / octave=2 を基準にしない。  
- **root_pc:** `Project::root_pitch_class(degree)`（0..=11）  
- **候補:** `bass_register_pitches(pc)` → 帯域内に必ず low/high の2音  
- **テンプレ:** リズム (start/dur/vel) を保持。音高は  
  - 相対 pitch class（template_root 基準）  
  - **slot:** `pattern_pitch >= template_root + 12` → high、else low  
  - `bass_pitch_from_pattern` で最終 MIDI  
- **オクターブ交互テンプレは潰さない**（4分ルート化しない）。slot で low/high を選ぶだけ。  
- 生成後 `clamp_bass_note_pitches` は安全網（正常系では no-op 想定）

---

## Piano

- `piano_pitch_from_pattern`: テンプレはリズム、音程は `chord_pitches`（quality 反映）  
- `trim_piano_pitch_class_dupes`: **同一 onset かつ同一 PC** → vel 最大1つ  
- `trim_same_pitch_temporal_overlap`: **同一 pitch の時間重なり禁止**（契約）  
- **異 pitch の同時発音（和音）は仕様**（SPEC-v1）  
- テンプレ Gate のべったり感は **assets 品質**（Domino 手編集）

---

## Chord blocks

- `enforce_chord_timeline_no_overlap`  
- オーバーラップ解決で **ブロック削除禁止**  
- テスト: `enforce_chord_blocks_never_delete_on_overlap`

---

## 生成置換

```text
generate_from_patterns
  → (must) assign unique ids
  → (must) pitch map + range clamp
  → (should) overlap cleanup per track policy
  → replace_notes_in_range
```

- `replace_notes_in_range` は範囲と重なるノートのみ除去  
- 範囲外を消さない（既存テストを維持）

---

## シンコペーション

- `syncopation_window_len` / 短ブロック適応窓  
- case02（sync ON + 短 G◆）を壊さない

---

## 選択・ツール（Tab3）

- ツールは排他的モード  
- 選択集合 ⊆ 実在 id  
- プレイヘッドは挿入点の単一ソース  
- クリップボード paste は playhead 基準（Domino 的「位置を決めてから貼る」）
