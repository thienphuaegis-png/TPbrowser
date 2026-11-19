import express from 'express';
import fs from 'node:fs';
import path from 'node:path';
import {
  BrowserCommand,
  applyCommand,
  buildConfigPayload,
  buildHistoryPayload,
  buildNewTabPayload,
  buildShopeeAnomaliesCsv,
  buildTabShellPayload,
  buildTitlebarPayload,
  buildWorkspacePayload,
  createInitialState,
} from './src/browserState.js';
import { buildDashboardSnapshot } from './src/ecomDashboard.js';

const app = express();
const PORT = Number(process.env.PORT) || 3000;
const HOST = process.env.HOST || '0.0.0.0';
const ROOT_DIR = process.cwd();

app.use(express.json({ limit: '2mb' }));

const ALLOWED_SURFACES = new Set([
  'titlebar',
  'tab_shell',
  'newtab',
  'workspace',
  'history',
  'settings_modal',
  'ai_panel',
  'chat_header',
]);

let state = createInitialState();

function loadSkills(): unknown[] {
  try {
    const raw = fs.readFileSync(path.join(ROOT_DIR, 'skills', 'skills.json'), 'utf8');
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

let cachedMotionJs: string | null = null;
function getMotionJs(): string {
  if (cachedMotionJs === null) {
    try {
      cachedMotionJs = fs.readFileSync(path.join(ROOT_DIR, 'assets', 'vendor', 'motion.js'), 'utf8');
    } catch {
      cachedMotionJs = '';
    }
  }
  return cachedMotionJs;
}

const FONT_FACE_CSS = `
@font-face {
  font-family: "Cormorant Garamond Gnosis";
  src: url("/assets/fonts/CormorantGaramondGnosis-Variable-Roman.woff2") format("woff2");
  font-weight: 300 700;
  font-style: normal;
  font-display: swap;
}
@font-face {
  font-family: "Cormorant Garamond Gnosis";
  src: url("/assets/fonts/CormorantGaramondGnosis-Variable-Italic.woff2") format("woff2");
  font-weight: 300 700;
  font-style: italic;
  font-display: swap;
}
`;

const BRAND_ASSETS_JS = `
window.__TP_BRAND_ASSETS = {
  logo_ribbon_light: "/assets/icons/logo_ribbon_light.png",
  logo_ribbon_dark: "/assets/icons/logo_ribbon_dark.png",
  logo_arrow_light: "/assets/icons/logo_arrow_light.png",
  logo_arrow_dark: "/assets/icons/logo_arrow_dark.png",
  app_ribbon_light: "/assets/icons/app_ribbon_light.png",
  app_ribbon_dark: "/assets/icons/app_ribbon_dark.png",
  app_arrow_light: "/assets/icons/app_arrow_light.png",
  app_arrow_dark: "/assets/icons/app_arrow_dark.png"
};
window.__tpResolveBrandAsset = function(kind, logoStyle, uiTheme) {
  var k = (kind === "logo") ? "logo" : "app";
  var s = (logoStyle === "arrow") ? "arrow" : "ribbon";
  var isLight = (uiTheme === "carrara-light" || uiTheme === "warm-paper" || uiTheme === "light");
  var t = isLight ? "dark" : "light";
  var key = k + "_" + s + "_" + t;
  return (window.__TP_BRAND_ASSETS && window.__TP_BRAND_ASSETS[key]) || "";
};
window.__tpSetFavicon = function(a, b, c, d) {
  var src = a;
  var elId = "tp_favicon";
  if (arguments.length >= 2 && window.__tpResolveBrandAsset) {
    elId = a || "tp_favicon";
    src = window.__tpResolveBrandAsset(b, c, d);
  }
  if (!src) return;
  var link = document.getElementById(elId) || document.querySelector("link[rel~='icon']");
  if (link && link.getAttribute("href") !== src) link.setAttribute("href", src);
};
`;

function buildIpcShimScript(surfaceName: string, skipIntro: boolean): string {
  const extraTitlebar =
    surfaceName === 'titlebar'
      ? `
    (function setupTitlebarOverlayBridge() {
      var style = document.createElement('style');
      style.textContent =
        '.tb-win-controls { display: none !important; } ' +
        'html.tp-overlay-open, html.tp-overlay-open body { background: transparent !important; border-bottom: none !important; } ' +
        'html.tp-overlay-open .tb-main-bar { background: var(--tb-bg); border-bottom: 1px solid var(--tb-border); }';
      document.head.appendChild(style);
      function notifyOverlay() {
        var pop = document.getElementById('zen_split_popover');
        var cmd = document.getElementById('command_bar_overlay');
        var open = (pop && pop.style.display && pop.style.display !== 'none') ||
                   (cmd && cmd.style.display && cmd.style.display !== 'none');
        document.documentElement.classList.toggle('tp-overlay-open', !!open);
        try { window.parent.postMessage({ __tpTitlebarOverlay: !!open }, '*'); } catch (_) {}
      }
      window.addEventListener('DOMContentLoaded', function() {
        var obs = new MutationObserver(notifyOverlay);
        ['zen_split_popover', 'command_bar_overlay'].forEach(function(id) {
          var el = document.getElementById(id);
          if (el) obs.observe(el, { attributes: true, attributeFilter: ['style', 'class'] });
        });
        notifyOverlay();
      });
    })();
  `
      : '';

  const extraTabShell =
    surfaceName === 'tab_shell'
      ? `
    (function setupTabShellWebPolish() {
      var style = document.createElement('style');
      style.textContent =
        '.v-window-row { display: none !important; } ' +
        '.url-form button { border: 0; background: var(--accent-soft); color: var(--accent-text); width: 24px; height: 24px; border-radius: 999px; cursor: pointer; display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; font-size: 12px; font-weight: 700; transition: background 0.15s ease, color 0.15s ease; } ' +
        '.url-form button:hover { background: var(--accent-grad); color: var(--accent-fg); }';
      document.head.appendChild(style);
    })();
  `
      : '';

  const extraWorkspace =
    surfaceName === 'workspace' && skipIntro
      ? `
    window.addEventListener('DOMContentLoaded', function() {
      var intro = document.getElementById('tp_workspace_intro');
      if (intro) intro.remove();
    });
  `
      : '';

  return `<script>
(function() {
  window.ipc = {
    postMessage: function(raw) {
      try {
        window.parent.postMessage({ __tpIpc: true, surface: ${JSON.stringify(surfaceName)}, payload: raw }, '*');
      } catch (_) {}
    }
  };
  window.addEventListener('message', function(ev) {
    var d = ev && ev.data;
    if (!d || typeof d !== 'object') return;
    if (d.__tpHostCall && typeof window[d.__tpHostCall] === 'function') {
      try { window[d.__tpHostCall].apply(window, d.args || []); } catch (err) { console.error('[TP surface call error]', d.__tpHostCall, err); }
    }
    if (d.__tpConfigEvent) {
      try { window.dispatchEvent(new CustomEvent('tp_config_changed', { detail: d.__tpConfigEvent })); } catch (_) {}
    }
    if (d.__tpSkipIntro) {
      var intro = document.getElementById('tp_workspace_intro');
      if (intro) intro.remove();
    }
  });
  window.addEventListener('keydown', function(ev) {
    if ((ev.altKey && !ev.ctrlKey && !ev.metaKey && (ev.code === 'Comma' || ev.code === 'KeyT' || ev.code === 'KeyA' || ev.code === 'KeyW')) ||
        ((ev.ctrlKey || ev.metaKey) && ev.code === 'KeyK')) {
      ev.preventDefault();
      try { window.parent.postMessage({ __tpShortcut: { altKey: ev.altKey, ctrlKey: ev.ctrlKey, metaKey: ev.metaKey, code: ev.code } }, '*'); } catch (_) {}
    }
  });
  ${extraTitlebar}
  ${extraTabShell}
  ${extraWorkspace}
})();
</script>`;
}

function buildFullClientSnapshot() {
  const activeTab = state.tabs[state.active_idx] ?? state.tabs[0];
  const pairedTab =
    state.view_mode !== 'single' && state.split_paired_tab_id
      ? state.tabs.find((t) => t.id === state.split_paired_tab_id) ?? null
      : null;

  return {
    ui_theme: state.ui_theme,
    logo_style: state.logo_style,
    tab_orientation: state.tab_orientation,
    sidebar_collapsed: state.sidebar_collapsed,
    sidebar_width: state.sidebar_width,
    show_bookmarks_bar: state.show_bookmarks_bar,
    view_mode: state.view_mode,
    zoom_pct: state.zoom_pct,
    ai_mode: state.ai_mode,
    chat_channel: state.chat_channel,
    ai_dock_width: state.ai_dock_width,
    settings_open: state.settings_open,
    always_show_floating_orbs: state.always_show_floating_orbs,
    show_unread_badges: state.show_unread_badges,
    unread_ai: state.unread_ai,
    unread_fb: state.unread_fb,
    unread_zalo: state.unread_zalo,
    floating_orbs: state.floating_orbs,
    active_tab: activeTab
      ? { id: activeTab.id, url: activeTab.url, title: activeTab.title, worker_kind: activeTab.worker_kind }
      : null,
    paired_tab: pairedTab
      ? { id: pairedTab.id, url: pairedTab.url, title: pairedTab.title, worker_kind: pairedTab.worker_kind }
      : null,
    titlebar: buildTitlebarPayload(state),
    tab_shell: buildTabShellPayload(state),
    newtab: buildNewTabPayload(state),
    workspace: buildWorkspacePayload(state),
    history: buildHistoryPayload(state),
    config: buildConfigPayload(state),
    skills: loadSkills(),
  };
}

app.use('/assets', express.static(path.join(ROOT_DIR, 'assets')));
app.use(express.static(path.join(ROOT_DIR, 'public')));

app.get('/vendor/chart.umd.js', (_req, res) => {
  res.sendFile(path.join(ROOT_DIR, 'node_modules', 'chart.js', 'dist', 'chart.umd.js'));
});

app.get('/surface/:name', (req, res) => {
  const name = String(req.params.name || '').trim();
  if (!ALLOWED_SURFACES.has(name)) {
    return res.status(404).send('Unknown surface');
  }
  const filePath = path.join(ROOT_DIR, 'src', `${name}.html`);
  if (!fs.existsSync(filePath)) {
    return res.status(404).send('Surface template missing');
  }

  let html = fs.readFileSync(filePath, 'utf8');
  if (state.ui_theme !== 'obsidian-gold') {
    html = html.replace('data-theme="obsidian-gold"', `data-theme="${state.ui_theme}"`);
  }

  let bootstrapJs = '';
  if (name === 'titlebar') {
    bootstrapJs = `window.__TP_BOOTSTRAP__ = ${JSON.stringify(buildTitlebarPayload(state))};`;
  } else if (name === 'tab_shell') {
    bootstrapJs = `window.__TP_BOOTSTRAP__ = ${JSON.stringify(buildTabShellPayload(state))};`;
  } else if (name === 'workspace') {
    bootstrapJs = `window.__TP_ECOM_BOOTSTRAP__ = ${JSON.stringify(
      buildDashboardSnapshot({ platform: 'all', shopId: 'all', timeframe: '30d' })
    )};`;
  }

  html = html
    .replace('/*__TP_FONT_FACE__*/', FONT_FACE_CSS)
    .replace('/*__TP_MOTION_JS__*/', getMotionJs())
    .replace('/*__TP_BRAND_ASSETS_JS__*/', BRAND_ASSETS_JS)
    .replace('/*__TP_BOOTSTRAP_JS__*/', bootstrapJs);

  const skipIntro = req.query.autostart === '1' || req.query.skipIntro === '1';
  const shim = buildIpcShimScript(name, skipIntro);
  html = html.replace('<head>', `<head>\n${shim}`);

  res.type('html').send(html);
});

app.get('/api/state', (_req, res) => {
  res.json(buildFullClientSnapshot());
});

app.get('/api/skills', (_req, res) => {
  res.json(loadSkills());
});

app.post('/api/cmd', (req, res) => {
  const cmdObj = (req.body && typeof req.body === 'object' ? req.body : {}) as BrowserCommand;
  const cmd = String(cmdObj.cmd || '');

  if (cmd === 'export_shopee_csv') {
    return res.json({
      ok: true,
      action: 'download_csv',
      download_url: '/api/export/shopee.csv',
      toast: 'Đang tải xuống file CSV bất thường giá Shopee...',
      state: buildFullClientSnapshot(),
    });
  }

  if (cmd === 'check_app_update') {
    return res.json({
      ok: true,
      action: 'update_status',
      update_status: {
        has_update: false,
        current_commit: state.app_commit,
        message: 'TPbrowser Web Runtime đang chạy phiên bản mới nhất (v0.9.11-web).',
      },
      state: buildFullClientSnapshot(),
    });
  }

  const result = applyCommand(state, cmdObj);
  return res.json({
    ok: true,
    changed: result.changed,
    toast: result.toast,
    state: buildFullClientSnapshot(),
  });
});

app.get('/api/ecom/dashboard', (req, res) => {
  const snapshot = buildDashboardSnapshot({
    platform: req.query.platform,
    shopId: req.query.shopId,
    timeframe: req.query.timeframe,
  });
  res.json(snapshot);
});

app.get('/api/export/ecom.csv', (req, res) => {
  const snapshot = buildDashboardSnapshot({
    platform: req.query.platform,
    shopId: req.query.shopId,
    timeframe: req.query.timeframe,
  });
  const esc = (v: unknown) => `"${String(v ?? '').replace(/"/g, '""')}"`;
  const rows = [
    ['Ma SKU', 'Ten San Pham', 'Nen Tang', 'Gian Hang', 'Ton Kho', 'Traffic', 'CTR (%)', 'CR (%)', 'Danh Gia', 'De Xuat Hanh Dong'],
    ...snapshot.skus.map((s) => [
      s.code,
      s.name,
      s.platform === 'shopee' ? 'Shopee' : 'TikTok Shop',
      s.shopName,
      s.stock,
      s.traffic,
      s.ctr.toFixed(2),
      s.cr.toFixed(2),
      s.rating.toFixed(2),
      s.actionLabel,
    ]),
  ];
  const csv = '\uFEFF' + rows.map((r) => r.map(esc).join(',')).join('\r\n');
  res.setHeader('Content-Type', 'text/csv; charset=utf-8');
  res.setHeader(
    'Content-Disposition',
    `attachment; filename="tpbrowser_${snapshot.platform}_${snapshot.shopId}_${snapshot.timeframe}.csv"`
  );
  res.send(csv);
});

app.get('/api/export/shopee.csv', (_req, res) => {
  const csv = buildShopeeAnomaliesCsv(state);
  res.setHeader('Content-Type', 'text/csv; charset=utf-8');
  res.setHeader('Content-Disposition', 'attachment; filename="tpbrowser_shopee_anomalies.csv"');
  res.send(csv);
});

app.post('/api/reset', (_req, res) => {
  state = createInitialState();
  res.json({ ok: true, state: buildFullClientSnapshot() });
});

app.get('/health', (_req, res) => {
  res.json({ status: 'ok', app: 'TPbrowser', version: state.app_version });
});

app.get('/', (_req, res) => {
  res.sendFile(path.join(ROOT_DIR, 'public', 'index.html'));
});

app.listen(PORT, HOST, () => {
  console.log(`TPbrowser Web Runtime listening at http://${HOST}:${PORT}`);
});
