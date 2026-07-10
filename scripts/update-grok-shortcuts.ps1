# Refresh Desktop + taskbar Grok Build shortcuts for WSL2 + JpoProducer.
# Usage: pwsh -File scripts/update-grok-shortcuts.ps1

$ErrorActionPreference = "Stop"

$WtExe = "$env:LocalAppData\Microsoft\WindowsApps\wt.exe"
if (-not (Test-Path $WtExe)) {
    $WtExe = "wt.exe"
}

$Icon = "$env:USERPROFILE\.grok\icons\grok-blackhole.ico"
$ProfileNew = "Grok Build (JpoProducer)"
$ProfileResume = "Grok Build (resume)"

$targets = @(
    "$env:USERPROFILE\OneDrive\Desktop\Grok Build.lnk",
    "$env:USERPROFILE\OneDrive\Desktop\Grok Build (resume).lnk",
    "$env:APPDATA\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar\Grok Build.lnk"
)

$Wsh = New-Object -ComObject WScript.Shell

function New-GrokShortcut($Path, $ProfileName) {
    $dir = Split-Path $Path -Parent
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
    $s = $Wsh.CreateShortcut($Path)
    $s.TargetPath = $WtExe
    $s.Arguments = "-w 0 nt -p `"$ProfileName`""
    $s.WorkingDirectory = $env:USERPROFILE
    if (Test-Path $Icon) { $s.IconLocation = "$Icon,0" }
    $s.Description = "Grok Build in WSL ~/JpoProducer ($ProfileName)"
    $s.Save()
    Write-Host "Updated: $Path"
}

New-GrokShortcut $targets[0] $ProfileNew
New-GrokShortcut $targets[1] $ProfileResume
New-GrokShortcut $targets[2] $ProfileNew

Write-Host ""
Write-Host "Shortcuts point to Windows Terminal profiles:"
Write-Host "  - $ProfileNew   -> wsl Ubuntu, cd ~/JpoProducer, grok"
Write-Host "  - $ProfileResume -> wsl Ubuntu, cd ~/JpoProducer, grok -c"