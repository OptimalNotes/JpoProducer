# JpoProducer 引継ぎ書

**最終更新:** 2026-07-20  
**仕様の真実:** [`SPEC-v1.md`](SPEC-v1.md)  
**入力フィードバック:** `D:\JPOP改善シート.txt`  
**リポジトリ:** https://github.com/OptimalNotes/JpoProducer  
**ローカル:** `C:\Users\user\JpoProducer\`

---

## 2026-07-20 — 改善シート実装

### Progress
| 項目 | 内容 |
|------|------|
| Len / Grid | **完全分離**。Len 既定 **1 拍**、Grid 既定 1/16 |
| Stamp 貼付 | playhead または **末尾へ**。ループ超過は切り捨て |
| Dense | **廃止** |
| ユーザー stamp | 「現在を保存」→ `jpo_user_stamps.json`（exe 隣）。クリックで貼付 |
| Grok 進行 | Progress から削除 |

### Bed
| 項目 | 内容 |
|------|------|
| 流れ | プリセット選択 → **Simple Bed** |
| 長さ | UI なし（常にループ全体 `0..loop_beats()`） |

### Edit
| 項目 | 内容 |
|------|------|
| ツール | **Q** Select / **W** Draw / **E** Erase |
| Len / Grid | 別コンボ |
| Scale | **Scale** トグルで縦ピッチをスケール吸着 |
| Key 変更 | メロディックノートを半音平行移動（Ch10 除外、Bass 再 clamp） |
| 左パネル | « / » で折りたたみ |
| 下パネル | 「下パネル（Grok / Vel）」で開閉（Grok はここに） |

### Arrange
| 項目 | 内容 |
|------|------|
| ＋ | チェイン末尾の大きな **＋** ブロック + Bank の「→ チェインへ」 |
| Key | Arrange 中はツールバー Key 非表示 |
| Bank | 縦スクロール領域を拡大 |

### テスト
`cargo test` → **38 passed**（`paste_stamp_clips_to_loop_end` 追加）

### 次（残）
- Arrange ドラッグ並べ替え
- Velocity グラフ
- Grok API 直結
- main.rs 分割
- portable zip（依頼時）
