from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def _titlebar():
    return (ROOT / "src/titlebar.html").read_text(encoding="utf-8")


def test_split_picker_requires_an_active_user_tab_and_one_user_tab_pair():
    html = _titlebar()
    assert "function selectableSplitTabs(st)" in html
    assert "return tab && !tab.worker_kind" in html
    assert "var canSelectPair = !!activeTab && !activeTab.worker_kind" in html
    assert "button.disabled = !pairedTab;" in html
    assert "t.id === activeTab.id" in html
    assert "Select another user tab first" in html


def test_split_layout_commands_only_use_a_valid_selected_pair():
    html = _titlebar()
    assert "function selectedSplitPair(st)" in html
    assert "var pair = selectedSplitPair(lastSyncState);" in html
    assert "if (!pair) {" in html
    assert "pair_tab_id: pair.id" in html
    assert "function selectSplitMode(mode)" in html


def test_split_modes_unsplit_and_secondary_widget_contracts_remain_wired():
    html = _titlebar()
    for mode in ("columns", "rows", "spiral"):
        assert "selectSplitMode('" + mode + "')" in html
    assert "cmd:'unsplit'" in html

    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    assert 'let is_split_visible = is_split && (idx == self.active_idx || Some(t.id) == self.split_paired_tab_id);' in native
    assert 'idx == self.active_idx && self.always_show_floating_orbs' in native


def test_chrome_cleanup_keeps_space_and_tab_hooks_without_duplicate_or_personal_ui():
    titlebar = _titlebar()
    tab_shell = (ROOT / "src/tab_shell.html").read_text(encoding="utf-8")
    newtab = (ROOT / "src/newtab.html").read_text(encoding="utf-8")
    ai_panel = (ROOT / "src/ai_panel.html").read_text(encoding="utf-8")

    assert 'id="workspace_picker"' not in titlebar
    assert "createWorkspaceFromPicker" not in titlebar

    for hook in ('id="vertical_root"', 'id="space_toggle"', 'id="space_list"', 'id="space_create_form"', 'id="v_tab_list"'):
        assert hook in tab_shell
    assert 'class="google-card"' not in tab_shell
    assert 'id="v_g_avatar"' not in tab_shell

    for banned in ('Google', 'google-account-pill', 'id="google_modal"', 'defaultBookmarks', 'Gmail Cá Nhân',
                   'Tài Khoản Google', 'Bùi Thiên Phú', 'cá nhân', 'id="nt_profile_badge"',
                   'cowork-quick-bar'):
        assert banned not in newtab
    for bookmark_hook in ('id="nt_bm_grid"', 'function renderBookmarks()', 'function saveNewBookmark()',
                          'cmd: "save_bookmarks"', 'var currentBookmarks = [];'):
        assert bookmark_hook in newtab

    assert '<b>AI Cowork sẵn sàng — Nhắn việc cần làm.</b>' in ai_panel
    assert 'rê chuột lên các biểu tượng phía trên để xem chức năng' not in ai_panel


def test_vertical_orientation_uses_full_height_sidebar_and_preserves_commands():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    titlebar = _titlebar()
    tab_shell = (ROOT / "src/tab_shell.html").read_text(encoding="utf-8")
    settings = (ROOT / "src/settings_modal.html").read_text(encoding="utf-8")

    assert 'TabOrientation::VerticalLeft => 0' in native
    assert 'let titlebar_visible = self.tab_orientation == TabOrientation::HorizontalTop;' in native
    assert 'self.titlebar_shell.set_visible(titlebar_visible)' in native
    assert '.with_visible(init_tab_orient == TabOrientation::HorizontalTop)' in native
    assert 'shell.tab_bar.y += top_reserved as i32;' in native
    assert "cmd:'nav_back'" in titlebar
    assert "cmd:'nav_forward'" in titlebar
    assert "cmd:'reload'" in titlebar

    assert 'class="v-window-row"' in tab_shell
    for command in ('open_settings_modal', 'win_minimize', 'win_toggle_maximize', 'win_close'):
        assert "cmd:'" + command + "'" in tab_shell
    assert tab_shell.index('id="v_tab_list"') < tab_shell.index('class="space-section')
    vertical_header = tab_shell.split('id="v_tab_list"', 1)[0]
    assert 'toggle_tab_orientation' not in tab_shell
    assert 'nav_back' not in vertical_header
    assert 'nav_forward' not in vertical_header
    assert "cmd:'reload'" not in vertical_header

    assert 'send({ cmd: "toggle_tab_orientation" });' not in native
    assert "toggle_tab_orientation" not in titlebar
    assert 'Alt + T' not in settings
    assert "applySetting('tab_orientation', 'vertical')" in settings
    assert "applySetting('tab_orientation', 'horizontal')" in settings


def test_vertical_shell_context_menu_exposes_existing_navigation_ipc():
    tab_shell = (ROOT / "src/tab_shell.html").read_text(encoding="utf-8")

    assert 'id="vertical_context_menu"' in tab_shell
    assert 'verticalRoot.addEventListener("contextmenu"' in tab_shell
    assert 'event.preventDefault();' in tab_shell
    for command in ("nav_back", "nav_forward", "reload"):
        assert "sendRust({ cmd: \"" + command + "\" })" in tab_shell
    assert 'document.addEventListener("mousedown"' in tab_shell
    assert 'event.key === "Escape"' in tab_shell


def test_vertical_header_uses_tpbrand_not_adjacent_navigation_or_collapse_button():
    tab_shell = (ROOT / "src/tab_shell.html").read_text(encoding="utf-8")
    header = tab_shell.split('<div class="v-header">', 1)[1].split('<form class="v-controls', 1)[0]

    assert 'id="vertical_brand"' in header
    assert 'window.__tpResolveBrandAsset("app", currentLogoStyle, currentTheme)' in tab_shell
    assert 'id="btn_collapse_side"' not in header
    assert 'toggle_sidebar_collapse' not in header
    for command in ("nav_back", "nav_forward", "reload"):
        assert "cmd:'" + command + "'" not in header


def test_vertical_shell_groups_pinned_normal_and_worker_tabs_before_bottom_spaces():
    tab_shell = (ROOT / "src/tab_shell.html").read_text(encoding="utf-8")

    for hook in ('id="v_pinned_section"', 'id="v_pinned_grid"', 'id="v_tab_list"',
                 'id="v_worker_section"', 'id="v_worker_list"', 'class="space-section'):
        assert hook in tab_shell
    assert tab_shell.index('id="v_pinned_section"') < tab_shell.index('id="v_tab_list"') < tab_shell.index('class="space-section') < tab_shell.index('id="v_worker_section"')
    assert 'function isWorkerTab(t)' in tab_shell
    assert 'return !!t.worker_kind;' in tab_shell
    assert 'var workerTabs = tabs.filter(isWorkerTab);' in tab_shell
    assert 'var pinnedTabs = tabs.filter(function(t) { return !isWorkerTab(t) && !!t.pinned; });' in tab_shell
    assert 'var normalTabs = tabs.filter(function(t) { return !isWorkerTab(t) && !t.pinned; });' in tab_shell


def test_vertical_shell_uses_ctrl_shift_selection_and_drag_group_split_without_checkboxes():
    tab_shell = (ROOT / "src/tab_shell.html").read_text(encoding="utf-8")
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")

    assert 'type="checkbox"' not in tab_shell
    for hook in ('event.ctrlKey || event.metaKey', 'event.shiftKey', 'cmd: "set_tab_selection"',
                 "cmd: 'group_selected_tabs'", "cmd: 'split_selected_tabs'", "cmd: 'scan_selected_tabs'",
                 'draggable = true', 'cmd: "group_tabs"', "cmd: 'split_tab_group'"):
        assert hook in tab_shell
    for command in ('SetTabSelection', 'GroupTabs', 'SplitTabGroup', 'ScanSelectedTabs'):
        assert command in native
    assert 'selected_tab_ids: HashSet<u64>' in native
    assert 'tab_selection_anchor_id: Option<u64>' in native
    assert 'tab_group_id: Option<u64>' in native


def test_tab_selection_grouping_and_scanning_are_workspace_worker_scoped():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")

    selection = native.split('AppCmd::SetTabSelection { tab_id, mode } => {', 1)[1].split('AppCmd::GroupTabs', 1)[0]
    assert 'self.prune_tab_selection();' in selection
    assert 'self.tab_selection_anchor_id' in selection
    assert 'visible_user_tab_indices()' in selection
    assert 'self.tab_selection_anchor_id = Some(tab_id);' in selection

    group = native.split('AppCmd::GroupTabs { source_id, target_id } => {', 1)[1].split('AppCmd::SplitTabGroup', 1)[0]
    assert 'self.active_user_tab_ids()' in group
    assert 'self.selected_active_user_tab_ids()' in group

    split = native.split('AppCmd::SplitTabGroup { tab_ids } => {', 1)[1].split('AppCmd::ScanSelectedTabs', 1)[0]
    assert 'self.selected_active_user_tab_ids()' in split
    assert 'tab.tab_group_id = None;' in split
    assert 'valid_tab_id' in split

    scan = native.split('AppCmd::ScanSelectedTabs => {', 1)[1].split('AppCmd::CopyToClipboard', 1)[0]
    assert 'self.selected_active_user_tab_ids()' in scan


def test_tab_metadata_is_bound_to_sender_and_session_groups_round_trip():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")

    assert 'Self::handle_ipc(&proxy_ipc, req.body(), Some(tab_id));' in native
    meta = native.split('} else if body.contains("\\\"cmd\\\":\\\"tab_meta\\\"") {', 1)[1].split('} else if body.contains("\\\"cmd\\\":\\\"channel_unread\\\"") {', 1)[0]
    assert 'sender_tab_id.unwrap_or(0)' in meta
    assert 'extract_u64(body, "\\\"tab_id\\\":")' not in meta
    assert '"tab_group_id"' in native
    assert 'tab.tab_group_id = item.get("tab_group_id")' in native


def test_worker_tabs_cannot_be_pinned_and_shell_uses_stable_ids_for_actions():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    tab_shell = (ROOT / "src/tab_shell.html").read_text(encoding="utf-8")

    toggle = native.split('AppCmd::ToggleTabPinned => {', 1)[1].split('AppCmd::OpenCommandBar', 1)[0]
    assert 'tab.worker_kind.is_none()' in toggle
    vertical = tab_shell.split('function renderVerticalTab(t, isAct, iconOnly) {', 1)[1].split('var workerTabs = tabs.filter(isWorkerTab);', 1)[0]
    assert 'if (t.worker_kind) { sendRust({ cmd: "focus_tab_by_id", tab_id: t.id }); return; }' in vertical
    assert 'cmd: "focus_tab_by_id", tab_id: t.id' in vertical
    assert 'cmd: "close_tab_by_id", tab_id: t.id' in vertical
    assert 'if (isAct && !iconOnly && !t.worker_kind) {' in vertical
