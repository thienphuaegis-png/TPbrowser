@echo off
cd /d "%~dp0"
if exist "target\release\TPbrowser.exe" copy /y "target\release\TPbrowser.exe" "TPbrowser.exe" >nul 2>&1
start "" "TPbrowser.exe"
