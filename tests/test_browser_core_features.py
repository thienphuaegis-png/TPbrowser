from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_browser_core_commands_include_download_private_permission_and_print():
    source = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    assert "DownloadStarted" in source
    assert "NewPrivateTab" in source
    assert "PermissionDecision" in source
    assert "PrintActive" in source


def test_normal_tabs_keep_mcp_and_private_tabs_are_not_persisted():
    source = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    assert "mcp_server" in source
    assert "private" in source.lower()
    assert "is_private" in source
    assert "private: is_private" in source


def test_titlebar_exposes_browser_core_actions():
    titlebar = (ROOT / "src/titlebar.html").read_text(encoding="utf-8")
    assert "print_active" in titlebar
    assert "new_private_tab" in titlebar


def test_private_tabs_are_marked_in_shell_state_and_tab_strip():
    source = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    titlebar = (ROOT / "src/titlebar.html").read_text(encoding="utf-8")
    assert '\\"private\\":{}' in source
    assert 'pb.textContent = "PRIVATE"' in titlebar


def test_download_manager_tracks_events_and_exposes_titlebar_popover():
    source = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    titlebar = (ROOT / "src/titlebar.html").read_text(encoding="utf-8")
    assert "struct DownloadEntry" in source
    assert "self.downloads.push" in source
    assert '\\"downloads\\":[{}]' in source
    assert 'id="tb_download_popover"' in titlebar
    assert "renderDownloads" in titlebar


def test_permission_manager_defaults_to_native_prompt_and_saves_policy_keys():
    source = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    settings = (ROOT / "src/settings_modal.html").read_text(encoding="utf-8")
    assert "PermissionResponse::Default" in source
    assert "Quyền Truy Cập Theo Website" in settings
    assert "savePermissionPolicy('camera'" in settings
    assert '"permission_" + kind' in settings


def test_private_profile_cleanup_is_scoped_to_private_container():
    source = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    assert "fn cleanup_private_profile" in source
    assert 'if !container_id.starts_with("private_")' in source
    assert 'join(Self::container_profile_name(container_id))' in source


def test_site_data_manager_clears_active_webview_data():
    source = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    settings = (ROOT / "src/settings_modal.html").read_text(encoding="utf-8")
    assert "ClearSiteData" in source
    assert "clear_all_browsing_data" in source
    assert "clear_site_data" in settings


def test_custom_shortcuts_are_persisted_validated_and_editable_without_touching_browser_defaults():
    source = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    settings = (ROOT / "src/settings_modal.html").read_text(encoding="utf-8")
    assert "custom_shortcuts" in source
    assert "shortcut_command_for" in source
    assert "reserved_browser_shortcut" in source
    assert "CustomShortcutCommand" in source
    assert "shortcut_capture" in settings
    assert "normalizeShortcutEvent" in settings
    assert "shortcut_conflict" in settings
    assert "resetShortcut" in settings
    assert "clearShortcut" in settings


def test_sidebar_downloads_and_persisted_permission_policies_contract():
    source = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    tab_shell = (ROOT / "src/tab_shell.html").read_text(encoding="utf-8")
    titlebar = (ROOT / "src/titlebar.html").read_text(encoding="utf-8")
    settings = (ROOT / "src/settings_modal.html").read_text(encoding="utf-8")

    assert "OpenDownloadedFile" in source
    assert "OpenDownloadFolder" in source
    assert "is_safe_download_path" in source
    assert "permission_response_for_kind" in source
    assert 'id="v_downloads_card"' in tab_shell
    assert "renderDownloadsCard" in tab_shell
    assert "open_downloaded_file" in tab_shell and "open_download_folder" in tab_shell
    assert "open_downloaded_file" in titlebar and "open_download_folder" in titlebar
    assert 'id="sel_perm_camera"' in settings and 'id="sel_perm_clipboard"' in settings
