# Removes admin mode: stops it, deletes the startup task and the Program Files folder.
# Also cleans up an admin mode installed before the rename to EzDesk ("Desktop Switcher").
# Usage:  powershell -ExecutionPolicy Bypass -File .\uninstall.ps1   (or double-click "Uninstall admin mode.cmd")
# Simple mode needs no uninstaller: untick "Start with Windows" in the tray menu, Exit, delete the folder.
param([string]$InstallDir = "$env:ProgramFiles\EzDesk")
$ErrorActionPreference = 'Stop'

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host 'Asking for admin rights (UAC prompt)...'
    $p = Start-Process powershell.exe -Verb RunAs -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`" -InstallDir `"$InstallDir`"" -Wait -PassThru
    exit $p.ExitCode
}

foreach ($name in 'EzDesk', 'Desktop Switcher') {
    if (Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue) {
        Stop-ScheduledTask -TaskName $name
        Unregister-ScheduledTask -TaskName $name -Confirm:$false
    }
}
$dirs = @($InstallDir, "$env:ProgramFiles\DesktopSwitcher")
Get-Process EzDesk, DesktopSwitcher, AutoHotkey64 -ErrorAction SilentlyContinue |
    Where-Object { $p = $_.Path; $dirs | Where-Object { $p -like "$_\*" } } | Stop-Process -Force
Start-Sleep -Milliseconds 800
foreach ($d in $dirs) { if (Test-Path $d) { Remove-Item $d -Recurse -Force } }
