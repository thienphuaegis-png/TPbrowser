import test from 'node:test';
import assert from 'node:assert/strict';
import {
  applyCommand,
  buildConfigPayload,
  buildShopeeAnomaliesCsv,
  buildTitlebarPayload,
  buildWorkspacePayload,
  createInitialState,
  normalizeTheme,
  normalizeUrl,
} from './browserState';

test('normalizeUrl handles internal tp:// URLs, domains, and search queries', () => {
  assert.equal(normalizeUrl(''), 'tp://newtab');
  assert.equal(normalizeUrl('tp://workspace'), 'tp://workspace');
  assert.equal(normalizeUrl('tp://history'), 'tp://history');
  assert.equal(normalizeUrl('banhang.shopee.vn'), 'https://banhang.shopee.vn');
  assert.equal(normalizeUrl('https://example.com/a'), 'https://example.com/a');
  assert.equal(normalizeUrl('gia vang hom nay'), 'https://www.google.com/search?q=gia%20vang%20hom%20nay');
});

test('normalizeTheme maps light aliases to carrara-light and defaults to obsidian-gold', () => {
  assert.equal(normalizeTheme('light'), 'carrara-light');
  assert.equal(normalizeTheme('warm-paper'), 'carrara-light');
  assert.equal(normalizeTheme('carrara-light'), 'carrara-light');
  assert.equal(normalizeTheme('obsidian-gold'), 'obsidian-gold');
  assert.equal(normalizeTheme('unknown'), 'obsidian-gold');
});

test('tab lifecycle: new_tab, navigate, back/forward, close last tab resets to tp://newtab, reopen closed', () => {
  const st = createInitialState();
  const initialCount = st.tabs.length;

  applyCommand(st, { cmd: 'new_tab', url: 'https://example.com' });
  assert.equal(st.tabs.length, initialCount + 1);
  assert.equal(st.tabs[st.active_idx].url, 'https://example.com');

  applyCommand(st, { cmd: 'navigate_active', url: 'https://banhang.shopee.vn' });
  assert.equal(st.tabs[st.active_idx].url, 'https://banhang.shopee.vn');

  applyCommand(st, { cmd: 'nav_back' });
  assert.equal(st.tabs[st.active_idx].url, 'https://example.com');

  applyCommand(st, { cmd: 'nav_forward' });
  assert.equal(st.tabs[st.active_idx].url, 'https://banhang.shopee.vn');

  const closingId = st.tabs[st.active_idx].id;
  applyCommand(st, { cmd: 'close_tab_by_id', tab_id: closingId });
  assert.equal(st.closed_tabs[st.closed_tabs.length - 1], 'https://banhang.shopee.vn');

  applyCommand(st, { cmd: 'reopen_closed_tab' });
  assert.equal(st.tabs[st.active_idx].url, 'https://banhang.shopee.vn');

  // Close all tabs in default workspace until 1 remains, then close the last one
  while (st.tabs.length > 1) {
    applyCommand(st, { cmd: 'close_tab_by_id', tab_id: st.tabs[0].id });
  }
  applyCommand(st, { cmd: 'navigate_active', url: 'https://example.org' });
  applyCommand(st, { cmd: 'close_tab_by_id', tab_id: st.tabs[0].id });
  assert.equal(st.tabs.length, 1);
  assert.equal(st.tabs[0].url, 'tp://newtab');
});

test('tab orientation, split mode, settings modal, and theme switch', () => {
  const st = createInitialState();
  assert.equal(st.tab_orientation, 'vertical');

  applyCommand(st, { cmd: 'toggle_tab_orientation' });
  assert.equal(st.tab_orientation, 'horizontal');

  applyCommand(st, { cmd: 'set_split_mode', mode: 'columns', pair_tab_id: 2 });
  assert.equal(st.view_mode, 'columns');
  assert.equal(st.split_paired_tab_id, 2);

  applyCommand(st, { cmd: 'unsplit' });
  assert.equal(st.view_mode, 'single');
  assert.equal(st.split_paired_tab_id, null);

  applyCommand(st, { cmd: 'open_settings_modal', section: 'ai_cowork' });
  assert.equal(st.settings_open, true);
  assert.equal(buildConfigPayload(st).open_section, 'ai_cowork');

  applyCommand(st, { cmd: 'save_setting', key: 'ui_theme', value: 'carrara-light' });
  assert.equal(st.ui_theme, 'carrara-light');
  assert.equal(buildTitlebarPayload(st).ui_theme, 'carrara-light');
});

test('spaces lifecycle: create, rename, and delete migrates tabs back to default', () => {
  const st = createInitialState();
  applyCommand(st, { cmd: 'create_workspace', name: 'Kho Hàng' });
  const createdId = st.active_workspace_id;
  assert.notEqual(createdId, 'default');
  assert.ok(st.tabs.some((t) => t.workspace_id === createdId));

  applyCommand(st, { cmd: 'rename_workspace', workspace_id: createdId, name: 'Kho Tổng' });
  assert.equal(st.workspaces.find((w) => w.id === createdId)?.name, 'Kho Tổng');

  applyCommand(st, { cmd: 'delete_workspace', workspace_id: createdId });
  assert.equal(st.active_workspace_id, 'default');
  assert.equal(st.workspaces.some((w) => w.id === createdId), false);
  assert.equal(st.tabs.some((t) => t.workspace_id === createdId), false);
});

test('e-commerce shop selection, background worker scan, and CSV export', () => {
  const st = createInitialState();
  applyCommand(st, { cmd: 'set_shopee_shop_selection', platform: 'shopee', shop_id: '882410912', selected: false });
  assert.equal(
    st.selected_shops.some((s) => s.shop_id === '882410912'),
    false
  );

  applyCommand(st, { cmd: 'run_ecom_scan', scope: 'shopee' });
  const ws = buildWorkspacePayload(st);
  assert.equal(ws.worker_tabs.length, 1);
  assert.equal(ws.worker_tabs[0].worker_kind, 'bot');

  const csv = buildShopeeAnomaliesCsv(st);
  assert.ok(csv.startsWith('\uFEFF"Order SN"'));
  assert.ok(csv.includes('261118A9K4M2QX'));
});
