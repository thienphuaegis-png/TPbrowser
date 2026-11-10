import json
import os
import re

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def test_agent_gatekeeper_files_exist_and_point_to_trio():
    agents_path = os.path.join(ROOT, "AGENTS.md")
    claude_path = os.path.join(ROOT, "CLAUDE.md")
    assert os.path.exists(agents_path), "AGENTS.md must exist at repo root"
    assert os.path.exists(claude_path), "CLAUDE.md (@AGENTS.md bridge) must exist at repo root"
    with open(agents_path, "r", encoding="utf-8") as f:
        lines = f.readlines()
    assert len(lines) < 60, f"AGENTS.md must stay compact (<60 lines), found {len(lines)}"
    txt = "".join(lines)
    for req in (".ai/STATE.md", ".ai/checkpoints/PROJECT_STATE.json", ".ai/MAP.html", "codegraph"):
        assert req in txt, f"AGENTS.md missing required reference: {req}"
    with open(claude_path, "r", encoding="utf-8") as f:
        assert f.read().strip().startswith("@AGENTS.md"), "CLAUDE.md must import @AGENTS.md"


def test_state_md_is_compact_and_structured():
    state_path = os.path.join(ROOT, ".ai", "STATE.md")
    assert os.path.exists(state_path), ".ai/STATE.md must exist"
    with open(state_path, "r", encoding="utf-8") as f:
        lines = f.readlines()
    assert len(lines) < 100, f".ai/STATE.md must be < 100 lines, found {len(lines)}"
    txt = "".join(lines)
    for heading in ("**Checkpoint:**", "## 1.", "## 2.", "## 3.", "## 4."):
        assert heading in txt, f".ai/STATE.md missing heading: {heading}"


def test_map_html_has_valid_roadmap_and_categories():
    map_path = os.path.join(ROOT, ".ai", "MAP.html")
    assert os.path.exists(map_path), ".ai/MAP.html must exist"
    with open(map_path, "r", encoding="utf-8") as f:
        txt = f.read()
    assert "ROADMAP_VERSION" in txt
    assert "DEFAULT_TASKS" in txt
    cats = set(re.findall(r'data-cat="([A-Z]+)"', txt))
    task_cats = set(re.findall(r'cat:\s*"([A-Z]+)"', txt))
    assert task_cats.issubset(cats), f"Task categories {task_cats} not in filter buttons {cats}"


def test_project_state_json_is_valid():
    p_path = os.path.join(ROOT, ".ai", "checkpoints", "PROJECT_STATE.json")
    assert os.path.exists(p_path), ".ai/checkpoints/PROJECT_STATE.json must exist"
    with open(p_path, "r", encoding="utf-8") as f:
        data = json.load(f)
    assert data.get("project") == "TPbrowser"
    assert data.get("git_commit") not in (None, "", "unknown", "uncommitted")
    assert len(data.get("architecture_layers", [])) >= 3
    assert len(data.get("guided_tour", [])) >= 3
