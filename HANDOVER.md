# JpoProducer 引継ぎ書

**最終更新:** 2026-07-15  
**仕様の真実:** [`SPEC-v1.md`](SPEC-v1.md)  
**リポジトリ:** https://github.com/OptimalNotes/JpoProducer  
**開発環境:** [`ENV.md`](ENV.md) — **WSL2 `~/JpoProducer` がメイン**、Windows は配布用  
**ローカル (Win):** `C:\Users\user\JpoProducer\`

---

## プロダクト要約（SPEC）

1. **密な J-Pop コード進行**（細かい切り替え・シンコ）  
2. **単純伴奏ベッド**（アレンジしない）  
3. **普通の MIDI 編集**  
4. **4/8/16 ループを繋いで骨格**  
5. **Grok に進行を渡して MIDI パート追加**

**現行タブ:** **1 Progress / 2 Bed / 3 Edit / 4 Arrange**

---

## 受け入れ ❌ 修正（2026-07-15 再実施待ち）

| ID | 症状 | 修正 |
|----|------|------|
| **1.1** | 短いブロックが重複 | enforce の snap-down 禁止 + place が gap で dur 制限 |
| **1.3** | ◆2つ目無音、Clear→Bed で両方無音 | sync wrap + melodic fill 位相 + refill 一括 |
| **3.2** | ペーストが playhead に乗らない | `snap_playhead`（1/16）を Len から分離 |
| **4.3** | 2ループ目 1/4 重なり・つなぎシンコ感 | `clip_note_to_loop` で export/再生 |
| UX | 先頭へ戻したい | ツールバー **`|◀`** |

**tests:** 33 passed（dense place / dual sync / clip / paste playhead を追加）

詳細ログ: [`tests/golden/ACCEPTANCE.md`](tests/golden/ACCEPTANCE.md)

### 次

1. **ユーザー再受け入れ**（上記 ID を中心に）  
2. まだ ❌ なら最小修正のみ  
3. v1.0.0 タグ + pack  

### 残ギャップ（DoD 外・任意）

| ID | 内容 |
|----|------|
| G-P2b | Piano01 Gate 短縮（assets） |
| G-UX | Sketch 統合、つなぎ目からの再生選択 |
| G-struct | main.rs 分割 |

---

## ピッチマッピング

| トラック | 方式 | clamp |
|---------|------|-------|
| Bass Ch3 | `bass_pitch_from_pattern` | 28–51 |
| Piano Ch2 | `piano_pitch_from_pattern` | 36–96 |
| Drum Ch10 | 転調なし | — |

---

## 検証

```powershell
cd C:\Users\user\JpoProducer
cargo test
cargo run
```

手動: [`tests/golden/ACCEPTANCE.md`](tests/golden/ACCEPTANCE.md)

---

## 次セッション

1. ユーザー: ACCEPTANCE 記入  
2. ❌ のみ修正  
3. （任意）Piano01 Gate、Sketch 統合、main 分割  
4. `1.0.0` + `pack.ps1`

## 参考

| パス | 内容 |
|------|------|
| `SPEC-v1.md` | 仕様の真実 |
| `src/main.rs` | 本体（Simple Bed / stamps / Grok context） |
| `assets/patterns/` | ベッド用パターン |
| `archive/jpo-v2/` | 凍結・再起動しない |
| Domino Desktop | テンプレ手編集のみ |
