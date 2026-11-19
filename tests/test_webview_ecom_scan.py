import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_tiktok_scan_uses_webview_worker_without_alternate_engine():
    source = (ROOT / 'src/main.rs').read_text(encoding='utf-8')
    scan = source.split('AppCmd::RunEcomScan { scope } => {', 1)[1].split(
        'AppCmd::SetEcomAutoScan(enabled) => {', 1
    )[0]
    assert 'if run_tiktok {' in scan
    assert 'self.ensure_ecom_worker(' in scan
    assert 'self.start_tiktok_sequence()' in scan
    assert 'camoufox' not in source.lower()
    assert 'camoufox' not in (ROOT / 'src/settings_modal.html').read_text(encoding='utf-8').lower()


def _tiktok_scan_js(source):
    return source.split('const TIKTOK_PASSIVE_SCAN_JS: &str = r##"', 1)[1].split('"##;', 1)[0]


def test_tiktok_ads_envelope_contract_is_explicit_and_backward_compatible():
    js = _tiktok_scan_js((ROOT / 'src/main.rs').read_text(encoding='utf-8'))
    for status in ('"ok"', '"partial"', '"loading"', '"needs_login"', '"captcha"', '"unavailable"', '"error"'):
        assert status in js
    for key in ('schema:', 'source_url:', 'captured_at:', 'ts:', 'period:', 'period_label:', 'metrics:',
                'campaigns_provenance:', 'provenance:', 'raw:', 'source:'):
        assert key in js
    # Legacy fields remain for existing consumers.
    for legacy in ('logged_in:', 'need_login:', 'captcha_active:', 'cost_ads:', 'sales_ads:', 'roas_ads:',
                   'orders_ads:', 'campaigns:', 'cmd: "update_platform_data"', 'platform: "tiktok"'):
        assert legacy in js


def test_tiktok_ads_collector_is_passive_and_never_zero_fills():
    js = _tiktok_scan_js((ROOT / 'src/main.rs').read_text(encoding='utf-8'))
    for forbidden in ('.click(', '.submit(', 'dispatchEvent', 'location.href =', 'location.assign',
                      'location.replace', 'fetch(', 'XMLHttpRequest'):
        assert forbidden not in js
    assert 'if (!s) return 0;' not in js
    assert 'isNaN(v) ? 0 : v' not in js
    assert 'GMV Max Toàn Cửa Hàng' not in js
    # Login/CAPTCHA are re-checked right before posting and send empty metrics.
    assert js.count('gateStatus()') >= 3
    assert 'function sendGate(gate)' in js


def test_workspace_renders_tiktok_status_without_breaking_ads_table():
    html = (ROOT / 'src/workspace.html').read_text(encoding='utf-8')
    for anchor in ('id="tt_sales"', 'id="tt_kpi_sub"', 'id="tt_ts"', 'id="tt_captcha_box"', 'id="tt_tbody"',
                   "onclick=\"runScan('tiktok')\"", 'id="tt_status_pill"', 'id="tt_provenance_tbody"'):
        assert anchor in html
    assert 'function ttStatusOf(tt)' in html
    assert 'function renderTikTokProvenance(tt, status)' in html
    assert 'restored_from_disk' in html
    assert 'Không xác định' in html
    assert 'fmtVnd(tt.sales_ads || 0)' not in html
    assert '(tt.orders_ads || 0)' not in html


def test_restored_tiktok_cache_is_marked_stale():
    source = (ROOT / 'src/main.rs').read_text(encoding='utf-8')
    loader = source.split('fn load_exports_cache(&mut self) {', 1)[1].split('fn save_export_file', 1)[0]
    assert 'mark_tiktok_cache_restored(&s)' in loader
    handler = source.split('"tiktok" => {', 1)[1].split('_ => {}', 1)[0]
    assert 'tiktok_status_messages(tiktok_ads_status(&data_json))' in handler
    assert 'captcha_active\\":true' not in handler
    restore = source.split('fn mark_tiktok_cache_restored(', 1)[1].split('\n}\n', 1)[0]
    assert '"status" if v.as_str() == Some("ok") => *v = JsonVal::Str("partial".to_string())' in restore
    assert '("is_today".to_string(), JsonVal::Bool(false))' in restore


def test_workspace_never_shows_restored_cache_as_today():
    html = (ROOT / 'src/workspace.html').read_text(encoding='utf-8')
    assert 'function ttViewStatus(tt)' in html
    assert '(s === "ok" && ttIsStale(tt)) ? "partial" : s' in html
    assert 'function ttIsToday(tt) { return !ttIsStale(tt) &&' in html
    render = html.split('if (st.tiktok) {', 1)[1].split('if (Array.isArray(st.logs))', 1)[0]
    assert 'ttViewStatus(tt)' in render and 'ttStatusOf(tt)' not in render
    assert 'TT_STALE_SUFFIX' in render
    report = html.split('"6) TIKTOK ADS [', 1)[1].split('\n', 1)[0]
    assert 'ttViewStatus(' in report and 'ttIsToday(tt)' in report and 'TT_STALE_SUFFIX' in report
    assert 'tt.period.is_today === true) ? (tt.period_label' not in report


def test_tiktok_collector_guards_async_steps_and_restores_scroll():
    js = _tiktok_scan_js((ROOT / 'src/main.rs').read_text(encoding='utf-8'))
    assert 'setTimeout(guarded(extractAndSend, "retry"), 1100)' in js
    assert 'setTimeout(guarded(sweepNext, "sweep"), 220)' in js
    assert 'guarded(extractAndSend, "extract")()' in js
    assert 'setTimeout(extractAndSend' not in js and 'setTimeout(sweepNext' not in js
    assert 'try { fn(); } catch (err) { postError(err, where); }' in js
    post_error = js.split('function postError(err, where) {', 1)[1].split('\n  }\n', 1)[0]
    assert 'restoreScroll();' in post_error and 'envelope("error"' in post_error
    assert 'scrollGuard = { el: scroller, left: scroller.scrollLeft };' in js
    finalize = js.split('function finalizeAndSend(', 1)[1].split('\n    }\n', 1)[0]
    assert finalize.lstrip().split('\n', 1)[1].lstrip().startswith('restoreScroll();')


def test_derived_metrics_force_partial_and_are_labelled():
    js = _tiktok_scan_js((ROOT / 'src/main.rs').read_text(encoding='utf-8'))
    finalize = js.split('function finalizeAndSend(', 1)[1].split('\n    }\n', 1)[0]
    assert 'indexOf("derived:") === 0' in finalize
    assert 'reasons.push(k + "_derived")' in finalize
    assert 'derived.length === 0 && campProv.source !== "unavailable" && pd.is_today === true' in finalize
    assert 'p.derived_metrics = derived;' in finalize
    assert 'derived_fields: derivedFields' in js
    html = (ROOT / 'src/workspace.html').read_text(encoding='utf-8')
    assert 'var TT_DERIVED_SUFFIX = " (suy ra)";' in html
    render = html.split('if (st.tiktok) {', 1)[1].split('if (Array.isArray(st.logs))', 1)[0]
    for key in ('"sales"', '"cost"', '"orders"', '"roas"'):
        assert 'ttMetric(tt, ' + key in render
        assert 'ttCampVal(c, ' + key in render
    assert 'SUY RA • ' in html


def test_tiktok_health_uses_one_tab_and_separate_state():
    source = (ROOT / 'src/main.rs').read_text(encoding='utf-8')
    html = (ROOT / 'src/workspace.html').read_text(encoding='utf-8')
    assert 'const TIKTOK_HEALTH_SCAN_JS' in source
    assert 'platform: "tiktok_health"' in source
    assert 'tiktok_health_latest.json' in source
    assert 'tiktok_health_json' in source
    assert 'self.continue_tiktok_sequence(' in source
    assert 'health-center?shop_region=VN' in source
    assert 'health-center/experience-score?shop_region=VN' in source
    assert 'id="tt_health_card"' in html
    assert 'id="tt_health_tbody"' in html
    assert 'lastData.tiktok_health' in html
    assert 'read_only: true' in source[source.index('const TIKTOK_HEALTH_SCAN_JS'):]
    assert '.click(' not in source[source.index('const TIKTOK_HEALTH_SCAN_JS'):source.index('const SHOPEE_SHOP_INDEX_SCAN_JS')]


def test_ecom_workers_match_explicit_jobs_and_shopee_scan_is_in_flight_guarded():
    source = (ROOT / 'src/main.rs').read_text(encoding='utf-8')

    worker = source.split('fn ensure_ecom_worker(', 1)[1].split('\n    fn trigger_worker_scan_by_id', 1)[0]
    assert 'job: &str' in worker
    assert 't.ecom_worker_job.as_deref() == Some(job)' in worker
    assert 't.url.contains(domain_hint)' not in worker
    assert 't.ecom_worker_job = Some(job.to_string());' in worker

    shop_start = source.split('fn start_shopee_shop_sequence(&mut self) {', 1)[1].split('\n    fn open_next_shopee_shop', 1)[0]
    assert 'self.shopee_shop_scan_in_flight()' in shop_start
    assert '"shopee_shop_scanner"' in shop_start
    trigger = source.split('fn trigger_worker_scan_by_id(&mut self,', 1)[1].split('\n    fn queue_discovered_shopee_shops', 1)[0]
    assert 'Some("shopee_discovery")' in trigger

    scan = source.split('AppCmd::RunEcomScan { scope } => {', 1)[1].split(
        'AppCmd::SetEcomAutoScan(enabled) => {', 1
    )[0]
    assert 'self.shopee_shop_scan_in_flight()' in scan
    assert 'return;' in scan
    assert '"shopee_orders"' in scan
    assert '"shopee_pas"' in scan


def test_scope_all_keeps_shopee_scan_on_shopee_workers():
    source = (ROOT / 'src/main.rs').read_text(encoding='utf-8')
    scan = source.split('AppCmd::RunEcomScan { scope } => {', 1)[1].split(
        'AppCmd::SetEcomAutoScan(enabled) => {', 1
    )[0]
    assert 's == "shopee_shops_selected"' in scan
    assert 's == "shopee_shops_all"' in scan
    assert 'self.queue_discovered_shopee_shops(run_selected_shopee_shops)' in scan
    assert 'let run_tiktok = s == "tiktok";' in scan


def test_shopee_shop_scan_defers_large_workspace_sync_until_sequence_done():
    source = (ROOT / 'src/main.rs').read_text(encoding='utf-8')

    helper = source.split('fn should_defer_workspace_sync(&self)', 1)[1].split('\n    }\n', 1)[0]
    assert 'self.shopee_shop_scan_in_flight()' in helper

    sync = source.split('fn sync_workspace_view(&self) {', 1)[1].split('\n    fn sync_workspace_view_now(&self) {', 1)[0]
    assert 'if self.should_defer_workspace_sync()' in sync
    assert 'return;' in sync

    update = source.split('AppCmd::UpdatePlatformData { platform, data_json } => {', 1)[1].split(
        'AppCmd::RunEcomScan { scope } => {', 1
    )[0]
    assert 'self.sync_workspace_view();' in update

    sequence_done = source.split('let Some((shop_id, display_name)) = self.shopee_shop_scan_queue.pop() else {', 1)[1].split('};', 1)[0]
    assert 'self.shopee_shop_scan_phase = "done".to_string();' in sequence_done
    assert 'self.sync_workspace_view();' in sequence_done


def test_shopee_shop_scan_is_sequential_and_opens_exact_shop():
    source = (ROOT / 'src/main.rs').read_text(encoding='utf-8')
    html = (ROOT / 'src/workspace.html').read_text(encoding='utf-8')
    assert 'https://banhang.shopee.vn/portal/shop' in source
    assert 'const SHOPEE_SHOP_INDEX_SCAN_JS' in source
    assert 'const SHOPEE_OPEN_SHOP_JS' in source
    assert 'shopee_shop_scan_queue' in source
    assert 'returning_index' in source
    assert 'shop_orders' in source
    assert 'OpenShopeeShop' in source
    assert 'cmd: "open_shopee_shop"' in html
    assert "runShopeeShopScan('selected')" in html
    assert "runShopeeShopScan('all')" in html
    assert 'data-row-key' in source[source.index('const SHOPEE_OPEN_SHOP_JS'):source.index('const SHELL_SHORTCUTS_JS')]
    assert 'detail_action_not_found' in source


def test_workspace_shopee_cleanup_keeps_one_scan_and_shop_context():
    html = (ROOT / 'src/workspace.html').read_text(encoding='utf-8')
    common_header = html.split('<nav class="island-nav">', 1)[1].split('</nav>', 1)[0]
    shopee_header = html.split('<header class="shopee-subnav-bar">', 1)[1].split('</header>', 1)[0]

    assert 'onclick="runScan(\'all\')"' in common_header
    assert common_header.count('onclick="exportShopeeCsv()"') == 1
    assert common_header.count('onclick="copyReportText()"') == 1
    assert 'exportShopeeCsv()' not in shopee_header
    assert 'copyReportText()' not in shopee_header

    dashboard_hero = html.split('<section id="workspace-dashboard"', 1)[1].split('<div class="hero-stats">', 1)[0]
    shopee_overview = html.split('<div id="shopee-view-tong-quan"', 1)[1].split('<!-- Đơn hàng & Tác vụ gần đây -->', 1)[0]
    assert '<div id="platform_shopbar_shopee"' in html
    assert 'id="platform_shopee_chips"' in html
    for obsolete_control in ('id="workspace-search"', 'id="workspace-play-btn"',
                             'id="workspace-status-text"', 'id="workspace-progress-bar"',
                             "quickFilterTag('Flash Sale')"):
        assert obsolete_control not in shopee_overview
    for removed_hero_scan in ("runScan('shopee_orders')", "runScan('shopee_pas')", "runScan('tiktok')"):
        assert removed_hero_scan not in dashboard_hero

    for shop_anchor in ('id="shopee-shop-selection-panel"', 'id="shopee_shop_rows"',
                        'id="shopee_master_toggle"', 'discoverShopeeShops()',
                        "runShopeeShopScan('selected')", "runShopeeShopScan('all')"):
        assert shop_anchor in html
    assert 'type="checkbox" class="sp-shop-checkbox"' in html


def test_workspace_shopee_header_has_no_tp_seller_brand_and_visible_scan_cta():
    html = (ROOT / 'src/workspace.html').read_text(encoding='utf-8')
    header = html.split('<header class="shopee-subnav-bar">', 1)[1].split('</header>', 1)[0]

    assert 'TP <span>SELLER</span>' not in header
    assert 'Shopee Operations' not in header
    assert 'id="shopee_workspace_scan"' in header
    assert "onclick=\"runShopeeShopScan('selected')\"" in header
    assert "onclick=\"runShopeeShopScan('all')\"" in header
    assert 'id="shopee-sub-nav"' in header
    assert 'id="shopee-shop-pill"' in header


def test_workspace_platform_dashboard_uses_theme_tokens_and_unknown_kpis_until_sync():
    """Catch a return to a fixed-width, dark Shopee island or fabricated KPIs."""
    html = (ROOT / 'src/workspace.html').read_text(encoding='utf-8')
    for contract in ('/* Platform dashboard: theme-first, no dark island or fabricated KPIs. */',
                     '.wrap { width: min(100%, 1920px); max-width: none;',
                     '#workspace-shopee, #workspace-tiktok {',
                     'background: var(--bg-base);',
                     '.platform-kpi-grid',
                     'grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));'):
        assert contract in html
    for anchor in ('id="sp_toship"', 'id="sp_shipping_sub"', 'id="sp_anom_count"',
                   'id="sp_bad_kpi"', 'id="sp_live_kpi"', 'id="sp_pas_sales"',
                   'id="sp_discounts_list"', 'id="sp_vouchers_list"',
                   'id="sp_flashsales_list"', 'id="tt_sales"', 'id="tt_kpi_sub"',
                   'id="tt_status_pill"', 'id="tt_provenance_tbody"'):
        assert anchor in html


def test_workspace_unknown_pas_and_live_kpis_do_not_fabricate_zeroes():
    """PAS and Live remain unknown until the payload proves a numeric/status value."""
    html = (ROOT / 'src/workspace.html').read_text(encoding='utf-8')

    for contract in (
        'function hasFiniteKpiNumber(value)',
        'function hasUsableLiveSessions(live)',
        'function hasExplicitLiveStatus(live)',
        'function liveKpiValue(live)',
        'function pasVndValue(value)',
        'function pasNumberValue(value)',
        'return hasFiniteKpiNumber(value) ? fmtVnd(value) : "—";',
        'return hasFiniteKpiNumber(value) ? value : "—";',
        'pasVndValue(pas.sales_ads)',
        'pasVndValue(pas.cost_ads)',
        'pasNumberValue(pas.roas_ads)',
        'pasNumberValue(pas.orders_ads)',
        'liveKpiValue(live)',
    ):
        assert contract in html

    sync = html.split('function syncPlatformOverviewKpis(sp, pas, tt, health) {', 1)[1].split('\n    window.__tpSyncWorkspace', 1)[0]
    assert 'pas.sales_ads === undefined' not in sync
    assert 'sessions.length + " phiên"' not in sync

    pas_render = html.split('if (st.shopee_pas && activePlatformShop.shopee === "all") {', 1)[1].split('\n      if (st.tiktok)', 1)[0]
    assert 'fmtVnd(pas.sales_ads || 0)' not in pas_render
    assert 'fmtVnd(pas.cost_ads || 0)' not in pas_render
    assert '(pas.roas_ads || 0)' not in pas_render
    assert '(pas.orders_ads || 0)' not in pas_render

    live_render = html.split('var sp = resolveShopeeSnapshot(st.shopee_orders, st.shopee_shop_orders, activePlatformShop.shopee);', 1)[1].split('\n      if (st.shopee_pas && activePlatformShop.shopee === "all")', 1)[0]
    assert 'document.getElementById("sp_live_kpi").textContent = liveKpiValue(live);' in live_render
    assert 'textContent = live.is_live_now ? "ĐANG LIVE"' not in live_render


def test_workspace_tiktok_tokens_are_theme_aware_and_has_no_chartjs_cdn():
    html = (ROOT / 'src/workspace.html').read_text(encoding='utf-8')
    for contract in (
        'html[data-theme="obsidian-gold"] body[data-platform="tiktok"]',
        'html[data-theme="dark"] body[data-platform="tiktok"]',
        'html[data-theme="carrara-light"] body[data-platform="tiktok"]',
        '--gold-bright: var(--text-main);',
        '--gold-main: var(--text-sub);',
        '--cta-bg: #111111;',
        '#25f4ee',
        '#fe2c55',
    ):
        assert contract in html
    assert 'https://cdn.jsdelivr.net/npm/chart.js' not in html
    assert '<script src=' not in html


def test_shopee_overview_orders_uses_numeric_count_and_never_stringifies_order_objects():
    """The overview count must use verified scan totals, never an order-object array."""
    html = (ROOT / 'src/workspace.html').read_text(encoding='utf-8')
    assert 'function shopeeOverviewOrdersKpiValue(sp)' in html

    finite = html.split('function hasFiniteKpiNumber(value) {', 1)[1].split('\n    }\n', 1)[0]
    count = html.split('function shopeeOverviewOrdersKpiValue(sp) {', 1)[1].split('\n    }\n', 1)[0]
    set_kpi = html.split('function setWorkspaceKpi(id, value) {', 1)[1].split('\n    }\n', 1)[0]
    script = f'''\
var elements = {{}};
var document = {{ getElementById: function (id) {{ return elements[id] || (elements[id] = {{ textContent: null }}); }} }};
function hasFiniteKpiNumber(value) {{{finite}\n}}
function shopeeOverviewOrdersKpiValue(sp) {{{count}\n}}
function setWorkspaceKpi(id, value) {{{set_kpi}\n}}
var payloads = [
  {{ scanned_orders: 12, orders: [{{ order_sn: 'A' }}] }},
  {{ scanned_orders: NaN, orders: [{{ order_sn: 'A' }}, {{ order_sn: 'B' }}] }},
  {{ scanned_orders: Infinity, orders: {{ order_sn: 'A' }} }},
  {{ scanned_orders: '12', orders: [{{ order_sn: 'A' }}] }},
  {{ orders: [{{ order_sn: 'A' }}] }},
  {{ orders: {{ order_sn: 'A' }} }},
  null
];
var out = payloads.map(function (sp) {{
  var el = document.getElementById('sp_overview_orders');
  var value = shopeeOverviewOrdersKpiValue(sp);
  setWorkspaceKpi('sp_overview_orders', value);
  return el.textContent;
}});
console.log(JSON.stringify(out));
'''
    actual = json.loads(subprocess.run(
        ['node', '-e', script], check=True, capture_output=True, text=True
    ).stdout)
    assert actual == [12, 2, '—', 1, 1, '—', '—']

    sync = html.split('function syncPlatformOverviewKpis(sp, pas, tt, health) {', 1)[1].split('\n    window.__tpSyncWorkspace', 1)[0]
    assert 'setWorkspaceKpi("sp_overview_orders", shopeeOverviewOrdersKpiValue(sp));' in sync
    for existing_kpi in ('sp.to_ship', 'sp.shipping', 'sp.unpaid', 'liveKpiValue(live)', 'pasVndValue(pas.sales_ads)'):
        assert existing_kpi in sync


def test_content_agent_ctrl_wheel_uses_existing_zoom_step_ipc_only():
    agent = (ROOT / 'src/content_agent.js').read_text(encoding='utf-8')
    wheel = agent.split('function zoomFromCtrlWheel(e) {', 1)[1].split('\n  }\n', 1)[0]
    assert 'if (!e.ctrlKey) return;' in wheel
    assert 'e.preventDefault();' in wheel
    assert 'cmd: "zoom_step"' in wheel
    assert 'delta: e.deltaY < 0 ? 10 : -10' in wheel
    assert 'window.addEventListener("wheel", zoomFromCtrlWheel, { capture: true, passive: false });' in agent

    native = (ROOT / 'src/main.rs').read_text(encoding='utf-8')
    assert 'AppCmd::ZoomStep(delta)' in native
    assert 'clamp(50, 250)' in native


def test_shopee_discovery_reports_verified_rows_without_cookie_inference():
    discovery = (ROOT / 'src/shopee_discovery.rs').read_text(encoding='utf-8')
    script = discovery.split('pub const SCRIPT: &str = r#"', 1)[1].split('"#;', 1)[0]
    normalize = discovery.split('pub fn normalize(', 1)[1].split('\n}\n\n#[cfg(test)]', 1)[0]

    assert 'document.cookie' not in script
    assert 'captcha' in script
    assert 'result("ok", Some(shops), true)' in normalize
    assert 'result("loading", None, false)' in normalize
    assert 'if loading { return result("loading", None, false); }' in normalize
    assert 'if captcha { return result("captcha", None, false); }' in normalize
    assert 'result("ok", Some(shops), true)' in normalize
    assert 'result("needs_login", None, false)' in normalize


def test_shopee_workspace_has_explicit_selected_and_all_scan_commands():
    html = (ROOT / 'src/workspace.html').read_text(encoding='utf-8')
    main = (ROOT / 'src/main.rs').read_text(encoding='utf-8')

    assert 'Quét shop đã chọn' in html
    assert 'Quét tất cả shop' in html
    assert "runShopeeShopScan('selected')" in html
    assert "runShopeeShopScan('all')" in html
    assert 'Chọn ít nhất một shop trước khi quét.' in html
    assert "runScan(mode === 'all' ? 'shopee_shops_all' : 'shopee_shops_selected')" in html
    scan = main.split('AppCmd::RunEcomScan { scope } => {', 1)[1].split(
        'AppCmd::SetEcomAutoScan(enabled) => {', 1
    )[0]
    assert 's == "shopee_shops_selected"' in scan
    assert 's == "shopee_shops_all"' in scan
    assert 'self.queue_discovered_shopee_shops(run_selected_shopee_shops)' in scan


def test_shopee_selection_starts_empty_and_sequence_uses_verified_shop_ids():
    html = (ROOT / 'src/workspace.html').read_text(encoding='utf-8')
    main = (ROOT / 'src/main.rs').read_text(encoding='utf-8')

    render = html.split('function renderShopeeShopSelection(discovery) {', 1)[1].split('\n    function selectPlatformShop', 1)[0]
    assert 'var checked = selectedShopKeys[key] === true;' in render
    assert 'selectedShopKeys[key] = true;' not in render
    assert '(st.shopee_shop_discovery || { status: "unavailable", account_identity: "unavailable", shops: [] })' in html
    assert 'fn queue_discovered_shopee_shops(&mut self, selected_only: bool) -> usize {' in main
    queue = main.split('fn queue_discovered_shopee_shops(&mut self, selected_only: bool) -> usize {', 1)[1].split('\n    fn start_shopee_shop_sequence', 1)[0]
    assert 'shop_id' in queue
    assert 'self.selected_shops.iter().filter' in queue
    assert 'self.shopee_shop_scan_queue = queued.into_iter().rev().collect();' in queue
    sequence = main.split('fn start_shopee_shop_sequence(&mut self) {', 1)[1].split('\n    fn open_next_shopee_shop', 1)[0]
    assert 'self.shopee_shop_scan_queue.clear();' not in sequence
    index = main.split('fn handle_shopee_shop_index(&mut self, data_json: &str) {', 1)[1].split('\n    fn store_shopee_shop_orders', 1)[0]
    assert 'self.shopee_shop_scan_queue = discovered.into_iter().rev().collect();' not in index


def test_shopee_scope_resolves_only_the_selected_shop_snapshot():
    """A contextual Shopee scope must never substitute a platform or other-shop payload."""
    html = (ROOT / 'src/workspace.html').read_text(encoding='utf-8')
    helper = html.split('function resolveShopeeSnapshot(', 1)[1].split('\n    }\n', 1)[0]
    reset = html.split('function renderUnknownShopeeSnapshot() {', 1)[1].split('\n    }\n', 1)[0]
    platform_reset = html.split('function clearPlatformOnlyShopeeValues() {', 1)[1].split('\n    }\n', 1)[0]
    script = f'''\
function resolveShopeeSnapshot({helper}\n}}
var elements = {{}};
var document = {{ getElementById: function (id) {{ return elements[id] || (elements[id] = {{ textContent: 'stale', innerHTML: 'stale' }}); }} }};
function renderUnknownShopeeSnapshot() {{{reset}\n}}
function clearPlatformOnlyShopeeValues() {{{platform_reset}\n}}
var platform = {{ source: 'platform' }};
var perShop = {{ shop_a: {{ source: 'shop-a' }} }};
var resolved = [
  resolveShopeeSnapshot(platform, perShop, 'all'),
  resolveShopeeSnapshot(platform, perShop, 'shop_a'),
  resolveShopeeSnapshot(platform, perShop, 'missing'),
  resolveShopeeSnapshot(platform, null, 'shop_a')
];
renderUnknownShopeeSnapshot();
['sp_overview_pas', 'sp_overview_pas_detail', 'sp_overview_health_penalty', 'sp_overview_health_detail', 'stat_rev'].forEach(function (id) {{ document.getElementById(id).textContent = 'stale'; }});
clearPlatformOnlyShopeeValues();
console.log(JSON.stringify({{ resolved: resolved, reset: {{
  reviews: elements.sp_reviews_list.innerHTML,
  login: elements.sp_login_box.textContent,
  ratingBadge: elements.sp_rating_badge.textContent,
  anomalyBadge: elements.sp_anom_badge.textContent,
  liveBadge: elements.sp_live_badge.textContent,
  pas: elements.sp_overview_pas.textContent,
  pasDetail: elements.sp_overview_pas_detail.textContent,
  health: elements.sp_overview_health_penalty.textContent,
  healthDetail: elements.sp_overview_health_detail.textContent,
  revenue: elements.stat_rev.textContent,
  platformPas: elements.sp_overview_pas.textContent,
  platformPasDetail: elements.sp_overview_pas_detail.textContent,
  platformHealth: elements.sp_overview_health_penalty.textContent,
  platformHealthDetail: elements.sp_overview_health_detail.textContent
}} }}));
'''
    completed = subprocess.run(['node', '-e', script], capture_output=True, text=True)
    assert completed.returncode == 0, completed.stderr
    actual = json.loads(completed.stdout)
    assert actual['resolved'] == [{'source': 'platform'}, {'source': 'shop-a'}, None, None]
    assert actual['reset']['reviews'] != 'stale'
    for key in ('login', 'ratingBadge', 'anomalyBadge', 'liveBadge', 'pas', 'pasDetail', 'health', 'healthDetail', 'revenue', 'platformPas', 'platformPasDetail', 'platformHealth', 'platformHealthDetail'):
        assert actual['reset'][key] != 'stale'

    assert 'id="platform_shopbar_shopee"' in html
    assert 'id="platform_shopee_chips"' in html
    assert "resolveShopeeSnapshot(st.shopee_orders, st.shopee_shop_orders, activePlatformShop.shopee)" in html
    select = html.split('function selectPlatformShop(platform, shopId, button) {', 1)[1].split('\n    }\n', 1)[0]
    assert 'window.__tpSyncWorkspace(lastData.workspaceState);' in select


def test_shopee_shop_scope_resets_promos_reports_and_legacy_executive_values():
    """Sparse shop snapshots must not retain platform data or synthesize executive values."""
    html = (ROOT / 'src/workspace.html').read_text(encoding='utf-8')
    reset = html.split('function renderUnknownShopeeSnapshot() {', 1)[1].split('\n    }\n', 1)[0]
    pas_report = html.split('function shopeePasReportLine(', 1)[1].split('\n    }\n', 1)[0]
    script = f'''\
var elements = {{}};
var document = {{ getElementById: function (id) {{ return elements[id] || (elements[id] = {{ textContent: 'stale', innerHTML: 'stale' }}); }} }};
function renderUnknownShopeeSnapshot() {{{reset}\n}}
function shopeePasReportLine({pas_report}\n}}
renderUnknownShopeeSnapshot();
console.log(JSON.stringify({{
  promos: elements.sp_overview_promos.textContent,
  promosDetail: elements.sp_overview_promos_detail.textContent,
  tasks: elements['task-list-container'].innerHTML,
  shopPas: shopeePasReportLine('shop-a', {{ sales_ads: 99, cost_ads: 1, roas_ads: 99 }}),
  allUnknownPas: shopeePasReportLine('all', {{}})
}}));
'''
    completed = subprocess.run(['node', '-e', script], capture_output=True, text=True)
    assert completed.returncode == 0, completed.stderr
    actual = json.loads(completed.stdout)
    assert actual['promos'] != 'stale'
    assert actual['promosDetail'] != 'stale'
    assert actual['tasks'] != 'stale'
    assert 'cấp nền tảng' in actual['shopPas']
    assert '99' not in actual['shopPas']
    assert '0' not in actual['allUnknownPas']

    sync = html.split('var sp = resolveShopeeSnapshot(st.shopee_orders, st.shopee_shop_orders, activePlatformShop.shopee);', 1)[1].split('\n      if (st.shopee_pas && activePlatformShop.shopee === "all")', 1)[0]
    assert 'activePlatformShop.shopee === "all" && typeof updateShopeeExecutiveDashboard === "function"' in sync
    promo_filter = html.split('function setPromoFilter(mode, btn) {', 1)[1].split('\n    }\n', 1)[0]
    assert 'lastData.shopee_orders' in promo_filter
    assert 'updateShopeeExecutiveDashboard(sp,' not in promo_filter
    assert 'shopeePasReportLine(activePlatformShop.shopee, lastData.shopee_pas)' in html
