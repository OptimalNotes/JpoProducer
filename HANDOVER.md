# JpoProducer 引継ぎ書

**最終更新:** 2026-07-22  
**仕様の真実:** [`SPEC-v1.md`](SPEC-v1.md)  
**リポジトリ:** https://github.com/OptimalNotes/JpoProducer  
**開発:** WSL `~/JpoProducer` ／ 配布・GUI: Windows `C:\Users\user\JpoProducer\`  
**ポータブル:** `dist/JpoProducer-portable-YYYY-MM-DD/` または `.zip`（`pack.ps1 -Zip`）

---

## 次セッションの入口（ここだけ読めば再開できる）

1. **コードの真実:** GitHub `main` を `git pull`  
2. **ローカル Win で GUI:** `C:\Users\user\JpoProducer` → `cargo run --release` または portable の `jpo.exe`  
3. **SF2:** `FluidR3 GM.SF2` を exe と同じフォルダ（git 外）  
4. **API (S2):** 任意で環境変数 `XAI_API_KEY` または `GROK_API_KEY`  

### いまの完成度スナップショット

| 領域 | 状態 |
|------|------|
| Progress (Tab1) | モデルレス + スタンプ **S2 末尾追記** + 進行クリア。ユーザー確認で良好 |
| Bed (Tab2) | Simple Bed 既存 |
| Edit (Tab3) | Transport 固定、下レーン **Vel \| 音色 \| Grok**、2oct 既定 |
| Grok | **発注デスク**（アンケート → ジョブ）。S1 コピー / S2 API。雑談取込拒否 |
| 音色 | GM カテゴリ表 → `track.patch`（途中 PC なし） |
| Ctrl+C/V | 相対時刻・**コード切り詰めなし**（間隔・長さ保持） |
| テスト | `cargo test` → **47 passed**（作業時点） |

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

## 既知・次にやるとよいこと

| 優先 | 内容 |
|------|------|
| 手確認 | 別 PC で portable zip 起動・SF2・貼り付け・Grok S1 往復 |
| UX | Grok デスクの項目密度（狭ディスプレイ向けさらにスクロール最適化） |
| S2 | 実 API キーでの E2E、モデル名 `grok-3` の妥当性 |
| 任意 | f32 リテラル warning 一掃、`main.rs` 分割 |
| v1.0 | `ACCEPTANCE.md` 再実施、version 1.0.0 タグ |

**Tab1 は凍結寄り。** 触るなら明示的な要望から。

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

- ローカル `main` コミット: `fad37f9` — *feat: Tab3 order desk (Grok S1/S2), timbre lane, paste fidelity, portable pack*  
- **push:** 環境に GitHub 認証が無い場合は手元で `git push origin main`（Win の GitHub Desktop / `gh auth` でも可）  
- 古い stash `pre-sync local WIP 20260722` は origin 取り込み前の残骸。不要なら `git stash drop`  

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
