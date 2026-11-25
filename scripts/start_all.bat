@echo off
chcp 65001 >nul
title TPbrowser + MCP Tunnel (All-in-One)

echo.
echo  ╔══════════════════════════════════════════╗
echo  ║     TPbrowser — Khởi động toàn bộ       ║
echo  ║   1. TPbrowser.exe                       ║
echo  ║   2. Cloudflare HTTPS Tunnel             ║
echo  ╚══════════════════════════════════════════╝
echo.

cd /d "%~dp0.."

:: Khởi động TPbrowser (nền)
echo  [1/2] Đang khởi động TPbrowser...
start "" "TPbrowser.exe"
timeout /t 2 /nobreak >nul

:: Khởi động Cloudflare Tunnel
echo  [2/2] Đang khởi động Cloudflare Tunnel...
echo.
set "TP_MCP_DISPLAY_TOKEN=%TP_MCP_TOKEN%"
if "%TP_MCP_DISPLAY_TOKEN%"=="" (
  for /f "usebackq delims=" %%T in (`powershell -NoProfile -Command "if (Test-Path 'config.json') { try { $c = Get-Content 'config.json' -Raw | ConvertFrom-Json; if ($c.mcp_auth_token) { $c.mcp_auth_token } } catch {} }"`) do set "TP_MCP_DISPLAY_TOKEN=%%T"
)
if not "%TP_MCP_DISPLAY_TOKEN%"=="" (
  echo  ► MCP Bearer Token: %TP_MCP_DISPLAY_TOKEN% ◄
  echo  ► Header: Authorization: Bearer %TP_MCP_DISPLAY_TOKEN% ◄
)
echo  ► URL HTTPS sẽ hiện ra dưới đây ◄
echo  ► Copy dòng có "trycloudflare.com" vào Gemini Web ◄
echo.
echo ──────────────────────────────────────────────

cloudflared.exe tunnel --url http://127.0.0.1:17777 2>&1

pause
