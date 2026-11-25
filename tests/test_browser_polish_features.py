from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_devtools_command_is_wired():
    source = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    titlebar = (ROOT / "src/titlebar.html").read_text(encoding="utf-8")
    assert "OpenDevTools" in source
    assert "open_devtools" in source
    assert "devtools" in titlebar


def test_reader_mode_command_is_wired():
    source = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    titlebar = (ROOT / "src/titlebar.html").read_text(encoding="utf-8")
    assert "ToggleReaderMode" in source
    assert "reader_mode" in source
    assert "reader_mode" in titlebar


def test_mcp_remains_enabled():
    source = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    assert "mod mcp_server;" in source
    assert "McpDispatch" in source
