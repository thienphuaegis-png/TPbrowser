#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Generate a consolidated markdown export of TPbrowser codebase for AI analysis.
Usage: python scripts/gen_full_export.py
"""

import os
import sys
import subprocess
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / ".ai" / "FULL_PROJECT_EXPORT.md"

FILES = [
    "AGENTS.md",
    "CLAUDE.md",
    "README.md",
    ".ai/STATE.md",
    ".ai/checkpoints/PROJECT_STATE.json",
    ".ai/plans/2026-11-19-dot3-auto-unload-glance-media-handoff.md",
    ".ai/plans/2026-11-19-zen-aurelia-roadmap-plan.md",
    ".ai/AI_STUDIO_SYNC_NOTE.md",
    "Cargo.toml",
    "build.rs",
    "src/main.rs",
    "src/layout.rs",
    "src/json_mini.rs",
    "src/mcp_server.rs",
    "src/shopee_discovery.rs",
    "src/workspace.html",
    "src/tab_shell.html",
    "src/titlebar.html",
    "src/newtab.html",
    "src/history.html",
    "src/settings_modal.html",
    "src/ai_panel.html",
    "src/chat_header.html",
    "src/glance_header.html",
    "src/sidebar_resizer.html",
    "src/dock_resizer.html",
    "src/content_agent.js",
    "src/ecom_agents.js",
    "skills/skills.json",
    "tests/test_checkpoint_integrity.py",
    "tests/test_titlebar_split_contract.py",
    "tests/test_webview_ecom_scan.py",
    "scripts/generate_checkpoint.py",
    "scripts/start_all.bat",
    "scripts/start_tunnel.bat",
    "CAP_NHAT_CHECKPOINT.bat",
    "CHAY_TPBROWSER.bat",
    ".gitignore",
]

def lang_for(filename: str) -> str:
    ext = Path(filename).suffix.lower()
    mapping = {
        ".rs": "rust",
        ".toml": "toml",
        ".html": "html",
        ".js": "javascript",
        ".py": "python",
        ".json": "json",
        ".bat": "bat",
        ".md": "markdown",
    }
    return mapping.get(ext, "")

def get_git_commit() -> str:
    try:
        res = subprocess.run(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=str(ROOT),
            capture_output=True,
            text=True,
            check=True
        )
        return res.stdout.strip()
    except Exception:
        return "unknown"

def main():
    now_iso = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    commit = get_git_commit()

    lines = []
    lines.append("# TPbrowser — Full Project Export (for AI review)")
    lines.append("")
    lines.append(f"Generated: {now_iso}")
    lines.append(f"Git commit: {commit}")
    lines.append("")
    lines.append(
        "Trình duyệt cá nhân viết bằng Rust (tao + wry / WebView2 trên Windows). "
        "Đây là toàn bộ source code + docs + tests cốt lõi + kế hoạch Handoff Đợt 3, "
        "gộp vào 1 file duy nhất để dán cho AI chat (Gemini / Google AI Studio / Claude) đọc và phân tích toàn diện."
    )
    lines.append("")
    lines.append("## Mục lục")
    lines.append("")

    valid_files = []
    for rel_path in FILES:
        p = ROOT / rel_path
        if p.is_file():
            valid_files.append(rel_path)
            lines.append(f"- `{rel_path}`")
        else:
            print(f"[WARN] File not found: {rel_path}", file=sys.stderr)

    lines.append("")
    lines.append("---")
    lines.append("")

    for rel_path in valid_files:
        p = ROOT / rel_path
        lang = lang_for(rel_path)
        try:
            content = p.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            content = p.read_text(encoding="latin1", errors="replace")

        lines.append(f"## File: `{rel_path}`")
        lines.append("")
        lines.append(f"```{lang}")
        lines.append(content)
        lines.append("```")
        lines.append("")
        lines.append("---")
        lines.append("")

    out_content = "\n".join(lines)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(out_content, encoding="utf-8")

    total_lines = len(lines)
    total_bytes = len(out_content.encode("utf-8"))
    print(f"[OK] Exported {len(valid_files)} files to {OUT}")
    print(f"Total lines: {total_lines}")
    print(f"Total size: {total_bytes / (1024 * 1024):.2f} MB ({total_bytes} bytes)")

if __name__ == "__main__":
    main()
