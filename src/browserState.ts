import {
  BookmarkItem,
  DEFAULT_BOOKMARKS,
  DEFAULT_FLOATING_ORBS,
  DEFAULT_SPACES,
  FloatingOrbItem,
  HistoryItem,
  SelectedShop,
  ShopeeSnapshot,
  WorkspaceSpace,
  createSeedHistory,
  createSeedShopSnapshots,
  createSeedShopeeDiscovery,
  createSeedShopeeHealth,
  createSeedShopeePas,
  createSeedTikTok,
  createSeedTikTokHealth,
} from './seedWorkspace.js';
import { buildDashboardSnapshot } from './ecomDashboard.js';

export type TabOrientation = 'vertical' | 'horizontal';
export type ContentViewMode = 'single' | 'columns' | 'rows' | 'spiral';
export type AiDisplayMode = 'bubble' | 'popup' | 'dock';
export type ChatChannel = 'ai' | 'fb' | 'zalo' | 'custom';

export interface BrowserTab {
  id: number;
  url: string;
  title: string;
  favicon: string;
  profile: string;
  container_id: string;
  workspace_id: string;
  worker_kind: '' | 'bot' | 'ai';
  worker_status: string;
  pinned: boolean;
  group_id: number;
  nav_stack: string[];
  nav_cursor: number;
}

export interface BrowserState {
  next_tab_id: number;
  next_group_id: number;
  ui_theme: 'obsidian-gold' | 'carrara-light';
  logo_style: 'ribbon' | 'arrow';
  tab_orientation: TabOrientation;
  sidebar_collapsed: boolean;
  sidebar_width: number;
  show_bookmarks_bar: boolean;
  startup_mode: 'restore' | 'newtab';
  background_mode: boolean;
  startup_boost: boolean;
  floating_omnibox: boolean;
  compact_titlebar: boolean;
  view_mode: ContentViewMode;
  split_paired_tab_id: number | null;
  zoom_pct: number;
  ai_mode: AiDisplayMode;
  chat_channel: ChatChannel;
  ai_dock_width: number;
  ai_provider: 'openai' | 'gemini';
  ai_api_key: string;
  ai_model: string;
  ai_base_url: string;
  auto_minimize_on_task: boolean;
  show_unread_badges: boolean;
  always_show_floating_orbs: boolean;
  adblock_enabled: boolean;
  google_user: string;
  unread_ai: number;
  unread_fb: number;
  unread_zalo: number;
  settings_open: boolean;
  settings_section: string;
  active_workspace_id: string;
  workspaces: WorkspaceSpace[];
  tabs: BrowserTab[];
  active_idx: number;
  selected_tab_ids: number[];
  closed_tabs: string[];
  bookmarks: BookmarkItem[];
  history: HistoryItem[];
  floating_orbs: FloatingOrbItem[];
  ecom_auto_scan: boolean;
  ecom_status_text: string;
  ecom_logs: string[];
  shopee_discovery: ReturnType<typeof createSeedShopeeDiscovery>;
  selected_shops: SelectedShop[];
  shopee_orders: ShopeeSnapshot;
  shopee_shop_orders: Record<string, ShopeeSnapshot>;
  shopee_pas: ReturnType<typeof createSeedShopeePas>;
  shopee_health: ReturnType<typeof createSeedShopeeHealth>;
  tiktok: ReturnType<typeof createSeedTikTok>;
  tiktok_health: ReturnType<typeof createSeedTikTokHealth>;
  app_version: string;
  app_commit: string;
}

export interface BrowserCommand {
  cmd: string;
  [key: string]: unknown;
}

export function normalizeTheme(raw: unknown): 'obsidian-gold' | 'carrara-light' {
  const s = String(raw || '').trim().toLowerCase();
  if (s === 'carrara-light' || s === 'warm-paper' || s === 'light') {
    return 'carrara-light';
  }
  return 'obsidian-gold';
}

export function normalizeUrl(raw: string): string {
  const trimmed = String(raw || '').trim();
  if (!trimmed) return 'tp://newtab';
  const low = trimmed.toLowerCase();
  if (low === 'tp://newtab' || low === 'about:blank') return 'tp://newtab';
  if (low === 'tp://history') return 'tp://history';
  if (low === 'tp://workspace') return 'tp://workspace';
  if (low.startsWith('http://') || low.startsWith('https://')) return trimmed;
  if (!trimmed.includes(' ') && trimmed.includes('.')) {
    return `https://${trimmed}`;
  }
  return `https://www.google.com/search?q=${encodeURIComponent(trimmed)}`;
}

export function titleFromUrl(url: string): string {
  if (url === 'tp://newtab') return 'Không gian bắt đầu';
  if (url === 'tp://workspace') return 'Sàn TMĐT';
  if (url === 'tp://history') return 'Lịch sử duyệt web';
  try {
    const parsed = new URL(url);
    return parsed.hostname.replace(/^www\./, '') || url;
  } catch {
    return url;
  }
}

export function createInitialState(): BrowserState {
  const shopSnapshots = createSeedShopSnapshots();
  const discovery = createSeedShopeeDiscovery();
  const tabs: BrowserTab[] = [
    {
      id: 1,
      url: 'tp://workspace',
      title: 'Sàn TMĐT',
      favicon: '',
      profile: 'default',
      container_id: 'default',
      workspace_id: 'default',
      worker_kind: '',
      worker_status: '',
      pinned: true,
      group_id: 0,
      nav_stack: ['tp://workspace'],
      nav_cursor: 0,
    },
    {
      id: 2,
      url: 'tp://newtab',
      title: 'Không gian bắt đầu',
      favicon: '',
      profile: 'default',
      container_id: 'default',
      workspace_id: 'default',
      worker_kind: '',
      worker_status: '',
      pinned: false,
      group_id: 0,
      nav_stack: ['tp://newtab'],
      nav_cursor: 0,
    },
    {
      id: 3,
      url: 'tp://history',
      title: 'Lịch sử duyệt web',
      favicon: '',
      profile: 'default',
      container_id: 'default',
      workspace_id: 'default',
      worker_kind: '',
      worker_status: '',
      pinned: false,
      group_id: 0,
      nav_stack: ['tp://history'],
      nav_cursor: 0,
    },
  ];

  return {
    next_tab_id: 4,
    next_group_id: 1,
    ui_theme: 'obsidian-gold',
    logo_style: 'ribbon',
    tab_orientation: 'vertical',
    sidebar_collapsed: false,
    sidebar_width: 228,
    show_bookmarks_bar: true,
    startup_mode: 'restore',
    background_mode: true,
    startup_boost: false,
    floating_omnibox: true,
    compact_titlebar: false,
    view_mode: 'single',
    split_paired_tab_id: null,
    zoom_pct: 100,
    ai_mode: 'bubble',
    chat_channel: 'ai',
    ai_dock_width: 360,
    ai_provider: 'openai',
    ai_api_key: '',
    ai_model: 'gemini-2.5-flash',
    ai_base_url: 'http://127.0.0.1:8045/v1',
    auto_minimize_on_task: true,
    show_unread_badges: true,
    always_show_floating_orbs: true,
    adblock_enabled: true,
    google_user: 'Bùi Thiên Phú',
    unread_ai: 0,
    unread_fb: 2,
    unread_zalo: 4,
    settings_open: false,
    settings_section: 'look_feel',
    active_workspace_id: 'default',
    workspaces: DEFAULT_SPACES.map((s) => ({ ...s })),
    tabs,
    active_idx: 0,
    selected_tab_ids: [],
    closed_tabs: ['https://banhang.shopee.vn/portal/sale/order'],
    bookmarks: DEFAULT_BOOKMARKS.map((b) => ({ ...b })),
    history: createSeedHistory(),
    floating_orbs: DEFAULT_FLOATING_ORBS.map((o) => ({ ...o })),
    ecom_auto_scan: false,
    ecom_status_text: 'Sẵn sàng — 2 shop Shopee & TikTok Seller đã đồng bộ dữ liệu mẫu (Web Runtime)',
    ecom_logs: [
      '• Khởi chạy TPbrowser Web Shell trên Node.js 22 (port 3000)',
      '• Đã nạp dữ liệu kiểm toán 200 đơn hàng Shopee (2 shop) và TikTok Ads',
      '• Sẵn sàng bấm Quét để mô phỏng luồng Background Worker & cập nhật log',
    ],
    shopee_discovery: discovery,
    selected_shops: discovery.shops.map((s) => ({ platform: 'shopee', shop_id: s.shop_id })),
    shopee_orders: shopSnapshots.platform,
    shopee_shop_orders: shopSnapshots.byShop,
    shopee_pas: createSeedShopeePas(),
    shopee_health: createSeedShopeeHealth(),
    tiktok: createSeedTikTok(),
    tiktok_health: createSeedTikTokHealth(),
    app_version: '0.9.11-web',
    app_commit: '3d4cff6',
  };
}

function visibleTabEntries(state: BrowserState): Array<{ nativeIdx: number; tab: BrowserTab }> {
  return state.tabs
    .map((tab, nativeIdx) => ({ nativeIdx, tab }))
    .filter(({ tab }) => tab.workspace_id === state.active_workspace_id);
}

function ensureActiveTabInWorkspace(state: BrowserState): void {
  const visible = visibleTabEntries(state);
  if (visible.length === 0) {
    const tab: BrowserTab = {
      id: state.next_tab_id++,
      url: 'tp://newtab',
      title: 'Không gian bắt đầu',
      favicon: '',
      profile: 'default',
      container_id: 'default',
      workspace_id: state.active_workspace_id,
      worker_kind: '',
      worker_status: '',
      pinned: false,
      group_id: 0,
      nav_stack: ['tp://newtab'],
      nav_cursor: 0,
    };
    state.tabs.push(tab);
    state.active_idx = state.tabs.length - 1;
    return;
  }
  const active = state.tabs[state.active_idx];
  if (!active || active.workspace_id !== state.active_workspace_id) {
    state.active_idx = visible[0].nativeIdx;
  }
}

function pushLog(state: BrowserState, message: string): void {
  state.ecom_logs.push(`• ${message}`);
  if (state.ecom_logs.length > 60) {
    state.ecom_logs = state.ecom_logs.slice(-60);
  }
}

function pushHistory(state: BrowserState, url: string, title: string): void {
  if (!url.startsWith('http://') && !url.startsWith('https://')) return;
  const nowSec = Math.floor(Date.now() / 1000);
  const last = state.history[state.history.length - 1];
  if (last && last.url === url && nowSec - last.ts < 5) return;
  state.history.push({ title: title || titleFromUrl(url), url, ts: nowSec });
  if (state.history.length > 200) {
    state.history = state.history.slice(-200);
  }
}

function openOrFocusInternalTab(state: BrowserState, internalUrl: 'tp://workspace' | 'tp://history'): void {
  const existingIdx = state.tabs.findIndex(
    (t) => t.workspace_id === state.active_workspace_id && t.url === internalUrl
  );
  if (existingIdx !== -1) {
    state.active_idx = existingIdx;
    return;
  }
  const tab: BrowserTab = {
    id: state.next_tab_id++,
    url: internalUrl,
    title: titleFromUrl(internalUrl),
    favicon: '',
    profile: 'default',
    container_id: 'default',
    workspace_id: state.active_workspace_id,
    worker_kind: '',
    worker_status: '',
    pinned: false,
    group_id: 0,
    nav_stack: [internalUrl],
    nav_cursor: 0,
  };
  state.tabs.push(tab);
  state.active_idx = state.tabs.length - 1;
}

export function applyCommand(state: BrowserState, cmdObj: BrowserCommand): { changed: boolean; toast?: string } {
  const cmd = String(cmdObj?.cmd || '');
  if (!cmd) return { changed: false };

  switch (cmd) {
    case 'new_tab': {
      const url = normalizeUrl(String(cmdObj.url || 'tp://newtab'));
      const containerId = String(cmdObj.container_id || (cmdObj.isolated ? `acc_${state.next_tab_id}` : 'default'));
      const title = titleFromUrl(url);
      const tab: BrowserTab = {
        id: state.next_tab_id++,
        url,
        title,
        favicon: '',
        profile: containerId,
        container_id: containerId,
        workspace_id: state.active_workspace_id,
        worker_kind: '',
        worker_status: '',
        pinned: false,
        group_id: 0,
        nav_stack: [url],
        nav_cursor: 0,
      };
      state.tabs.push(tab);
      state.active_idx = state.tabs.length - 1;
      pushHistory(state, url, title);
      return { changed: true };
    }

    case 'focus_tab_by_id': {
      const targetId = Number(cmdObj.tab_id);
      const idx = state.tabs.findIndex((t) => t.id === targetId);
      if (idx !== -1) {
        state.active_workspace_id = state.tabs[idx].workspace_id;
        state.active_idx = idx;
        return { changed: true };
      }
      return { changed: false };
    }

    case 'focus_index': {
      const idx = Number(cmdObj.index);
      if (Number.isInteger(idx) && idx >= 0 && idx < state.tabs.length) {
        state.active_idx = idx;
        return { changed: true };
      }
      return { changed: false };
    }

    case 'close_tab_by_id':
    case 'close_index': {
      const idx =
        cmd === 'close_tab_by_id'
          ? state.tabs.findIndex((t) => t.id === Number(cmdObj.tab_id))
          : Number(cmdObj.index);
      if (!Number.isInteger(idx) || idx < 0 || idx >= state.tabs.length) {
        return { changed: false };
      }
      const closing = state.tabs[idx];
      if (closing.url.startsWith('http://') || closing.url.startsWith('https://')) {
        state.closed_tabs.push(closing.url);
        if (state.closed_tabs.length > 30) state.closed_tabs.shift();
      }
      const sameWorkspace = state.tabs.filter((t) => t.workspace_id === closing.workspace_id);
      if (sameWorkspace.length === 1 && !closing.worker_kind) {
        closing.url = 'tp://newtab';
        closing.title = titleFromUrl('tp://newtab');
        closing.nav_stack = ['tp://newtab'];
        closing.nav_cursor = 0;
        closing.pinned = false;
        state.active_idx = idx;
        return { changed: true };
      }
      state.tabs.splice(idx, 1);
      state.selected_tab_ids = state.selected_tab_ids.filter((id) => id !== closing.id);
      if (state.split_paired_tab_id === closing.id) {
        state.split_paired_tab_id = null;
        state.view_mode = 'single';
      }
      if (state.active_idx >= state.tabs.length) {
        state.active_idx = Math.max(0, state.tabs.length - 1);
      } else if (idx < state.active_idx) {
        state.active_idx = Math.max(0, state.active_idx - 1);
      }
      ensureActiveTabInWorkspace(state);
      return { changed: true };
    }

    case 'reopen_closed_tab': {
      const url = state.closed_tabs.pop();
      if (!url) return { changed: false, toast: 'Không có tab nào vừa đóng' };
      return applyCommand(state, { cmd: 'new_tab', url, isolated: false });
    }

    case 'navigate_active': {
      ensureActiveTabInWorkspace(state);
      const tab = state.tabs[state.active_idx];
      if (!tab) return { changed: false };
      const normalized = normalizeUrl(String(cmdObj.url || 'tp://newtab'));
      tab.url = normalized;
      tab.title = titleFromUrl(normalized);
      tab.nav_stack = tab.nav_stack.slice(0, tab.nav_cursor + 1);
      tab.nav_stack.push(normalized);
      tab.nav_cursor = tab.nav_stack.length - 1;
      pushHistory(state, normalized, tab.title);
      return { changed: true };
    }

    case 'nav_back': {
      const tab = state.tabs[state.active_idx];
      if (!tab || tab.nav_cursor <= 0) return { changed: false };
      tab.nav_cursor -= 1;
      tab.url = tab.nav_stack[tab.nav_cursor];
      tab.title = titleFromUrl(tab.url);
      return { changed: true };
    }

    case 'nav_forward': {
      const tab = state.tabs[state.active_idx];
      if (!tab || tab.nav_cursor >= tab.nav_stack.length - 1) return { changed: false };
      tab.nav_cursor += 1;
      tab.url = tab.nav_stack[tab.nav_cursor];
      tab.title = titleFromUrl(tab.url);
      return { changed: true };
    }

    case 'reload':
      return { changed: true };

    case 'open_workspace':
      openOrFocusInternalTab(state, 'tp://workspace');
      return { changed: true };

    case 'open_history':
      openOrFocusInternalTab(state, 'tp://history');
      return { changed: true };

    case 'clear_history':
      state.history = [];
      return { changed: true };

    case 'bookmark_active': {
      const tab = state.tabs[state.active_idx];
      if (!tab || tab.url.startsWith('tp://')) {
        return { changed: false, toast: 'Chỉ ghim được trang web http(s)://' };
      }
      const existing = state.bookmarks.findIndex((b) => b.url === tab.url);
      if (existing !== -1) {
        state.bookmarks.splice(existing, 1);
        return { changed: true, toast: 'Đã bỏ ghim trang hiện tại' };
      }
      state.bookmarks.push({ title: tab.title || titleFromUrl(tab.url), url: tab.url });
      return { changed: true, toast: 'Đã ghim vào Bookmarks' };
    }

    case 'save_bookmarks': {
      try {
        const parsed = JSON.parse(String(cmdObj.bookmarks_json || '[]'));
        if (Array.isArray(parsed)) {
          state.bookmarks = parsed
            .filter((b) => b && typeof b.url === 'string' && b.url.trim())
            .map((b) => ({ title: String(b.title || b.url), url: String(b.url) }));
          return { changed: true };
        }
      } catch {
        return { changed: false };
      }
      return { changed: false };
    }

    case 'toggle_tab_orientation':
      state.tab_orientation = state.tab_orientation === 'vertical' ? 'horizontal' : 'vertical';
      return { changed: true };

    case 'toggle_sidebar_collapse':
      state.sidebar_collapsed = !state.sidebar_collapsed;
      return { changed: true };

    case 'toggle_bookmarks_bar':
      state.show_bookmarks_bar = !state.show_bookmarks_bar;
      return { changed: true };

    case 'toggle_tab_pinned': {
      const tab = state.tabs[state.active_idx];
      if (!tab || tab.worker_kind) return { changed: false };
      tab.pinned = !tab.pinned;
      return { changed: true };
    }

    case 'set_tab_selection': {
      const tabId = Number(cmdObj.tab_id);
      const mode = String(cmdObj.mode || 'replace');
      const visibleUserTabs = visibleTabEntries(state)
        .map((e) => e.tab)
        .filter((t) => !t.worker_kind);
      const target = visibleUserTabs.find((t) => t.id === tabId);
      if (!target) return { changed: false };

      if (mode === 'toggle') {
        if (state.selected_tab_ids.includes(tabId)) {
          state.selected_tab_ids = state.selected_tab_ids.filter((id) => id !== tabId);
        } else {
          state.selected_tab_ids = [...state.selected_tab_ids, tabId];
        }
      } else if (mode === 'range') {
        const anchorId = state.selected_tab_ids[state.selected_tab_ids.length - 1] ?? state.tabs[state.active_idx]?.id;
        const idxA = visibleUserTabs.findIndex((t) => t.id === anchorId);
        const idxB = visibleUserTabs.findIndex((t) => t.id === tabId);
        if (idxA !== -1 && idxB !== -1) {
          const [start, end] = idxA < idxB ? [idxA, idxB] : [idxB, idxA];
          state.selected_tab_ids = visibleUserTabs.slice(start, end + 1).map((t) => t.id);
        } else {
          state.selected_tab_ids = [tabId];
        }
      } else {
        state.selected_tab_ids = [];
      }
      return { changed: true };
    }

    case 'group_tabs': {
      const src = state.tabs.find((t) => t.id === Number(cmdObj.source_id) && !t.worker_kind);
      const dst = state.tabs.find((t) => t.id === Number(cmdObj.target_id) && !t.worker_kind);
      if (!src || !dst || src.id === dst.id || src.workspace_id !== dst.workspace_id) return { changed: false };
      const gid = dst.group_id || src.group_id || state.next_group_id++;
      src.group_id = gid;
      dst.group_id = gid;
      return { changed: true };
    }

    case 'group_selected_tabs': {
      const targets = state.tabs.filter(
        (t) => !t.worker_kind && t.workspace_id === state.active_workspace_id && state.selected_tab_ids.includes(t.id)
      );
      if (targets.length < 2) return { changed: false };
      const gid = state.next_group_id++;
      for (const t of targets) t.group_id = gid;
      state.selected_tab_ids = [];
      return { changed: true };
    }

    case 'split_tab_group':
    case 'split_selected_tabs': {
      const ids =
        cmd === 'split_tab_group' && Array.isArray(cmdObj.tab_ids)
          ? cmdObj.tab_ids.map(Number)
          : [...state.selected_tab_ids];
      for (const t of state.tabs) {
        if (ids.includes(t.id)) t.group_id = 0;
      }
      state.selected_tab_ids = [];
      return { changed: true };
    }

    case 'set_split_mode': {
      const modeRaw = String(cmdObj.mode || 'columns');
      const mode: ContentViewMode =
        modeRaw === 'rows' || modeRaw === 'spiral' || modeRaw === 'columns' ? modeRaw : 'columns';
      const pairId = Number(cmdObj.pair_tab_id);
      const active = state.tabs[state.active_idx];
      const pair = state.tabs.find(
        (t) => t.id === pairId && !t.worker_kind && (!active || t.id !== active.id)
      );
      if (!pair) return { changed: false };
      state.view_mode = mode;
      state.split_paired_tab_id = pair.id;
      return { changed: true };
    }

    case 'unsplit':
      state.view_mode = 'single';
      state.split_paired_tab_id = null;
      return { changed: true };

    case 'zoom_step': {
      const delta = Number(cmdObj.delta) || 0;
      state.zoom_pct = Math.max(50, Math.min(200, state.zoom_pct + delta));
      return { changed: true };
    }

    case 'zoom_reset':
      state.zoom_pct = 100;
      return { changed: true };

    case 'set_ui_theme':
      state.ui_theme = normalizeTheme(cmdObj.theme);
      return { changed: true };

    case 'open_settings_modal':
      state.settings_open = true;
      if (typeof cmdObj.section === 'string' && cmdObj.section) {
        state.settings_section = cmdObj.section;
      }
      return { changed: true };

    case 'close_settings_modal':
      state.settings_open = false;
      return { changed: true };

    case 'toggle_settings_modal':
      state.settings_open = !state.settings_open;
      return { changed: true };

    case 'set_settings_section':
      if (typeof cmdObj.section === 'string' && cmdObj.section) {
        state.settings_section = cmdObj.section;
      }
      return { changed: true };

    case 'save_setting': {
      const key = String(cmdObj.key || '');
      const val = String(cmdObj.value ?? '');
      const boolVal = val === 'true' || val === '1';
      switch (key) {
        case 'ui_theme':
          state.ui_theme = normalizeTheme(val);
          break;
        case 'logo_style':
          state.logo_style = val === 'arrow' ? 'arrow' : 'ribbon';
          break;
        case 'tab_orientation':
          state.tab_orientation = val === 'horizontal' ? 'horizontal' : 'vertical';
          break;
        case 'sidebar_width': {
          const w = Number(val);
          if (Number.isFinite(w)) state.sidebar_width = Math.max(160, Math.min(420, Math.round(w)));
          break;
        }
        case 'sidebar_collapsed':
          state.sidebar_collapsed = boolVal;
          break;
        case 'show_bookmarks_bar':
          state.show_bookmarks_bar = boolVal;
          break;
        case 'startup_mode':
          state.startup_mode = val === 'newtab' ? 'newtab' : 'restore';
          break;
        case 'background_mode':
          state.background_mode = boolVal;
          break;
        case 'startup_boost':
          state.startup_boost = boolVal;
          break;
        case 'floating_omnibox':
          state.floating_omnibox = boolVal;
          break;
        case 'view_mode':
          if (val === 'single') {
            state.view_mode = 'single';
            state.split_paired_tab_id = null;
          } else if (val === 'columns' || val === 'rows' || val === 'spiral') {
            const active = state.tabs[state.active_idx];
            const candidate = visibleTabEntries(state)
              .map((e) => e.tab)
              .find((t) => !t.worker_kind && (!active || t.id !== active.id));
            if (candidate) {
              state.view_mode = val;
              state.split_paired_tab_id = candidate.id;
            }
          }
          break;
        case 'ai_mode':
          if (val === 'bubble' || val === 'popup' || val === 'dock') state.ai_mode = val;
          break;
        case 'ai_dock_width': {
          const w = Number(val);
          if (Number.isFinite(w)) state.ai_dock_width = Math.max(260, Math.min(1000, Math.round(w)));
          break;
        }
        case 'auto_minimize_on_task':
          state.auto_minimize_on_task = boolVal;
          break;
        case 'show_unread_badges':
          state.show_unread_badges = boolVal;
          break;
        case 'always_show_floating_orbs':
          state.always_show_floating_orbs = boolVal;
          break;
        case 'adblock_enabled':
          state.adblock_enabled = boolVal;
          break;
        case 'google_user':
          state.google_user = val;
          break;
        case 'ai_provider':
          state.ai_provider = val === 'gemini' ? 'gemini' : 'openai';
          break;
        case 'ai_api_key':
          state.ai_api_key = val;
          break;
        case 'ai_model':
          state.ai_model = val;
          break;
        case 'ai_base_url':
          state.ai_base_url = val;
          break;
        default:
          return { changed: false };
      }
      return { changed: true };
    }

    case 'save_ai_config': {
      if (typeof cmdObj.provider === 'string') {
        state.ai_provider = cmdObj.provider === 'gemini' ? 'gemini' : 'openai';
      }
      if (typeof cmdObj.api_key === 'string') state.ai_api_key = cmdObj.api_key;
      if (typeof cmdObj.model === 'string') state.ai_model = cmdObj.model;
      if (typeof cmdObj.base_url === 'string') state.ai_base_url = cmdObj.base_url;
      if (typeof cmdObj.auto_minimize_on_task === 'boolean') {
        state.auto_minimize_on_task = cmdObj.auto_minimize_on_task;
      }
      return { changed: true };
    }

    case 'set_google_user':
      state.google_user = String(cmdObj.name || cmdObj.user || '').trim();
      return { changed: true };

    case 'save_floating_orbs': {
      try {
        const parsed = JSON.parse(String(cmdObj.orbs_json || '[]'));
        if (Array.isArray(parsed)) {
          state.floating_orbs = parsed;
          return { changed: true };
        }
      } catch {
        return { changed: false };
      }
      return { changed: false };
    }

    case 'set_ai_mode': {
      const m = String(cmdObj.mode || 'bubble');
      if (m === 'bubble' || m === 'popup' || m === 'dock') {
        state.ai_mode = m;
        return { changed: true };
      }
      return { changed: false };
    }

    case 'toggle_ai_mode':
      state.ai_mode = state.ai_mode === 'bubble' ? 'dock' : 'bubble';
      return { changed: true };

    case 'toggle_chat_dock_style':
      state.ai_mode = state.ai_mode === 'dock' ? 'popup' : 'dock';
      return { changed: true };

    case 'open_chat_channel':
    case 'toggle_chat_channel': {
      const ch = String(cmdObj.channel || 'ai') as ChatChannel;
      if (!['ai', 'fb', 'zalo', 'custom'].includes(ch)) return { changed: false };
      if (cmd === 'toggle_chat_channel' && state.ai_mode !== 'bubble' && state.chat_channel === ch) {
        state.ai_mode = 'bubble';
      } else {
        state.chat_channel = ch;
        if (state.ai_mode === 'bubble') state.ai_mode = 'dock';
        if (ch === 'ai') state.unread_ai = 0;
        if (ch === 'fb') state.unread_fb = 0;
        if (ch === 'zalo') state.unread_zalo = 0;
      }
      return { changed: true };
    }

    case 'set_dock_width': {
      const w = Number(cmdObj.width);
      if (Number.isFinite(w)) {
        state.ai_dock_width = Math.max(260, Math.min(1000, Math.round(w)));
        return { changed: true };
      }
      return { changed: false };
    }

    case 'set_sidebar_width': {
      const w = Number(cmdObj.width);
      if (Number.isFinite(w)) {
        state.sidebar_width = Math.max(160, Math.min(420, Math.round(w)));
        return { changed: true };
      }
      return { changed: false };
    }

    case 'set_active_workspace': {
      const wsId = String(cmdObj.workspace_id || '');
      if (!state.workspaces.some((w) => w.id === wsId)) return { changed: false };
      state.active_workspace_id = wsId;
      state.selected_tab_ids = [];
      state.view_mode = 'single';
      state.split_paired_tab_id = null;
      ensureActiveTabInWorkspace(state);
      return { changed: true };
    }

    case 'create_workspace': {
      const name = String(cmdObj.name || '').trim().slice(0, 64);
      if (!name) return { changed: false };
      const id = `space_${Date.now().toString(36)}_${state.workspaces.length + 1}`;
      state.workspaces.push({ id, name });
      state.active_workspace_id = id;
      ensureActiveTabInWorkspace(state);
      return { changed: true };
    }

    case 'rename_workspace': {
      const wsId = String(cmdObj.workspace_id || '');
      const name = String(cmdObj.name || '').trim().slice(0, 64);
      const ws = state.workspaces.find((w) => w.id === wsId);
      if (!ws || !name) return { changed: false };
      ws.name = name;
      return { changed: true };
    }

    case 'delete_workspace': {
      const wsId = String(cmdObj.workspace_id || '');
      if (wsId === 'default') return { changed: false };
      const idx = state.workspaces.findIndex((w) => w.id === wsId);
      if (idx === -1) return { changed: false };
      state.workspaces.splice(idx, 1);
      for (const tab of state.tabs) {
        if (tab.workspace_id === wsId) tab.workspace_id = 'default';
      }
      if (state.active_workspace_id === wsId) {
        state.active_workspace_id = 'default';
      }
      ensureActiveTabInWorkspace(state);
      return { changed: true };
    }

    case 'discover_shopee_shops': {
      const stamp = new Date().toLocaleTimeString('vi-VN', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
      state.shopee_discovery = createSeedShopeeDiscovery();
      state.ecom_status_text = `Đã đồng bộ ${state.shopee_discovery.shops.length} shop Shopee lúc ${stamp}`;
      pushLog(state, `Đồng bộ danh sách shop Shopee thành công (${state.shopee_discovery.shops.length} shop) lúc ${stamp}`);
      return { changed: true };
    }

    case 'set_shopee_shop_selection': {
      const platform = String(cmdObj.platform || 'shopee');
      const shopId = String(cmdObj.shop_id || '').trim();
      const selected = Boolean(cmdObj.selected);
      if (!shopId) return { changed: false };
      const exists = state.selected_shops.some((s) => s.platform === platform && s.shop_id === shopId);
      if (selected && !exists) {
        state.selected_shops.push({ platform, shop_id: shopId });
      } else if (!selected && exists) {
        state.selected_shops = state.selected_shops.filter(
          (s) => !(s.platform === platform && s.shop_id === shopId)
        );
      }
      return { changed: true };
    }

    case 'open_shopee_shop': {
      const shopId = String(cmdObj.shop_id || '');
      const shop = state.shopee_discovery.shops.find((s) => s.shop_id === shopId);
      const url = `https://banhang.shopee.vn/portal/sale/order?shop_id=${encodeURIComponent(shopId)}`;
      pushLog(state, `Mở trang quản trị ${shop ? shop.display_name : shopId}`);
      return applyCommand(state, { cmd: 'new_tab', url, isolated: false });
    }

    case 'set_ecom_auto_scan':
      state.ecom_auto_scan = Boolean(cmdObj.enabled);
      pushLog(state, state.ecom_auto_scan ? 'Đã bật tự động quét định kỳ (5 phút/lần)' : 'Đã tắt tự động quét định kỳ');
      return { changed: true };

    case 'run_ecom_scan':
    case 'scan_selected_tabs': {
      const scope = String(cmdObj.scope || 'all');
      const nowLabel = new Date().toLocaleTimeString('vi-VN', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
      const workerUrl =
        scope === 'tiktok'
          ? 'https://seller-vn.tiktok.com/ads-creation/dashboard'
          : 'https://banhang.shopee.vn/portal/sale/order';
      const workerTitle =
        scope === 'tiktok'
          ? 'Worker TikTok Ads'
          : scope === 'shopee_selected'
            ? `Worker Shopee (${state.selected_shops.length} shop)`
            : 'Worker Shopee Seller';

      let worker = state.tabs.find((t) => t.worker_kind === 'bot' && t.url === workerUrl);
      if (!worker) {
        worker = {
          id: state.next_tab_id++,
          url: workerUrl,
          title: workerTitle,
          favicon: '',
          profile: 'worker',
          container_id: 'default',
          workspace_id: state.active_workspace_id,
          worker_kind: 'bot',
          worker_status: `Hoàn tất lúc ${nowLabel}`,
          pinned: false,
          group_id: 0,
          nav_stack: [workerUrl],
          nav_cursor: 0,
        };
        state.tabs.push(worker);
      } else {
        worker.title = workerTitle;
        worker.worker_status = `Hoàn tất lúc ${nowLabel}`;
      }

      state.shopee_orders.ts = `Hôm nay ${nowLabel}`;
      for (const key of Object.keys(state.shopee_shop_orders)) {
        state.shopee_shop_orders[key].ts = `Hôm nay ${nowLabel}`;
      }
      state.shopee_pas.ts = `Hôm nay ${nowLabel}`;
      state.tiktok.ts = `Hôm nay ${nowLabel}`;
      state.ecom_status_text = `Quét (${scope}) hoàn tất lúc ${nowLabel} — ${state.shopee_orders.anomalies.length} bất thường giá & ${state.shopee_orders.fake_reviews.length} cảnh báo đánh giá`;
      pushLog(state, `Background BOT hoàn tất quét [${scope}] lúc ${nowLabel}`);
      return { changed: true };
    }

    default:
      return { changed: false };
  }
}

export function buildTitlebarPayload(state: BrowserState) {
  const visible = visibleTabEntries(state);
  const visibleActiveIdx = Math.max(
    0,
    visible.findIndex((e) => e.nativeIdx === state.active_idx)
  );
  const activeTab = state.tabs[state.active_idx] ?? visible[0]?.tab;
  const activeUrl = activeTab?.url || 'tp://newtab';
  const activeTitle = activeTab?.title || 'Không gian bắt đầu';
  const isBookmarked =
    !activeUrl.startsWith('tp://') && state.bookmarks.some((b) => b.url === activeUrl);

  return {
    ui_theme: state.ui_theme,
    logo_style: state.logo_style,
    orientation: state.tab_orientation,
    show_bookmarks_bar: state.show_bookmarks_bar,
    bookmarks: state.bookmarks,
    history: state.history,
    active_idx: visibleActiveIdx,
    tabs: visible.map(({ nativeIdx, tab }) => ({
      id: tab.id,
      native_idx: nativeIdx,
      url: tab.url,
      title: tab.title,
      favicon: tab.favicon,
      profile: tab.profile,
      container_id: tab.container_id,
      worker_kind: tab.worker_kind,
      worker_status: tab.worker_status,
      pinned: tab.pinned,
      selected: state.selected_tab_ids.includes(tab.id),
      group_id: tab.group_id,
    })),
    active_workspace_id: state.active_workspace_id,
    workspaces: state.workspaces,
    title: `TPbrowser — ${activeTitle}`,
    active_url: activeUrl,
    is_bookmarked: isBookmarked,
    view_mode: state.view_mode,
    paired_tab_id: state.split_paired_tab_id ?? 0,
    zoom_pct: state.zoom_pct,
    pinned: true,
    maximized: true,
  };
}

export function buildTabShellPayload(state: BrowserState) {
  const tb = buildTitlebarPayload(state);
  return {
    orientation: state.tab_orientation,
    sidebar_collapsed: state.sidebar_collapsed,
    view_mode: state.view_mode,
    ai_mode: state.ai_mode,
    chat_channel: state.chat_channel,
    ai_working: false,
    active_idx: tb.active_idx,
    active_workspace_id: state.active_workspace_id,
    workspaces: state.workspaces,
    google_user: state.google_user,
    ui_theme: state.ui_theme,
    logo_style: state.logo_style,
    adblock_enabled: state.adblock_enabled,
    tabs: tb.tabs,
  };
}

export function buildNewTabPayload(state: BrowserState) {
  return {
    google_user: state.google_user,
    ui_theme: state.ui_theme,
    logo_style: state.logo_style,
    floating_omnibox: state.floating_omnibox,
    adblock_enabled: state.adblock_enabled,
    show_unread_badges: state.show_unread_badges,
    ai_mode: state.ai_mode,
    chat_channel: state.chat_channel,
    unread_ai: state.unread_ai,
    unread_fb: state.unread_fb,
    unread_zalo: state.unread_zalo,
    bookmarks: state.bookmarks,
  };
}

export function buildWorkspacePayload(state: BrowserState) {
  return {
    ui_theme: state.ui_theme,
    logo_style: state.logo_style,
    auto_scan: state.ecom_auto_scan,
    status_text: state.ecom_status_text,
    worker_tabs: state.tabs
      .filter((t) => !!t.worker_kind)
      .map((t) => ({
        id: t.id,
        worker_kind: t.worker_kind,
        title: t.title,
        url: t.url,
        status: t.worker_status || 'Đang chạy ngầm',
      })),
    shopee_shop_discovery: state.shopee_discovery,
    selected_shops: state.selected_shops,
    shopee_orders: state.shopee_orders,
    shopee_shop_orders: state.shopee_shop_orders,
    shopee_pas: state.shopee_pas,
    shopee_health: state.shopee_health,
    tiktok: state.tiktok,
    tiktok_health: state.tiktok_health,
    shopee_shop_index: null,
    ecom_dashboard: buildDashboardSnapshot({ platform: 'all', shopId: 'all', timeframe: '30d' }),
    logs: state.ecom_logs,
  };
}

export function buildHistoryPayload(state: BrowserState) {
  return {
    ui_theme: state.ui_theme,
    logo_style: state.logo_style,
    history: state.history,
    closed_tabs: state.closed_tabs,
  };
}

export function buildConfigPayload(state: BrowserState) {
  return {
    ui_theme: state.ui_theme,
    logo_style: state.logo_style,
    startup_mode: state.startup_mode,
    background_mode: state.background_mode,
    startup_boost: state.startup_boost,
    floating_omnibox: state.floating_omnibox,
    compact_titlebar: state.compact_titlebar,
    sidebar_width: state.sidebar_width,
    tab_orientation: state.tab_orientation,
    sidebar_collapsed: state.sidebar_collapsed,
    show_bookmarks_bar: state.show_bookmarks_bar,
    view_mode: state.view_mode,
    ai_provider: state.ai_provider,
    ai_api_key: state.ai_api_key,
    ai_model: state.ai_model,
    ai_base_url: state.ai_base_url,
    auto_minimize_on_task: state.auto_minimize_on_task,
    ai_mode: state.ai_mode,
    show_unread_badges: state.show_unread_badges,
    always_show_floating_orbs: state.always_show_floating_orbs,
    floating_orbs: state.floating_orbs,
    ai_dock_width: state.ai_dock_width,
    adblock_enabled: state.adblock_enabled,
    google_user: state.google_user,
    chat_channel: state.chat_channel,
    unread_ai: state.unread_ai,
    unread_fb: state.unread_fb,
    unread_zalo: state.unread_zalo,
    open_section: state.settings_section,
    app_version: state.app_version,
    app_commit: state.app_commit,
  };
}

export function buildShopeeAnomaliesCsv(state: BrowserState): string {
  const esc = (v: unknown) => `"${String(v ?? '').replace(/"/g, '""')}"`;
  const rows = [
    ['Order SN', 'Buyer', 'Status', 'Item Name', 'Model', 'Qty', 'Effective Price', 'Median Price', 'Reasons'],
    ...state.shopee_orders.anomalies.map((a) => [
      a.order_sn,
      a.buyer,
      a.status,
      a.item_name,
      a.model_name,
      a.qty,
      a.effective_price,
      a.median_price,
      a.reasons.join('; '),
    ]),
  ];
  return '\uFEFF' + rows.map((r) => r.map(esc).join(',')).join('\r\n');
}
