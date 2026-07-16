# JpoProducer 引継ぎ書

**最終更新:** 2026-07-16  
**仕様の真実:** [`SPEC-v1.md`](SPEC-v1.md)  
**リポジトリ:** https://github.com/OptimalNotes/JpoProducer  
**開発環境:** [`ENV.md`](ENV.md) — **WSL2 `~/JpoProducer` がメイン**、Windows は配布用  
**ローカル (Win):** `C:\Users\user\JpoProducer\`

---

## プロダクト要約（SPEC）

1. **密な J-Pop コード進行**  2. **単純伴奏ベッド**  3. **MIDI 編集**  
4. **4/8/16 ループ接続**  5. **Grok パート**

**現行タブ:** **1 Progress / 2 Bed / 3 Edit / 4 Arrange**

---

## UI 洗練（2026-07-16）

| 項目 | 内容 |
|------|------|
| 共通 | 下帯を Zoom/Loop のみに圧縮。長い英語 Tip を削減 |
| Progress | 操作をタイムライン直上に集約、タイムライン高さを拡大、Grok 折りたたみ |
| Bed | **Simple Bed** を主ボタンとして上段へ、パターンは副 |
| Edit | トラック列やや狭く、ピアノロール高さ拡大、Grok 折りたたみ |
| **Arrange** | **色付きブロック・チェイン UI**（エフェクトチェイン型）。クリックでそこから再生、右上チップで色変更、再生中はブロック内塗り＋枠ハイライト（縦線中心ではない） |

`ArrangeSlot.color_idx` を追加（旧 .jpo は default 0）。

### 次

1. ユーザーがスクショで UI 確認  
2. 微調整 → 配布 zip / v1.0 は UI 一段落後  

### 残ギャップ（任意）

| ID | 内容 |
|----|------|
| DnD | チェインのドラッグ並べ替え（現状 ← →） |
| G-struct | main.rs 分割 |
| G-P2b | Piano01 Gate |

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
