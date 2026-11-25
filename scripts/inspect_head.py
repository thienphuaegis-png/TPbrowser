import subprocess

with open('src/workspace.html', 'r', encoding='utf-8') as f:
    curr = f.read()

terms = [
    '<nav class="island-nav">',
    '</nav>',
    '<header class="shopee-subnav-bar">',
    '</header>',
    '<section id="workspace-dashboard"',
    'id="shopee-view-tong-quan"',
    '<!-- Đơn hàng & Tác vụ gần đây -->',
    'id="shopee-shop-selection-panel"',
    'id="workspace-tiktok"',
    'sp_toship',
    'sp_shipping_sub',
    'sp_anom_count',
    'sp_bad_kpi',
    'sp_live_kpi',
    'sp_pas_sales',
    'sp_discounts_list',
    'sp_vouchers_list',
    'sp_flashsales_list',
    'tt_sales',
    'tt_kpi_sub',
    'tt_status_pill',
    'tt_provenance_tbody',
    'tt_ts',
    'tt_captcha_box',
    'tt_tbody',
    'tt_health_card',
    'tt_health_tbody',
    '.platform-kpi-grid',
    'grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));',
    '__tpSyncWorkspace',
    'selectedShopKeys',
    'set_shopee_shop_selection',
    'selected_shops',
    'openDirect',
    'export_shopee_csv',
    'run_ecom_scan',
    'sp_anom_tbody',
    'sp_reviews_list',
    'sp_live_tbody'
]

for t in terms:
    pos = curr.find(t)
    print(f"{t}: {pos}")
