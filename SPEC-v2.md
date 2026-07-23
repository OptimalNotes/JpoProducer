# JpoProducer SPEC v2.0 — 定義の再凍結

**版:** v2.0（2026-07-23）  
**位置づけ:** プロダクトの**唯一の真実**。本 SPEC が `SPEC-v1.md` に優先する。  
**前版:** [`SPEC-v1.md`](SPEC-v1.md) は履歴・思想の出典。矛盾時は **v2 が勝つ**。  
**実装ギャップ:** [`HANDOVER.md`](HANDOVER.md)

---

## 0. なぜ v2 を書くか

v1 は「当時の開発余力」に合わせて切った仕様だった。

- API / 高度な入力状態機械 / モジュール分割は「難しいから後」
- シンコはヒントフラグ程度の定義
- 4 タブはショートカット事故の応急処置をそのまま Ship 条件に近づけた
- スタンプ保存・Loop SoT・複数選択ジェスチャは実装が先行し契約が薄かった

**いま**は五本柱が仮実装として一周でき、全体像が見えた。  
ボトルネックは「作れないこと」ではなく、**定義が古いこと**と、積み上げパッチによる**二系統の真実**である。

v2 の態度:

> 最高に美しいスケッチツールを、契約で先に固定し、動いている資産を**捨てずに整形し直す**。  
> ゼロからの全面書き換えはしない（学習・テスト・パターン・UX 発見を捨てるため）。  
> ただし「v1 の遠慮」は捨てる。定義は今の実力に合わせて書く。

---

## 1. プロダクト一文

> **J-Pop / J-Rock 特有の「細かく・速いコード切り替え」とシンコペを、最速で置き、  
> メロディが浮かぶ程度の単純伴奏を一瞬で敷き、  
> 普通の MIDI 編集で整え、4/8/16 小節ループをつないで曲の骨格にし、  
> Grok（および任意の LLM）に進行を渡してパートを足す。  
> 単一 exe + SF2 一枚で、誰でも今日から試せる。**

Domino / DAW の置き換えではない。**進行と骨格の爆速スケッチ + 共創**が本体。

---

## 2. 設計原則（v2 の憲法）

| # | 原則 | 意味 |
|---|------|------|
| D1 | **Single Source of Truth** | 同じ事実を二箇所に持たない。持つなら「真実 / キャッシュ」を文書化する |
| D2 | **契約が先、パッチが後** | シンコ・選択・スタンプ・入力は文章とテストで固定してからコードを触る |
| D3 | **純関数コア** | 生成・pitch・cleanup・時間変換は UI なしでテストできる |
| D4 | **入力は状態機械** | フラグの寄せ集めではなく `InputFocus` で許可ショートカットを決める |
| D5 | **レイアウトは一人の所有者** | 拍↔x / pitch↔y は `TimelineLayout` のみ |
| D6 | **薄い境界、厚い体験** | exe+SF2 で完結。API・拡張は任意の加速レーン |
| D7 | **Reshape, don't rewrite** | 動く `main.rs` を殺し切らない。核から切り出し、契約テストで守る |
| D8 | **1 テーマ 1 変更** | 分割と挙動修正を同時にしない（分割は挙動ゼロ専用 PR） |

### やること / やらないこと（スコープの美学）

| やる | やらない |
|------|----------|
| 密な進行・シンコの**被覆契約** | 自動で名曲を吐く |
| Simple Bed（薄い土台） | フルアレンジ自動化 |
| 普通のピアノロール（複数選択含む） | Domino 全機能・CC マクロ・プラグインホスト |
| 4/8/16 ループパズル | 無限タイムライン DAW |
| Grok 共創（コピー + API + import） | 全 LLM 公式サポート必須 |
| 美しいモジュール境界 | リライト祭り・archive/jpo-v2 復活 |

---

## 3. 五本柱（優先は不変）

| # | 柱 | ユーザー価値 | v2 での強化点 |
|---|-----|-------------|---------------|
| **P1** | 密なコード進行 | 16 分級の切り替え・前ノリをストレスなく | スタンプ規約・シンコ印の意味を明確化 |
| **P2** | 単純伴奏ベッド | メロが書ける薄い P+B+D | **シンコ = 窓の完全被覆**（無音禁止） |
| **P3** | 普通の MIDI 編集 | 選択・複選移動・描画・コピペ・Undo | 選択モデル単一 + ジェスチャ契約 |
| **P4** | ループ接続 | Bank → 並べて骨格 | Loop SoT を一文で固定 |
| **P5** | Grok 共創 | 進行を渡しパートを import | S1 クリップボード必須 / S2 API は**第一級の任意** |

優先順位を逆転させない（例: 見た目統合のために生成契約を壊さない）。

---

## 4. 情報アーキテクチャ

### 4.1 ターゲット: 3 ワークスペース

創作の本流:

```text
進行を書く → ベッドを敷く → メロ/パートを書く・Grok → ループを並べる
```

| Workspace | 役割 | やらないこと |
|-----------|------|--------------|
| **Progress** | コード進行専用 | ノート編集、ベッド詳細 |
| **Sketch** | 1 ループの制作机（Bed + Roll + Grok） | 曲全体の並べ替え |
| **Arrange** | Loop Bank・順序・通し・フル export | ノート編集 |

**現行 4 タブ（Progress / Bed / Edit / Arrange）** は v2 移行中の許容形態。  
**Ship 条件:** 五本柱 + 本 SPEC の契約。見た目の 3 統合は推奨だが、**契約を満たせば 4 タブのまま 2.0 タグ可**。  
統合は「挙動ゼロでパネルを寄せる」作業とし、生成アルゴリズムと同時にやらない。

### 4.2 入力フォーカス（不変条件）

```text
ChordStrip | PianoRoll | GrokText | Arrange | None
```

| Focus | 許可 |
|-------|------|
| ChordStrip | ブロック操作。ノート Ctrl+C/V 無効 |
| PianoRoll | Select/Draw/Erase、Ctrl+C/X/V/D、Q/W/E、矢印、Delete、Undo |
| GrokText | 文字入力・フィールド内コピペ。ロールの Q/W/E やノート Ctrl は奪わない（Ctrl+Z は可） |
| Arrange | Bank / スロットのみ |
| None | Space 再生などグローバルのみ |

ロールまたはツールバー操作で **必ず PianoRoll に遷移**し、TextEdit フォーカスを解放する。  
実装はフラグ散在ではなく、この表に従う一関数 `shortcuts_allowed(focus, key)` が理想。

### 4.3 TimelineLayout（不変条件）

Sketch / Edit における:

- 鍵盤ガター幅
- コンテンツ矩形
- `px_per_beat`
- 可視拍範囲
- pitch 行（min/max, row count）

は **単一の `TimelineLayout`** が所有する。  
chord ルーラー・ピアノロール・Vel レーンは同じ layout を読む。幅の独自 cap 禁止。

---

## 5. データモデル

### 5.1 基本型

```text
NoteId        — プロジェクト内で一意な u64
Note          — { id, start_beats, dur_beats, pitch, vel }
ChordBlock    — { start, dur, degree, quality, octave, syncopation_fill }
TrackData     — { ch, notes[], patch, mute, solo, vol }
Project       — { bpm, key_root, is_minor, tracks[1..=16], chord_blocks[] }
LoopSketch    — 名前付き 1 ループのスナップショット（下記 SoT）
```

- 時間は常に **拍**。生成は **BPM 非依存**
- SMF は import/export/再生スケジュール境界のみ
- Ch1 の「実 MIDI ノート」は SoT ではない（export 時に `ChordBlock` から展開）

### 5.2 チャンネル契約

| Ch | 役割 |
|----|------|
| 1 | 進行（ブロック SoT） |
| 2 | Piano ベッド + 手編集 |
| 3 | Bass（生成帯域 MIDI 28–51） |
| 10 | Drum（ピッチ転調しない） |
| 2–16 | Sketch 手編集 |

### 5.3 Loop の SoT（v2 で固定）

```text
真実:  loop_bank[active_idx]  （LoopSketch）
編集バッファ: Project（画面が触る対象）
```

規則:

1. ループ切替・保存・Arrange 通し再生・`.jpo` 保存の**直前**に `Project → bank[active]` を flush  
2. ループ切替時は `bank[i] → Project` を load  
3. ツールバーの Key は **アクティブループ**のキー（将来のループ単位キーと一致）  
4. BPM はプロジェクト共通（v2 では単一 BPM のまま）

「ときどき capture」は禁止。flush 漏れはバグ。

### 5.4 選択モデル（単一）

```text
NoteSelection {
  primary: Option<(track_idx, NoteId)>,
  ids: Set<(track_idx, NoteId)>,  // primary ⊆ ids（空なら primary=None）
}
```

- ハイライト・コピー・Delete・移動はすべて `ids`  
- `selected_note` と `selection.notes` の二重保持は **廃止対象**（移行完了まで同等動作をテストで固定）

### 5.5 複数選択ジェスチャ（契約）

| 操作 | 結果 |
|------|------|
| 空きドラッグ | 箱選択（additive は Ctrl） |
| Ctrl+クリック | トグル複選 |
| **既に選択集合に含まれるノートをドラッグ開始（Ctrl なし）** | **集合を維持したまま全員移動** |
| 未選択ノートをクリック（Ctrl なし） | その音だけ選択 → 移動可 |
| 右端 | 選択集合の Gate 一括（または primary のみ — v2.0 は **集合一括**） |

「掴んだ瞬間に clear して 1 音になる」は **バグ**（H2）。

---

## 6. 柱ごとの仕様

### 6.1 P1 — 密な進行

**SoT:** `chord_blocks[]`（非重複。enforce で**削除しない**）

| 項目 | 仕様 |
|------|------|
| グリッド | 既定 1/16 拍。Grid と Len は分離 |
| 最小長 | 0.25 拍 |
| 操作 | 空き配置・移動・右端伸長・パレット・ダブルクリック削除 |
| シンコ印 ◆ | ブロック属性 `syncopation_fill`。**伴奏生成への指示**（見た目だけではない） |
| UI | 小節/拍/16 分線、短いラベル、ホバー全文 |

### 6.2 進行スタンプ（H3 の正本）

**目的:** ユーザーと公式の「進行テンプレ」を、再現可能・上書き明確・配布可能にする。

#### ファイル

| 項目 | 仕様 |
|------|------|
| 場所 | 実行ファイル隣 `stamps/`（ポータブル）。開発時は seed 元 `assets/stamps/` |
| 拡張子 | `.jpostamp`（JSON） |
| 文字コード | UTF-8 |

#### スキーマ（論理）

```text
Stamp {
  schema: 1,
  name: string,           // 表示名（ファイル名と一致推奨）
  bars_hint: 4 | 8 | 16 | null,  // 目安。適用時にクリップ可
  blocks: ChordBlock[],   // start は 0 始まり相対。apply 前に normalize
  // 含めない: bpm, absolute key（degree 相対なのでキー非依存）
}
```

#### ルール

1. **相対時刻:** 保存時に最小 start を 0 に正規化  
2. **追記:** 適用 = 現在進行の **末尾**に貼る（空なら 0）。playhead 非依存  
3. **名前:** UI 表示名。ファイル名は sanitize。同名保存は **確認ダイアログのうえ上書き**  
4. **seed:** 初回のみ `assets/stamps` → `stamps/` コピー。以降ユーザー改変を assets で潰さない  
5. **削除:** `stamps/` 内はユーザー所有として削除可。assets 直読みはしない（seed 後）  
6. **読み込み失敗:** 壊れたファイルはスキップし、一覧に出さない（アプリは落ちない）  
7. **メタ bars_hint:** 適用時ループ長より長い分はクリップ（既存 `paste_stamp_blocks_clipped` 思想）

### 6.3 P2 — Simple Bed + シンコ（H1 の正本）

#### Simple Bed

| 操作 | 挙動 |
|------|------|
| Simple Bed | アクティブループ範囲（または明示 gen 範囲）に Ch2/3/10 を一括生成 |
| Clear Bed | 同範囲の Ch2/3/10 をクリア |
| パターン | Piano / Bass / Drum 各選択。既定は素直な 1 種 |
| Preview | 非破壊試聴 → Apply |

#### 生成パイプライン（唯一入口）

```text
BedRequest
  → place patterns on chord timeline (beats, BPM-independent)
  → pitch map (piano quality / bass register / drum identity)
  → apply_sync_policy   // ◆ ブロックのみ
  → cleanup (unique ids, same-pitch overlap, bass band, chord-tail clip)
  → BedResult + Diagnostics
  → replace_notes_in_range
```

#### シンコペーション契約（最重要）

**定義:** `syncopation_fill == true` の各ブロックについて、

```text
W = sync_window_len(block.dur)   // 例: min(2.0, max(0.5, block.dur - 1.5)) 等。関数はテスト固定
window = [block.start, block.start + W)
tail   = [block.start + W, block.end)
```

1. **window は伴奏で完全被覆する**  
   - 半拍ビン（0.5 拍）で見たとき、window 内に **連続無音 ≥ 0.75 拍を禁止**  
   - Piano / Bass / Drum それぞれに適用（ドラムも可。キット音で埋める）  
2. 被覆手段: sync パターンの **tile または wrap**。onset が足りなければ位相をずらして埋める  
3. **tail** は通常パターンで埋める（block 位相維持が基本）。tail 先頭 1 拍の無音も禁止  
4. グローバル Sync チェックは「ポリシー有効」。窓の対象は **◆ ブロック**  
5. 回帰: `0723test.jpo`（Am◆ @ 7.5）で Ch2/3/10 の 8.0–9.5 無音が **再現禁止**

> v1 の「窓に何かあれば OK」「refill に何かあれば OK」は不十分。**連続被覆**が契約。

#### cleanup 契約

| 項目 | 扱い |
|------|------|
| NoteId 重複 | バグ |
| Bass 帯域外 | バグ（生成後） |
| 同一 pitch 時間重なり（Piano） | バグ |
| 異 pitch 同時発音 | 仕様 |
| quality 無視 | バグ |

### 6.4 P3 — MIDI 編集

| 必須 | 内容 |
|------|------|
| ツール | Q Select / W Draw / E Erase（排他） |
| 選択 | §5.4–5.5 |
| 編集 | 移動・右端 Gate・Delete |
| クリップボード | 相対時刻。貼付 = playhead。コード境界での勝手な切り詰め **禁止** |
| Undo | プロジェクトスナップショット（深度 16+） |
| オニオン | スケール + コードトーン |
| 再生 | Space、SF2、Transport 常時可視 |
| 縦 | 既定 ~2 oct |
| 下レーン | Vel（既定）/ 音色 / Grok。Grok を開いたときだけ Text フォーカスしやすくする |

### 6.5 P4 — Arrange

| 項目 | 仕様 |
|------|------|
| ループ長 | 4 / 8 / 16 のみ |
| Bank | 名前・複製・切替（切替前 flush） |
| シーケンス | スロット + 繰返し |
| Export | シーケンス全体 Type 1 MIDI |
| 再生 | 通し。ループ境界でノートがはみ出して次を汚さない |

### 6.6 P5 — Grok 共創

| 経路 | 必須？ | 内容 |
|------|--------|------|
| **S1** ジョブコピー | **必須** | System 契約 + 進行 + アンケート → クリップボード。オフライン完結 |
| **S2** API | **第一級の任意** | キーがあれば送信。無くても S1 で 100% 使える |
| Import | **必須** | `JPO_NOTES_V1` / 素の note 行 / MIDI。NoteId 採番。playhead + トラック |

v1 の「API は複雑だから後回し」は撤回する。  
ただし **キー無し単体完結** は D6 のため死守する。

ジョブに必ず含める: Key / Mode / BPM / 拍範囲 / コード一覧 / 役割・密度等アンケート / 「雑談禁止・ノートデータのみ」契約。

---

## 7. モジュール境界（美しいコードの形）

目標ツリー（挙動ゼロで段階的に到達）:

```text
src/
  main.rs              # 起動・eframe 入口のみへ収束
  app.rs               # JpoApp 状態・タブ
  model.rs             # Note, Project, LoopSketch, selection
  time_layout.rs       # TimelineLayout, beat↔x, pitch↔y
  input.rs             # InputFocus, shortcut table
  gen/
    mod.rs
    pipeline.rs        # BedRequest → BedResult
    pitch.rs
    sync.rs            # 被覆契約
    cleanup.rs
  edit/
    clipboard.rs
    gestures.rs
  stamps.rs
  midi_io.rs
  audio.rs
  ui/
    progress.rs
    sketch.rs
    arrange.rs
```

**分割規則:** 1 PR = 1 切り出し、全テスト緑、挙動変更ゼロ。  
挙動修正（H1/H2 等）と分割を混ぜない。

---

## 8. テスト契約

### 自動（必須）

- 既存: id / bass / piano quality / overlap / chord enforce / paste / stamps 相対 等  
- **新規必須:**
  - シンコ連続被覆（`0723test` 相当 or golden）  
  - pitch_at_y / beat_at_x ラウンドトリップ  
  - 複数選択: 「集合に含まれるノートを掴んでも ids が shrink しない」純関数 or シナリオ  
  - Loop flush: 編集 → switch → 戻る でノートが消えない  

### 手動（ACCEPTANCE 更新）

五本柱 + シンコ無音なし + 複選移動 + スタンプ保存/追記/上書き確認 + S1 往復 + portable。

---

## 9. v2.0 Definition of Done

### 自動

- [ ] `cargo test` 全パス（被覆・複選・flush を含む）

### 手動

- [ ] P1–P5 が手で一周  
- [ ] H1 シンコ無音解消（0723test 目視 + 自動）  
- [ ] H2 複数選択一括移動  
- [ ] H3 スタンプ規約どおり保存・追記・上書き  
- [ ] 座標一致・Q/W/E・Ctrl+C/V（回帰）  
- [ ] `.jpo` 保存読込、Space 再生、portable zip  

### 出荷

- [ ] version `2.0.0`（または市場向けに `1.0.0` をこの DoD に載せるなら README で明示）  
- [ ] `pack.ps1` が別 PC で起動  

### v2.0 に含めない（意図的）

- Chrome 拡張（Co-Producer）本体  
- Arrange 本格 DnD  
- 日本語フォント同梱必須  
- `main.rs` 完全分割完了（**推奨・進行中で可**。DoD は核モジュール切り出しまでを推奨ゲート）  
- 全 GM 音色の耳チューニング  

---

## 10. 実装スケジュール（Reshape ロードマップ）

順序厳守。柱をまたぐ巨大 PR 禁止。

| Phase | 内容 | 成果 |
|-------|------|------|
| **0** | 本 SPEC 凍結・文書ポインタ更新 | 全員が同じ地図 |
| **1** | 契約テスト追加（赤でよい）: 被覆・複選・flush | 後退検知 |
| **2** | H2 選択ジェスチャ + 選択モデル単一化 | 複選移動 |
| **3** | H1 シンコを sync.rs 契約どおり実装 | 0723test 緑 |
| **4** | H3 スタンプ UI を規約に合わせる | 保存体験が説明可能 |
| **5** | TimelineLayout + InputFocus 整理 | 座標・ショートカット再発防止 |
| **6** | gen pipeline 切り出し（挙動ゼロ） | 美しい核 |
| **7** | Sketch 導線（Bed 統合 or 常駐バー）挙動ゼロ寄り | 体験 |
| **8** | ACCEPTANCE 再実施・version・portable | Ship |

保留だった「小さな不具合」は Phase 2–4 に吸収する。  
その前に仕様を変えないパッチだけを積み増さない。

---

## 11. プロセス

1. 仕様変更は **先に SPEC-v2** → コード → HANDOVER  
2. 1 セッション 1 テーマ  
3. 生成 / pitch / sync / cleanup 変更はテスト必須  
4. golden をバグに合わせて改ざんしない  
5. Domino GUI 自動化しない  
6. 開発: WSL or Windows。GUI 試聴は Windows 推奨  
7. ユーザー承認なしに DoD 外スコープを広げない  

---

## 12. v2.1+（今は実装しない・忘れない）

- Grok Co-Producer（Chrome 拡張・クリップボード往復）  
- ループ単位 Key の UI 完成度向上  
- Arrange ドラッグ並べ替え  
- 伴奏バリエーション（Simple の範囲内）  
- 構造化 JSON の Grok 往復強化  
- 日本語フォント同梱  

---

## 13. v1 からの移行メモ

| v1 | v2 |
|----|----|
| シンコ = ヒント + 薄い splice | **窓の完全被覆** |
| 選択の二重モデル | **単一 NoteSelection** |
| Loop の暗黙 capture | **bank[active] SoT + 明示 flush** |
| スタンプ実装先行 | **スキーマと上書き規則** |
| API 後回し | **S2 は任意の第一級** |
| 4 タブを正当化 | **3 がターゲット、4 は移行許容** |
| 遠慮したスケジュール | **Reshape 前提の大胆な定義** |

コードの全面破棄はしない。  
**定義を先に正し、テストで赤を出し、核から美しく付け替える。**

---

## 改訂履歴

| 日付 | 内容 |
|------|------|
| 2026-07-23 | v2.0 初版。全体像レビューを受け、定義を再凍結。H1/H2/H3 を契約化。Reshape 方針を明文化 |
| 2026-07-15 | （v1）五本柱 + 3 ワークスペース案。履歴として SPEC-v1.md に残す |
