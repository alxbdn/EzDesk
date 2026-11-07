# Admin mode: installs EzDesk into Program Files and starts it at logon with admin rights,
# so its shortcuts also work over admin windows (Task Manager, installers, admin terminals).
# Usage:  powershell -ExecutionPolicy Bypass -File .\install.ps1     (or double-click "Install admin mode.cmd")
# Works from the release zip (uses the bundled files) or from a clone of the repo (downloads what's missing).
# Safe to run again: it updates an existing install in place, and replaces a pre-rename
# "Desktop Switcher" install (its task, folder and startup shortcut).
param(
    [string]$InstallDir = "$env:ProgramFiles\EzDesk",
    [string]$User       = "$env:USERDOMAIN\$env:USERNAME",   # captured before elevation, so the task is yours
    [double]$Outline    = 0,
    [string]$Log        = ""
)
$ErrorActionPreference = 'Stop'

# VirtualDesktopAccessor ships separate builds for Windows 11 and Windows 10; the script loads the right one.
$Dlls = @(
    @{ Name = 'VirtualDesktopAccessor.dll'
       Url  = 'https://github.com/Ciantic/VirtualDesktopAccessor/releases/download/2024-12-16-windows11/VirtualDesktopAccessor.dll'
       Sha  = '8740C572A1C000E3B87FFEB1E4C397EAE9AF3BD4A2ABDC3BCFFACAB4493F8FF5' },
    @{ Name = 'VirtualDesktopAccessor-win10.dll'
       Url  = 'https://github.com/Ciantic/VirtualDesktopAccessor/releases/download/2019-windows10/VirtualDesktopAccessor.dll'
       Sha  = '145431BC2090281838DE31824BC829DD35688B13488F4850F4BC5F25F5E599DA' }
)
$TaskName  = 'EzDesk'
# Before the rename to EzDesk
$OldTaskName   = 'Desktop Switcher'
$OldInstallDir = "$env:ProgramFiles\DesktopSwitcher"

# --- Re-launch elevated if needed; show the elevated run's output here when it finishes ----------
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    $Log = Join-Path $env:TEMP 'ezdesk-install.log'
    Remove-Item $Log -ErrorAction SilentlyContinue
    $argList = "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`" -InstallDir `"$InstallDir`" -User `"$User`" -Outline $Outline -Log `"$Log`""
    Write-Host 'Asking for admin rights (UAC prompt)...'
    $p = Start-Process powershell.exe -Verb RunAs -ArgumentList $argList -Wait -PassThru
    if (Test-Path $Log) { Get-Content $Log }
    exit $p.ExitCode
}
function Say($msg) { Write-Host $msg; if ($Log) { Add-Content -Path $Log -Value $msg } }

try {
    # Release zip: this script sits in admin\ next to the app files. Repo clone: same folder.
    $src = if (Test-Path "$PSScriptRoot\EzDesk.ahk") { $PSScriptRoot } else { Split-Path $PSScriptRoot }
    if (-not (Test-Path "$src\EzDesk.ahk")) { throw "EzDesk.ahk not found next to $PSScriptRoot" }

    # The elevated run may be a different admin account, so look up the *target* user's profile.
    $sid = (New-Object Security.Principal.NTAccount $User).Translate([Security.Principal.SecurityIdentifier]).Value
    $userProfile = (Get-CimInstance Win32_UserProfile | Where-Object SID -eq $sid).LocalPath

    # --- Runtime: bundled EzDesk.exe (renamed AutoHotkey v2), else an AutoHotkey install ------------
    $candidates = @(
        "$src\EzDesk.exe",
        "$userProfile\AppData\Local\Programs\AutoHotkey\v2\AutoHotkey64.exe",
        "$env:ProgramFiles\AutoHotkey\v2\AutoHotkey64.exe",
        "$InstallDir\EzDesk.exe"
    )
    $exe = $candidates | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    if (-not $exe) {
        Say 'AutoHotkey v2 not found, installing it with winget...'
        winget install --id AutoHotkey.AutoHotkey --exact --source winget
        $exe = $candidates | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
        if (-not $exe) { throw 'AutoHotkey v2 install failed. Install it from https://www.autohotkey.com and run this again.' }
    }
    Say "Runtime: $exe"

    # --- Stop every running copy (admin task or simple mode, either name) so files can be replaced --
    foreach ($t in $TaskName, $OldTaskName) {
        if (Get-ScheduledTask -TaskName $t -ErrorAction SilentlyContinue) { Stop-ScheduledTask -TaskName $t }
    }
    Get-CimInstance Win32_Process -Filter "Name='EzDesk.exe' OR Name='DesktopSwitcher.exe' OR Name='AutoHotkey64.exe'" |
        Where-Object { $_.Name -ne 'AutoHotkey64.exe' -or $_.CommandLine -like '*EzDesk.ahk*' -or $_.CommandLine -like '*DesktopSwitcher.ahk*' } |
        ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    Start-Sleep -Milliseconds 800

    # Simple mode's startup shortcut would start a second copy at logon: remove it (either name).
    $startup = "$userProfile\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup"
    foreach ($lnk in "$startup\EzDesk.lnk", "$startup\Desktop Switcher.lnk") {
        if (Test-Path $lnk) { Remove-Item $lnk -Force; Say "Removed the simple-mode startup shortcut ($(Split-Path $lnk -Leaf))" }
    }

    # Admin mode from before the rename: its task and folder are replaced by the EzDesk ones.
    if (Get-ScheduledTask -TaskName $OldTaskName -ErrorAction SilentlyContinue) {
        Unregister-ScheduledTask -TaskName $OldTaskName -Confirm:$false; Say "Removed the old '$OldTaskName' startup task"
    }
    if ((Test-Path $OldInstallDir) -and ($OldInstallDir -ne $InstallDir) -and (Test-Path "$OldInstallDir\DesktopSwitcher.ahk")) {
        Remove-Item $OldInstallDir -Recurse -Force; Say "Removed the old install folder $OldInstallDir"
    }

    # --- Files -----------------------------------------------------------------------------------
    New-Item -ItemType Directory -Force "$InstallDir\icons" | Out-Null
    Copy-Item "$src\EzDesk.ahk" $InstallDir -Force
    if ($exe -ne "$InstallDir\EzDesk.exe") { Copy-Item $exe "$InstallDir\EzDesk.exe" -Force }
    Remove-Item "$InstallDir\AutoHotkey64.exe", "$InstallDir\install-startup.ps1" -ErrorAction SilentlyContinue   # older layouts

    foreach ($d in $Dlls) {
        $dll = Join-Path $InstallDir $d.Name; $bundled = Join-Path $src $d.Name
        if ((Test-Path $bundled) -and (Get-FileHash $bundled).Hash -eq $d.Sha) {
            Copy-Item $bundled $dll -Force
        } elseif (-not (Test-Path $dll) -or (Get-FileHash $dll).Hash -ne $d.Sha) {
            Say "Downloading $($d.Name)..."
            $tmp = Join-Path $env:TEMP "$($d.Name).download"
            [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
            Invoke-WebRequest -UseBasicParsing $d.Url -OutFile $tmp
            if ((Get-FileHash $tmp).Hash -ne $d.Sha) { Remove-Item $tmp; throw "Downloaded $($d.Name) does not match the expected SHA256. Not installing it." }
            Move-Item $tmp $dll -Force
        }
    }
    Remove-Item "$InstallDir\make-icons.ps1", "$InstallDir\README.md" -ErrorAction SilentlyContinue   # leftovers of the first, manual install
    Say 'DLLs: verified (Windows 11 + Windows 10)'

    if (Test-Path "$src\icons\1.ico") { Copy-Item "$src\icons\*.ico" "$InstallDir\icons" -Force }
    else { & "$src\make-icons.ps1" -OutDir "$InstallDir\icons" -Outline $Outline | Out-Null }
    Say 'Icons: ready'

    # --- Lock the folder: Administrators own it, everyone else read/execute ----------------------
    icacls "$InstallDir" /setowner '*S-1-5-32-544' /T /C /Q | Out-Null
    icacls "$InstallDir" /reset /T /C /Q | Out-Null
    Say 'Permissions: admin-only write'

    # --- Startup task ----------------------------------------------------------------------------
    $action    = New-ScheduledTaskAction -Execute "$InstallDir\EzDesk.exe" -Argument "`"$InstallDir\EzDesk.ahk`" --startup" -WorkingDirectory $InstallDir
    $trigger   = New-ScheduledTaskTrigger -AtLogOn -User $User
    $trigger.Delay = 'PT5S'                                          # let Explorer start first
    $principal = New-ScheduledTaskPrincipal -UserId $User -LogonType Interactive -RunLevel Highest
    $settings  = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
                    -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew   # no 72h kill
    Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
    Start-ScheduledTask -TaskName $TaskName
    Say "Startup task '$TaskName': registered for $User and started"
    Say "Done. Admin mode installed to $InstallDir"
} catch {
    Say "ERROR: $($_.Exception.Message)"
    exit 1
}
