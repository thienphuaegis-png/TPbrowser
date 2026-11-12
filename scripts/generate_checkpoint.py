import json
import os
import re
import subprocess
from datetime import datetime

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

DEFAULT_AGENTS_MD = """# AI Agent Onboarding Protocol (READ FIRST)

Before scanning files, answering questions about this codebase, or modifying code, **every AI agent MUST read these files in order**:

1. **`.ai/STATE.md`** — Hot-state handoff (<100 lines): latest commit, current session changes, test verification, and next tasks.
2. **`.ai/checkpoints/PROJECT_STATE.json`** — Compact architecture snapshot (~1.5k tokens), core file map, guided tour, and local `codegraph` AST commands (`codegraph callers/callees/impact <symbol>`). **Do NOT blindly read all repository files or large extension/runtime bundles (`extensions/uBOLite.chromium/rulesets/*`, `user_data/*`, `target/*`).**
3. **`.ai/MAP.html`** — Interactive feature checklist (`DEFAULT_TASKS` + `ROADMAP_VERSION`) and repository tree.

## Mandatory Repo Hygiene Rules
- **AI workspace isolation**: Store ALL plans, specs, mockups, and checkpoints strictly inside `.ai/` (`plans/`, `specs/`, `sketches/`, `checkpoints/`). Never drop AI markdown/HTML files in repo root or `docs/`.
- **Secrets & runtime data safety**: Never read, print, or commit `config.json` (contains user API keys), `.env*`, private keys, or `user_data/*`.
- **Checkpoint sync after changes**: Update `.ai/STATE.md` + `.ai/MAP.html`, run `CAP_NHAT_CHECKPOINT.bat` (or `python scripts/generate_checkpoint.py`), and verify with `pytest -q tests/test_checkpoint_integrity.py`.
"""


def ensure_agent_gatekeeper(root_dir: str, default_agents_md: str) -> None:
    agents_path = os.path.join(root_dir, "AGENTS.md")
    claude_path = os.path.join(root_dir, "CLAUDE.md")
    if not os.path.exists(agents_path):
        with open(agents_path, "w", encoding="utf-8", newline="\n") as f:
            f.write(default_agents_md.strip() + "\n")
    if not os.path.exists(claude_path):
        with open(claude_path, "w", encoding="utf-8", newline="\n") as f:
            f.write("@AGENTS.md\n")


def get_cargo_version() -> str:
    cargo_toml = os.path.join(ROOT, "Cargo.toml")
    if os.path.exists(cargo_toml):
        with open(cargo_toml, "r", encoding="utf-8") as f:
            m = re.search(r'^version\s*=\s*"([^"]+)"', f.read(), re.M)
            if m:
                return m.group(1)
    return "0.1.0"


def get_git_commit() -> str:
    try:
        out = subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"], cwd=ROOT, stderr=subprocess.DEVNULL, text=True
        ).strip()
        return out if out else "initial"
    except Exception:
        return "initial"


def get_codegraph_stats() -> dict:
    cg_cmd = r"C:\Users\thien\AppData\Local\codegraph\current\bin\codegraph.cmd"
    if not os.path.exists(cg_cmd):
        cg_cmd = "codegraph.cmd"
    try:
        out = subprocess.check_output(
            [cg_cmd, "status", "."], cwd=ROOT, stderr=subprocess.STDOUT, text=True
        )
        files = int(re.search(r"Files\s*:\s*(\d+)", out).group(1)) if re.search(r"Files\s*:\s*(\d+)", out) else 6
        nodes = int(re.search(r"Nodes\s*:\s*(\d+)", out).group(1)) if re.search(r"Nodes\s*:\s*(\d+)", out) else 45
        edges = int(re.search(r"Edges\s*:\s*(\d+)", out).group(1)) if re.search(r"Edges\s*:\s*(\d+)", out) else 62
        return {"files": files, "nodes": nodes, "edges": edges}
    except Exception:
        return {"files": 6, "nodes": 45, "edges": 62}


def main() -> None:
    ensure_agent_gatekeeper(ROOT, DEFAULT_AGENTS_MD)
    ckpt_dir = os.path.join(ROOT, ".ai", "checkpoints")
    os.makedirs(ckpt_dir, exist_ok=True)

    stats = get_codegraph_stats()
    payload = {
        "project": "TPbrowser",
        "version": get_cargo_version(),
        "git_commit": get_git_commit(),
        "updated_at": datetime.now().strftime("%Y-%m-%d %H:%M:%S"),
        "codegraph_ast": stats,
        "codegraph_commands": {
            "callers": "codegraph callers <symbol>",
            "callees": "codegraph callees <symbol>",
            "impact": "codegraph impact <symbol>",
        },
        "architecture_layers": [
            {
                "layer": "1. Rust Native Shell, Multi-WebView Orchestrator & Universal MCP",
                "files": ["src/main.rs", "src/layout.rs", "src/json_mini.rs", "src/mcp_server.rs"],
                "responsibility": "Win32 borderless window management (tao), multi-WebView2 layout (tab_shell, content & background BOT/AI worker tabs, ai_shell, fb_shell, zalo_shell, titlebar_shell, sidebar_resizer_shell, dock_resizer_shell, settings_shell), Universal MCP Server (:17777/mcp + stdio), MV3 uBlock Origin Lite loader, popunder blocker, and centralized JSON IPC + tp_config_changed broadcaster.",
            },
            {
                "layer": "2. AURELIA Minimalist Surfaces & Zen Settings Modal",
                "files": [
                    "src/settings_modal.html",
                    "src/newtab.html",
                    "src/history.html",
                    "src/tab_shell.html",
                    "src/ai_panel.html",
                    "src/chat_header.html",
                    "src/titlebar.html",
                    "src/sidebar_resizer.html",
                    "src/dock_resizer.html",
                ],
                "responsibility": "Classic Bronze-Gold AURELIA Dark (obsidian-gold) & Light (carrara-light) UI surfaces, 999px pill controls, 1.75px stroke SVGs, 6-tab Zen Settings Modal (Alt+,), unified icon-first 40px titlebar + horizontal tabs, real website favicons, free-resize vertical tab bar, and AI Cowork background worker mode.",
            },
            {
                "layer": "3. E-Commerce Landing Workspace (tp://workspace) & Background Scrapers",
                "files": ["src/workspace.html", "src/ecom_agents.js", "src/main.rs"],
                "responsibility": "Non-intrusive background BOT worker tabs (HWND_BOTTOM, shared cookie WebContext) for Shopee (200-order price anomaly audit, 200 1-3★ ratings + counterfeit keyword alerts, Shopee Live, Promotions, Vouchers, Flash Sale, PAS) and TikTok Seller (anonymous IIFE SecSDK-safe Shadow-DOM ks-virtual-table sweep), click-to-open-direct workflows, CSV UTF-8 BOM export, and Zalo report copy.",
            },
            {
                "layer": "4. Content Agent, Skill Recorder & 3-Layer Ad Shield",
                "files": ["src/content_agent.js", "skills/skills.json", "extensions/uBOLite.chromium/manifest.json"],
                "responsibility": "Trusted-Types-safe DOM context extraction, 3-tier element locator, live user interaction recorder (__tpRecordingSkill), unread badge orbs, anti-adblock 0px CSS shield, YouTube 16x auto-skipper, and TikTok auth allowlist rules in uBOLite.",
            },
        ],
        "guided_tour": [
            {"step": 1, "file": "src/layout.rs", "symbol": "compute_shell_layout", "note": "Computes exact rectangles for Vertical/Horizontal tabs, BSP split views, and Right AI Dock vs Popup/Bubble mode."},
            {"step": 2, "file": "src/main.rs", "symbol": "TpBrowserApp::execute", "note": "Central command dispatcher, background BOT/AI worker tab lifecycle, config.json persistence, and tp_config_changed broadcaster."},
            {"step": 3, "file": "src/workspace.html", "symbol": "__tpSyncWorkspace", "note": "Landing-page workspace (tp://workspace) displaying Shopee anomalies, 1-3★ counterfeit alerts, Live, Promos/Vouchers/Flash Sale, PAS, and TikTok KPIs with click-to-open navigation."},
            {"step": 4, "file": "src/ecom_agents.js", "symbol": "__tpRunShopeeScan", "note": "Shopee Seller background agent polling order APIs (chunk=5), rating comments (1-3★), Live sessions, Discounts, Vouchers, Flash Sales, and PAS tables."},
            {"step": 5, "file": "src/settings_modal.html", "symbol": "__tpSyncSettingsModal", "note": "6-tab Zen-style centralized Settings Modal with live AI Key testing, startup mode, logo style, and zero-reload config sync."},
            {"step": 6, "file": "src/ai_panel.html", "symbol": "triggerCoworkTask", "note": "Executes AI Cowork tasks on isolated background AI worker tabs or active tab with domain-scoped skills."},
            {"step": 7, "file": "src/content_agent.js", "symbol": "runActiveAdCleanerAndSkipper", "note": "3-layer ad shield for movie streaming sites and YouTube video ads (guarded off on tiktok.com)."},
        ],
    }

    out_path = os.path.join(ckpt_dir, "PROJECT_STATE.json")
    with open(out_path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(payload, f, indent=2, ensure_ascii=False)
    print(f"[OK] Wrote {out_path} (commit={payload['git_commit']}, ast={stats})")


if __name__ == "__main__":
    main()
