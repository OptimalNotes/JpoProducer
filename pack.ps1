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

# SF2: prefer jpo/ copy, then project root
$Sf2Candidates = @(
    (Join-Path $Root "FluidR3 GM.SF2"),
    (Join-Path $Root "..\FluidR3 GM.SF2")
)
$Sf2Src = $Sf2Candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $Sf2Src) { throw "FluidR3 GM.SF2 not found (place it in jpo/ or JpoProducer/)" }

if (Test-Path $OutDir) { Remove-Item $OutDir -Recurse -Force }
New-Item -ItemType Directory -Path $OutDir | Out-Null

Copy-Item $ExeSrc (Join-Path $OutDir "jpo.exe")
Copy-Item $Sf2Src (Join-Path $OutDir "FluidR3 GM.SF2")

@'
JpoProducer — portable pack
===========================

1. Unzip this folder anywhere (Desktop, USB stick, etc.)
2. Double-click jpo.exe
   - jpo.exe and FluidR3 GM.SF2 must stay in the SAME folder.
3. No Rust / cargo needed on this PC.

If the app does not start:
- Windows 10/11 64-bit required
- Install "Microsoft Visual C++ Redistributable" (2015-2022 x64)
  https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist

Tips:
- Save sketches as .jpo (Tools menu -> Save)
- Play needs SF2: sidebar should show "SF2: found"
'@ | Set-Content -Path (Join-Path $OutDir "START.txt") -Encoding UTF8

Write-Host "==> Packed to: $OutDir"
Write-Host "    jpo.exe"
Write-Host "    FluidR3 GM.SF2"
Write-Host "    START.txt"

if ($Zip) {
    $ZipPath = "$OutDir.zip"
    if (Test-Path $ZipPath) { Remove-Item $ZipPath -Force }
    Compress-Archive -Path $OutDir -DestinationPath $ZipPath
    Write-Host "==> Zip: $ZipPath"
}

Write-Host "Done. Copy the folder (or zip) to the other PC."