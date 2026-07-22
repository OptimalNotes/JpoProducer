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
JpoProducer — portable pack
===========================

【起動】
1. このフォルダをどこかへ展開（Desktop / USB など）
2. jpo.exe をダブルクリック
   ※ jpo.exe と FluidR3 GM.SF2 は必ず同じフォルダに置く
3. この PC に Rust / cargo は不要

【起動しないとき】
- Windows 10/11 64-bit
- Microsoft Visual C++ Redistributable (2015-2022 x64)
  https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist

【タブ】
1 Progress … コード進行（スタンプは末尾追記・進行クリアあり）
2 Bed      … Piano/Bass/Drum 単純伴奏
3 Edit     … ピアノロール / Vel・音色・Grok 下レーン
4 Arrange  … ループ並べ

【Edit 下レーン】
- Vel  … ベロシティ棒グラフ
- 音色 … GM 音色表（トラック単位。曲中 PC はしない）
- Grok … 発注デスク
  S1: 「ジョブをコピー」→ 外の Grok チャットに貼る
  S2: 環境変数 XAI_API_KEY または GROK_API_KEY があれば API 送信
  結果のノート列を「結果を取込」（雑談のみは拒否）

【その他】
- スケッチ保存: Tools → Save（.jpo）
- stamps\ … 進行スタンプ（Progress で追記）
- patterns\ … Bed 用パターン MIDI（exe から相対で読める配置）
- Space = 再生、Edit の Q/W/E = Select/Draw/Erase

開発リポジトリ: https://github.com/OptimalNotes/JpoProducer
引継ぎ: リポジトリ内 HANDOVER.md
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