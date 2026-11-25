@echo off
setlocal
cd /d "%~dp0"

echo ========================================================
echo   [TPbrowser] Dang bien dich ban Release va khoi chay...
echo ========================================================

REM 1. Tat tien trinh TPbrowser neu dang chay de tranh khoa file
taskkill /F /IM TPbrowser.exe >nul 2>&1

REM 2. Bien dich ban Release toi uu
cargo build --release
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo [LOI] Bien dich that bai! Vui long kiem tra code.
    pause
    exit /b 1
)

REM 3. Copy ban build moi ra thu muc goc
echo.
echo [OK] Bien dich thanh cong! Dang cap nhat TPbrowser.exe...
copy /y "target\release\TPbrowser.exe" "TPbrowser.exe" >nul 2>&1

REM 4. Khoi chay de test ngay
echo [OK] Dang khoi chay TPbrowser...
start "" "TPbrowser.exe"

exit /b 0
