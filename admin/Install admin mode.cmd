@echo off
rem Admin mode: installs to Program Files and starts with Windows with admin rights.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1"
pause
