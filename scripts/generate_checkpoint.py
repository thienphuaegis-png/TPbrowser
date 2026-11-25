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
        def parse_stat(label: str, default: int) -> int:
            m = re.search(rf"{label}\s*:\s*([\d.,]+)", out)
            if not m:
                return default
            digits = re.sub(r"[^\d]", "", m.group(1))
            return int(digits) if digits else default

        files = parse_stat("Files", 6)
        nodes = parse_stat("Nodes", 45)
        edges = parse_stat("Edges", 62)
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
                "responsibility": "Win32 borderless window management (tao), multi-WebView2 layout (tab_shell, content/private/warm-standby tabs, glance shells, ai_shell, fb_shell, zalo_shell, titlebar_shell, resizers, settings_shell), Universal MCP Server (:17777/mcp + stdio), Profile7 COM extension discovery & Glance popup launcher, multi-window recovery, and centralized JSON IPC.",
            },
            {
                "layer": "2. AURELIA Minimalist Surfaces, Command Bar & Zen Settings",
                "files": [
                    "src/settings_modal.html",
                    "src/newtab.html",
                    "src/history.html",
                    "src/tab_shell.html",
                    "src/ai_panel.html",
                    "src/chat_header.html",
                    "src/titlebar.html",
                    "src/glance_header.html",
                    "src/sidebar_resizer.html",
                    "src/dock_resizer.html",
                ],
                "responsibility": "Bronze-Gold AURELIA Dark (obsidian-gold) & Light (carrara-light) UI surfaces, transparent single-host Command Bar & Ctrl+Tab MRU Switcher HUD, synchronized Zen right-click menus, unified 40px titlebar + horizontal/vertical tabs, live extension menu, free-resize sidebar, and AI Cowork dock/bubble.",
            },
            {
                "layer": "3. Content Agent, Better Find Bar & 3-Layer Ad Shield",
                "files": ["src/content_agent.js", "skills/skills.json", "extensions/uBOLite.chromium/manifest.json"],
                "responsibility": "Trusted-Types-safe DOM context extraction, theme-aware Better Find Bar with match counter & CSS Custom Highlight API, Alt+Click Glance interception, Zen web context menu with clean tracking URLs, Zen Boosts + Zap, YouTube 16x auto-skipper, and uBOLite MV3 manifest rules.",
            },
            {
                "layer": "4. Contract, Integrity & Core Verification Suite",
                "files": [
                    "tests/test_titlebar_split_contract.py",
                    "tests/test_browser_core_features.py",
                    "tests/test_browser_polish_features.py",
                    "tests/test_checkpoint_integrity.py",
                    "tests/test_webview_ecom_scan.py",
                ],
                "responsibility": "Automated Rust unit suite + Python UI/architecture/security/checkpoint contract verification and headless live MCP smoke automation.",
            },
        ],
        "guided_tour": [
            {"step": 1, "file": "src/layout.rs", "symbol": "compute_shell_layout", "note": "Computes exact rectangles for Vertical/Horizontal tabs, Compact Mode, BSP split views, and Right AI Dock vs Bubble mode."},
            {"step": 2, "file": "src/main.rs", "symbol": "TpBrowserApp::execute", "note": "Central command dispatcher handling MRU HUD, Command Bar overlay, Window Recovery, WarmStandbyTab, Profile7 extension popups, and config/session persistence."},
            {"step": 3, "file": "src/tab_shell.html", "symbol": "__tpOpenMruSwitcher", "note": "Transparent host rendering the vertical sidebar, Space switcher, centered Command Bar, and Ctrl+Tab MRU Switcher HUD."},
            {"step": 4, "file": "src/titlebar.html", "symbol": "renderExtensionMenu", "note": "Unified horizontal titlebar with smart URL suggestions, Bookmarks bar, Downloads popover, and live Extension popup/dashboard launcher."},
            {"step": 5, "file": "src/settings_modal.html", "symbol": "__tpSyncSettingsModal", "note": "Zen-style Settings Modal with shortcuts, context menu toggles, permission policies, privacy shields, and zero-reload sync."},
            {"step": 6, "file": "src/ai_panel.html", "symbol": "triggerCoworkTask", "note": "Executes AI Cowork tasks with domain-scoped skills and chat selection helpers."},
            {"step": 7, "file": "src/content_agent.js", "symbol": "__tpToggleFindBar", "note": "In-page agent powering Better Find Bar (x/y, Aa, CSS.highlights), Zen web context menu, Alt+Click Glance, Boosts, and YouTube ad skipping."},
        ],
    }

    out_path = os.path.join(ckpt_dir, "PROJECT_STATE.json")
    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(payload, f, indent=2, ensure_ascii=False)
    print(f"[OK] Wrote {out_path} (commit={payload['git_commit']}, ast={stats})")


if __name__ == "__main__":
    main()
