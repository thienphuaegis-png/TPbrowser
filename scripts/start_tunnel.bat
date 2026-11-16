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
cloudflared.exe tunnel --url http://127.0.0.1:17777 2>&1

pause
