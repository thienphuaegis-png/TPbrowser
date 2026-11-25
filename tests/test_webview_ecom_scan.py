from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_browser_shell_does_not_expose_workspace_entrypoint():
    titlebar = (ROOT / "src/titlebar.html").read_text(encoding="utf-8")
    newtab = (ROOT / "src/newtab.html").read_text(encoding="utf-8")
    settings = (ROOT / "src/settings_modal.html").read_text(encoding="utf-8")

    assert "open_workspace" not in titlebar
    assert "open_workspace" not in newtab
    assert "Màn hình TMĐT Workspace" not in settings


def test_browser_shell_does_not_inject_ecommerce_scanner():
    source = (ROOT / "src/main.rs").read_text(encoding="utf-8")

    assert 'include_str!("ecom_agents.js")' not in source
    init = source.split("let init_js = format!(", 1)[1].split("builder = builder.with_initialization_script", 1)[0]
    assert "ECOM_AGENTS_JS" not in init
    assert "start_multi_shop_queue(target_shops)" not in source


def test_legacy_workspace_tabs_fall_back_to_newtab():
    source = (ROOT / "src/main.rs").read_text(encoding="utf-8")

    assert 'url == "tp://workspace"' in source
    assert 'inject_ui_html(NEWTAB_HTML, &self.ui_theme)' in source
    assert 'inject_ui_html(WORKSPACE_HTML, &self.ui_theme)' not in source


def test_automatic_ecommerce_scan_is_disabled():
    source = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    reminders = source.split("fn check_reminders(&mut self)", 1)[1].split("\n    }", 1)[0]
    run_handler = source.split("AppCmd::RunEcomScan { scope } => {", 1)[1].split("AppCmd::SetEcomAutoScan(enabled) => {", 1)[0]

    assert "self.execute(AppCmd::RunEcomScan" not in reminders
    assert "start_multi_shop_queue" not in run_handler
    assert "đã được tắt" in run_handler


def test_native_ecommerce_ipc_entrypoints_are_not_exposed():
    source = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    assert '"cmd":\\"run_ecom_scan\\"' not in source
    assert '"cmd":\\"workspace_scan\\"' not in source
    assert '"cmd":\\"discover_shopee_shops\\"' not in source
    assert 'AppCmd::RunEcomScan { scope: "shopee_orders"' not in source
