# JpoProducer 引継ぎ書

**最終更新:** 2026-07-23  
**仕様の真実:** [`SPEC-v2.md`](SPEC-v2.md)（v1 は履歴）  
**リポジトリ:** https://github.com/OptimalNotes/JpoProducer  
**開発:** WSL `~/JpoProducer` ／ 配布・GUI: Windows `C:\Users\user\JpoProducer\`  
**ポータブル:** `dist/JpoProducer-portable-YYYY-MM-DD/` または `.zip`（`pack.ps1 -Zip`）

---

## いまのフェーズ（SPEC-v2）

| Phase | 状態 |
|-------|------|
| **0** 定義再凍結 | ✅ SPEC-v2 |
| **1** 契約テスト | ✅ |
| **2** H2 複選移動 | ✅ |
| **3** H1 シンコ被覆（穴なし） | ✅ 連続被覆は達成。**音楽的品質は後回し**（下記） |
| **4** H3 スタンプ規約 | ✅ schema/bars_hint/上書き確認/名前欄 |
| **5** TimelineLayout + InputFocus | ✅（2026-07-23） |
| **6+** gen pipeline 切り出し / Sketch 導線 / Ship | 未着手 |

**やらない:** コードのゼロからの全面破棄。**Reshape, don't rewrite.**  
ロードマップ詳細は SPEC-v2 §10。

### 契約テスト（`cargo test`）

| 結果 | 内容 |
|------|------|
| ✅ **64 passed / 0 failed** | Layout/Focus 契約を含む全緑 |

### Phase 5 変更要約

- `TimelineLayout`: beat↔x / pitch↔y の単一所有者。`TIMELINE_KEY_WIDTH` 共有  
- ルーラー・ピアノロール・鍵盤が layout 経由で変換  
- `InputFocus` { None, ChordStrip, PianoRoll, GrokText, Arrange }  
- ショートカット可否は `allows_edit_shortcuts`（フラグ寄せ集めを縮小）

### 後回し（ユーザー確認）

| ID | 内容 |
|----|------|
| **H1b** | シンコの**音楽的**な仕上がりが不自然（穴は消えたが間違った感じ）。被覆ロジックの再設計が必要。Phase 3 では「穴ゼロ」のみ優先済み |

### Phase 4 変更要約（H3）

- スキーマ: `schema: 1`, `bars_hint`, `name`, `blocks`（旧ファイルも読める）  
- 保存: 相対正規化 + `infer_bars_hint`  
- 同名: **上書き確認ダイアログ**（勝手な `_2` リネーム廃止）  
- UI: 名前入力欄 + 保存 / 上書き Window  
- `assets/stamps/README.txt` を SPEC-v2 に合わせて更新  

### Phase 3 変更要約（H1）

- tile + `ensure_onset_coverage`（穴なし契約）  
- **音楽性は H1b として未解決**  

### Phase 2 変更要約（H2）

- 複選移動: `selection_on_note_press` 配線済み

---

## 次セッションの入口（ここだけ読めば再開できる）

### 再開手順（Windows / WSL）

```powershell
# Windows（配布・GUI）
cd C:\Users\user\JpoProducer
git pull origin main
cargo test
cargo run --release
```

```bash
# WSL（編集向けクローンがある場合）
cd ~/JpoProducer && git pull origin main && cargo test
```

1. **仕様:** [`SPEC-v2.md`](SPEC-v2.md)（v1 は履歴）  
2. **このファイル** の Phase 表と「次にやること」  
3. **GUI:** `cargo run --release`（SF2 は exe と同じフォルダ）  
4. **API (S2):** 任意で `XAI_API_KEY` / `GROK_API_KEY`  

### 次にやること（優先候補）

| 優先 | 内容 |
|------|------|
| **A** | **H1b** シンコの音楽的仕上がり（穴は消えたが音が不自然 — ユーザー確認済み） |
| **B** | Phase 6: `gen/` pipeline 切り出し（挙動ゼロ） |
| **C** | ACCEPTANCE 再実施 → version / portable → Ship 準備 |

4 タブ構成は**維持**（ユーザー快適・意図的）。3 ワークスペース統合は急がない。

### いまの完成度スナップショット

| 領域 | 状態 |
|------|------|
| Progress (Tab1) | モデルレス + スタンプ追記/保存（SPEC-v2 規約）・進行クリア |
| Bed (Tab2) | Simple Bed。シンコは**穴なし**だが音楽性は H1b 後回し |
| Edit (Tab3) | 複選一括移動 OK。TimelineLayout + InputFocus。Vel/音色/Grok レーン |
| Grok | 発注デスク S1/S2 |
| テスト | `cargo test` → **64 passed** |
| Ctrl+C/V | 相対時刻・**コード切り詰めなし**。Grok TextEdit 残留フォーカス対策済み |
| テスト | `cargo test` → **49 passed**（2026-07-23） |

---

## 2026-07-23 修正（ユーザー報告 Tab3 / Tab2）

| 症状 | 原因 | 修正 |
|------|------|------|
| Ctrl+C/V が効かない | Grok 等 TextEdit のフォーカスが残り `wants_keyboard_input` でショートカット全体をスキップ | ロール操作で `clear_keyboard_focus` + `piano_roll_focused` 時は編集キーを許可 |
| 入力座標ズレ | 鍵盤ガター無しの chord ルーラー、roll 幅 cap(940) vs Vel(1100)、pitch 行数 off-by-one | Edit ルーラーに key ガター、幅統一、`pitch_row_y`/`pitch_at_y` を row count 一致 |
| Q/W/E が不安定 | 同上フォーカス + ツール切替が TextEdit を外さない | ツールボタンでフォーカス解除、Edit タブでは Q/W/E を Ch1 でも処理 |
| Draw で位置がおかしい | `drag_started` と `clicked` の二重配置 | Create 済みなら click で再 place しない |
| Tab2 シンコ後 1 拍無音 | refill が block 位相のみ・overlap clear が強すぎる場合 | 先頭1拍が空なら rephase refill、fill 開始境界は clip してから差し替え |

**手確認推奨:** Tab3 でノート選択 → Ctrl+C/V、Q/W/E、クリック位置＝見た目。Tab2 で ◆ + Sync → Simple Bed 後の隙間。

---

## 2026-07-22 セッションで入れた主な変更

### Progress
- スタンプ = 末尾追記（空なら 0）。playhead 非依存  
- UI: コンボ + 追記 + 進行クリア + 現在を保存  
- モデルレス（Draw/Erase なし）  

### Edit
- Play が Vol に被らない Transport（固定幅 LTR）  
- Vel 棒グラフレーン  
- 音色レーン（幅拡大）  
- Grok 発注デスク + 縦スクロール  
- テキスト入力中はショートカット無効（Enter で選択が飛ばない）  
- 貼り付け: 相対クリップ + chord clip 廃止  

### Grok ジョブ（アンケート項目）
小節レンジ / 役割 / 位置づけ / 音域 / 密度 / リズム / 曲の雰囲気 / 欲しいフレーズ / 禁止 / ボーカル余地 / Bed 避け  
→ `build_grok_part_job()` が System 契約 + 進行 + アンケートを1本化  

### 依存
- `ureq`（S2 API）  

---

## 保留バグレポート（2026-07-23 ユーザー確認・**未着手**）

> 修正しない。着手指示があるまで触らない。再現素材を優先する。

### 確認済み（直った）

| 項目 | 状態 |
|------|------|
| Tab3 座標不一致 | ✅ 直った（ユーザー確認） |
| Tab3 Q / W / E | ✅ 直った（ユーザー確認） |

### 未解決・新規（保留）

| ID | 領域 | 内容 | 再現素材・メモ |
|----|------|------|----------------|
| **H1** | Tab2 シンコ | **まだ直っていない。** シンコ（◆）付きブロック後に伴奏が途切れる／穴が開く | スクショ: `OneDrive\画像\Screenshots\シンコぺ直ってない.png` — Am◆ 下の Piano/Bass にギャップ（赤丸）。プロジェクト: `Desktop\0723test.jpo`（BPM 128, C maj, Piano03 + Bass8beat01 + Drum8beat_01, Sync ON）。画面は 8 小節ループ、進行 C–G–**Am◆**–Em–F–C–F–G 系 |
| **H2** | Tab3 複数選択移動 | 音符を複数選択しても**まとめて移動できない** | 箱選択 or 複選後、ドラッグで全員が追従しない／1 音だけ動く等。詳細は再確認時に切り分け |
| **H3** | Progress スタンプ | **ユーザー進行テンプレ保存のルールが未定義**。保存 UI はあるが規約が汚く／不明 | ファイル形式（`stamps/*.jpostamp`）、命名、ループ長・キーの扱い、上書き、同梱 vs ユーザー領域、append 仕様との関係を SPEC 化してから綺麗にしたい |

### 以前リスト（継続）

| 優先 | 内容 |
|------|------|
| 手確認 | 別 PC で portable zip 起動・SF2・貼り付け・Grok S1 往復 |
| UX | Grok デスクの項目密度（狭ディスプレイ） |
| S2 | 実 API キー E2E |
| 任意 | warning 一掃、`main.rs` 分割 |
| v1.0 | `ACCEPTANCE.md` 再実施、version 1.0.0 タグ |

**Tab1 は凍結寄り**（H3 スタンプ規約は例外として設計議論可）。  
**未 push:** 2026-07-23 部分修正（座標/QWE 等）はローカル。H1 未解決のためシンコ関連コミットは急ぎ push しない方がよい。

---

## 後回しタスク: Grok Co-Producer（Chrome 拡張）

**本体が一段落してから。** 詳細: [`docs/FUTURE-grok-co-producer.md`](docs/FUTURE-grok-co-producer.md)

| 項目 | 方針 |
|------|------|
| 名前のニュアンス | Grok を**共同制作のプロデューサー**に（仮称 Grok Co-Producer） |
| 公式範囲 | **Grok 専用**拡張。他 LLM はジョブ貼り付け DIY + GitHub |
| 本体 | 単体 + Fluid で完結。任意 LLM にジョブ可（推奨 Grok） |
| 通信 | まず **クリップボード**（故障少）。必要なら後から localhost HTTP |
| 配布 | 最初は Chrome **デベロッパーモード**でフォルダ読み込み（ストア不要） |

ストーリー例:
1. ソフト DL + Fluid → そのまま使える／LLM チャットで作曲補助  
2. 拡張で Grok 連携 → 往復がラク  
3. 他 LLM がいいならソース見て自分で  

---

## ポータブル配布物（この PC・2026-07-22 時点）

別 Windows PC へ持っていく用（再生成: `pwsh -File pack.ps1 -Zip`）:

| 種類 | フルパス |
|------|----------|
| **ZIP（おすすめ）** | `C:\Users\user\JpoProducer\dist\JpoProducer-portable-2026-07-22.zip`（約 130 MB） |
| **展開フォルダ** | `C:\Users\user\JpoProducer\dist\JpoProducer-portable-2026-07-22\` |

中身: `jpo.exe` + `FluidR3 GM.SF2` + `START.txt` + `patterns\` + `stamps\`  
使い方: zip をコピー → 展開 → `jpo.exe`（SF2 は同じフォルダのまま）

---

## 開発コマンド

```powershell
# Windows
cd C:\Users\user\JpoProducer
cargo test
cargo build --release
pwsh -File pack.ps1 -Zip
# → dist\JpoProducer-portable-YYYY-MM-DD.zip
```

```bash
# WSL
cd ~/JpoProducer
cargo test
# GUI は Win 側推奨
```

---

## ポータブル pack に入るもの

- `jpo.exe`  
- `FluidR3 GM.SF2`  
- `START.txt`  
- `patterns\*.mid`（Bed 用）  
- `stamps\*.jpostamp`（進行スタンプ）  

**入らない:** Rust ツールチェイン、`.jpo` ユーザースケッチ、API キー  

---

## Git / ブランチ

- origin/main（取り込み済み）: `6c9f1f6` docs Co-Producer defer / `7844bf6` Tab3 order desk  
- **2026-07-23 修正は未コミット**（上記バグ修正 + HANDOVER）。GUI 確認後に commit 推奨  
- **push:** `git push origin main`（Git Credential Manager / GitHub Desktop）  


### ポータブル（この PC 上）

```
C:\Users\user\JpoProducer\dist\JpoProducer-portable-2026-07-22\
C:\Users\user\JpoProducer\dist\JpoProducer-portable-2026-07-22.zip   (~130 MB)
```

USB / 別 PC へ zip をコピー → 展開 → `jpo.exe`  

---

## エージェント向け（AGENTS.md 要約）

1. 非自明な変更前にこの HANDOVER を読む  
2. スキル `jpo-producer` + invariants  
3. 生成・pitch 変更後は `cargo test`  
4. Tab 隔離: ノート編集は Tab3 のみ  
5. NoteId ユニーク必須  
6. 仕様変更は先に SPEC  
