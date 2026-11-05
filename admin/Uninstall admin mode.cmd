@echo off
rem Removes admin mode (startup task + Program Files folder).
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0uninstall.ps1"
pause
