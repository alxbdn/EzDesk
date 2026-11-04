# Installs Desktop Switcher into Program Files and starts it at logon with admin rights.
# Usage (from the repo folder):  powershell -ExecutionPolicy Bypass -File .\install.ps1
# Safe to run again: it updates an existing install in place.
param(
    [string]$InstallDir = "$env:ProgramFiles\DesktopSwitcher",
    [string]$User       = "$env:USERDOMAIN\$env:USERNAME",   # captured before elevation, so the task is yours
    [double]$Outline    = 1,
    [string]$Log        = ""
)
$ErrorActionPreference = 'Stop'

$DllUrl    = 'https://github.com/Ciantic/VirtualDesktopAccessor/releases/download/2024-12-16-windows11/VirtualDesktopAccessor.dll'
$DllSha256 = '8740C572A1C000E3B87FFEB1E4C397EAE9AF3BD4A2ABDC3BCFFACAB4493F8FF5'
$TaskName  = 'Desktop Switcher'

# --- Re-launch elevated if needed; show the elevated run's output here when it finishes ----------
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    $Log = Join-Path $env:TEMP 'desktop-switcher-install.log'
    Remove-Item $Log -ErrorAction SilentlyContinue
    $argList = "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`" -InstallDir `"$InstallDir`" -User `"$User`" -Outline $Outline -Log `"$Log`""
    Write-Host 'Asking for admin rights (UAC prompt)...'
    $p = Start-Process powershell.exe -Verb RunAs -ArgumentList $argList -Wait -PassThru
    if (Test-Path $Log) { Get-Content $Log }
    exit $p.ExitCode
}
function Say($msg) { Write-Host $msg; if ($Log) { Add-Content -Path $Log -Value $msg } }

try {
    $src = $PSScriptRoot

    # --- AutoHotkey v2 ---------------------------------------------------------------------------
    # The elevated run may be a different admin account, so look up the *target* user's profile.
    $sid = (New-Object Security.Principal.NTAccount $User).Translate([Security.Principal.SecurityIdentifier]).Value
    $userProfile = (Get-CimInstance Win32_UserProfile | Where-Object SID -eq $sid).LocalPath
    $candidates = @(
        "$userProfile\AppData\Local\Programs\AutoHotkey\v2\AutoHotkey64.exe",
        "$env:LOCALAPPDATA\Programs\AutoHotkey\v2\AutoHotkey64.exe",
        "$env:ProgramFiles\AutoHotkey\v2\AutoHotkey64.exe",
        "$InstallDir\AutoHotkey64.exe"
    )
    $ahk = $candidates | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    if (-not $ahk) {
        Say 'AutoHotkey v2 not found, installing it with winget...'
        winget install --id AutoHotkey.AutoHotkey --exact --source winget
        $ahk = $candidates | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
        if (-not $ahk) { throw 'AutoHotkey v2 install failed. Install it from https://www.autohotkey.com and run this again.' }
    }
    Say "AutoHotkey: $ahk"

    # --- Stop a running instance so its files can be replaced ------------------------------------
    if (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue) { Stop-ScheduledTask -TaskName $TaskName }
    Get-Process AutoHotkey64 -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$InstallDir\*" } | Stop-Process -Force
    Start-Sleep -Milliseconds 800

    # --- Files -----------------------------------------------------------------------------------
    New-Item -ItemType Directory -Force "$InstallDir\icons" | Out-Null
    Copy-Item "$src\DesktopSwitcher.ahk", "$src\make-icons.ps1" $InstallDir -Force
    if ($ahk -ne "$InstallDir\AutoHotkey64.exe") { Copy-Item $ahk "$InstallDir\AutoHotkey64.exe" -Force }

    $dll = "$InstallDir\VirtualDesktopAccessor.dll"
    if (-not (Test-Path $dll) -or (Get-FileHash $dll).Hash -ne $DllSha256) {
        Say 'Downloading VirtualDesktopAccessor.dll...'
        $tmp = Join-Path $env:TEMP 'VirtualDesktopAccessor.dll.download'
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -UseBasicParsing $DllUrl -OutFile $tmp
        if ((Get-FileHash $tmp).Hash -ne $DllSha256) { Remove-Item $tmp; throw 'Downloaded DLL does not match the expected SHA256. Not installing it.' }
        Move-Item $tmp $dll -Force
    }
    Say 'DLL: verified'

    & "$InstallDir\make-icons.ps1" -OutDir "$InstallDir\icons" -Outline $Outline | Out-Null
    Say 'Icons: generated'

    # --- Lock the folder: Administrators own it, everyone else read/execute ----------------------
    icacls "$InstallDir" /setowner '*S-1-5-32-544' /T /C /Q | Out-Null
    icacls "$InstallDir" /reset /T /C /Q | Out-Null
    Say 'Permissions: admin-only write'

    # --- Startup task ----------------------------------------------------------------------------
    $action    = New-ScheduledTaskAction -Execute "$InstallDir\AutoHotkey64.exe" -Argument "`"$InstallDir\DesktopSwitcher.ahk`"" -WorkingDirectory $InstallDir
    $trigger   = New-ScheduledTaskTrigger -AtLogOn -User $User
    $trigger.Delay = 'PT5S'                                          # let Explorer start first
    $principal = New-ScheduledTaskPrincipal -UserId $User -LogonType Interactive -RunLevel Highest
    $settings  = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
                    -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew   # no 72h kill
    Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
    Start-ScheduledTask -TaskName $TaskName
    Say "Startup task '$TaskName': registered for $User and started"
    Say "Done. Installed to $InstallDir"
} catch {
    Say "ERROR: $($_.Exception.Message)"
    exit 1
}
