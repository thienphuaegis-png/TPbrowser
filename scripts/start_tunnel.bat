@echo off
chcp 65001 >nul
title TPbrowser MCP Tunnel

echo.
echo  ╔══════════════════════════════════════════╗
echo  ║      TPbrowser — Cloudflare Tunnel       ║
echo  ║   http://127.0.0.1:8045 → HTTPS public  ║
echo  ╚══════════════════════════════════════════╝
echo.
echo  Đang khởi động tunnel...
echo  URL HTTPS sẽ hiện ra bên dưới (dòng "trycloudflare.com")
echo  Copy URL đó vào Gemini Web > Kết nối MCP
echo.
echo ──────────────────────────────────────────────

cd /d "%~dp0.."

set "TP_MCP_DISPLAY_TOKEN=%TP_MCP_TOKEN%"
if "%TP_MCP_DISPLAY_TOKEN%"=="" (
  for /f "usebackq delims=" %%T in (`powershell -NoProfile -Command "if (Test-Path 'config.json') { try { $c = Get-Content 'config.json' -Raw | ConvertFrom-Json; if ($c.mcp_auth_token) { $c.mcp_auth_token } } catch {} }"`) do set "TP_MCP_DISPLAY_TOKEN=%%T"
)
if not "%TP_MCP_DISPLAY_TOKEN%"=="" (
  echo  MCP Bearer Token : %TP_MCP_DISPLAY_TOKEN%
  echo  Header Authorization: Bearer %TP_MCP_DISPLAY_TOKEN%
) else (
  echo  [Luu y] Hay chay TPbrowser.exe it nhat 1 lan de tao mcp_auth_token trong config.json ^(hoac set TP_MCP_TOKEN^).
)
echo ──────────────────────────────────────────────
cloudflared.exe tunnel --url http://127.0.0.1:17777 2>&1

pause
