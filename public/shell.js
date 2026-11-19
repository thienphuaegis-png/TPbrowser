(function () {
  const autostart = new URLSearchParams(window.location.search).get('autostart') === '1';

  const frameTitlebar = document.getElementById('frame_titlebar');
  const frameTabShell = document.getElementById('frame_tab_shell');
  const frameChatHeader = document.getElementById('frame_chat_header');
  const frameAiPanel = document.getElementById('frame_ai_panel');
  const frameSettings = document.getElementById('frame_settings');

  const titlebarSlot = document.getElementById('titlebar_slot');
  const sidebarSlot = document.getElementById('sidebar_slot');
  const sidebarResizer = document.getElementById('sidebar_resizer');
  const btnSidebarCollapse = document.getElementById('btn_toggle_sidebar_collapse');
  const contentStage = document.getElementById('content_stage');
  const panePrimary = document.getElementById('pane_primary');
  const paneSecondary = document.getElementById('pane_secondary');
  const dockResizer = document.getElementById('dock_resizer');
  const chatPanelSlot = document.getElementById('chat_panel_slot');
  const chatBodyAi = document.getElementById('chat_body_ai');
  const chatBodyChannel = document.getElementById('chat_body_channel');
  const floatingOrbsHost = document.getElementById('floating_orbs_host');
  const settingsOverlay = document.getElementById('settings_overlay');
  const toastEl = document.getElementById('tp_toast');

  let currentState = null;
  let toastTimer = null;

  /** Map<string, HTMLElement> keyed by paneKey ("primary:url" or "secondary:url") */
  const surfaceCache = new Map();

  function showToast(msg) {
    if (!msg) return;
    toastEl.textContent = msg;
    toastEl.hidden = false;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toastEl.hidden = true;
    }, 2600);
  }

  function callFrame(frame, fnName, ...args) {
    if (!frame || !frame.contentWindow) return;
    try {
      frame.contentWindow.postMessage({ __tpHostCall: fnName, args }, '*');
    } catch (_) {}
  }

  function emitConfigChanged(frame, cfg) {
    if (!frame || !frame.contentWindow) return;
    try {
      frame.contentWindow.postMessage({ __tpConfigEvent: cfg }, '*');
    } catch (_) {}
  }

  function surfaceRouteForUrl(url) {
    if (url === 'tp://newtab') return '/surface/newtab';
    if (url === 'tp://workspace') return autostart ? '/surface/workspace?autostart=1' : '/surface/workspace';
    if (url === 'tp://history') return '/surface/history';
    return null;
  }

  function syncInternalSurfaceFrame(frame, url, st) {
    if (!frame || !st) return;
    if (url === 'tp://newtab') {
      callFrame(frame, '__tpSyncNewTab', st.newtab);
    } else if (url === 'tp://workspace') {
      callFrame(frame, '__tpSyncWorkspace', st.workspace);
      if (autostart) {
        try { frame.contentWindow.postMessage({ __tpSkipIntro: true }, '*'); } catch (_) {}
      }
    } else if (url === 'tp://history') {
      callFrame(frame, '__tpSyncHistory', st.history);
    }
    emitConfigChanged(frame, st.config);
  }

  function buildExternalTabElement(tab) {
    const wrap = document.createElement('div');
    wrap.className = 'external-tab-view';
    wrap.dataset.tabUrl = tab.url;

    const banner = document.createElement('div');
    banner.className = 'external-tab-banner';
    banner.innerHTML =
      '<span class="ext-url"></span>' +
      '<div style="display:flex;gap:6px;flex-shrink:0;">' +
        '<button type="button" class="tp-btn" data-action="home">Về Trang Chủ</button>' +
        '<button type="button" class="tp-btn" data-action="workspace">Mở Sàn TMĐT</button>' +
      '</div>';
    banner.querySelector('.ext-url').textContent = tab.url;
    banner.querySelector('[data-action="home"]').addEventListener('click', () => {
      sendCmd({ cmd: 'navigate_active', url: 'tp://newtab' });
    });
    banner.querySelector('[data-action="workspace"]').addEventListener('click', () => {
      sendCmd({ cmd: 'open_workspace' });
    });

    const body = document.createElement('div');
    body.className = 'external-tab-body';

    const card = document.createElement('div');
    card.className = 'external-preview-card';

    const h2 = document.createElement('h2');
    h2.textContent = tab.title || tab.url;

    const p = document.createElement('p');
    p.textContent =
      'Trang web bên ngoài được quản lý qua WebView2 trên bản TPbrowser Desktop. Trong chế độ Web Preview, bạn có thể mở trực tiếp trang này ở tab trình duyệt mới hoặc quay lại Không gian TMĐT.';

    const urlBox = document.createElement('div');
    urlBox.className = 'external-preview-url';
    urlBox.textContent = tab.url;

    const actions = document.createElement('div');
    actions.className = 'external-preview-actions';

    const openLink = document.createElement('a');
    openLink.className = 'tp-btn tp-btn-cta';
    openLink.href = tab.url;
    openLink.target = '_blank';
    openLink.rel = 'noopener noreferrer';
    openLink.textContent = 'Mở liên kết ở tab trình duyệt ↗';

    const bmBtn = document.createElement('button');
    bmBtn.type = 'button';
    bmBtn.className = 'tp-btn';
    bmBtn.textContent = '★ Ghim trang này';
    bmBtn.addEventListener('click', () => sendCmd({ cmd: 'bookmark_active' }));

    actions.appendChild(openLink);
    actions.appendChild(bmBtn);

    card.appendChild(h2);
    card.appendChild(p);
    card.appendChild(urlBox);
    card.appendChild(actions);
    body.appendChild(card);

    wrap.appendChild(banner);
    wrap.appendChild(body);
    return wrap;
  }

  function renderPane(paneEl, paneName, tab, st) {
    if (!tab) {
      paneEl.hidden = true;
      return;
    }
    paneEl.hidden = false;
    const route = surfaceRouteForUrl(tab.url);

    // Hide all existing children first
    Array.from(paneEl.children).forEach((child) => {
      child.style.display = 'none';
    });

    if (route) {
      const cacheKey = `${paneName}:${tab.url}`;
      let iframe = surfaceCache.get(cacheKey);
      if (!iframe) {
        iframe = document.createElement('iframe');
        iframe.className = 'tab-surface-frame';
        iframe.title = tab.title || tab.url;
        iframe.src = route;
        iframe.dataset.tpUrl = tab.url;
        iframe.addEventListener('load', () => {
          if (currentState) syncInternalSurfaceFrame(iframe, tab.url, currentState);
        });
        paneEl.appendChild(iframe);
        surfaceCache.set(cacheKey, iframe);
      } else {
        if (iframe.parentNode !== paneEl) paneEl.appendChild(iframe);
        iframe.style.display = 'block';
        syncInternalSurfaceFrame(iframe, tab.url, st);
      }
      iframe.style.zoom = `${st.zoom_pct || 100}%`;
    } else {
      const extKey = `${paneName}:ext:${tab.id}:${tab.url}`;
      let view = surfaceCache.get(extKey);
      if (!view) {
        view = buildExternalTabElement(tab);
        paneEl.appendChild(view);
        surfaceCache.set(extKey, view);
      } else {
        if (view.parentNode !== paneEl) paneEl.appendChild(view);
        view.style.display = 'flex';
      }
    }
  }

  function renderFloatingOrbs(st) {
    floatingOrbsHost.hidden = !st.always_show_floating_orbs;
    floatingOrbsHost.innerHTML = '';
    const orbs = Array.isArray(st.floating_orbs) ? st.floating_orbs : [];
    const unreadMap = {
      ai: st.unread_ai || 0,
      fb: st.unread_fb || 0,
      zalo: st.unread_zalo || 0,
    };

    for (const orb of orbs) {
      if (!orb || orb.enabled === false) continue;
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'tp-orb-btn';
      const isBuiltin = orb.id === 'ai' || orb.id === 'fb' || orb.id === 'zalo';
      const isActive = isBuiltin && st.ai_mode !== 'bubble' && st.chat_channel === orb.id;
      if (isActive) btn.classList.add('active');
      btn.title = orb.title || orb.label || 'App';
      btn.setAttribute('aria-label', `Mở ${orb.title || orb.label || 'App'}`);
      btn.textContent = orb.label || '•';

      if (isActive) {
        const dot = document.createElement('span');
        dot.className = 'tp-orb-dot';
        btn.appendChild(dot);
      }

      const unread = isBuiltin && st.show_unread_badges ? unreadMap[orb.id] || 0 : 0;
      if (unread > 0) {
        const badge = document.createElement('span');
        badge.className = 'tp-orb-unread';
        badge.textContent = unread > 99 ? '99+' : String(unread);
        btn.appendChild(badge);
      }

      btn.addEventListener('click', () => {
        if (isBuiltin) {
          sendCmd({ cmd: 'toggle_chat_channel', channel: orb.id });
        } else if (orb.url) {
          sendCmd({ cmd: 'new_tab', url: orb.url, isolated: false });
        }
      });
      floatingOrbsHost.appendChild(btn);
    }
  }

  function renderChatPanel(st) {
    const open = st.ai_mode === 'dock' || st.ai_mode === 'popup';
    document.body.classList.toggle('ai-dock', st.ai_mode === 'dock');
    document.body.classList.toggle('ai-popup', st.ai_mode === 'popup');
    document.body.classList.toggle('ai-bubble', st.ai_mode === 'bubble');

    chatPanelSlot.hidden = !open;
    dockResizer.hidden = st.ai_mode !== 'dock';
    if (st.ai_mode === 'dock') {
      const dockW = st.ai_dock_width || 360;
      chatPanelSlot.style.width = `${dockW}px`;
      document.documentElement.style.setProperty('--ai-dock-w', `${dockW}px`);
    } else {
      chatPanelSlot.style.width = '';
    }

    const ch = st.chat_channel || 'ai';
    const isAi = ch === 'ai';
    chatBodyAi.hidden = !isAi;
    chatBodyChannel.hidden = isAi;

    if (!isAi) {
      const badge = document.getElementById('channel_badge_icon');
      const title = document.getElementById('channel_card_title');
      const link = document.getElementById('channel_external_link');
      if (ch === 'fb') {
        badge.textContent = 'FB';
        title.textContent = 'Messenger';
        link.href = 'https://www.messenger.com';
      } else {
        badge.textContent = 'ZL';
        title.textContent = 'Zalo Web';
        link.href = 'https://chat.zalo.me';
      }
    }

    callFrame(frameChatHeader, '__tpSyncChatHeader', {
      channel: ch,
      custom_title: ch === 'fb' ? 'Messenger' : ch === 'zalo' ? 'Zalo' : 'AI Cowork',
      ai_mode: st.ai_mode,
      ui_theme: st.ui_theme,
      show_unread_badges: st.show_unread_badges,
      unread_ai: st.unread_ai,
      unread_fb: st.unread_fb,
      unread_zalo: st.unread_zalo,
    });
  }

  function applyStateToHost(st) {
    if (!st) return;
    currentState = st;

    // Theme & favicon
    const theme = st.ui_theme === 'carrara-light' ? 'carrara-light' : 'obsidian-gold';
    document.documentElement.setAttribute('data-theme', theme);
    const fav = document.getElementById('host_favicon');
    if (fav) {
      const style = st.logo_style === 'arrow' ? 'arrow' : 'ribbon';
      const tone = theme === 'carrara-light' ? 'dark' : 'light';
      fav.href = `/assets/icons/app_${style}_${tone}.png`;
    }

    // Orientation & sidebar geometry
    const isHoriz = st.tab_orientation === 'horizontal';
    document.body.classList.toggle('orient-horizontal', isHoriz);
    document.body.classList.toggle('orient-vertical', !isHoriz);
    document.body.classList.toggle('show-bm-bar', !!st.show_bookmarks_bar);

    const sbWidth = st.sidebar_collapsed ? 54 : (st.sidebar_width || 228);
    sidebarSlot.style.width = `${sbWidth}px`;
    btnSidebarCollapse.textContent = st.sidebar_collapsed ? '»' : '«';

    // Content stage split layout
    const isSplit = st.view_mode !== 'single' && !!st.paired_tab;
    contentStage.className = 'content-stage ' + (isSplit ? `split-${st.view_mode}` : 'split-single');
    renderPane(panePrimary, 'primary', st.active_tab, st);
    renderPane(paneSecondary, 'secondary', isSplit ? st.paired_tab : null, st);

    // Chat dock/popup & floating orbs
    renderChatPanel(st);
    renderFloatingOrbs(st);

    // Zen Settings Modal
    settingsOverlay.hidden = !st.settings_open;

    // Sync child surfaces
    callFrame(frameTitlebar, '__tpSyncTitlebar', st.titlebar);
    callFrame(frameTabShell, '__tpSyncShellState', st.tab_shell);
    if (st.settings_open) {
      callFrame(frameSettings, '__tpSyncSettingsModal', st.config);
    }

    emitConfigChanged(frameTitlebar, st.config);
    emitConfigChanged(frameTabShell, st.config);
    emitConfigChanged(frameChatHeader, st.config);
    emitConfigChanged(frameAiPanel, st.config);
    emitConfigChanged(frameSettings, st.config);

    // Sync AI panel config + skills
    callFrame(frameAiPanel, '__tpLoadAiConfigFromRust', {
      ui_theme: st.ui_theme,
      provider: st.config.ai_provider,
      api_key: st.config.ai_api_key,
      model: st.config.ai_model,
      base_url: st.config.ai_base_url,
      auto_minimize_on_task: st.config.auto_minimize_on_task,
      skills: st.skills || [],
    });
  }

  async function fetchState() {
    try {
      const resp = await fetch('/api/state');
      if (!resp.ok) return;
      const st = await resp.json();
      applyStateToHost(st);
    } catch (err) {
      console.error('[TPbrowser] Failed to fetch state:', err);
    }
  }

  async function sendCmd(payload) {
    if (!payload || !payload.cmd) return;

    // Client-only UI commands
    if (payload.cmd === 'ui_shell_ready' || payload.cmd === 'settings_modal_ready' || payload.cmd === 'ai_panel_ready' || payload.cmd === 'get_history') {
      if (currentState) {
        applyStateToHost(currentState);
      } else {
        await fetchState();
      }
      return;
    }

    if (payload.cmd === 'copy_to_clipboard') {
      if (navigator.clipboard && payload.text) {
        navigator.clipboard.writeText(String(payload.text)).catch(() => {});
      }
      showToast('Đã sao chép vào bộ nhớ tạm');
      return;
    }

    if (payload.cmd === 'win_minimize' || payload.cmd === 'win_toggle_maximize' || payload.cmd === 'win_close' || payload.cmd === 'drag_window') {
      return;
    }

    if (payload.cmd === 'run_ai_prompt') {
      await sendCmd({ cmd: 'open_chat_channel', channel: 'ai' });
      showToast('Đã gửi yêu cầu tới AI Cowork');
      return;
    }

    if (payload.cmd === 'focus_ai_panel') {
      await sendCmd({ cmd: 'open_chat_channel', channel: 'ai' });
      return;
    }

    try {
      const resp = await fetch('/api/cmd', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      if (!resp.ok) return;
      const data = await resp.json();
      if (data.toast) showToast(data.toast);
      if (data.action === 'download_csv' && data.download_url) {
        const a = document.createElement('a');
        a.href = data.download_url;
        a.download = 'tpbrowser_shopee_anomalies.csv';
        document.body.appendChild(a);
        a.click();
        a.remove();
      }
      if (data.action === 'update_status' && data.update_status) {
        callFrame(frameSettings, '__tpOnUpdateStatus', data.update_status);
      }
      if (data.state) {
        applyStateToHost(data.state);
      }
    } catch (err) {
      console.error('[TPbrowser] Command failed:', payload.cmd, err);
    }
  }

  // Listen to IPC messages from all TPbrowser surface iframes
  window.addEventListener('message', (ev) => {
    const data = ev && ev.data;
    if (!data || typeof data !== 'object') return;

    if (typeof data.__tpTitlebarOverlay === 'boolean') {
      titlebarSlot.classList.toggle('overlay-expanded', data.__tpTitlebarOverlay);
      return;
    }

    if (data.__tpShortcut) {
      handleShortcut(data.__tpShortcut);
      return;
    }

    if (data.__tpIpc && typeof data.payload === 'string') {
      try {
        const parsed = JSON.parse(data.payload);
        sendCmd(parsed);
      } catch (_) {}
    }
  });

  function handleShortcut(ev) {
    if (ev.altKey && !ev.ctrlKey && !ev.metaKey) {
      if (ev.code === 'Comma') {
        sendCmd({ cmd: 'toggle_settings_modal' });
      } else if (ev.code === 'KeyT') {
        sendCmd({ cmd: 'toggle_tab_orientation' });
      } else if (ev.code === 'KeyA') {
        sendCmd({ cmd: 'toggle_ai_mode' });
      } else if (ev.code === 'KeyW') {
        sendCmd({ cmd: 'open_workspace' });
      }
    } else if ((ev.ctrlKey || ev.metaKey) && ev.code === 'KeyK') {
      callFrame(frameTitlebar, '__tpOpenCommandBar');
    }
  }

  window.addEventListener('keydown', (ev) => {
    if ((ev.altKey && !ev.ctrlKey && !ev.metaKey && ['Comma', 'KeyT', 'KeyA', 'KeyW'].includes(ev.code)) ||
        ((ev.ctrlKey || ev.metaKey) && ev.code === 'KeyK')) {
      ev.preventDefault();
      handleShortcut(ev);
    }
  });

  // Sidebar collapse button
  btnSidebarCollapse.addEventListener('click', () => {
    sendCmd({ cmd: 'toggle_sidebar_collapse' });
  });

  document.getElementById('btn_switch_to_ai').addEventListener('click', () => {
    sendCmd({ cmd: 'open_chat_channel', channel: 'ai' });
  });

  // Drag resizers (Sidebar + AI Right Dock)
  function attachHorizontalResizer(handleEl, onDrag, onEnd) {
    handleEl.addEventListener('mousedown', (ev) => {
      ev.preventDefault();
      document.body.classList.add('resizing');
      handleEl.classList.add('dragging');
      const onMove = (moveEv) => onDrag(moveEv.clientX);
      const onUp = (upEv) => {
        document.body.classList.remove('resizing');
        handleEl.classList.remove('dragging');
        window.removeEventListener('mousemove', onMove);
        window.removeEventListener('mouseup', onUp);
        onEnd(upEv.clientX);
      };
      window.addEventListener('mousemove', onMove);
      window.addEventListener('mouseup', onUp);
    });
  }

  attachHorizontalResizer(
    sidebarResizer,
    (x) => {
      const clamped = Math.max(160, Math.min(420, Math.round(x)));
      sidebarSlot.style.width = `${clamped}px`;
    },
    (x) => {
      const clamped = Math.max(160, Math.min(420, Math.round(x)));
      sendCmd({ cmd: 'set_sidebar_width', width: clamped });
    }
  );

  attachHorizontalResizer(
    dockResizer,
    (x) => {
      const w = Math.max(260, Math.min(900, Math.round(window.innerWidth - x)));
      chatPanelSlot.style.width = `${w}px`;
      document.documentElement.style.setProperty('--ai-dock-w', `${w}px`);
    },
    (x) => {
      const w = Math.max(260, Math.min(900, Math.round(window.innerWidth - x)));
      sendCmd({ cmd: 'set_dock_width', width: w });
    }
  );

  // Sync frames on load
  [frameTitlebar, frameTabShell, frameChatHeader, frameAiPanel, frameSettings].forEach((f) => {
    f.addEventListener('load', () => {
      if (currentState) applyStateToHost(currentState);
    });
  });

  fetchState();
})();
