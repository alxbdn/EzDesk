# Removes admin mode: stops it, deletes the startup task and the Program Files folder.
# Usage:  powershell -ExecutionPolicy Bypass -File .\uninstall.ps1   (or double-click "Uninstall admin mode.cmd")
# Simple mode needs no uninstaller: untick "Start with Windows" in the tray menu, Exit, delete the folder.
param([string]$InstallDir = "$env:ProgramFiles\DesktopSwitcher")
$ErrorActionPreference = 'Stop'

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host 'Asking for admin rights (UAC prompt)...'
    $p = Start-Process powershell.exe -Verb RunAs -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`" -InstallDir `"$InstallDir`"" -Wait -PassThru
    exit $p.ExitCode
}

$task = Get-ScheduledTask -TaskName 'Desktop Switcher' -ErrorAction SilentlyContinue
if ($task) {
    Stop-ScheduledTask -TaskName 'Desktop Switcher'
    Unregister-ScheduledTask -TaskName 'Desktop Switcher' -Confirm:$false
}
Get-Process DesktopSwitcher, AutoHotkey64 -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$InstallDir\*" } | Stop-Process -Force
Start-Sleep -Milliseconds 800
if (Test-Path $InstallDir) { Remove-Item $InstallDir -Recurse -Force }
