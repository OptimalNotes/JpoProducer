# JpoProducer V2 引継ぎ書

**最終更新:** 2026-06-26  
**マイルストーン:** M1（編集コア + 最小ピアノロール）

---

## 1. 概要

| 項目 | 内容 |
|------|------|
| パス | `C:\Users\user\JpoProducer\jpo\jpo-v2\` |
| v1（凍結） | `C:\Users\user\JpoProducer\jpo\`（`src/main.rs` 単体） |
| GitHub | https://github.com/OptimalNotes/JpoProducer |
| スタック | Rust + egui 0.30 + eframe |

**方針:** v1 へのパッチは打ち止め。V2 は NoteId ベースの編集エンジンを中心に再構築。

---

## 2. クレート構成

```text
jpo-v2/
  crates/jpo_model/   … NoteId, Project, TrackRole, LoopSketch, snap
  crates/jpo_edit/    … Selection, Clipboard, Undo, EditEngine + テスト
  crates/jpo_app/     … egui 最小ピアノロール（M1）
  assets/patterns/  … v1 から継承したパターン MIDI（M3 用）
```

**未実装（今後）:** `jpo_generate`（M3）, `jpo_audio`（M2）

---

## 3. M1 完了内容（2026-06-26）

- `NoteId` による安定選択（インデックス選択を廃止）
- コピー / カット / ペースト / 複製 — ペースト時は **新しい NoteId** を発行
- Shift+クリック複数選択、Shift+ドラッグ箱選択
- Undo / Redo、矢印ナッジ
- ウィンドウタイトル: `JpoProducer v2.0.0 (<git hash>)`
- **単体テスト 17 件**（jpo_edit 15 + jpo_model 2）

---

## 4. ビルド・実行

```powershell
cd "C:\Users\user\JpoProducer\jpo\jpo-v2"
cargo test          # AI が実行
cargo run           # 人間が GUI 確認
```

実行ファイル: `target\debug\jpo.exe`

---

## 5. M1 受け入れチェック（人間検証）

- [ ] Ch4 でクリック配置
- [ ] Shift+クリックで複数選択
- [ ] Ctrl+C → 再生ヘッド位置へ Ctrl+V（重複なし）
- [ ] Ctrl+X / Ctrl+D / Delete / Ctrl+A
- [ ] タイトルバーに `v2.0.0` とコミット hash

---

## 6. 次マイルストーン

| MS | 内容 |
|----|------|
| M2 | Chord timeline + SF2 再生 + オニオン |
| M3 | ジェネレータ再設計（ingest → interval → 1-block-1-apply） |
| M4 | Loop Bank, Arrange, MIDI export |

**M3 生成の前提（ユーザー指摘）:** パターン MIDI は正しい。失敗は「音楽的理解なしの貼り付け」。相対音程・単一パス・sync は上書きのみ。

---

## 7. 開発フロー

```text
AI  → cargo test / cargo build / git commit & push
人間 → cargo run（GUI）
```