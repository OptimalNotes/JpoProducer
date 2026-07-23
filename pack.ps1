# JpoProducer portable pack — run on the dev PC, copy the zip/folder to another Windows PC.
# Usage:  pwsh -File pack.ps1
#         pwsh -File pack.ps1 -Zip

param(
    [switch]$Zip
)

$ErrorActionPreference = "Stop"
$Root = $PSScriptRoot
$Date = Get-Date -Format "yyyy-MM-dd"
$OutName = "JpoProducer-portable-$Date"
$OutDir = Join-Path $Root "dist\$OutName"

Write-Host "==> Building release..."
Push-Location $Root
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build --release failed" }
} finally {
    Pop-Location
}

$ExeSrc = Join-Path $Root "target\release\jpo.exe"
if (-not (Test-Path $ExeSrc)) { throw "Missing $ExeSrc" }

# SF2: project root (same folder as Cargo.toml)
$Sf2Src = Join-Path $Root "FluidR3 GM.SF2"
if (-not (Test-Path $Sf2Src)) { throw "FluidR3 GM.SF2 not found (place it in JpoProducer/)" }

if (Test-Path $OutDir) { Remove-Item $OutDir -Recurse -Force }
New-Item -ItemType Directory -Path $OutDir | Out-Null

Copy-Item $ExeSrc (Join-Path $OutDir "jpo.exe")
Copy-Item $Sf2Src (Join-Path $OutDir "FluidR3 GM.SF2")

$PatternsSrc = Join-Path $Root "assets\patterns"
if (Test-Path $PatternsSrc) {
    $PatternsDst = Join-Path $OutDir "patterns"
    New-Item -ItemType Directory -Path $PatternsDst -Force | Out-Null
    Copy-Item (Join-Path $PatternsSrc "*.mid") $PatternsDst -Force
}

# Chord stamps next to exe (user can add/edit/delete *.jpostamp here)
$StampsSrc = Join-Path $Root "assets\stamps"
if (Test-Path $StampsSrc) {
    $StampsDst = Join-Path $OutDir "stamps"
    New-Item -ItemType Directory -Path $StampsDst -Force | Out-Null
    Copy-Item (Join-Path $StampsSrc "*.jpostamp") $StampsDst -Force -ErrorAction SilentlyContinue
    Copy-Item (Join-Path $StampsSrc "README.txt") $StampsDst -Force -ErrorAction SilentlyContinue
}

@'
JpoProducer — portable pack (SPEC-v2 / 2026-07)
===============================================

【起動】
1. この zip を USB / クラウドへコピー → 別 PC で展開
2. 展開したフォルダで jpo.exe をダブルクリック
   ※ jpo.exe と FluidR3 GM.SF2 は必ず同じフォルダのまま
3. この PC に Rust / cargo は不要

【起動しないとき】
- Windows 10/11 64-bit
- Microsoft Visual C++ Redistributable (2015-2022 x64)
  https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist

【タブ】
1 Progress … コード進行（スタンプ追記・名前付き保存・進行クリア）
2 Bed      … Piano/Bass/Drum 単純伴奏（Simple Bed）
3 Edit     … ピアノロール（複数選択してまとめて移動可）
4 Arrange  … ループ並べ

【Edit】
- Q / W / E = Select / Draw / Erase
- Ctrl+C/V = ノートコピー（相対時刻で貼る）
- 下レーン: Vel / 音色 / Grok 発注デスク
  S1: ジョブをコピー → 外の Grok に貼る
  S2: 環境変数 XAI_API_KEY または GROK_API_KEY があれば API
- Space = 再生

【フォルダの中身】
- jpo.exe
- FluidR3 GM.SF2   … 音源（削除しない）
- stamps\          … 進行スタンプ（自分で保存したのもここに増える）
- patterns\        … Bed 用パターン MIDI
- START.txt        … この説明

【スケッチの持ち運び】
- アプリ内 Tools → Save で .jpo を保存
- .jpo だけ別 PC にコピーして Open すれば続き可

開発: https://github.com/OptimalNotes/JpoProducer
'@ | Set-Content -Path (Join-Path $OutDir "START.txt") -Encoding UTF8

Write-Host "==> Packed to: $OutDir"
Write-Host "    jpo.exe"
Write-Host "    FluidR3 GM.SF2"
Write-Host "    START.txt"
if (Test-Path (Join-Path $OutDir "patterns")) {
    Write-Host "    patterns\ (*.mid)"
}
if (Test-Path (Join-Path $OutDir "stamps")) {
    Write-Host "    stamps\ (*.jpostamp chord presets)"
}

if ($Zip) {
    $ZipPath = "$OutDir.zip"
    if (Test-Path $ZipPath) { Remove-Item $ZipPath -Force }
    Compress-Archive -Path $OutDir -DestinationPath $ZipPath
    Write-Host "==> Zip: $ZipPath"
}

Write-Host "Done. Copy the folder (or zip) to the other PC."