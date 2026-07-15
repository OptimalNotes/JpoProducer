# MIDI / シーケンサ実装の業界知識（要約）

エージェント向け。詳細仕様書ではなく **バグを減らす設計原則**。

---

## 1. 二つの表現を混同しない

| 層 | 表現 | 用途 |
|----|------|------|
| **編集モデル** | Note = `{id, start, dur, pitch, vel, ch}` | UI・生成・選択・クリップボード |
| **ワイヤ/SMF** | NoteOn / NoteOff（delta-tick） | 再生スケジューラ・ファイル I/O |

よくあるバグ:

- 編集中に On/Off ペアで持ち、片方だけ動かす → 音が鳴りっぱなし  
- SMF 読み込みで Off が欠落したファイルを前提にしない  
- **PrettyMIDI / 現代エディタは「ノートイベントリスト」に正規化**してから扱う

Jpo は編集モデル側。export/import でのみ SMF に落とす。

---

## 2. 時間は「拍」または「tick」、秒は再生時だけ

- SMF: `ticks_per_quarter` + tempo meta → 実時間  
- **グリッド上の start/dur を BPM で伸ばさない**（テンポ変更でフレーズ形状が壊れる）  
- Jpo の beat-grid / `pattern_time_scale = 1.0` はこの原則に合う。壊すな

---

## 3. アイデンティティ（最重要・UI）

ピアノロール実装で頻出する失敗（HISE フォーラム等でも「ID tracking issue」）:

- 生成ノートがすべて `id=0`  
- 選択が `Vec` の index のまま、ソートや削除後にずれる  
- 描画が `id == selected` なのに id 非ユニーク → **全部ハイライト**

ルール:

1. 生成・import・paste のたびに **新しいユニーク id**  
2. 選択集合は `HashSet<NoteId>`（＋ track）  
3. 変異後も id で追跡。index は一時的な描画順だけ

---

## 4. ポリフォニーと「同じ音の重なり」

- MIDI ch 上で **同一 pitch の NoteOn が重なる**と、音源によっては NoteOff 1回で両方消える / ステックする  
- ベースは事実上モノフォニックに近く、**時間重なりを切る**のが一般的  
- ピアノは和音 OK だが、**同一 pitch の時間 overlap** はしばしば意図しない（テンプレ長 Gate が原因）

テスト可能な不変条件例:

```text
bass: no two notes on Ch3 with overlapping [start, start+dur) 
      OR at least no same-pitch overlap
bass: all pitches in [28, 51]  // product range
piano: optional same-pitch no-overlap after generate
all: unique NoteId across project (or per track)
```

---

## 5. チャンネル慣習

| Ch | 慣習 | Jpo |
|----|------|-----|
| 10 | ドラム（GM） | Drum、転調しない |
| その他 | メロ / 伴奏 | Ch1=コード、Ch2=Piano、Ch3=Bass |

ドラムピッチは「音高」ではなく **キットマップ**。生成で scale を掛けない。

---

## 6. 即時モード UI（egui）向け

- 毎フレーム: ヒットテスト → ドラッグ状態マシン → 描画  
- **ドラッグ中の仮位置**と **コミット後のプロジェクト**を分ける  
- スナップは「表示とコミット」の両方で同じ関数を使う  
- タブ切替でドラッグ状態を必ずクリア（Jpo `switch_tab`）

Domino はツールモードを明示。egui でも `enum Tool { Select, Draw, Erase }` を単一の真実に。

---

## 7. 生成パイプラインの定石

```text
patterns (SMF notes)
  → time map onto chord blocks (beat grid)
  → pitch map (bass fold / piano transpose)
  → cleanup (overlap trim, range clamp, unique ids)
  → replace_notes_in_range
  → (optional) preview render
```

**cleanup を生成の最後に必ず通す。** UI や耳だけに頼らない。  
各段を純関数にするとユニットテストしやすい。

---

## 8. 回帰テスト戦略

| 種類 | 内容 |
|------|------|
| 単位 | pitch fold, window, window window, paste offset |
| 不変条件 | id unique, bass range, optional no-overlap |
| ゴールデン | 固定 chord_blocks + patterns → 期待 pitch 集合 / 統計 |
| SMF 往復 | export → import で note 集合が保たれる（将来） |
| 非テスト | 見た目のピクセル。代わりに「選択 id 集合」を assert |

ユーザー報告は `tests/golden/caseNN/` に broken 根拠を残す。

---

## 9. 参考（概念）

- SMF / tick: MIDI 仕様・PrettyMIDI 的ノート正規化  
- Domino: 国産ピアノロール MIDI 専用の UX 完成形（ローカル Manual）  
- DAW 一般: 重なり整理・レガート強制・クオンタイズ分離（Logic 等の “trim overlapping notes”）  
