# JpoProducer 引継ぎ書

**最終更新:** 2026-07-02  
**方針:** v2 凍結 → **v1 を 4タブ方式に再設計（v0.3.0）**  
**リポジトリルート:** `C:\Users\user\JpoProducer\`（旧 `jpo/` サブフォルダは廃止）

---

## 現行アーキテクチャ（4タブ）

| Tab | 役割 | 入力 |
|-----|------|------|
| **1 Chord** | 細かいコード割り（Len デフォルト **1/8**、**1/16** 選択可） | タイムライン + Chord Strip |
| **2 Generate** | Piano/Bass/Drum 生成（v1 算法、F2 で修正予定） | ボタンのみ（タイムラインは閲覧） |
| **3 Edit** | ピアノロール編集、Grok MIDI import | ロールのみ（Ctrl+C/V はこのタブだけ） |
| **4 Arrange** | Loop Bank + 通しプレビュー + export | 並べ替え UI |

**全タブ共通:** SF2 再生（Space）、Loop 4/8/16、再生ヘッド

---

## Grok 連携（両方選べる）

下部パネル **Grok import**:
- **Natural language** — Grok 返答を貼り付け → `Apply at playhead`（`C | Am | F | G` / `I-vi-IV-V` 等）
- **MIDI file** — `.mid` を選択 → Tab3 の選択トラックへ import（Tab3 下部にも Import ボタン）

---

## ビルド

```powershell
cd "C:\Users\user\JpoProducer"
cargo run    # 人間が GUI 確認
cargo build
```

---

## フォルダ

| パス | 内容 |
|------|------|
| `src/main.rs` | **現行アプリ**（編集はここだけ） |
| `assets/patterns/` | ジェネレータ用パターン MIDI |
| `archive/python/` | 旧 Python 版（メンテなし） |
| `archive/jpo-v2/` | 凍結 v2（EditEngine 参考。Tab3 取り込みは F3） |
| `dist/` | `pack.ps1` の出力（git 管理外） |
| `FluidR3 GM.SF2` | ローカル配置のみ（git 管理外） |

---

## 凍結

- [`archive/jpo-v2/`](archive/jpo-v2/) — M1 のみ。Tab3 で EditEngine 再利用は将来検討（F3）

---

## 次フェーズ

- **F1:** 1/8 グリッドの J-Pop プリセット進行
- **F2:** ジェネレータ算法（コード追従シンコペ、1-pass）
- **F3:** Edit 安定化（v2 NoteId 編集の取り込み）
- **F4:** Arrange 仕上げ

---

## F0 受け入れチェック

- [ ] 4タブ切替ができる
- [ ] **どのタブでも** Space で SF2 再生
- [ ] Tab1 でコード配置、Tab2 で Generate、Tab3 でノート編集が**同時に干渉しない**
- [ ] Grok NL 貼り付けでコードブロックが置ける
- [ ] Grok MIDI import で Tab3 にノートが入る