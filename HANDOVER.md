# JpoProducer 引継ぎ書

**最終更新:** 2026-07-15  
**仕様の真実:** [`SPEC-v1.md`](SPEC-v1.md)（必ず先に読む）  
**リポジトリ:** https://github.com/OptimalNotes/JpoProducer  
**開発環境:** [`ENV.md`](ENV.md) — **WSL2 `~/JpoProducer` がメイン**、Windows は配布用  
**ローカル (Win):** `C:\Users\user\JpoProducer\`

> 実装前に SPEC の五本柱と DoD を確認。コード内は `src/main.rs`。  
> **完成の定義は SPEC §7。** このファイルは「いまの実装ギャップ」だけを追う。

---

## プロダクト要約（SPEC より）

1. **密な J-Pop コード進行**（細かい切り替え・シンコ）  
2. **単純伴奏ベッド**（アレンジしない）  
3. **普通の MIDI 編集**  
4. **4/8/16 ループを繋いで骨格**  
5. **Grok に進行を渡して MIDI パート追加**

**ターゲット UI:** 3 ワークスペース（Progress / Sketch / Arrange）。  
**現行コード:** 4 タブ（Chord / Generate / Edit / Arrange）— 応急の入力隔離。Ship は柱が満たされれば 4 タブのままで可。

---

## 現行 UI マップ

| 現行タブ | SPEC 上の位置 | 状態 |
|----------|---------------|------|
| 1 Chord | Progress | 基本操作あり。密グリッド・スタンプは未強化 |
| 2 Generate | Sketch の Bed | 生成・Preview あり。Simple Bed 主ボタン化は未 |
| 3 Edit | Sketch のロール | Select/Draw/Erase・コピペ・playhead あり |
| 4 Arrange | Arrange | Bank・スロット・通し export あり |
| 下部 Grok | Grok Parts | NL 進行 + MIDI import。P5 としてはまだ弱い |

**全タブ共通:** SF2 再生（Space）、Loop 4/8/16、再生ヘッド

---

## 実装ギャップ（SPEC DoD との差）

| ID | 柱 | 内容 | 優先 |
|----|-----|------|------|
| G-P1a | P1 | Progress の 16 分密グリッド視認・短ブロックラベル | 高 |
| G-P1b | P1 | J-Pop 進行スタンプ（複数ブロック一発） | 中 |
| G-P2a | P2 | UI 上「Simple Bed」を主操作に（中身は現行 Generate で可） | 中 |
| G-P2b | P2 | Piano テンプレ Gate のべったり（assets。same-pitch cleanup は済） | 低〜中 |
| G-P3a | P3 | Undo の手触り・Gate/Vel 一貫性（受け入れで確認） | 受け入れ後 |
| G-P5a | P5 | Grok context にコードタイムラインを厚く・Parts ドック格上げ | 高 |
| G-UX | — | Tab2+3 → Sketch 統合（任意・Ship 後でも可） | 任意 |
| G-struct | — | `main.rs` 分割（挙動ゼロ） | 推奨・DoD 外 |

### 生成まわり（技術・回帰済み中心）

| 項目 | 状態 |
|------|------|
| NoteId ユニーク（選択ハイライト） | **done** |
| Bass E1–D#2 register map | **done**（テストあり） |
| Piano quality map（Em 等） | **done** |
| same-pitch 時間 overlap cleanup | **done** |
| シンコ適応窓 | **done**（手動 OK 確認済） |
| 異 pitch 和音の同時発音 | **仕様**（バグにしない） |
| テンプレ Gate 短縮 | **assets 待ち** |

---

## ピッチマッピング（現行）

| トラック | 方式 | clamp |
|---------|------|-------|
| Bass Ch3 | `bass_pitch_from_pattern` | 28–51 |
| Piano Ch2 | `piano_pitch_from_pattern` | 36–96 |
| Drum Ch10 | 転調なし | — |

BPM: beat-grid の start/dur は BPM 非依存。

---

## Golden / 受け入れ

```powershell
cd C:\Users\user\JpoProducer
cargo test
```

| パス | 内容 |
|------|------|
| `tests/golden/ACCEPTANCE.md` | **v1.0 手動ゲート**（五本柱） |
| `tests/golden/case01/` | MidiTest 回帰 |
| `tests/golden/case02/` | sync 短ブロック |
| `tests/golden/case03/` | 2026-07-10 報告時点の broken 保管 |

---

## 次にやること（SPEC §8 順）

1. **受け入れ 1 周** — `ACCEPTANCE.md` をユーザーが記入  
2. **❌ だけ修正** — 新機能禁止  
3. **P1 密グリッド / P5 Grok context** を優先強化  
4. Simple Bed ラベル化、テンプレ Gate  
5. （推奨）main.rs 分割 → `1.0.0` + `pack.ps1`

---

## ビルド

```powershell
cd "C:\Users\user\JpoProducer"
cargo run
cargo test
cargo build --release
```

---

## 参考パス

| パス | 内容 |
|------|------|
| `SPEC-v1.md` | **仕様の真実** |
| `src/main.rs` | 現行アプリ |
| `assets/patterns/` | ベッド用パターン MIDI |
| `archive/jpo-v2/` | 凍結 v2（再起動しない） |
| `C:\Users\user\OneDrive\Desktop\Domino\` | UX/テンプレ手編集 |

## Grok 連携（エージェント）

スキル `/jpo-producer` + `AGENTS.md`。変更時は SPEC → テスト → HANDOVER。
