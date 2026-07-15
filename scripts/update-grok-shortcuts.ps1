# Refresh Desktop + taskbar Grok Build shortcuts for WSL2 + JpoProducer,
# and ensure Windows Terminal profiles launch Linux grok correctly.
# Usage: pwsh -File scripts/update-grok-shortcuts.ps1

$ErrorActionPreference = "Stop"

$WtExe = "$env:LocalAppData\Microsoft\WindowsApps\wt.exe"
if (-not (Test-Path $WtExe)) {
    $WtExe = "wt.exe"
}

$Icon = "$env:USERPROFILE\.grok\icons\grok-blackhole.ico"
$ProfileNew = "Grok Build (JpoProducer)"
$ProfileResume = "Grok Build (resume)"

# IMPORTANT: `wsl ... -- grok` does NOT load ~/.bashrc, so bare `grok` fails
# with "command not found". Use bash -lc + absolute path under $HOME/.grok/bin.
$CmdNew = "wsl.exe -d Ubuntu --cd ~/JpoProducer -- bash -lc 'exec `$HOME/.grok/bin/grok'"
$CmdResume = "wsl.exe -d Ubuntu --cd ~/JpoProducer -- bash -lc 'exec `$HOME/.grok/bin/grok -c'"

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

function Ensure-WtProfiles {
    $settingsCandidates = @(
        "$env:LOCALAPPDATA\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json",
        "$env:LOCALAPPDATA\Packages\Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe\LocalState\settings.json",
        "$env:LOCALAPPDATA\Microsoft\Windows Terminal\settings.json"
    )
    $settingsPath = $settingsCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $settingsPath) {
        Write-Warning "Windows Terminal settings.json not found; skip profile fix."
        return
    }

    $raw = Get-Content -LiteralPath $settingsPath -Raw -Encoding UTF8
    $json = $raw | ConvertFrom-Json

    # ConvertFrom-Json may not preserve order; we only patch known profiles.
    $wanted = @{
        $ProfileNew    = $CmdNew
        $ProfileResume = $CmdResume
    }

    $list = @($json.profiles.list)
    $byName = @{}
    foreach ($p in $list) {
        if ($null -ne $p.name) { $byName[$p.name] = $p }
    }

    $guidNew = "{8f3c1a2b-4d5e-6f70-8192-a3b4c5d6e7f8}"
    $guidResume = "{9a4d2b3c-5e6f-7081-92a3-b4c5d6e7f809}"

    function New-ProfileObject($Name, $Guid, $Commandline) {
        [pscustomobject]@{
            name              = $Name
            guid              = $Guid
            hidden            = $false
            commandline       = $Commandline
            startingDirectory = "%USERPROFILE%"
            tabTitle          = "Grok Build"
            icon              = $Icon
        }
    }

    $changed = $false
    if ($byName.ContainsKey($ProfileNew)) {
        if ($byName[$ProfileNew].commandline -ne $CmdNew) {
            $byName[$ProfileNew].commandline = $CmdNew
            $changed = $true
            Write-Host "Fixed WT profile commandline: $ProfileNew"
        } else {
            Write-Host "WT profile OK: $ProfileNew"
        }
    } else {
        $json.profiles.list += (New-ProfileObject $ProfileNew $guidNew $CmdNew)
        $changed = $true
        Write-Host "Added WT profile: $ProfileNew"
    }

    if ($byName.ContainsKey($ProfileResume)) {
        if ($byName[$ProfileResume].commandline -ne $CmdResume) {
            $byName[$ProfileResume].commandline = $CmdResume
            $changed = $true
            Write-Host "Fixed WT profile commandline: $ProfileResume"
        } else {
            Write-Host "WT profile OK: $ProfileResume"
        }
    } else {
        $json.profiles.list += (New-ProfileObject $ProfileResume $guidResume $CmdResume)
        $changed = $true
        Write-Host "Added WT profile: $ProfileResume"
    }

    if ($changed) {
        $stamp = Get-Date -Format "yyyyMMddHHmmss"
        Copy-Item -LiteralPath $settingsPath -Destination "$settingsPath.bak.$stamp" -Force
        # Keep readable JSON
        $json | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $settingsPath -Encoding UTF8
        Write-Host "Wrote: $settingsPath"
    }
}

New-GrokShortcut $targets[0] $ProfileNew
New-GrokShortcut $targets[1] $ProfileResume
New-GrokShortcut $targets[2] $ProfileNew
Ensure-WtProfiles

Write-Host ""
Write-Host "Shortcuts point to Windows Terminal profiles:"
Write-Host "  - $ProfileNew   -> bash -lc exec `$HOME/.grok/bin/grok"
Write-Host "  - $ProfileResume -> bash -lc exec `$HOME/.grok/bin/grok -c"
Write-Host "Double-click Desktop 'Grok Build' to open WSL ~/JpoProducer + Grok."
