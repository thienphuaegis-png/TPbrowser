from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def _titlebar():
    return (ROOT / "src/titlebar.html").read_text(encoding="utf-8")


def _tab_shell():
    return (ROOT / "src/tab_shell.html").read_text(encoding="utf-8")


def test_split_picker_requires_an_active_user_tab_and_one_user_tab_pair():
    html = _tab_shell()
    assert 'cmd: "set_split_mode"' in html
    assert 'mode: "columns", pair_tab_id: tab.id' in html
    assert 'mode: "rows", pair_tab_id: tab.id' in html
    assert 'cmd: "swap_split_tabs"' in html
    assert 'cmd: "unsplit"' in html


def test_split_layout_commands_only_use_a_valid_selected_pair():
    html = _tab_shell()
    assert 'if (!isAct) {' in html
    assert 'addItem("◫ Chia đôi dọc với Tab này"' in html
    assert 'addItem("⊟ Chia đôi ngang với Tab này"' in html
    assert 'addItem("Tách thành cửa sổ mới"' in html
    assert 'requestTabDetach(tab.id, "context_menu");' in html
    assert 'cmd: "set_split_mode", mode: "columns", pair_tab_id: tab.id' in html
    assert 'cmd: "set_split_mode", mode: "rows", pair_tab_id: tab.id' in html


def test_split_modes_unsplit_and_secondary_widget_contracts_remain_wired():
    html = _tab_shell()
    for mode in ("columns", "rows"):
        assert 'mode: "' + mode + '"' in html
    assert 'cmd: "unsplit"' in html

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


def test_vertical_header_uses_menu_and_native_window_controls_only():
    tab_shell = (ROOT / "src/tab_shell.html").read_text(encoding="utf-8")
    header = tab_shell.split('<div class="v-header">', 1)[1].split('<form class="v-controls', 1)[0]

    assert 'id="vertical_brand"' in header
    assert 'class="ghost-btn vertical-menu"' in header
    assert 'menu-glyph' in header
    assert 'id="btn_collapse_side"' not in header
    assert 'toggle_sidebar_collapse' not in header
    for command in ("nav_back", "nav_forward", "reload"):
        assert "cmd:'" + command + "'" not in header
    for command in ("win_minimize", "win_toggle_maximize", "win_close"):
        assert "cmd:'" + command + "'" in header


def test_zen_sidebar_drag_below_threshold_enters_compact_mode():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    layout = (ROOT / "src/layout.rs").read_text(encoding="utf-8")

    assert "SIDEBAR_COLLAPSE_THRESHOLD" in layout
    assert "pub const SIDEBAR_COLLAPSE_THRESHOLD: i32 = 60;" in layout
    assert "if cur_x < SIDEBAR_COLLAPSE_THRESHOLD" in native
    assert "proxy.send_event(AppCmd::ToggleCompactMode)" in native
    assert "COMPACT_MODE_ENABLED.store(self.compact_mode, Ordering::Relaxed);" in native
    assert "AppCmd::SetCompactSidebarHover(hover)" in native


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


def test_vertical_shell_hides_selection_actions_but_keeps_native_group_split_ipc():
    tab_shell = (ROOT / "src/tab_shell.html").read_text(encoding="utf-8")
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")

    assert 'type="checkbox"' not in tab_shell
    for hook in ('event.ctrlKey || event.metaKey', 'event.shiftKey', 'cmd: "set_tab_selection"',
                 'draggable = true'):
        assert hook in tab_shell
    for removed in ('id="v_selection_actions"', 'class="tab-selection-actions',
                    'class="small-pill-btn"', '>Nhóm</button>', '>Tách</button>',
                    "cmd: 'group_selected_tabs'", "cmd: 'split_selected_tabs'"):
        assert removed not in tab_shell
    assert 'cmd: "group_tabs"' not in tab_shell
    for command in ('SetTabSelection', 'GroupTabs', 'SplitTabGroup'):
        assert command in native
    assert 'body.contains("\\\"cmd\\\":\\\"group_selected_tabs\\\"")' in native
    assert 'body.contains("\\\"cmd\\\":\\\"split_selected_tabs\\\"")' in native
    assert 'selected_tab_ids: HashSet<u64>' in native
    assert 'tab_selection_anchor_id: Option<u64>' in native
    assert 'tab_group_id: Option<u64>' in native


def test_tab_selection_grouping_is_workspace_worker_scoped():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")

    selection = native.split('AppCmd::SetTabSelection { tab_id, mode } => {', 1)[1].split('AppCmd::GroupTabs', 1)[0]
    assert 'self.prune_tab_selection();' in selection
    assert 'self.tab_selection_anchor_id' in selection
    assert 'visible_user_tab_indices()' in selection
    assert 'self.tab_selection_anchor_id = Some(tab_id);' in selection

    group = native.split('AppCmd::GroupTabs { source_id, target_id } => {', 1)[1].split('AppCmd::SplitTabGroup', 1)[0]
    assert 'self.active_user_tab_ids()' in group
    assert 'self.selected_active_user_tab_ids()' in group

    split = native.split('AppCmd::SplitTabGroup { tab_ids } => {', 1)[1].split('AppCmd::CopyToClipboard', 1)[0]
    assert 'self.selected_active_user_tab_ids()' in split
    assert 'tab.tab_group_id = None;' in split
    assert 'valid_tab_id' in split


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


def test_tab_drag_reorder_uses_local_drag_state_and_separate_grouping_controls():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    tab_shell = _tab_shell()
    titlebar = _titlebar()

    vertical = tab_shell.split('function renderVerticalTab(t, isAct, iconOnly) {', 1)[1].split('function renderFolderBlock', 1)[0]
    assert 'var verticalDragTabId = null;' in tab_shell
    assert 'verticalDragTabId = t.id;' in vertical
    assert 'if (verticalDragTabId && verticalDragTabId !== t.id)' in vertical
    assert 'var sourceId = event.dataTransfer.getData("text/tp-tab-id");' not in vertical
    assert 'sendRust({ cmd: "reorder_tab", source_id: sourceId, target_id: t.id, after: rel >= 0.5 });' in vertical
    assert 'cmd: "group_tabs", source_id: sourceId, target_id: t.id' not in vertical
    assert 'function appendVerticalEndDropTarget(container, tabs)' in tab_shell
    assert 'Thả ở cuối danh sách' in tab_shell

    assert 'var horizontalDragTabId = null;' in titlebar
    assert 'horizontalDragTabId = t.id;' in titlebar
    assert 'if (horizontalDragTabId && horizontalDragTabId !== t.id)' in titlebar
    assert 'var sourceId = ev.dataTransfer.getData("text/tp-tab-id");' not in titlebar
    assert 'function appendHorizontalEndDropTarget(box, tabs)' in titlebar
    assert 'Thả ở cuối' in titlebar

    reorder = native.split('AppCmd::ReorderTab { source_id, target_id, after } => {', 1)[1].split('AppCmd::OpenInSplit', 1)[0]
    assert 't.worker_kind.is_none()' in reorder
    assert 'self.tabs[s_idx].pinned != self.tabs[d_idx].pinned' in reorder
    assert 'self.tabs[s_idx].essential != self.tabs[d_idx].essential' in reorder
    assert 'self.active_idx = new_active;' in reorder
    assert 'std::thread::spawn(move || {\n            let _ = fs::write(session_path, body);' not in native
    assert 'let _ = fs::write(session_path, body);' in native


def test_multi_window_foundation_routes_the_new_window_command_by_window_id():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    titlebar = _titlebar()
    tab_shell = _tab_shell()

    assert 'id="btn_new_window"' in titlebar
    assert "cmd:'new_window'" in titlebar
    assert 'cmd: "new_window"' in tab_shell
    assert 'Cửa sổ mới' in tab_shell
    assert 'AppCmd::NewWindow' in native
    assert 'enum AppEvent' in native
    assert 'WindowCommand { window_id: WindowId, origin_tab_id: Option<u64>, command: AppCmd }' in native
    assert 'struct WindowRegistry' in native
    assert 'secondary: HashMap<WindowId, TpBrowserApp>' in native
    assert 'Event::WindowEvent { window_id, event, .. } if window_id != windows.primary_window_id' in native
    assert 'Event::UserEvent(AppEvent::WindowCommand { window_id, origin_tab_id, command: cmd })' in native


def test_multi_window_secondary_close_does_not_exit_the_primary_and_skips_flat_session_write():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")

    secondary_events = native.split('Event::WindowEvent { window_id, event, .. } if window_id != windows.primary_window_id => {', 1)[1].split('Event::WindowEvent { event, .. } =>', 1)[0]
    assert 'WindowEvent::CloseRequested => close_secondary = true' in secondary_events
    assert 'windows.close_secondary(window_id);' in secondary_events
    assert 'ControlFlow::Exit' not in secondary_events
    assert 'persist_session: restore_session' in native
    assert 'if !self.persist_session {' in native
    assert 'if restore_session && init_startup == "restore" {' in native


def test_vertical_drag_out_signals_native_detach_without_requiring_a_prewarmed_window():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    tab_shell = _tab_shell()
    titlebar = _titlebar()

    vertical = tab_shell.split('function renderVerticalTab(t, isAct, iconOnly) {', 1)[1].split('function renderFolderBlock', 1)[0]
    assert 'row.addEventListener("dragend", function(event) {' in vertical
    assert 'var outsideStrip = event.clientX < rect.left - 24 || event.clientX > rect.right + 24' in vertical
    assert 'requestTabDetach(t.id, "drag_out", event);' in vertical
    assert 'screen_x: event && Number.isFinite(event.screenX) ? Math.trunc(event.screenX) : null' in tab_shell
    assert 'showDetachHint("Đang tách tab sang cửa sổ mới…");' in tab_shell
    assert 'addItem("Tách thành cửa sổ mới"' in tab_shell
    assert 'item.addEventListener("dragend", function(ev) {' in titlebar
    assert 'cmd: "detach_tab_to_new_window"' in titlebar
    assert 'screen_x: Number.isFinite(ev.screenX) ? Math.trunc(ev.screenX) : null' in titlebar
    assert 'DetachTabToNewWindow { tab_id: u64, intent: TabDetachIntent, fallback_cursor: Option<WinPoint> }' in native
    assert 'AppCmd::DetachTabToNewWindow { tab_id, intent, fallback_cursor }' in native


def test_detach_native_routing_rejects_workers_and_rolls_back_without_reload():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")

    parser = native.split('} else if body.contains("\\\"cmd\\\":\\\"detach_tab_to_new_window\\\"") {', 1)[1].split('} else if body.contains("\\\"cmd\\\":\\\"toggle_bookmarks_bar\\\"") {', 1)[0]
    assert 'Self::valid_tab_id(*id)' in parser
    assert 'GetCursorPos(&mut point)' in parser
    assert 'as_i64()?.try_into().ok()?' in parser
    assert 'fallback_cursor: native_cursor.or(js_cursor)' in parser
    assert 'TabDetachIntent::DragOut' in parser
    assert 'TabDetachIntent::ContextMenu' in parser

    transfer = native.split('fn move_live_tab(source: &mut TpBrowserApp, target: &mut TpBrowserApp, tab_id: u64, source_is_primary: bool) -> bool {', 1)[1].split('fn detach_from_primary', 1)[0]
    assert 'tab.id == tab_id && tab.worker_kind.is_none()' in transfer
    assert 'target.tabs.len() >= 14 || target.tabs.iter().any(|tab| tab.id == tab_id)' in transfer
    assert 'webview.reparent(target.window.hwnd())' in transfer
    assert 'if source.tabs[source_index].webview.reparent(target.window.hwnd()).is_err() {' in transfer
    assert 'let mut tab = source.tabs.remove(source_index);' in transfer
    assert 'target.tabs.push(tab);' in transfer
    assert 'target.tabs.clear();' not in transfer
    assert 'target.active_idx = target.tabs.len() - 1;' in transfer
    assert 'source.tabs.iter().position(|tab| tab.worker_kind.is_none())' in transfer
    assert 'else if source_is_primary {' in transfer
    assert 'source.save_session();' in transfer
    assert 'if target.persist_session {' in transfer
    assert 'target.save_session();' in transfer
    assert 'target.active_workspace_id = tab.workspace_id.clone();' in transfer
    assert 'tab.workspace_id = target.active_workspace_id.clone();' in transfer
    assert '.load_url(' not in transfer
    assert '.with_url(' not in transfer
    assert 'windows.close_secondary(target_id);' in native
    assert 'A reparented BrowserTab keeps Wry callbacks and IPC closures' in native
    assert 'origin_tab_id: Option<u64>' in native
    assert 'fn for_tab(&self, tab_id: u64) -> Self' in native
    assert 'let proxy_ipc = self.proxy.for_tab(tab_id);' in native
    assert 'let proxy_popup = self.proxy.for_tab(tab_id);' in native
    assert 'let proxy_load = self.proxy.for_tab(tab_id);' in native
    assert 'let proxy_download = self.proxy.for_tab(tab_id);' in native
    assert 'let proxy_download_done = self.proxy.for_tab(tab_id);' in native
    assert 'self.proxy.for_tab(tab_id)' in native
    assert 'Event::UserEvent(AppEvent::WindowCommand { window_id, origin_tab_id, command: cmd })' in native


def test_detach_hit_tests_managed_windows_then_stages_monitor_aware_fallback():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    tab_shell = _tab_shell()

    detach = native.split('Event::UserEvent(AppEvent::WindowCommand { window_id, command: AppCmd::DetachTabToNewWindow { tab_id, intent, fallback_cursor }, .. }) => {', 1)[1].split('Event::UserEvent(AppEvent::WindowCommand { window_id, origin_tab_id, command: cmd }) =>', 1)[0]
    assert 'detach_in_flight: HashSet<(WindowId, u64)>' in native
    assert 'fn begin_detach(&mut self, source_id: WindowId, tab_id: u64) -> bool' in native
    assert 'if !eligible || !windows.begin_detach(window_id, tab_id)' in detach
    assert 'tab.id == tab_id && tab.worker_kind.is_none()' in detach
    assert 'windows.managed_window_at(&app, cursor)' in detach
    assert 'if let Some(target_id) = existing_target' in detach
    assert 'windows.create_secondary(' in detach
    assert 'size, true, fallback_cursor' in detach
    assert detach.index('if let Some(target_id) = existing_target') < detach.index('windows.create_secondary(')
    assert 'windows.finish_detach(window_id, tab_id);' in detach
    assert 'function requestTabDetach(tabId, intent, event)' in tab_shell
    assert 'if (!tabId || detachRequests[tabId]) return;' in tab_shell
    assert tab_shell.count('requestTabDetach(') == 3

    create = native.split('fn create_secondary(', 1)[1].split('fn close_secondary', 1)[0]
    assert 'position: Option<WinPoint>' in create
    assert 'with_position(PhysicalPosition::new(position.x, position.y))' in create
    assert 'fn clamp_secondary_position(cursor: WinPoint, size: (u32, u32)) -> WinPoint' in native
    assert 'MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST)' in native
    assert 'cursor.x.clamp(info.work.left, max_x)' in native
    assert 'GetAncestor(hit, 2)' in native


def test_detach_intent_makes_source_window_drag_a_noop_but_keeps_context_menu_detach():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    tab_shell = _tab_shell()
    titlebar = _titlebar()

    request = tab_shell.split('function requestTabDetach(tabId, intent, event) {', 1)[1].split('function showDetachHint', 1)[0]
    assert 'intent: intent' in request
    assert 'requestTabDetach(tab.id, "context_menu");' in tab_shell
    assert 'requestTabDetach(t.id, "drag_out", event);' in tab_shell
    assert 'intent: "drag_out"' in titlebar

    detach = native.split('Event::UserEvent(AppEvent::WindowCommand { window_id, command: AppCmd::DetachTabToNewWindow', 1)[1].split('Event::UserEvent(AppEvent::WindowCommand { window_id, origin_tab_id, command: cmd })', 1)[0]
    assert 'let hovered_managed_window = fallback_cursor' in detach
    assert 'if intent == TabDetachIntent::DragOut && hovered_managed_window == Some(window_id)' in detach
    assert detach.index('hovered_managed_window == Some(window_id)') < detach.index('windows.create_secondary(')
    assert 'TabDetachIntent::ContextMenu' in native


def test_transfer_routes_every_browser_tab_callback_and_preserves_shell_commands():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")

    proxy = native.split('struct WindowProxy {', 1)[1].split('const DEFAULT_WORKSPACE_ID', 1)[0]
    assert 'origin_tab_id: Option<u64>' in proxy
    assert 'fn for_tab(&self, tab_id: u64) -> Self' in proxy
    assert 'origin_tab_id: self.origin_tab_id' in proxy

    dispatch = native.split('Event::UserEvent(AppEvent::WindowCommand { window_id, origin_tab_id, command: cmd }) => {', 1)[1].split('let dur = t_evt.elapsed()', 1)[0]
    assert 'if let Some(tab_id) = origin_tab_id {' in dispatch
    assert 'app.tabs.iter().any(|tab| tab.id == tab_id)' in dispatch
    assert 'windows.secondary.values_mut().find' in dispatch
    assert 'else if window_id == windows.primary_window_id {' in dispatch
    assert 'else if let Some(secondary) = windows.secondary.get_mut(&window_id)' in dispatch


def test_secondary_creation_uses_physical_dimensions_and_propagates_failure():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    create = native.split('fn create_secondary(', 1)[1].split('fn close_secondary', 1)[0]
    new_window = native.split('command: AppCmd::NewWindow, .. }) => {', 1)[1].split('command: AppCmd::DetachTabToNewWindow', 1)[0]

    assert 'PhysicalSize::new(size.0.max(860), size.1.max(560))' in create
    assert 'with_min_inner_size(PhysicalSize::new(860, 560))' in create
    assert 'with_position(PhysicalPosition::new(position.x, position.y))' in create
    assert 'builder.build(target)\n            .map_err' in create
    assert '.expect("Failed to create secondary TPbrowser window")' not in create
    assert 'let size = app.window.inner_size();' in new_window
    assert 'to_logical::<u32>' not in new_window


def test_secondary_staging_is_hidden_lightweight_and_cleanup_is_explicit():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")

    create = native.split('fn create_secondary(', 1)[1].split('fn close_secondary', 1)[0]
    assert '.with_visible(!staging)' in create
    assert '.with_inner_size(PhysicalSize::new(size.0.max(860), size.1.max(560)))' in create
    assert '.with_min_inner_size(PhysicalSize::new(860, 560))' in create
    assert 'builder.build(target)\n            .map_err(|err| wry::Error::Io' in create
    assert 'TpBrowserApp::new(window, proxy, exe_dir, false, true)?' in create
    assert 'if !staging {' in create
    assert 'app.add_tab("tp://newtab", false);' in create

    constructor = native.split('fn new(', 1)[1].split('fn ensure_fb_shell', 1)[0]
    assert 'secondary_runtime: bool' in constructor
    assert 'if !secondary_runtime {' in constructor
    protected = constructor.split('if !secondary_runtime {', 1)[1].split('Ok(app)', 1)[0]
    for prohibited_staging_service in ('app.init_system_tray();', 'app.load_session_or_default();', 'app.spawn_compact_sidebar_watcher();'):
        assert prohibited_staging_service in protected

    cleanup = native.split('fn close_secondary(&mut self, window_id: WindowId) -> bool {', 1)[1].split('fn begin_detach', 1)[0]
    for cleanup_step in ('app.remove_system_tray();', 'app.tabs.clear();', 'app.fb_shell = None;', 'app.glance_body_shell = None;'):
        assert cleanup_step in cleanup
    assert 'target.window.set_visible(true);' in native


def test_existing_window_transfer_covers_all_directions_and_closes_empty_secondary():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    detach = native.split('let moved = if window_id == windows.primary_window_id {', 1)[1].split('if !moved {', 1)[0]

    assert 'windows.detach_from_primary(&mut app, target_id, tab_id)' in detach
    assert 'target_id == windows.primary_window_id' in detach
    assert 'windows.detach_to_primary(&mut app, window_id, tab_id)' in detach
    assert 'windows.detach_from_secondary(window_id, target_id, tab_id)' in detach
    assert 'fn detach_to_primary' in native
    assert 'source.tabs.is_empty()' in native
    assert 'windows.close_secondary(window_id);' in native
    assert 'Every such callback carries' in native
    assert 'runtime.tabs.iter().any(|tab| tab.id == tab_id)' in native


def test_second_host_exits_even_when_wake_tcp_fails():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    startup = native.split('let mutex_name = b"TPbrowser_SingleInstance_Mutex\\0";', 1)[1].split('let is_silent =', 1)[0]

    assert 'if handle != 0 && GetLastError() == 183 {' in startup
    assert 'if let Ok(mut stream) = std::net::TcpStream::connect("127.0.0.1:17777") {' in startup
    assert startup.rfind('std::process::exit(0);') > startup.rfind('if let Ok(mut stream)')


def test_titlebar_popover_and_url_dropdown_contract():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    titlebar = _titlebar()

    # Native Rust HWND expansion contracts
    assert "titlebar_popover_open: bool" in native
    assert "OpenTitlebarPopover" in native
    assert "CloseTitlebarPopover" in native
    assert 'else if self.titlebar_popover_open {' in native
    assert 'logical.height.min(380).max(tb_h)' in native
    assert 'cmd\\":\\"open_titlebar_popover\\"' in native
    assert 'cmd\\":\\"close_titlebar_popover\\"' in native
    assert '"pinned_extensions"' in native

    # Titlebar HTML DOM & IPC contracts
    assert 'id="tb_url_dropdown"' in titlebar
    assert 'class="tb-url-dropdown"' in titlebar
    assert 'id="tb_url_suggestions"' not in titlebar
    assert 'cmd: "open_titlebar_popover"' in titlebar
    assert 'cmd: "close_titlebar_popover"' in titlebar
    assert 'function openUrlDropdown()' in titlebar
    assert 'function closeUrlDropdown()' in titlebar
    assert 'function renderUrlDropdown(' in titlebar
    assert 'key: "pinned_extensions"' in titlebar


def test_horizontal_urlbar_phase1_debounces_and_ranks_category_suggestions():
    titlebar = _titlebar()

    assert 'var urlDropdownRenderTimer = null;' in titlebar
    assert 'setTimeout(function() {' in titlebar
    assert '}, 80);' in titlebar
    assert 'scheduleUrlDropdownRender(omniInp.value);' in titlebar
    assert 'function urlSuggestionScore(query, title, url, recentRank)' in titlebar
    assert 'if (host.indexOf(q) === 0) score += 140;' in titlebar
    assert 'if (loweredTitle.indexOf(q) === 0) score += 100;' in titlebar
    assert 'items.sort(function(a, b) { return b.score - a.score' in titlebar
    for badge in ('badge: "Đang mở"', 'badge: "Bookmark"', 'badge: "Lịch sử"'):
        assert badge in titlebar


def test_vertical_urlbar_phase2_uses_state_backed_ranked_dropdown_without_rail_reflow():
    tab_shell = _tab_shell()

    for hook in ('id="v_url_form"', 'id="v_url_dropdown"', 'class="v-url-dropdown"', 'row.className = "v-url-row"'):
        assert hook in tab_shell
    assert '.v-url-wrap { position: relative; min-width: 0; }' in tab_shell
    assert 'position: absolute;' in tab_shell
    assert 'top: calc(100% + 6px);' in tab_shell
    assert 'right: 0;' in tab_shell
    assert 'z-index: 80;' in tab_shell
    assert 'max-height: min(360px, calc(100vh - 116px));' in tab_shell
    assert 'function renderVerticalUrlDropdown(query)' in tab_shell
    assert 'var state = lastShellState || {};' in tab_shell
    assert '(state.tabs || []).forEach' in tab_shell
    assert '(state.bookmarks || []).forEach' in tab_shell
    assert '(state.history || []).slice().reverse().forEach' in tab_shell
    assert 'badge: "Đang mở"' in tab_shell
    assert 'badge: "Bookmark"' in tab_shell
    assert 'badge: "Lịch sử"' in tab_shell
    assert 'items.sort(function(a, b) { return b.score - a.score' in tab_shell


def test_vertical_urlbar_phase2_debounces_classifies_and_keeps_native_ipc_paths():
    tab_shell = _tab_shell()

    assert 'var verticalUrlDropdownTimer = null;' in tab_shell
    assert 'function scheduleVerticalUrlDropdownRender(query)' in tab_shell
    assert '}, 80);' in tab_shell
    assert 'function verticalUrlInputLooksLikeAddress(input)' in tab_shell
    assert r'/^[a-z][a-z0-9+.-]*:\/\//i.test(value)' in tab_shell
    assert 'host === "localhost" || host.indexOf("localhost:") === 0' in tab_shell
    assert 'ipv4.length === 4' in tab_shell
    assert 'function verticalUrlSuggestionScore(query, title, url, recentRank)' in tab_shell
    assert 'event.key === "ArrowDown" || event.key === "ArrowUp"' in tab_shell
    assert 'event.key === "Enter"' in tab_shell
    assert 'event.key === "Escape"' in tab_shell
    assert 'cmd: "focus_tab_by_id", tab_id: item.tab_id' in tab_shell
    assert 'cmd: "navigate_active", url: item.url' in tab_shell
    assert 'cmd: "run_ai_prompt", prompt: item.prompt' in tab_shell
    assert 'sendRust({ cmd: "open_chat_channel", channel: "ai" });' in tab_shell


def test_horizontal_urlbar_phase1_classifies_addresses_and_keeps_native_copy_ipc():
    titlebar = _titlebar()
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")

    assert 'function urlInputLooksLikeAddress(input)' in titlebar
    assert r'/^[a-z][a-z0-9+.-]*:\/\//i.test(value)' in titlebar
    assert 'host === "localhost" || host.indexOf("localhost:") === 0' in titlebar
    assert 'ipv4.length === 4' in titlebar
    assert 'navigator.clipboard' not in titlebar
    assert 'sendRust({ cmd: "copy_to_clipboard", text: val });' in titlebar
    assert 'fn looks_like_web_address(input: &str) -> bool' in native
    assert "let host = input.split(['/', '?', '#']).next().unwrap_or(\"\");" in native
    assert 'hostname.eq_ignore_ascii_case("localhost")' in native
    assert 'part.parse::<u8>().is_ok()' in native
    assert 'if trimmed.contains("://") {' in native
    assert 'if Self::looks_like_web_address(trimmed) {' in native


def test_zen_translations_glance_and_autopip_contract():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    titlebar = _titlebar()
    content_agent = (ROOT / "src/content_agent.js").read_text(encoding="utf-8")
    settings = (ROOT / "src/settings_modal.html").read_text(encoding="utf-8")

    # 1. Native Rust contracts
    assert "translate_target_lang" in native
    assert "translate_auto_prompt" in native
    assert "translate_never_domains" in native
    assert "glance_trigger" in native
    assert "glance_external_from_pinned" in native
    assert "default_search_engine" in native
    assert "auto_pip_on_switch" in native
    assert "TranslateReq" in native
    assert "TranslateRes" in native
    assert "__tpAutoPip" in native
    assert "normalize_url_with_engine" in native

    # 2. Titlebar HTML translation bridge
    assert "window.__tpExecuteTranslation" in titlebar
    assert "translate.googleapis.com" in titlebar
    assert 'cmd: "translate_res"' in titlebar

    # 3. Content agent DOM & Auto PiP contracts (No innerHTML!)
    assert ".innerHTML" not in content_agent
    assert ".outerHTML" not in content_agent
    assert "insertAdjacentHTML" not in content_agent
    assert "window.__tpAutoPip" in content_agent
    assert "window.__tpOnTranslated" in content_agent
    assert "translateFullPage" in content_agent
    assert "createTreeWalker" in content_agent
    assert 'cmd: "translate_req"' in content_agent
    assert "isGlanceTriggerPressed" in content_agent
    assert "openGlanceFromAltClick" in content_agent

    # 4. Settings Modal UI controls
    assert 'id="seg_lang_vi"' in settings
    assert 'id="sw_translate_auto"' in settings
    assert 'id="inp_translate_never"' in settings
    assert 'id="seg_glance_alt"' in settings
    assert 'id="sw_glance_external"' in settings
    assert 'id="seg_se_google"' in settings
    assert 'id="sw_auto_pip"' in settings
    assert 'type="checkbox"' not in settings


def test_youtube_telemetry_requests_are_not_blocked_by_local_rules():
    content_agent = (ROOT / "src/content_agent.js").read_text(encoding="utf-8")
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    vie_rules = (ROOT / "extensions/uBOLite.chromium/rulesets/main/vie-1.json").read_text(encoding="utf-8")
    assert "releaseYouTubeClickSurface" in content_agent
    assert "data-tp-youtube-overlay-released" in content_agent
    assert 'frame.closest("#movie_player, #masthead, ytd-app")' not in content_agent
    assert 'querySelectorAll("iframe, [style*=\'position\'], [class], [id]")' not in content_agent
    assert "tp-yt-iron-overlay-backdrop" in content_agent
    assert "AppCmd::FocusContentTab" in native
    assert "pos != self.active_idx" in native
    assert "youtube.com/youtubei/v1/log_event" in vie_rules
    assert "youtube.com/api/stats/qoe" in vie_rules
    assert "youtube.com/ptracking" in vie_rules


def test_mcp_auth_middleware_and_task_selection_json_contract():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    mcp = (ROOT / "src/mcp_server.rs").read_text(encoding="utf-8")
    tunnel = (ROOT / "scripts/start_tunnel.bat").read_text(encoding="utf-8")

    # C1: MCP HTTP server Bearer token & proxy enforcement
    assert "mcp_auth_token" in native
    assert "TP_MCP_TOKEN" in native
    assert "cf-connecting-ip" in mcp.lower()
    assert "x-forwarded-for" in mcp.lower()
    assert "WWW-Authenticate: Bearer" in mcp
    assert "401" in mcp
    assert "mcp_auth_token" in tunnel

    # C2: CreateTaskFromSelection uses JsonVal::obj instead of manual format escaping
    selection_arm = native.split("AppCmd::CreateTaskFromSelection { text, source, contact } =>", 1)[1].split("AppCmd::ScanZaloChat =>", 1)[0]
    assert "JsonVal::obj" in selection_arm
    assert "Self::escape_json" not in selection_arm


def test_bookmark_folders_tags_and_keywords_contract():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    titlebar = _titlebar()
    tab_shell = _tab_shell()
    content_agent = (ROOT / "src/content_agent.js").read_text(encoding="utf-8")
    newtab = (ROOT / "src/newtab.html").read_text(encoding="utf-8")

    assert "fn normalize_bookmarks_json" in native
    assert '"folder"' in native and '"tags"' in native and '"keyword"' in native
    assert "meta.keyword" in titlebar and "meta.tags" in titlebar and "meta.folder" in titlebar
    assert "meta.keyword" in tab_shell and "meta.tags" in tab_shell and "meta.folder" in tab_shell
    assert "__tp_col_folders" in content_agent and "__tp_col_editor" in content_agent
    assert ".innerHTML" not in content_agent
    assert "bm_folder_inp" in newtab and "bm_tags_inp" in newtab and "bm_keyword_inp" in newtab


def test_zen_boosts_and_zap_contract():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    content_agent = (ROOT / "src/content_agent.js").read_text(encoding="utf-8")
    tab_shell = _tab_shell()
    titlebar = _titlebar()
    settings = (ROOT / "src/settings_modal.html").read_text(encoding="utf-8")

    assert "fn normalize_boosts_json" in native
    assert "AppCmd::SaveDomainBoost" in native
    assert "AppCmd::ResetDomainBoost" in native
    assert "AppCmd::OpenBoostsPanel" in native
    assert "AppCmd::StartZapMode" in native
    assert "__tp_boost_css" in content_agent
    assert "window.__tpOpenBoosts" in content_agent
    assert "window.__tpStartZapMode" in content_agent
    assert ".innerHTML" not in content_agent
    assert "open_boosts_panel" in tab_shell and "start_zap_mode" in tab_shell
    assert "open_boosts_panel" in titlebar and "start_zap_mode" in titlebar
    assert "boosts_manager_list" in settings

def test_secondary_staging_defers_auxiliary_shells_until_transfer_commits():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")

    constructor = native.split('fn new(', 1)[1].split('fn ensure_optional_shells', 1)[0]
    helper = native.split('fn ensure_optional_shells(&mut self)', 1)[1].split('fn hydrate_secondary_shells', 1)[0]
    transfer = native.split('fn move_live_tab(source: &mut TpBrowserApp, target: &mut TpBrowserApp, tab_id: u64, source_is_primary: bool) -> bool {', 1)[1].split('fn detach_from_primary', 1)[0]
    for shell in (
        "chat_header_shell",
        "ai_shell",
        "dock_resizer_shell",
        "sidebar_resizer_shell",
        "settings_shell",
    ):
        assert f"{shell}: Option<WebView>" in native
        assert f"if self.{shell}.is_none()" in helper
    assert "optional_shells_enabled: !secondary_runtime" in constructor
    assert "if !self.optional_shells_enabled {" in helper
    hydrate = native.split('fn hydrate_secondary_shells(&mut self)', 1)[1].split('fn ensure_fb_shell', 1)[0]
    assert "self.optional_shells_enabled = true;" in hydrate
    assert "self.ensure_optional_shells();" not in hydrate
    assert "WebViewBuilder" not in hydrate
    assert "target.hydrate_secondary_shells();" in transfer
    assert transfer.index("target.tabs.push(tab);") < transfer.index("target.hydrate_secondary_shells();")
    assert "TpBrowserApp::new(window, proxy, exe_dir, false, true)?" in native


def test_optional_secondary_shells_are_ensured_before_secondary_feature_entry_and_cleaned_up():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")

    assert "trait OptionalWebViewExt" in native
    assert "impl OptionalWebViewExt for Option<WebView>" in native
    assert "self.as_ref().map_or(Ok(())," in native
    for command in (
        "AppCmd::OpenChatChannel(ch) => {",
        "AppCmd::ToggleChatChannel(ch) => {",
        "AppCmd::OpenSettingsModal(section) => {",
        "AppCmd::FocusAiPanel => {",
        "AppCmd::StartDockResize => {",
        "AppCmd::StartSidebarResize => {",
    ):
        section = native.split(command, 1)[1].split("}\n            AppCmd::", 1)[0]
        assert "self.ensure_optional_shells();" in section
    cleanup = native.split('fn close_secondary(&mut self, window_id: WindowId) -> bool {', 1)[1].split('fn begin_detach', 1)[0]
    for shell in (
        "chat_header_shell",
        "ai_shell",
        "dock_resizer_shell",
        "sidebar_resizer_shell",
        "settings_shell",
    ):
        assert f"app.{shell} = None;" in cleanup
    assert '"Settings WebView unavailable"' in native
    assert '"AI WebView unavailable"' in native


def test_secondary_lazy_shell_benchmark_hooks_are_present():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")

    assert 'Self::log_perf("secondary hydrate optional shells"' not in native
    for label in (
        "secondary hydrate chat header shell",
        "secondary hydrate AI shell",
        "secondary hydrate dock resizer shell",
        "secondary hydrate sidebar resizer shell",
        "secondary hydrate settings shell",
    ):
        assert f'Self::log_perf("{label}"' in native
    assert 'if staging { "secondary staging" } else { "secondary ready" }' in native
    assert 'TP_DEBUG_PERF' in native


def test_window_sync_and_recovery_contract():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    mcp = (ROOT / "src/mcp_server.rs").read_text(encoding="utf-8")
    settings = (ROOT / "src/settings_modal.html").read_text(encoding="utf-8")

    assert "struct RecoverySnapshot" in native
    assert "fn clamp_window_bounds_to_work_area" in native
    assert "fn serialize_recovery_snapshot" in native
    assert "fn parse_recovery_snapshot" in native
    assert "AppCmd::RecoverWindows" in native
    assert "AppCmd::SyncWindowGeometry" in native
    assert "WindowEvent::Moved(_)" in native
    assert "WindowEvent::ScaleFactorChanged" in native
    assert "window_recovery.json" in native
    assert "tpbrowser://windows" in native and "tpbrowser://windows" in mcp
    assert "tp_recover_windows" in native and "tp_recover_windows" in mcp
    assert "tp_window_recovery_status" in native and "tp_window_recovery_status" in mcp
    assert "btn_recover_windows" in settings and "recover_windows" in settings
    assert "btn_sync_window_geometry" in settings and "sync_window_geometry" in settings
    assert '<input type="checkbox"' not in settings


def test_newtab_fast_path_and_prewarm_contract():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    newtab = (ROOT / "src/newtab.html").read_text(encoding="utf-8")
    titlebar = (ROOT / "src/titlebar.html").read_text(encoding="utf-8")
    content_agent = (ROOT / "src/content_agent.js").read_text(encoding="utf-8")

    assert "struct WarmStandbyTab" in native
    assert "AppCmd::PrewarmNewTab" in native
    assert "fn ensure_warm_newtab(&mut self)" in native
    assert "fn cached_newtab_html(" in native
    assert "fn mark_session_dirty(&self)" in native
    assert "last_tb_bookmarks_rev" in native and "last_tb_history_rev" in native
    assert "Object.assign({}, lastSyncState, st)" in titlebar
    assert "IS_INTERNAL_HTML_PAGE" in content_agent
    assert "backdrop-filter: blur(" not in newtab
    assert 'window.Motion.animate(".clock-hero"' not in newtab


def test_zen_context_menu_customization_and_surfaces_contract():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    tab_shell = _tab_shell()
    titlebar = _titlebar()
    content_agent = (ROOT / "src/content_agent.js").read_text(encoding="utf-8")
    settings = (ROOT / "src/settings_modal.html").read_text(encoding="utf-8")

    assert "fn normalize_context_menu_prefs" in native
    assert "fn clean_tracking_url" in native
    assert '"context_menu_prefs"' in native
    assert "cleanTrackingUrl(" in tab_shell and "getZenContextPrefs()" in tab_shell
    assert 'id="tb_tab_context_menu"' in titlebar
    assert "openHorizTabContextMenu(" in titlebar
    assert "open_titlebar_popover" in titlebar and "close_titlebar_popover" in titlebar
    assert "openZenWebContextMenu" in content_agent
    assert "shouldBypassZenWebContextMenu" in content_agent
    assert "detectContextTargets" in content_agent
    assert 'window.addEventListener("contextmenu"' in content_agent
    assert 'window.addEventListener("blur"' in tab_shell
    assert ".innerHTML" not in content_agent
    assert "grp_zen_context_menu" in settings and "sw_ctx_hide_separators" in settings
    assert '<input type="checkbox"' not in settings
    assert "__tp_web_sel_pill" not in content_agent
    assert "Lập Plan Cowork" not in content_agent
    assert "__tp_chat_sel_pill" in native


def test_centered_command_bar_and_ctrl_tab_hud_contract():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    tab_shell = _tab_shell()
    content_agent = (ROOT / "src/content_agent.js").read_text(encoding="utf-8")

    # Transparent single-host tab_shell overlay + backend MRU HUD state
    assert "mru_hud_open" in native
    assert ".with_transparent(true)" in native
    assert "StepMruTab(i32)" in native
    assert "CommitMruTab(Option<u64>)" in native
    assert "CancelMruTab" in native
    assert 'step_mru_tab' in native
    assert 'commit_mru_tab' in native
    assert 'cancel_mru_tab' in native and 'cancel_mru_tab' in tab_shell

    # Ctrl+Tab / Ctrl+Shift+Tab + Control release across SHELL_SHORTCUTS_JS & content_agent.js
    assert 'cmd: "step_mru_tab"' in native and 'cmd: "commit_mru_tab"' in native
    assert 'cmd: "step_mru_tab"' in content_agent and 'cmd: "commit_mru_tab"' in content_agent
    # Keep Ctrl+PageDown / Ctrl+PageUp contracts intact
    assert '"next_tab"' in native and '"prev_tab"' in native

    # Centered Command Bar + Ctrl+Tab Switcher HUD markup, styles, and pin behavior
    assert "body.overlay-fullscreen.mode-vertical #vertical_root" in tab_shell
    assert 'id="ctrl_tab_hud_overlay"' in tab_shell
    assert "window.__tpOpenMruSwitcher" in tab_shell
    assert "window.__tpCloseMruSwitcher" in tab_shell
    assert "toggle_ui_theme" in tab_shell and "recover_windows" in tab_shell
    assert ".innerHTML" not in content_agent


def test_better_find_bar_counter_case_and_theme_contract():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    content_agent = (ROOT / "src/content_agent.js").read_text(encoding="utf-8")

    # Light & Dark theme support synced from ToggleFindBar and state.ui_theme
    assert "window.__tpToggleFindBar(undefined" in native
    assert "#__tp_bubble_root[data-theme='carrara-light'] #__tp_find_bar" in content_agent
    assert "#__tp_find_bar.no-match #__tp_find_count" in content_agent

    # Match count badge, Aa match-case toggle, CSS Custom Highlight API + zero innerHTML
    assert 'id: "__tp_find_count"' in content_agent
    assert 'id: "__tp_find_case"' in content_agent
    assert "collectFindMatches(" in content_agent
    assert "CSS.highlights.set(\"tp-find\"" in content_agent
    assert "CSS.highlights.set(\"tp-find-active\"" in content_agent
    assert "window.__tpCloseFindBar" in content_agent
    assert ".innerHTML" not in content_agent


def test_extension_popup_and_dashboard_integration_contract():
    native = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    titlebar = _titlebar()
    tab_shell = _tab_shell()

    assert "ProfileGetBrowserExtensionsCompletedHandler" in native
    assert "ICoreWebView2Profile7" in native
    assert "GetBrowserExtensions" in native
    assert "BrowserExtensionsLoaded" in native
    assert "OpenExtensionPopup" in native
    assert "open_extension" in native and "open_extension" in titlebar and "open_extension" in tab_shell
    assert "popup.html" in native and "dashboard.html" in native
    assert "chrome-extension://" in native
    assert "mhjfbmdgcfjbbpaeojofohoefgiehjai" in native
