# バグ削減プラン（B）— Domino / 業界知見反映版

目的: 変更しても壊れにくい **編集モデル + 生成 cleanup + 回帰**。  
スキル: `/jpo-producer`  
基準 UX: デスクトップ Domino（丸パクリではなく完成軸）

---

## 設計原則（計画全体に効く）

1. **Note 編集モデルと SMF を分離**（industry）  
2. **永続 ID なしに UI を信用しない**（Domino 以前の前提、HISE 等でも定番事故）  
3. **生成の最後に必ず cleanup**（range / overlap / ids）  
4. **Domino = Tab3 と“きれいな MIDI”の基準**。生成・コードブロックは Jpo 独自  
5. **テストで測れるものだけを“直った”と言う**（耳は受け入れ、回帰は数値）

---

## フェーズ 0 — 止血（UI の前提）

| ID | 作業 | 完了条件 | 根拠 |
|----|------|----------|------|
| 0.1 | Generate / 適用経路で **全ノートにユニーク NoteId** | 1音選択 → その音だけハイライト | Domino 以前; バグ #3 |
| 0.2 | `cargo test`: 生成 or replace 後 id 一意 | 失敗→修正が赤で分かる | industry §3 |
| 0.3 | import / 旧経路も id 採番を監査 | `NoteId(0)` の一括挿入が残っていない | grep `NoteId(0)` |
| 0.4 | HANDOVER #3 → done | 文書一致 | |

**これより前にオニオンや見た目の微調整をしない。** ID が壊れていると選択系が全部嘘になる。

---

## フェーズ 1 — 生成 cleanup を「製品の一部」にする

Domino は手動できれいな MIDI を書ける。Jpo の生成は **機械的に dirty になりやすい**ので、  
DAW の “trim overlapping notes” 相当を **コードで固定**する。

### 1.1 パイプラインを明示（リファクタは後でも関数境界だけ先に）

```text
load patterns
  → place on chord timeline (beats)
  → map pitch (bass fold / piano / drums identity)
  → cleanup_notes(track_policy)
  → assign_ids
  → replace_notes_in_range
```

`cleanup_notes` のポリシー案:

| Track | ポリシー | テスト |
|-------|----------|--------|
| Bass Ch3 | pitch ∈ [28,51]、**同一 pitch 時間重なり禁止**（後勝ち or 前を切る） | min/max, overlap=0 |
| Piano Ch2 | same-pitch 時間重なり禁止（#1）。PC 重複 onset は既存 trim | overlap count |
| Drum Ch10 | pitch 変更しない。短い Gate 維持 | 非改変サンプル |

### 1.2 単位テストを先に厚くする

- Bass fold の境界音（E, D#, C）— 既存を拡充  
- `syncopation_window_len` — 既存維持  
- **新規:** `assert_unique_ids`, `assert_bass_range`, `assert_no_same_pitch_overlap(ch)`  
- BPM 120 vs 180 グリッド — 既存維持

### 1.3 Golden を数値契約にする

| ケース | 次の契約 |
|--------|----------|
| case01 | 生成結果の統計 or fixed との許容差を **テストコードから**実行 |
| case02 | sync 短ブロック: 窓内にノート存在（既存シナリオを自動実行） |
| case03 | broken の問題を assert 化 → fixed または期待 min/max/overlap |

運用: ユーザー報告 → `tests/golden/caseNN/` → 赤テスト → 修正。

### 1.4 パターン品質（Domino 手作業）

- `Piano01.mid` 等の **長すぎる Gate** は overlap の主因  
- Domino で ≤1.5 拍目安に編集 → リポジトリへ → 生成テストで前後比較  
- エージェントは Domino を操作せず、**期待値テストとファイル差し替え**で支援

---

## フェーズ 2 — Tab3 を「小さな Domino」に（機能追加は薄い）

バグ修正のあと、UX ギャップを埋める。**巨大化しない。**

| 優先 | 項目 | Domino 対応 | 備考 |
|------|------|-------------|------|
| P0 | 安定選択・playhead paste | ヘッダー位置 + 選択 | ほぼ Phase E 済み。ID 修正が前提 |
| P1 | Draw デフォルト Gate/Vel の一貫 | ツールバー Vel/Gate | 既存 Len/Snap と統合 |
| P2 | 位置スナップと長さスナップの分離 | Tick/Gate quantize | 仕様コメント → 実装 |
| P3 | 簡易 Undo（プロジェクト or ノート配列スナップショット） | Ctrl+Z | 深度は浅くてよい（16〜64） |
| P4 | Vel の簡易編集 | イベントグラフの最小版 | 後回し可 |
| — | ストローク / スライス / CC マクロ | Domino 看板機能 | **やらない**（範囲外） |

受け入れ（Domino 目線の Tab3 MVP）:

- [ ] ペンで置ける・右端 Gate・ドラッグ移動  
- [ ] 選択・複数移動・Delete  
- [ ] プレイヘッド位置へペースト  
- [ ] 選択ハイライトが選択分のみ（#3）  
- [ ] Space 再生、オニオン表示  

---

## フェーズ 3 — `main.rs` 分割（挙動変更ゼロ）

分割は **フェーズ 0–1 のテストが赤を検知できる状態** でから。

```text
src/
  main.rs           # 起動のみ
  model.rs          # Note, NoteId, ChordBlock, Project
  gen/
    pitch.rs        # map_pattern_pitch, melodic_pitch, trim_*
    generate.rs     # generate_from_patterns
    cleanup.rs      # range, overlap, ids
  edit/
    clipboard.rs
    chord.rs
    selection.rs
  midi_io.rs
  ui/               # タブ別（最後）
  audio.rs
```

手順: 純関数（pitch/cleanup）→ generate → edit → ui。  
各ステップで `cargo test` 緑。

---

## フェーズ 4 — ランタイム防御と開発プロセス

- `debug_assert!`（id 一意、bass range）  
- 生成後ログ: `out_of_range=N overlap=M`（debug）  
- セッション: `/jpo-producer`、1 テーマ 1 変更、HANDOVER 更新  
- Windows クローンは配布用。日常は WSL

---

## 成功指標

| 指標 | 目標 |
|------|------|
| #3 | 再発しない（テスト固定） |
| #2 | Ch3 が帯域 assert を常時パス |
| #1 | 生成 Piano の same-pitch overlap = 0（方針確定後） |
| 回帰 | 生成変更で Tab3 選択が壊れない |
| 構造 | cleanup / pitch が UI なしでテスト可能 |
| スコープ | Domino 全機能を追わない |

---

## 今すぐの順番（更新版）

1. **#3 NoteId + unique テスト**  
2. **cleanup 骨格 + bass range assert（#2）**  
3. **same-pitch overlap 方針決定 + テスト + 実装（#1）**  
4. **case03 を数値契約に**  
5. **patterns を Domino で必要分だけ清掃**  
6. **pitch/cleanup モジュール切り出し**  
7. **薄い Undo / snap 分離（任意）**

---

## やらないことリスト（最適化）

- Domino の CC マクロ・音源 XML・ストロークの完全再現  
- egui でイベントリスト全機能  
- 分割と #1–#3 修正の同時実施  
- WSL 移行だけでバグが消えるという期待  
