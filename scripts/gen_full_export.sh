#!/bin/bash
OUT=".ai/FULL_PROJECT_EXPORT.md"
> "$OUT"

lang_for() {
  case "$1" in
    *.rs) echo "rust" ;;
    *.toml) echo "toml" ;;
    *.html) echo "html" ;;
    *.js) echo "javascript" ;;
    *.py) echo "python" ;;
    *.json) echo "json" ;;
    *.bat) echo "bat" ;;
    *) echo "" ;;
  esac
}

{
  echo "# TPbrowser — Full Project Export (for AI review)"
  echo ""
  echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "Git commit: $(git rev-parse --short HEAD)"
  echo ""
  echo "Trình duyệt cá nhân viết bằng Rust (tao + wry / WebView2 trên Windows). Đây là toàn bộ source code + docs + tests cốt lõi, gộp vào 1 file duy nhất để dán cho AI chat (Gemini/ChatGPT/Claude) đọc và phân tích toàn diện."
  echo ""
} >> "$OUT"

FILES=(
  AGENTS.md
  CLAUDE.md
  README.md
  .ai/STATE.md
  .ai/checkpoints/PROJECT_STATE.json
  .ai/plans/2026-11-19-dot3-auto-unload-glance-media-handoff.md
  .ai/plans/2026-11-19-zen-aurelia-roadmap-plan.md
  .ai/AI_STUDIO_SYNC_NOTE.md
  Cargo.toml
  build.rs
  src/main.rs
  src/layout.rs
  src/json_mini.rs
  src/mcp_server.rs
  src/shopee_discovery.rs
  src/workspace.html
  src/tab_shell.html
  src/titlebar.html
  src/newtab.html
  src/history.html
  src/settings_modal.html
  src/ai_panel.html
  src/chat_header.html
  src/glance_header.html
  src/sidebar_resizer.html
  src/dock_resizer.html
  src/content_agent.js
  src/ecom_agents.js
  skills/skills.json
  tests/test_checkpoint_integrity.py
  tests/test_titlebar_split_contract.py
  tests/test_webview_ecom_scan.py
  scripts/generate_checkpoint.py
  scripts/start_all.bat
  scripts/start_tunnel.bat
  CAP_NHAT_CHECKPOINT.bat
  CHAY_TPBROWSER.bat
  .gitignore
)

echo "## Mục lục" >> "$OUT"
echo "" >> "$OUT"
for f in "${FILES[@]}"; do
  if [ -f "$f" ]; then
    echo "- \`$f\`" >> "$OUT"
  fi
done
echo "" >> "$OUT"
echo "---" >> "$OUT"
echo "" >> "$OUT"

for f in "${FILES[@]}"; do
  if [ -f "$f" ]; then
    lang=$(lang_for "$f")
    {
      echo "## File: \`$f\`"
      echo ""
      echo "\`\`\`$lang"
      cat "$f"
      echo ""
      echo "\`\`\`"
      echo ""
      echo "---"
      echo ""
    } >> "$OUT"
  else
    echo "SKIP (not found): $f" 1>&2
  fi
done

wc -l "$OUT"
wc -c "$OUT"
