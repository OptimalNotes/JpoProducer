# 開発環境（2026-07-10）

## 方針

| 用途 | 場所 | 理由 |
|------|------|------|
| **Grok Build + cargo 編集** | WSL2 Ubuntu `~/JpoProducer` | Linux ツールチェーン・高速 I/O・エージェント向き |
| **配布用 Windows exe** | `C:\Users\user\JpoProducer` | `cargo build --release` + `pack.ps1` |
| **GUI 試聴** | WSLg（`cargo run`）または Windows ネイティブ | WSLg は Win11 でそのまま表示可 |

> リポジトリの正本は **GitHub**。ローカルはクローン2本でも OK（同期は `git pull`）。

---

## WSL2（メイン開発）

### 初回セットアップ（済みならスキップ可）

```powershell
# 1) apt（管理者・パスワード不要）
wsl -d Ubuntu -u root -e bash /mnt/c/Users/user/JpoProducer/scripts/setup-wsl.sh

# 2) rust / clone / grok / test（ユーザー）
wsl -d Ubuntu -e bash /mnt/c/Users/user/JpoProducer/scripts/setup-wsl.sh
```

### 日常

```bash
cd ~/JpoProducer
cargo test
cargo run          # WSLg で GUI
grok               # 初回のみブラウザ認証
```

### Grok Build ショートカット

デスクトップの **Grok Build** / **Grok Build (resume)** は Windows Terminal 経由で:

- `~/JpoProducer` に cd
- `bash -lc` で `$HOME/.grok/bin/grok`（または `grok -c` で前回セッション再開）

> **注意:** `wsl ... -- grok` のままだと非ログインシェルになり `~/.bashrc` の PATH が載らず  
> `grok: command not found`（終了コード 127）になる。絶対パス + `bash -lc` が必要。

再生成（ショートカット + WT プロファイル修正）:

```powershell
pwsh -File C:\Users\user\JpoProducer\scripts\update-grok-shortcuts.ps1
```

---

## Windows（配布・レガシー）

```powershell
cd "C:\Users\user\JpoProducer"
cargo build --release
pwsh -File pack.ps1 -Zip
```

`target/` と `dist/` は git 管理外。削除しても `cargo build` で復元。

---

## クリーンアップ済み（2026-07-10）

- 削除: `target/`（~1.2GB）、`dist/`（旧 portable）、`_tmp_pitch.rs`、`GROKBUILD_HANDOFF.md`
- 残す: `archive/`（凍結 v2 / 旧 Python 参考）、`FluidR3 GM.SF2`（git 外・各自配置）

---

## パス早見表

|  | Windows | WSL2 |
|--|---------|------|
| プロジェクト | `C:\Users\user\JpoProducer` | `~/JpoProducer` |
| Grok 設定 | `%USERPROFILE%\.grok\config.toml` | `~/.grok/config.toml`（Windows からコピー可） |
| Domino 参考 | `Desktop\Domino\` | `/mnt/c/Users/user/OneDrive/Desktop/Domino/` |