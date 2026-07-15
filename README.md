# JpoProducer (Rust + egui)

**公式・唯一サポート対象のバージョン** — このフォルダが Git リポジトリのルートです。

| 文書 | 役割 |
|------|------|
| **[`SPEC-v1.md`](SPEC-v1.md)** | **仕様の真実**（五本柱・DoD・UI 方針） |
| [`HANDOVER.md`](HANDOVER.md) | 実装ギャップ・次の一手 |
| [`ENV.md`](ENV.md) | WSL2 + Grok Build |
| [`tests/golden/ACCEPTANCE.md`](tests/golden/ACCEPTANCE.md) | v1.0 手動受け入れ |

## フォルダ構成（2026-07 整理後）

```text
JpoProducer/                 ← ここが全部。GitHub もこのルート
├── SPEC-v1.md               ← 仕様の真実
├── src/main.rs              ← アプリ本体
├── Cargo.toml
├── assets/patterns/         ← 単純伴奏ベッド用パターン MIDI
├── pack.ps1                 ← ポータブル zip 作成
├── dist/                    ← 配布用ビルド（git 管理外）
├── target/                  ← cargo ビルド成果物（git 管理外）
├── FluidR3 GM.SF2           ← 再生用 SF2（git 管理外・各自配置）
└── archive/
    ├── python/              ← 旧 Python 版（参考のみ）
    └── jpo-v2/              ← 凍結した v2 試作（参考のみ）
```

**いつもの作業ディレクトリ:** WSL `~/JpoProducer`（Grok Build 推奨）／Windows `C:\Users\user\JpoProducer`（配布ビルド用）

単一 `jpo.exe` + 同梱 `FluidR3 GM.SF2` で完結（FluidSynth DLL 不要）。

---

## コンセプト（SPEC-v1）

J-Pop / J-Rock の**爆速スケッチ**。市場の「1 小節 1 コード」ツールが苦手な **細かい切り替え・シンコ**を先に置き、メロが浮かぶ程度の単純伴奏を敷き、普通の MIDI 編集とループ接続で骨格を作り、**Grok に進行を渡してパートを足す**。

### 五本柱

| 柱 | 内容 |
|----|------|
| **P1 密な進行** | 16 分単位の細かいコード、前ノリ・シンコ |
| **P2 単純ベッド** | Piano+Bass+Drum の薄い伴奏（アレンジしない） |
| **P3 MIDI 編集** | 選択・描画・コピペ・Undo・オニオン・SF2 再生 |
| **P4 ループ接続** | 4 / 8 / 16 小節を Bank → Arrange |
| **P5 Grok パート** | 進行コンテキストを渡し、MIDI パーツを import |

### UI 方針

- **ターゲット:** Progress / Sketch / Arrange の 3 ワークスペース（詳細は SPEC）  
- **現行実装:** 入力隔離のための 4 タブ（Chord / Generate / Edit / Arrange）。柱を満たせば v1.0 は 4 タブのまま Ship 可  
- Domino / DAW の置き換えはしない

---

## ビルド & 実行（Windows）

1. Rust 導入（初回のみ）: https://rustup.rs/

2. 開発実行:
   ```powershell
   cd "C:\Users\user\JpoProducer"
   cargo run
   ```

3. リリース:
   ```powershell
   cargo build --release
   ```
   → `target\release\jpo.exe`

4. **SF2 の置き方（必須）**  
   `jpo.exe` と **`FluidR3 GM.SF2` を同じフォルダ**にコピーして実行。  
   開発時は `jpo/` 内の SF2 でも可。見つからない場合は UI に表示される。  
   ※ SF2 は容量の都合で **Git リポジトリには含めていません**（各自で配置）。

5. **別 PC へ持ち運び**
   ```powershell
   pwsh -File pack.ps1 -Zip
   ```
   → `dist/JpoProducer-portable-YYYY-MM-DD.zip`（exe + SF2 + START.txt）

---

## いま動いている機能（実装スナップショット・詳細は SPEC / HANDOVER）

### 編集
- **Pencil / Eraser**、**Len**（1/16〜2拍）、**Snap ON/OFF**
- Ch1 **Chord Timeline**（ブロック塗り・伸縮・移動・パレット）
- Ch2–16 **Piano Roll**（鍵盤ガター・Pitch Zoom/Scroll・マウスホイール縦スクロール）
- **ダブルクリック削除**（ノート & コードブロック）、**Delete キー**、Eraser
- **箱選択**（空きエリアドラッグ）→ 複数選択の土台
- ノート移動はドラッグ開始時に Move / Resize を固定（迷い防止）

### 聴く
- **Play/Stop**: SF2 本番再生 + プレイヘッド
- **トラック M/S/Vol**: Mute / Solo / トラック音量（再生・プレビュー両方）
- **編集プレビュー**: ノート入力・移動（音程変化時）・コード入力・パレット変更で短い SF2 音

### 見る
- **オニオン**: キースケール（薄ピンク）+ コードトーン（薄青、ブロック範囲）、Scale/Chord 別スライダー（滑らかな濃さカーブ）
- トラック名短縮: `Chord` / `Ch2` … / `Drum`、音色名は固定幅で省略表示

### その他
- Key / BPM / Major-Minor（**現状は曲全体1設定 → 将来ループ単位に移行予定**）
- **ジェネレータ**（`assets/patterns/` のキー C MIDI テンプレ）: Piano/Bass/Drum パターン選択、**Syncopation fill**（短コードで 2 拍差し替え）
- **Grok (ideas)** クリップボードプロンプト
- **Export MIDI**（Type 1、Ch1 展開）

---

## ロードマップ

**現行の完成定義・作業順は [`SPEC-v1.md`](SPEC-v1.md) §7–8 のみ。**  
ギャップは [`HANDOVER.md`](HANDOVER.md)。手動ゲートは [`tests/golden/ACCEPTANCE.md`](tests/golden/ACCEPTANCE.md)。

過去の Phase A–D チェックリストは履歴であり、SPEC と矛盾する場合は **SPEC が勝つ**。

**意図的に後回し (v1.1+):** 無限タイムライン、オーディオ録音、フルミキサー、Grok API 直結必須化、Domino 全機能。

---

## 操作クイックリファレンス

### ツールバー
| 項目 | 説明 |
|------|------|
| BPM / Key / Major-Minor | テンポ・キー（将来ループ単位） |
| Pencil / Eraser | 描画 / 削除 |
| Len | 新規ノート・ブロックのデフォルト長 |
| Snap ON/OFF | Len に連動したグリッド吸着 |
| Vol | 再生 & プレビュー音量 |
| Play/Stop | SF2 再生 |

### Chord Timeline（Ch1）
- 空きをドラッグ → ブロック作成（プレビュー音あり）
- ブロック内ドラッグ → 移動 / 右端 → 伸縮
- クリック → パレット（プレビュー音あり）
- **ダブルクリック** → 削除

### Piano Roll（Ch2–16）
- クリック → ノート追加（プレビュー音あり）
- 内側ドラッグ → 移動（音程変化でプレビュー）/ 右端 → 伸縮
- **ダブルクリック** or **Delete** or Eraser → 削除
- 空きドラッグ → 箱選択

### 下部バー
- **Zoom / Scroll**（横）、**Pitch Zoom / Pitch Scroll**（縦）、Fit 8/16
- **Onion** 濃さ
- **Generate range**（拍）→ Generate All / Clear（※箱選択とは別）
- **Gen=Visible** / **Gen=4 bars** で範囲プリセット

### トラックリスト
`Chord` | `Ch2` … `Drum` + パッチ番号 & 短い音色名

---

## 開発メモ

- ソースはほぼ単一ファイル: [`src/main.rs`](src/main.rs)
- `cargo run` 推奨（ビルド後は exe を閉じてから再ビルド — Windows が exe をロックする）
- 権威仕様との差分は **ループ制約・Arrange・ループ単位キー・転調補助テンプレ** が README 側で新しく追加された方針

---

## 哲学（短く）

- **軽い exe + SF2 1 枚**で誰でも即試せる
- **コード進行が常に見える**（オニオン）
- **8小節で「これで行ける」**を最優先
- 細かいニュアンスは生成後・ループ複製後に手で直す
- 完成品は **Arrange → MIDI** で好きな DAW へ

*J-Pop / J-Rock のループパズル用スケッチツール — 爆速で書いて、貯めて、つなぐ。*