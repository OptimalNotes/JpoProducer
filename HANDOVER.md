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

**ターゲット UI:** Progress / Sketch / Arrange（3WS）。  
**現行:** 4 タブラベル = **1 Progress / 2 Bed / 3 Edit / 4 Arrange**（入力隔離は維持）。

---

## 2026-07-15 実装（SPEC 初回スライス）

| 項目 | 内容 | 状態 |
|------|------|------|
| タブラベル | Progress / Bed / Edit / Arrange | done |
| P1 グリッド | bar/beat/half/16th 視認、短ブロック小フォント | done |
| P1 既定 Len | **1/16 (0.25 beat)** | done |
| P1 スタンプ | 王道1bar / 王道½bar / Dense demo | done |
| P1 前ノリ | −1/16 −1/8 +1/16 ナッジ | done |
| P2 Simple Bed | 主ボタン + Preview + **Clear Bed（範囲のみ）** | done |
| P5 Grok Parts | 進行プロンプト / **part context（コード表付き）** / MIDI import | done |
| Edit に Grok | MIDI part モードを Edit でも表示 | done |
| tests | 28 passed | done |

### まだ残るギャップ

| ID | 柱 | 内容 | 優先 |
|----|-----|------|------|
| G-accept | 全体 | ユーザーが `ACCEPTANCE.md` を1周 | **次** |
| G-P2b | P2 | Piano01 Gate 短縮（Domino assets） | 耳で必要なら |
| G-UX | — | Tab2+3 → Sketch 統合 | v1.0 任意 |
| G-struct | — | `main.rs` 分割 | 推奨・DoD 外 |
| G-P3 | P3 | 受け入れ ❌ が出たらだけ修正 | 受け入れ後 |

生成 cleanup（NoteId / Bass 帯域 / Piano quality / same-pitch overlap / シンコ窓）は **回帰済み・維持**。

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
