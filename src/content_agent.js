// Script tiêm vào mỗi Tab web của TPbrowser:
// 1. Chuyển tiếp phím tắt điều khiển trình duyệt (Alt+T, Alt+A, Alt+N, Alt+P, Alt+W, Alt+M).
// 2. Hiển thị Bong bóng Chat AI Nổi (Floating Bubble / Mini-Pill) khi cửa sổ AI thu gọn — vẫn hoạt động ngầm & có nút [🔴 Làm mẫu / ⏹ Lưu Skill].
// 3. Chế độ "🔴 Làm mẫu 1 lần (Record)": Ghi nhận thao tác Click / Điền chữ thật của anh Phú trên trang web (kèm nhãn ngữ nghĩa & selector bền vững) để lưu thành Skill.
// 4. Thực thi thao tác Cowork (hỗ trợ cả thẻ contenteditable của Zalo/Messenger/Docs và tìm nút theo Nhãn ngữ nghĩa khi trang đổi giao diện).
(function () {
  var TAB_ID = __TAB_ID__;
  if ((window.location.hostname || "").toLowerCase().indexOf("tiktok.com") !== -1) {
    return;
  }
  var IS_INTERNAL_HTML_PAGE =
    window.location.protocol === "about:" || window.location.protocol === "data:";
  window.__tpRecordingSkill = false;

  function sendRust(payload) {
    try {
      if (window.ipc && typeof window.ipc.postMessage === "function") {
        window.ipc.postMessage(JSON.stringify(Object.assign({ tab_id: TAB_ID }, payload)));
      }
    } catch (_) {}
  }

  window.addEventListener(
    "mousedown",
    function () {
      sendRust({ cmd: "focus_this_tab" });
    },
    true
  );

  function zoomFromCtrlWheel(e) {
    if (!e.ctrlKey) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    if (!e.deltaY) return;
    sendRust({ cmd: "zoom_step", delta: e.deltaY < 0 ? 10 : -10 });
  }
  window.addEventListener("wheel", zoomFromCtrlWheel, { capture: true, passive: false });

  function pickTabFavicon() {
    try {
      var links = document.querySelectorAll("link[rel]");
      var best = "";
      var bestScore = -1;
      for (var i = 0; i < links.length; i++) {
        var rel = (links[i].getAttribute("rel") || "").toLowerCase();
        if (rel.indexOf("icon") === -1) continue;
        var href = links[i].getAttribute("href");
        if (!href) continue;
        try {
          var resolved = new URL(href, window.location.href).href;
          var score = 1;
          if (rel === "icon" || rel === "shortcut icon") score += 3;
          if (href.indexOf(".svg") !== -1 || href.indexOf("32") !== -1 || href.indexOf("48") !== -1) score += 2;
          if (rel.indexOf("apple-touch") !== -1) score -= 1;
          if (score > bestScore) {
            bestScore = score;
            best = resolved;
          }
        } catch (_) {}
      }
      if (best) return best;
      if (window.location.protocol === "http:" || window.location.protocol === "https:") {
        return window.location.origin + "/favicon.ico";
      }
    } catch (_) {}
    return "";
  }

  var lastMetaSig = "";
  function reportTabMeta() {
    try {
      var u = window.location.href || "";
      if (!u || u === "about:blank" || u.startsWith("data:")) return;
      var t = (document.title || "").trim();
      var f = pickTabFavicon();
      var sig = u + "|" + t + "|" + f;
      if (sig === lastMetaSig) return;
      lastMetaSig = sig;
      sendRust({ cmd: "tab_meta", url: u, title: t, favicon: f });
    } catch (_) {}
  }

  // Hook SPA History & URL changes (YouTube, Shopee, Facebook, Google...)
  (function hookSpaNavigation() {
    try {
      var origPush = history.pushState;
      if (typeof origPush === "function") {
        history.pushState = function () {
          var res = origPush.apply(this, arguments);
          setTimeout(reportTabMeta, 60);
          return res;
        };
      }
      var origReplace = history.replaceState;
      if (typeof origReplace === "function") {
        history.replaceState = function () {
          var res = origReplace.apply(this, arguments);
          setTimeout(reportTabMeta, 60);
          return res;
        };
      }
      window.addEventListener("popstate", function () { setTimeout(reportTabMeta, 60); }, true);
      window.addEventListener("hashchange", function () { setTimeout(reportTabMeta, 60); }, true);
    } catch (_) {}
  })();

  if (!IS_INTERNAL_HTML_PAGE) {
    if (document.readyState === "loading") {
      document.addEventListener("DOMContentLoaded", function () {
        reportTabMeta();
        setTimeout(reportTabMeta, 400);
      });
    } else {
      reportTabMeta();
    }
    window.addEventListener("load", function () {
      reportTabMeta();
      setTimeout(reportTabMeta, 600);
    });
    setInterval(reportTabMeta, 1000);
  }

  // Hàm trích xuất Nhãn ngữ nghĩa (Semantic Label) & Selector bền vững của 1 phần tử DOM
  function describeElement(el) {
    if (!el || !el.tagName) return null;
    var tag = el.tagName.toLowerCase();
    var label =
      el.getAttribute("aria-label") ||
      el.getAttribute("placeholder") ||
      el.getAttribute("title") ||
      el.getAttribute("name") ||
      (tag === "input" || tag === "textarea" ? "" : el.innerText) ||
      el.value ||
      "";
    label = String(label).replace(/\s+/g, " ").trim().slice(0, 65);

    var selector = tag;
    if (el.id && !/\d{5,}/.test(el.id)) {
      selector = "#" + CSS.escape(el.id);
    } else if (el.getAttribute("name")) {
      selector = tag + '[name="' + el.getAttribute("name").replace(/"/g, '\\"') + '"]';
    } else if (el.getAttribute("placeholder")) {
      selector = tag + '[placeholder="' + el.getAttribute("placeholder").replace(/"/g, '\\"') + '"]';
    } else if (el.getAttribute("aria-label")) {
      selector = tag + '[aria-label="' + el.getAttribute("aria-label").replace(/"/g, '\\"') + '"]';
    } else if (el.getAttribute("data-testid")) {
      selector = '[data-testid="' + el.getAttribute("data-testid").replace(/"/g, '\\"') + '"]';
    }
    return {
      tag: tag,
      label: label,
      selector: selector,
      is_contenteditable: !!el.isContentEditable
    };
  }

  window.__tpGlanceTrigger = "alt_click";
  window.__tpGlanceExternal = true;
  window.__tpIsPinnedOrEssential = false;
  window.__tpTranslateTargetLang = "vi";
  window.__tpTranslateAutoPrompt = true;
  window.__tpTranslateNeverDomains = "shopee.vn,seller-vn.tiktok.com,chat.zalo.me";

  window.addEventListener("tp_config_changed", function(ev) {
    if (ev && ev.detail) {
      if (ev.detail.glance_trigger) window.__tpGlanceTrigger = ev.detail.glance_trigger;
      if (typeof ev.detail.glance_external_from_pinned === "boolean") window.__tpGlanceExternal = ev.detail.glance_external_from_pinned;
      if (ev.detail.translate_target_lang) window.__tpTranslateTargetLang = ev.detail.translate_target_lang;
      if (typeof ev.detail.translate_auto_prompt === "boolean") window.__tpTranslateAutoPrompt = ev.detail.translate_auto_prompt;
      if (typeof ev.detail.translate_never_domains === "string") window.__tpTranslateNeverDomains = ev.detail.translate_never_domains;
      if (ev.detail.boosts && typeof ev.detail.boosts === "object" && window.__tpApplyBoosts) window.__tpApplyBoosts(ev.detail.boosts);
    }
  });

  window.__tpConfigureContent = function(cfg) {
    if (!cfg) return;
    if (cfg.glance_trigger) window.__tpGlanceTrigger = cfg.glance_trigger;
    if (typeof cfg.glance_external_from_pinned === "boolean") window.__tpGlanceExternal = cfg.glance_external_from_pinned;
    if (typeof cfg.is_pinned_or_essential === "boolean") window.__tpIsPinnedOrEssential = cfg.is_pinned_or_essential;
    if (cfg.translate_target_lang) window.__tpTranslateTargetLang = cfg.translate_target_lang;
    if (typeof cfg.translate_auto_prompt === "boolean") window.__tpTranslateAutoPrompt = cfg.translate_auto_prompt;
    if (typeof cfg.translate_never_domains === "string") window.__tpTranslateNeverDomains = cfg.translate_never_domains;
  };

  // Zen Glance: bắt ở pointerdown để Alt+Click hoặc Shift+Click không bị WebView/website nuốt mất click.
  function isGlanceTriggerPressed(e) {
    if (e.button !== 0 || e.ctrlKey || e.metaKey) return false;
    if (window.__tpGlanceTrigger === "shift_click") {
      return e.shiftKey && !e.altKey;
    }
    return e.altKey && !e.shiftKey;
  }

  function openGlanceFromAltClick(e) {
    var link = e.target && e.target.closest && e.target.closest("a[href]");
    if (!link) return;
    var href = link.getAttribute("href") || "";
    if (!href || href.indexOf("javascript:") === 0 || href.indexOf("#") === 0) return;

    var matched = isGlanceTriggerPressed(e);
    var isExternalFromPinned = false;
    if (!matched && window.__tpIsPinnedOrEssential && window.__tpGlanceExternal && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey && e.button === 0) {
      try {
        var testUrl = new URL(href, window.location.href);
        if (testUrl.origin !== window.location.origin && (testUrl.protocol === "http:" || testUrl.protocol === "https:")) {
          isExternalFromPinned = true;
        }
      } catch (_) {}
    }

    if (!matched && !isExternalFromPinned) return;

    try {
      var resolved = new URL(href, window.location.href).href;
      if (resolved.indexOf("http://") !== 0 && resolved.indexOf("https://") !== 0) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      sendRust({ cmd: "open_glance", url: resolved, title: (link.textContent || "").trim().slice(0, 80) || resolved });
    } catch (_) {}
  }
  window.addEventListener("pointerdown", openGlanceFromAltClick, true);

  // Alt+Shift+Click: mở link trực tiếp sang Split View (nửa phải màn hình)
  function openSplitFromAltShiftClick(e) {
    if (!e.altKey || !e.shiftKey || e.ctrlKey || e.metaKey || e.button !== 0) return;
    var link = e.target && e.target.closest && e.target.closest("a[href]");
    if (!link) return;
    var href = link.getAttribute("href") || "";
    if (!href || href.indexOf("javascript:") === 0 || href.indexOf("#") === 0) return;
    try {
      var resolved = new URL(href, window.location.href).href;
      if (resolved.indexOf("http://") !== 0 && resolved.indexOf("https://") !== 0) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      sendRust({ cmd: "open_in_split", url: resolved });
    } catch (_) {}
  }
  window.addEventListener("pointerdown", openSplitFromAltShiftClick, true);

  // Bắt sự kiện Click khi đang bật chế độ "🔴 Làm mẫu (Record)"
  window.addEventListener(
    "click",
    function (e) {
      if (!window.__tpRecordingSkill) return;
      var t = e.target;
      if (!t || (t.closest && t.closest("#__tp_bubble_root"))) return;
      var clickable =
        (t.closest && t.closest("button, a, input, textarea, select, [role='button'], [contenteditable='true']")) || t;
      var info = describeElement(clickable);
      if (!info) return;
      var inpType = (clickable.getAttribute && (clickable.getAttribute("type") || "").toLowerCase()) || "";
      if ((info.tag === "input" && inpType !== "submit" && inpType !== "button" && inpType !== "checkbox" && inpType !== "radio") || info.tag === "textarea" || info.is_contenteditable) {
        return;
      }
      sendRust({
        cmd: "recorded_skill_step",
        step_json: JSON.stringify({
          type: "click",
          tag: info.tag,
          label: info.label || info.selector,
          selector: info.selector
        })
      });
    },
    true
  );

  // Bắt sự kiện Điền chữ (Change) khi đang bật chế độ "🔴 Làm mẫu (Record)"
  window.addEventListener(
    "change",
    function (e) {
      if (!window.__tpRecordingSkill) return;
      var t = e.target;
      if (!t || (t.closest && t.closest("#__tp_bubble_root"))) return;
      var info = describeElement(t);
      if (!info) return;
      var val = t.value != null ? String(t.value) : (t.innerText || "");
      sendRust({
        cmd: "recorded_skill_step",
        step_json: JSON.stringify({
          type: "fill",
          tag: info.tag,
          label: info.label || info.selector,
          selector: info.selector,
          value: val.slice(0, 300)
        })
      });
    },
    true
  );

  window.addEventListener(
    "keydown",
    function (e) {
      if (e.isComposing || e.keyCode === 229) return;
      var code = e.code;

      if (e.key === "F5" && !e.altKey && !e.shiftKey) {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: e.ctrlKey ? "hard_reload" : "reload" });
        return;
      }

      if (e.key === "F3" && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        e.stopImmediatePropagation();
        var fb = document.getElementById("__tp_find_bar");
        if (fb && fb.style.display === "flex" && window.__tpFindStep) {
          window.__tpFindStep(!!e.shiftKey);
        } else if (window.__tpToggleFindBar) {
          window.__tpToggleFindBar(true);
        }
        return;
      }

      if (e.key === "F6" && !e.ctrlKey && !e.altKey && !e.shiftKey) {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "focus_url_bar" });
        return;
      }

      if (e.key === "Escape") {
        var colModal = document.getElementById("__tp_collections_modal");
        if (colModal && colModal.style.display === "flex") {
          e.preventDefault();
          colModal.style.display = "none";
          return;
        }
        var fbBar = document.getElementById("__tp_find_bar");
        if (fbBar && fbBar.style.display === "flex") {
          e.preventDefault();
          if (window.__tpCloseFindBar) window.__tpCloseFindBar();
          else fbBar.style.display = "none";
          return;
        }
      }

      // 1. NHÓM PHÍM TẮT CHUẨN TRÌNH DUYỆT (Ctrl) — Tuyệt đối không chiếm Ctrl+A, Ctrl+P, Ctrl+C/V/X/Z
      if (e.ctrlKey && !e.altKey && !e.metaKey) {
        if (code === "KeyT" && e.shiftKey) {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "reopen_closed_tab" });
          return;
        } else if (code === "KeyT" && !e.shiftKey) {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "new_tab", url: "tp://newtab", isolated: false });
          return;
        } else if ((code === "KeyW" || code === "F4") && !e.shiftKey) {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "close_active_tab" });
          return;
        } else if (code === "Tab") {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "step_mru_tab", delta: e.shiftKey ? -1 : 1 });
          return;
        } else if (code === "PageDown" && !e.shiftKey) {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "next_tab" });
          return;
        } else if (code === "PageUp" && !e.shiftKey) {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "prev_tab" });
          return;
        } else if (code === "KeyR") {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: e.shiftKey ? "hard_reload" : "reload" });
          return;
        } else if (code === "KeyL" && !e.shiftKey) {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "focus_url_bar" });
          return;
        } else if (code === "KeyC" && e.shiftKey) {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "copy_active_url" });
          return;
        } else if (code === "KeyK" && !e.shiftKey) {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "open_command_bar" });
          return;
        } else if (code === "KeyD" && !e.shiftKey) {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "bookmark_active" });
          return;
        } else if (code === "KeyB" && e.shiftKey) {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "toggle_bookmarks_bar" });
          return;
        } else if (code === "KeyF" && !e.shiftKey) {
          e.preventDefault();
          e.stopImmediatePropagation();
          if (window.__tpToggleFindBar) window.__tpToggleFindBar(true);
          return;
        } else if (code === "KeyH" && !e.shiftKey) {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "open_history" });
          return;
        } else if (code === "Equal" || code === "NumpadAdd") {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "zoom_step", delta: 10 });
          return;
        } else if (code === "Minus" || code === "NumpadSubtract") {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "zoom_step", delta: -10 });
          return;
        } else if ((code === "Digit0" || code === "Numpad0") && !e.shiftKey) {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "zoom_reset" });
          return;
        } else if (!e.shiftKey && /^(Digit|Numpad)[1-8]$/.test(code)) {
          var idx = parseInt(code.slice(-1), 10) - 1;
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "focus_visible_index", index: idx });
          return;
        } else if (!e.shiftKey && (code === "Digit9" || code === "Numpad9")) {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "focus_last_tab" });
          return;
        }
        return;
      }

      // 1.5. NHÓM PHÍM Ctrl+Alt: Compact Mode & chuyển Space
      if (e.ctrlKey && e.altKey && !e.shiftKey && !e.metaKey) {
        if (code === "KeyC") {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "toggle_compact_mode" });
        } else if (code === "ArrowLeft") {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "prev_workspace" });
        } else if (code === "ArrowRight") {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "next_workspace" });
        }
        return;
      }

      // 2. NHÓM PHÍM ĐIỀU HƯỚNG CHUẨN & PHÍM RIÊNG TPBROWSER / ZEN (Alt)
      if (!e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;

      if (code === "ArrowLeft") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "nav_back" });
      } else if (code === "ArrowRight") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "nav_forward" });
      } else if (code === "Home") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "go_home" });
      } else if (code === "KeyD") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "focus_url_bar" });
      } else if (code === "Comma") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "toggle_settings_modal" });
      } else if (code === "KeyA") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "toggle_chat_channel", channel: "ai" });
      } else if (code === "KeyZ") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "toggle_chat_channel", channel: "zalo" });
      } else if (code === "KeyF") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "toggle_chat_channel", channel: "fb" });
      } else if (code === "KeyM") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "cycle_view_mode" });
      } else if (code === "KeyT") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "toggle_tab_orientation" });
      } else if (code === "KeyS") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "toggle_sidebar_collapse" });
      } else if (code === "KeyB") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "toggle_collections_drawer" });
      } else if (code === "KeyP") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "new_tab", url: "tp://newtab", isolated: true });
      }
    },
    true
  );

  function sendIpc(payload) { sendRust(payload); }

  if (window.top !== window.self) return;

  function escapeHtml(s) {
    return String(s || "")
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  // Tự động nhận diện tài khoản Google nếu người dùng vừa đăng nhập trên trang của Google
  function tryDetectGoogleAccount() {
    try {
      var host = window.location.hostname || "";
      if (!host.includes("google.com")) return;
      var acctBtn = document.querySelector('a[aria-label*="@gmail.com"], a[aria-label*="Tài khoản Google:"], a[aria-label*="Google Account:"]');
      if (acctBtn) {
        var lbl = acctBtn.getAttribute("aria-label") || "";
        var m = lbl.match(/:\s*([^\n(]+)\s*\(([^)]+@[^)]+)\)/) || lbl.match(/([a-zA-Z0-9._%+-]+@gmail\.com)/);
        if (m) {
          var detected = m[2] ? m[1].trim() + " (" + m[2].trim() + ")" : m[1].trim();
          if (detected && window.__tpLastDetectedGoogle !== detected) {
            window.__tpLastDetectedGoogle = detected;
            sendRust({ cmd: "set_google_user", google_user: detected });
          }
        }
      }
    } catch (_) {}
  }

  var DEFAULT_WEB_ORBS = [
    { id: "ai", label: "AI", title: "Bật / Tắt Cửa sổ Chat AI Cowork", color: "#E2BE68", kind: "channel", channel: "ai", url: "", enabled: true },
    { id: "fb", label: "FB", title: "Bật / Tắt Cửa sổ Chat Facebook Messenger", color: "#38BDF8", kind: "channel", channel: "fb", url: "https://www.messenger.com", enabled: true },
    { id: "zalo", label: "Zalo", title: "Bật / Tắt Cửa sổ Chat Zalo Web", color: "#60A5FA", kind: "channel", channel: "zalo", url: "https://chat.zalo.me", enabled: true }
  ];
  var lastOrbSchemaSig = "";
  var lastChimeSeq = 0;

  function playGentleNotifyChime(kind) {
    try {
      var ctx = new (window.AudioContext || window.webkitAudioContext)();
      var now = ctx.currentTime;
      var f1 = kind === "message" ? 783.99 : 659.25;
      var f2 = kind === "message" ? 1046.5 : 987.77;
      var o1 = ctx.createOscillator(); var g1 = ctx.createGain();
      o1.type = "sine"; o1.frequency.setValueAtTime(f1, now);
      g1.gain.setValueAtTime(0.06, now); g1.gain.exponentialRampToValueAtTime(0.001, now + 0.25);
      o1.connect(g1); g1.connect(ctx.destination);
      o1.start(now); o1.stop(now + 0.26);

      var o2 = ctx.createOscillator(); var g2 = ctx.createGain();
      o2.type = "sine"; o2.frequency.setValueAtTime(f2, now + 0.14);
      g2.gain.setValueAtTime(0.08, now + 0.14); g2.gain.exponentialRampToValueAtTime(0.001, now + 0.55);
      o2.connect(g2); g2.connect(ctx.destination);
      o2.start(now + 0.14); o2.stop(now + 0.56);
    } catch (_) {}
  }

  var lastOrbsList = null;
  var lastBubbleState = null;
  var tpObserverBound = false;

  function domEl(tag, attrs, children) {
    var node = document.createElement(tag);
    if (attrs) {
      for (var k in attrs) {
        if (!Object.prototype.hasOwnProperty.call(attrs, k)) continue;
        if (k === "text") node.textContent = attrs[k];
        else if (k === "style") node.style.cssText = attrs[k];
        else if (k === "className") node.className = attrs[k];
        else node.setAttribute(k, attrs[k]);
      }
    }
    if (children) {
      for (var i = 0; i < children.length; i++) {
        if (children[i]) node.appendChild(children[i]);
      }
    }
    return node;
  }

  function svgEl(tag, attrs, children) {
    var node = document.createElementNS("http://www.w3.org/2000/svg", tag);
    if (attrs) {
      for (var k in attrs) {
        if (Object.prototype.hasOwnProperty.call(attrs, k)) {
          node.setAttribute(k, attrs[k]);
        }
      }
    }
    if (children) {
      for (var i = 0; i < children.length; i++) {
        if (children[i]) node.appendChild(children[i]);
      }
    }
    return node;
  }

  function appendOrbIconDom(btn, orb) {
    if (orb.id === "ai") {
      var aiSvg = svgEl("svg", { viewBox: "0 0 36 36", fill: "none" }, [
        svgEl("line", { x1: "18", y1: "4", x2: "18", y2: "8.5", stroke: "currentColor", "stroke-width": "2.2", "stroke-linecap": "round" }),
        svgEl("circle", { cx: "18", cy: "3.5", r: "2.2", fill: "currentColor" }),
        svgEl("rect", { x: "6.5", y: "8.5", width: "23", height: "18", rx: "6", stroke: "currentColor", "stroke-width": "2.2" }),
        svgEl("rect", { x: "11.5", y: "14", width: "4.2", height: "4", rx: "1.2", fill: "currentColor" }),
        svgEl("rect", { x: "20.3", y: "14", width: "4.2", height: "4", rx: "1.2", fill: "currentColor" }),
        svgEl("path", { d: "M13 21.5 H23", stroke: "currentColor", "stroke-width": "2", "stroke-linecap": "round" }),
        svgEl("path", { d: "M10.5 30 C13 28 23 28 25.5 30", stroke: "currentColor", "stroke-width": "2", "stroke-linecap": "round" })
      ]);
      btn.appendChild(aiSvg);
      btn.appendChild(domEl("span", { className: "tp-orb-count", id: "__tp_cnt_ai" }));
      return;
    }
    if (orb.id === "fb") {
      var fbSvg = svgEl("svg", { viewBox: "0 0 36 36", fill: "none" }, [
        svgEl("path", { d: "M18 3.5C9.4 3.5 2.5 9.8 2.5 17.6c0 4.3 2.1 8.1 5.5 10.7v5.2l5.2-2.8c1.5.4 3.1.6 4.8.6 8.6 0 15.5-6.3 15.5-14.1S26.6 3.5 18 3.5z", fill: "currentColor" }),
        svgEl("path", { d: "M9.6 21.3l5.9-6.3 3.1 3.2 5.6-3.2-5.9 6.3-3-3.2-5.7 3.2z", fill: "var(--tp-orb-cut)" })
      ]);
      btn.appendChild(fbSvg);
      btn.appendChild(domEl("span", { className: "tp-orb-count", id: "__tp_cnt_fb" }));
      return;
    }
    if (orb.id === "zalo") {
      var zaloTxt = svgEl("text", {
        x: "18",
        y: "21.2",
        "text-anchor": "middle",
        fill: "var(--tp-orb-cut)",
        "font-family": "Segoe UI, Arial, sans-serif",
        "font-weight": "900",
        "font-size": "11.2",
        "letter-spacing": "-0.2"
      });
      zaloTxt.textContent = "Zalo";
      var zaloSvg = svgEl("svg", { viewBox: "0 0 36 36", fill: "none" }, [
        svgEl("path", { d: "M18 3.5C9.4 3.5 2.5 9.8 2.5 17.6c0 4.3 2.1 8.1 5.5 10.7v5.2l5.2-2.8c1.5.4 3.1.6 4.8.6 8.6 0 15.5-6.3 15.5-14.1S26.6 3.5 18 3.5z", fill: "currentColor" }),
        zaloTxt
      ]);
      btn.appendChild(zaloSvg);
      btn.appendChild(domEl("span", { className: "tp-orb-count", id: "__tp_cnt_zalo" }));
      return;
    }
    var lbl = String(orb.label || orb.title || "?").slice(0, 4);
    btn.appendChild(domEl("span", {
      style: "font-size:12px !important; font-weight:800 !important; letter-spacing:0.2px !important;",
      text: lbl
    }));
  }

  function renderWebOrbs(orbsList) {
    var host = document.getElementById("__tp_web_orbs");
    if (!host) return;
    var list = (Array.isArray(orbsList) && orbsList.length > 0) ? orbsList : DEFAULT_WEB_ORBS;
    lastOrbsList = list;
    var sig = JSON.stringify(list);
    if (sig === lastOrbSchemaSig && host.children.length > 0) return;
    lastOrbSchemaSig = sig;
    while (host.firstChild) host.removeChild(host.firstChild);
    for (var i = 0; i < list.length; i++) {
      var o = list[i];
      if (o.enabled === false) continue;
      var btn = document.createElement("div");
      btn.className = "tp-web-orb";
      btn.id = o.id === "ai" ? "__tp_orb_ai" : (o.id === "fb" ? "__tp_orb_fb" : (o.id === "zalo" ? "__tp_orb_zalo" : "__tp_orb_" + o.id));
      btn.title = o.title || o.label || "";
      if (o.kind !== "channel" && o.color) {
        btn.style.setProperty("border-color", o.color, "important");
        btn.style.setProperty("color", o.color, "important");
      }
      appendOrbIconDom(btn, o);
      (function (item, btnEl) {
        btnEl.addEventListener("click", function (ev) {
          ev.stopPropagation();
          try { btnEl.animate([{ transform: "scale(0.9)" }, { transform: "scale(1.06)" }, { transform: "scale(1)" }], { duration: 240, easing: "cubic-bezier(0.22,1,0.36,1)" }); } catch (_) {}
          if (item.kind === "channel") {
            sendRust({ cmd: "toggle_chat_channel", channel: item.channel || item.id });
          } else {
            sendRust({ cmd: "open_custom_orb", id: item.id, kind: item.kind || "popup", url: item.url || "", title: item.title || item.label || "" });
          }
        });
      })(o, btn);
      host.appendChild(btn);
    }
  }

  var TP_BUBBLE_CSS = [
    "@keyframes tpAiPulseRing { 0%, 100% { box-shadow: 0 0 0 0 rgba(16,185,129,0.65), 0 0 18px rgba(226,190,104,0.65); border-color: #34D399; } 50% { box-shadow: 0 0 0 8px rgba(16,185,129,0), 0 0 26px rgba(226,190,104,0.95); border-color: #E2BE68; } }",
    "@keyframes tpBadgePop { 0% { transform: scale(0.5); } 70% { transform: scale(1.2); } 100% { transform: scale(1); } }",
    "#__tp_bubble_root { position: static !important; display: block !important; visibility: visible !important; opacity: 1 !important; }",
    "#__tp_bubble_root, #__tp_bubble_root * { box-sizing: border-box !important; }",
    "#__tp_active_ring { position: fixed !important; inset: 0 !important; border: 2px solid transparent; pointer-events: none !important; z-index: 2147483640 !important; transition: border-color 0.12s; }",
    "#__tp_active_ring.split-active { border-color: #D4AF37 !important; }",
    "#__tp_active_ring.recording-ring { border: 3px dashed #EF4444 !important; }",
    "#__tp_find_bar { position: fixed !important; top: 14px !important; right: 20px !important; z-index: 2147483647 !important; display: none; align-items: center; gap: 6px; background: rgba(17, 19, 24, 0.94) !important; backdrop-filter: blur(14px) !important; color: #F5F2EB !important; border: 1px solid rgba(226, 190, 104, 0.55) !important; border-radius: 999px !important; padding: 5px 10px 5px 14px !important; box-shadow: 0 12px 32px rgba(0, 0, 0, 0.52) !important; font-family: 'Segoe UI Variable', -apple-system, BlinkMacSystemFont, 'Segoe UI', Inter, sans-serif !important; }",
    "#__tp_find_bar.no-match { border-color: rgba(248, 113, 113, 0.7) !important; }",
    "#__tp_find_input { background: transparent !important; border: none !important; outline: none !important; color: inherit !important; font-size: 12.5px !important; width: 176px !important; user-select: text !important; margin: 0 !important; padding: 2px 0 !important; }",
    "#__tp_find_input::placeholder { color: rgba(245, 242, 235, 0.5) !important; }",
    "#__tp_find_count { font-size: 11px !important; font-weight: 600 !important; font-variant-numeric: tabular-nums !important; color: rgba(245, 242, 235, 0.68) !important; padding: 0 4px !important; min-width: 34px !important; text-align: center !important; white-space: nowrap !important; }",
    "#__tp_find_bar.no-match #__tp_find_count { color: #F87171 !important; }",
    "#__tp_bubble_root .tp-find-btn { background: rgba(255,255,255,0.08) !important; border: 1px solid rgba(226,190,104,0.28) !important; color: #F5F2EB !important; border-radius: 999px !important; padding: 3px 8px !important; font-size: 11px !important; font-weight: 700 !important; cursor: pointer !important; line-height: 1.2 !important; transition: background-color 0.14s ease, border-color 0.14s ease, color 0.14s ease !important; }",
    "#__tp_bubble_root .tp-find-btn:hover { background: rgba(226,190,104,0.22) !important; color: #E2BE68 !important; }",
    "#__tp_bubble_root .tp-find-btn.active { background: rgba(226,190,104,0.28) !important; border-color: #E2BE68 !important; color: #E2BE68 !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] #__tp_find_bar { background: rgba(250, 246, 238, 0.96) !important; color: #18130C !important; border-color: rgba(138, 94, 16, 0.36) !important; box-shadow: 0 10px 28px rgba(32, 22, 6, 0.16) !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] #__tp_find_bar.no-match { border-color: rgba(185, 28, 28, 0.65) !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] #__tp_find_input::placeholder { color: rgba(24, 19, 12, 0.45) !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] #__tp_find_count { color: #6E5D43 !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] #__tp_find_bar.no-match #__tp_find_count { color: #B91C1C !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] .tp-find-btn { background: rgba(138, 94, 16, 0.08) !important; border-color: rgba(138, 94, 16, 0.24) !important; color: #18130C !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] .tp-find-btn:hover { background: rgba(138, 94, 16, 0.16) !important; color: #8A5E10 !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] .tp-find-btn.active { background: rgba(138, 94, 16, 0.22) !important; border-color: #8A5E10 !important; color: #8A5E10 !important; }",
    "::highlight(tp-find) { background-color: rgba(226, 190, 104, 0.34); color: inherit; }",
    "::highlight(tp-find-active) { background-color: #E2BE68; color: #111318; }",
    "#__tp_link_pill { position: fixed !important; left: 10px !important; bottom: 10px !important; z-index: 2147483646 !important; display: none; max-width: 52vw !important; padding: 4px 10px !important; border-radius: 999px !important; background: rgba(17, 19, 24, 0.90) !important; color: #E2BE68 !important; border: 1px solid rgba(212, 175, 55, 0.35) !important; font: 500 11px/1.3 -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif !important; white-space: nowrap !important; overflow: hidden !important; text-overflow: ellipsis !important; pointer-events: none !important; }",
    "#__tp_link_pill.show-pill { display: block !important; }",
    "#__tp_bubble_wrap { position: fixed !important; bottom: 22px !important; right: 18px !important; z-index: 2147483647 !important; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Inter, sans-serif !important; display: flex !important; flex-direction: column !important; align-items: flex-end !important; gap: 10px !important; visibility: visible !important; opacity: 1 !important; pointer-events: auto !important; }",
    "#__tp_web_orbs { display: flex !important; flex-direction: column !important; gap: 11px !important; align-items: center !important; visibility: visible !important; opacity: 1 !important; }",
    "#__tp_bubble_root .tp-web-orb { width: 52px !important; height: 52px !important; min-width: 52px !important; min-height: 52px !important; border-radius: 50% !important; background: radial-gradient(circle at 30% 30%, #241F15, #0E0D0B 78%) !important; border: 1.75px solid rgba(226, 190, 104, 0.85) !important; box-shadow: 0 8px 24px rgba(0, 0, 0, 0.72), 0 0 14px rgba(212, 175, 55, 0.28) !important; color: #E2BE68 !important; --tp-orb-cut: #0E0D0B; display: flex !important; align-items: center !important; justify-content: center !important; cursor: pointer !important; transition: transform 0.22s cubic-bezier(0.34, 1.56, 0.64, 1), box-shadow 0.18s cubic-bezier(0.22, 1, 0.36, 1) !important; position: relative !important; visibility: visible !important; opacity: 1 !important; padding: 0 !important; margin: 0 !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] .tp-web-orb { background: radial-gradient(circle at 30% 30%, #FFFFFF, #EFE5D3 86%) !important; border: 1.75px solid rgba(138, 94, 16, 0.85) !important; box-shadow: 0 8px 24px rgba(32, 22, 6, 0.26), inset 0 1px 0 rgba(255, 255, 255, 0.94) !important; color: #8A5E10 !important; --tp-orb-cut: #FFFFFF; }",
    "#__tp_bubble_root .tp-web-orb:hover { transform: translateY(-3px) scale(1.08) !important; }",
    "#__tp_bubble_root .tp-web-orb:active { transform: scale(0.94) !important; }",
    "#__tp_bubble_root .tp-web-orb.active-orb { border-width: 2.5px !important; box-shadow: 0 0 0 3px rgba(212, 175, 55, 0.36), 0 8px 24px rgba(0, 0, 0, 0.5) !important; }",
    "#__tp_bubble_root .tp-web-orb.ai-flash-pulse { animation: tpAiPulseRing 1.15s ease-in-out infinite !important; border-width: 2.5px !important; }",
    "#__tp_bubble_root .tp-web-orb svg { width: 30px !important; height: 30px !important; display: block !important; flex-shrink: 0 !important; }",
    "#__tp_bubble_root .tp-orb-count { position: absolute !important; top: -5px !important; right: -5px !important; min-width: 20px !important; height: 20px !important; padding: 0 5px !important; border-radius: 999px !important; background: #EF4444 !important; color: #FFFFFF !important; border: 2px solid #0B0C0E !important; font-size: 10.5px !important; font-weight: 800 !important; line-height: 16px !important; text-align: center !important; box-shadow: 0 3px 8px rgba(0,0,0,0.45) !important; display: none; pointer-events: none !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] .tp-orb-count { border-color: #F6F2EA !important; }",
    "#__tp_bubble_root .tp-orb-count.show { display: inline-block !important; animation: tpBadgePop 0.28s cubic-bezier(0.34, 1.56, 0.64, 1) !important; }",
    "#__tp_bubble_pill { display: none; align-items: center; gap: 8px; background: #111318 !important; color: #F5F2EB !important; border: 1.5px solid #D4AF37 !important; border-radius: 999px !important; box-shadow: 0 8px 24px rgba(0, 0, 0, 0.55) !important; padding: 7px 14px !important; cursor: pointer !important; max-width: 420px !important; }",
    "#__tp_bubble_pill.show-pill { display: flex !important; }",
    "#__tp_bubble_pill.working { border-color: #F59E0B !important; }",
    "#__tp_bubble_pill.recording { border-color: #EF4444 !important; background: #3B0A0A !important; }",
    "#__tp_bubble_root .tp-pill-dot { width: 8px !important; height: 8px !important; border-radius: 50% !important; background: #D4AF37 !important; flex-shrink: 0 !important; }",
    "#__tp_bubble_pill.working .tp-pill-dot { background: #F59E0B !important; }",
    "#__tp_bubble_pill.recording .tp-pill-dot { background: #EF4444 !important; }",
    "#__tp_bubble_root .tp-pill-text { font-size: 11.5px !important; font-weight: 600 !important; white-space: nowrap !important; overflow: hidden !important; text-overflow: ellipsis !important; max-width: 250px !important; }",
    "#__tp_bubble_root .tp-pill-dock-btn { background: linear-gradient(135deg, #C79535 0%, #9A6B16 100%) !important; color: #FFFFFF !important; border: none !important; border-radius: 999px !important; padding: 4px 10px !important; font-size: 10.5px !important; font-weight: 700 !important; cursor: pointer !important; flex-shrink: 0 !important; }",
    "#__tp_bubble_root .tp-pill-dock-btn.stop-rec { background: #EF4444 !important; color: #FFFFFF !important; }",
    "#__tp_collections_modal { position: fixed !important; inset: 0 !important; z-index: 2147483647 !important; display: none; align-items: flex-start !important; justify-content: center !important; padding-top: 72px !important; background: rgba(8, 9, 12, 0.56) !important; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Inter, sans-serif !important; }",
    "#__tp_collections_card { width: min(520px, 92vw) !important; max-height: 76vh !important; background: #141720 !important; color: #F5F2EB !important; border: 1.5px solid #E2BE68 !important; border-radius: 16px !important; box-shadow: 0 24px 60px rgba(0, 0, 0, 0.72) !important; display: flex !important; flex-direction: column !important; overflow: hidden !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] #__tp_collections_card { background: #FAF6EE !important; color: #18130C !important; border-color: #8A5E10 !important; }",
    "#__tp_col_head { display: flex !important; align-items: center !important; justify-content: space-between !important; gap: 10px !important; padding: 13px 16px !important; border-bottom: 1px solid rgba(212, 175, 55, 0.28) !important; }",
    "#__tp_col_title { font-size: 13.5px !important; font-weight: 700 !important; color: #E2BE68 !important; letter-spacing: 0.4px !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] #__tp_col_title { color: #8A5E10 !important; }",
    "#__tp_col_search_wrap { padding: 10px 14px !important; border-bottom: 1px solid rgba(212, 175, 55, 0.18) !important; display: flex !important; gap: 8px !important; align-items: center !important; }",
    "#__tp_col_search { flex: 1 !important; border: 1px solid rgba(212, 175, 55, 0.36) !important; background: rgba(0, 0, 0, 0.25) !important; color: inherit !important; border-radius: 999px !important; padding: 7px 14px !important; font-size: 12.5px !important; outline: none !important; user-select: text !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] #__tp_col_search { background: #FFFFFF !important; border-color: rgba(138, 94, 16, 0.35) !important; }",
    "#__tp_col_folders { display: flex !important; flex-wrap: wrap !important; gap: 6px !important; padding: 8px 14px 4px 14px !important; }",
    "#__tp_bubble_root .tp-col-folder-pill { border: 1px solid rgba(212, 175, 55, 0.28) !important; background: rgba(255, 255, 255, 0.04) !important; color: inherit !important; border-radius: 999px !important; padding: 3px 10px !important; font-size: 11px !important; font-weight: 600 !important; cursor: pointer !important; }",
    "#__tp_bubble_root .tp-col-folder-pill.active { background: rgba(226, 190, 104, 0.22) !important; border-color: #E2BE68 !important; color: #E2BE68 !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] .tp-col-folder-pill.active { background: rgba(138, 94, 16, 0.14) !important; border-color: #8A5E10 !important; color: #8A5E10 !important; }",
    "#__tp_col_editor { display: none; flex-direction: column !important; gap: 7px !important; padding: 10px 14px !important; border-bottom: 1px solid rgba(212, 175, 55, 0.22) !important; background: rgba(212, 175, 55, 0.05) !important; }",
    "#__tp_col_editor.open { display: flex !important; }",
    "#__tp_bubble_root .tp-col-ed-row { display: flex !important; gap: 6px !important; align-items: center !important; }",
    "#__tp_bubble_root .tp-col-ed-inp { flex: 1 !important; min-width: 0 !important; border: 1px solid rgba(212, 175, 55, 0.36) !important; background: rgba(0, 0, 0, 0.25) !important; color: inherit !important; border-radius: 8px !important; padding: 5px 9px !important; font-size: 11.5px !important; outline: none !important; user-select: text !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] .tp-col-ed-inp { background: #FFFFFF !important; border-color: rgba(138, 94, 16, 0.35) !important; }",
    "#__tp_bubble_root .tp-col-meta { display: flex !important; flex-wrap: wrap !important; gap: 4px !important; margin-top: 3px !important; align-items: center !important; }",
    "#__tp_bubble_root .tp-col-chip { font-size: 10px !important; padding: 1px 6px !important; border-radius: 999px !important; border: 1px solid rgba(212, 175, 55, 0.3) !important; background: rgba(212, 175, 55, 0.12) !important; color: #E2BE68 !important; font-weight: 600 !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] .tp-col-chip { color: #8A5E10 !important; border-color: rgba(138, 94, 16, 0.3) !important; background: rgba(138, 94, 16, 0.08) !important; }",
    "#__tp_col_list { overflow-y: auto !important; padding: 10px 14px 14px 14px !important; display: flex !important; flex-direction: column !important; gap: 6px !important; }",
    "#__tp_bubble_root .tp-col-row { display: flex !important; align-items: center !important; justify-content: space-between !important; gap: 10px !important; padding: 8px 12px !important; border: 1px solid rgba(212, 175, 55, 0.22) !important; border-radius: 10px !important; cursor: pointer !important; background: rgba(255, 255, 255, 0.03) !important; }",
    "#__tp_bubble_root .tp-col-row:hover { border-color: #E2BE68 !important; background: rgba(212, 175, 55, 0.12) !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] .tp-col-row { background: #FFFFFF !important; border-color: rgba(138, 94, 16, 0.22) !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] .tp-col-row:hover { border-color: #8A5E10 !important; background: rgba(154, 107, 22, 0.08) !important; }",
    "#__tp_toast { position: fixed !important; top: 18px !important; left: 50% !important; transform: translateX(-50%) !important; z-index: 2147483647 !important; display: none; background: rgba(17, 19, 24, 0.96) !important; color: #E2BE68 !important; border: 1.5px solid #E2BE68 !important; border-radius: 999px !important; padding: 7px 16px !important; font-size: 12px !important; font-weight: 700 !important; box-shadow: 0 10px 28px rgba(0, 0, 0, 0.55) !important; pointer-events: none !important; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Inter, sans-serif !important; }",
    "#__tp_zap_highlight { position: fixed !important; z-index: 2147483645 !important; pointer-events: none !important; border: 2px dashed #E2BE68 !important; background: rgba(226, 190, 104, 0.16) !important; border-radius: 6px !important; display: none; box-sizing: border-box !important; }",
    "#__tp_zap_banner { position: fixed !important; top: 14px !important; left: 50% !important; transform: translateX(-50%) !important; z-index: 2147483647 !important; display: none; align-items: center !important; gap: 10px !important; background: rgba(17, 19, 24, 0.96) !important; color: #F5F2EB !important; border: 1.5px solid #E2BE68 !important; border-radius: 999px !important; padding: 6px 14px !important; font: 600 12px/1.3 -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif !important; box-shadow: 0 12px 32px rgba(0, 0, 0, 0.65) !important; }",
    "#__tp_boost_modal { position: fixed !important; inset: 0 !important; z-index: 2147483647 !important; display: none; align-items: center !important; justify-content: center !important; background: rgba(8, 9, 12, 0.56) !important; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Inter, sans-serif !important; }",
    "#__tp_boost_card { width: min(500px, 92vw) !important; max-height: 82vh !important; overflow-y: auto !important; background: #141720 !important; color: #F5F2EB !important; border: 1.5px solid #E2BE68 !important; border-radius: 16px !important; box-shadow: 0 24px 60px rgba(0, 0, 0, 0.72) !important; padding: 14px 16px !important; display: flex !important; flex-direction: column !important; gap: 10px !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] #__tp_boost_card { background: #FAF6EE !important; color: #18130C !important; border-color: #8A5E10 !important; }",
    "#__tp_boost_css_inp { width: 100% !important; min-height: 120px !important; border: 1px solid rgba(212, 175, 55, 0.36) !important; background: rgba(0, 0, 0, 0.25) !important; color: inherit !important; border-radius: 10px !important; padding: 8px 10px !important; font: 12px/1.45 Consolas, 'Courier New', monospace !important; outline: none !important; user-select: text !important; resize: vertical !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] #__tp_boost_css_inp { background: #FFFFFF !important; border-color: rgba(138, 94, 16, 0.35) !important; }",
    "#__tp_boost_zaps { display: flex !important; flex-wrap: wrap !important; gap: 6px !important; }"
  ].join("\n");

  function ensureSpaResilienceObserver() {
    if (IS_INTERNAL_HTML_PAGE || tpObserverBound || !document.documentElement) return;
    tpObserverBound = true;
    try {
      var mo = new MutationObserver(function () {
        if (document.body && !document.getElementById("__tp_bubble_root")) {
          mountTpBubbleUi();
          if (lastBubbleState && window.__tpUpdateBubbleUi) {
            window.__tpUpdateBubbleUi(lastBubbleState);
          }
        }
      });
      mo.observe(document.documentElement, { childList: true, subtree: true });
    } catch (_) {}
  }

  // Gắn Cụm Bong Bóng Góc Phải + Find Bar (Ctrl+F) + Khay Collections (Alt+B) trên MỌI trang web (100% Trusted Types safe, không dùng innerHTML)
  function mountTpBubbleUi() {
    if (!document.documentElement || !document.body) {
      setTimeout(mountTpBubbleUi, 30);
      return;
    }
    ensureSpaResilienceObserver();
    if (document.getElementById("__tp_bubble_root")) return;

    try {
      lastOrbSchemaSig = "";
      var styleEl = domEl("style", { text: TP_BUBBLE_CSS });
      var activeRing = domEl("div", { id: "__tp_active_ring" });

      var findInp = domEl("input", { id: "__tp_find_input", type: "text", placeholder: "Tìm trong trang..." });
      var findCount = domEl("span", { id: "__tp_find_count", "aria-live": "polite", text: "" });
      var findCase = domEl("button", { type: "button", className: "tp-find-btn", id: "__tp_find_case", "aria-pressed": "false", title: "Phân biệt chữ hoa / thường (Aa)", text: "Aa" });
      var findPrev = domEl("button", { type: "button", className: "tp-find-btn", id: "__tp_find_prev", title: "Trước (Shift+Enter / Shift+F3)", text: "▲" });
      var findNext = domEl("button", { type: "button", className: "tp-find-btn", id: "__tp_find_next", title: "Tiếp (Enter / F3)", text: "▼" });
      var findClose = domEl("button", { type: "button", className: "tp-find-btn", id: "__tp_find_close", title: "Đóng (Esc)", text: "✕" });
      var findBar = domEl("div", { id: "__tp_find_bar" }, [findInp, findCount, findCase, findPrev, findNext, findClose]);

      var colTitle = domEl("span", { id: "__tp_col_title", text: "COLLECTIONS & BOOKMARKS (Alt+B)" });
      var colPinBtn = domEl("button", { type: "button", className: "tp-find-btn", text: "★ Ghim trang này" });
      var colCloseBtn = domEl("button", { type: "button", className: "tp-find-btn", text: "✕" });
      var colHeadActions = domEl("div", { style: "display:flex !important; gap:6px !important; align-items:center !important;" }, [colPinBtn, colCloseBtn]);
      var colHead = domEl("div", { id: "__tp_col_head" }, [colTitle, colHeadActions]);
      var colSearch = domEl("input", { id: "__tp_col_search", type: "text", placeholder: "Lọc theo tên, URL, thư mục, #tag hoặc @keyword..." });
      var colSearchWrap = domEl("div", { id: "__tp_col_search_wrap" }, [colSearch]);
      var colFolders = domEl("div", { id: "__tp_col_folders" });
      var colEdTitle = domEl("input", { id: "__tp_col_ed_title", type: "text", className: "tp-col-ed-inp", placeholder: "Tiêu đề bookmark" });
      var colEdFolder = domEl("input", { id: "__tp_col_ed_folder", type: "text", className: "tp-col-ed-inp", placeholder: "Thư mục (vd: Công việc)" });
      var colEdTags = domEl("input", { id: "__tp_col_ed_tags", type: "text", className: "tp-col-ed-inp", placeholder: "Tags (vd: sheet, ads)" });
      var colEdKeyword = domEl("input", { id: "__tp_col_ed_keyword", type: "text", className: "tp-col-ed-inp", placeholder: "Keyword nhanh (vd: gs)" });
      var colEdSave = domEl("button", { type: "button", className: "tp-find-btn", text: "Lưu" });
      var colEdCancel = domEl("button", { type: "button", className: "tp-find-btn", text: "Hủy" });
      var colEditor = domEl("div", { id: "__tp_col_editor" }, [
        domEl("div", { className: "tp-col-ed-row" }, [colEdTitle, colEdFolder]),
        domEl("div", { className: "tp-col-ed-row" }, [colEdTags, colEdKeyword, colEdSave, colEdCancel])
      ]);
      var colList = domEl("div", { id: "__tp_col_list" });
      var colCard = domEl("div", { id: "__tp_collections_card" }, [colHead, colSearchWrap, colFolders, colEditor, colList]);
      var colModal = domEl("div", { id: "__tp_collections_modal" }, [colCard]);

      var toastEl = domEl("div", { id: "__tp_toast" });

      var orbsHost = domEl("div", { id: "__tp_web_orbs" });
      var pillDot = domEl("div", { className: "tp-pill-dot" });
      var statusSpan = domEl("span", { className: "tp-pill-text", id: "__tp_pill_status", text: "🤖 AI Cowork" });
      var stopRecBtn = domEl("button", { type: "button", className: "tp-pill-dock-btn stop-rec", id: "__tp_pill_stop_rec", style: "display:none;", text: "⏹ Lưu Skill" });
      var dockBtn = domEl("button", { type: "button", className: "tp-pill-dock-btn", id: "__tp_pill_dock_btn", text: "Mở Chat" });
      var pill = domEl("div", { id: "__tp_bubble_pill", title: "Trạng thái AI Cowork / Ghi mẫu Skill" }, [pillDot, statusSpan, stopRecBtn, dockBtn]);
      var bubbleWrap = domEl("div", { id: "__tp_bubble_wrap" }, [orbsHost, pill]);

      var linkPill = domEl("div", { id: "__tp_link_pill" });

      var root = domEl("div", { id: "__tp_bubble_root" }, [styleEl, activeRing, findBar, colModal, toastEl, bubbleWrap, linkPill]);
      document.body.appendChild(root);

      renderWebOrbs(lastOrbsList || DEFAULT_WEB_ORBS);

      pill.addEventListener("click", function (ev) {
        if (ev.target === stopRecBtn) return;
        sendRust({ cmd: "open_chat_channel", channel: "ai" });
      });
      dockBtn.addEventListener("click", function (ev) {
        ev.stopPropagation();
        sendRust({ cmd: "open_chat_channel", channel: "ai" });
      });
      stopRecBtn.addEventListener("click", function (ev) {
        ev.stopPropagation();
        sendRust({ cmd: "stop_skill_recording" });
      });

      var findMatchCase = false;
      var findMatches = [];
      var findCurrentIdx = -1;
      var findDebounceTimer = null;

      function clearFindHighlights() {
        try {
          if (typeof CSS !== "undefined" && CSS.highlights && typeof CSS.highlights.delete === "function") {
            CSS.highlights.delete("tp-find");
            CSS.highlights.delete("tp-find-active");
          }
        } catch (_) {}
      }

      function closeFindBar() {
        findBar.style.display = "none";
        findBar.classList.remove("no-match");
        findMatches = [];
        findCurrentIdx = -1;
        findCount.textContent = "";
        clearFindHighlights();
      }

      function collectFindMatches(query) {
        var out = [];
        if (!query || !document.body) return out;
        var needle = findMatchCase ? query : query.toLocaleLowerCase();
        var blockTags = {
          ADDRESS: 1, ARTICLE: 1, ASIDE: 1, BLOCKQUOTE: 1, DD: 1, DETAILS: 1,
          DIALOG: 1, DIV: 1, DL: 1, DT: 1, FIELDSET: 1, FIGCAPTION: 1, FIGURE: 1,
          FOOTER: 1, FORM: 1, H1: 1, H2: 1, H3: 1, H4: 1, H5: 1, H6: 1, HEADER: 1,
          HGROUP: 1, HR: 1, LI: 1, MAIN: 1, NAV: 1, OL: 1, P: 1, PRE: 1, SECTION: 1,
          TABLE: 1, TBODY: 1, TD: 1, TH: 1, THEAD: 1, TR: 1, UL: 1, BODY: 1
        };
        function getBlockHost(el) {
          var cur = el;
          while (cur && cur !== document.body) {
            if (blockTags[cur.tagName]) return cur;
            cur = cur.parentElement;
          }
          return document.body;
        }
        var visCache = typeof WeakMap === "function" ? new WeakMap() : null;
        function isElemVisible(el) {
          if (!el || el.nodeType !== 1) return false;
          if (el.closest && el.closest("#__tp_bubble_root, script, style, noscript, textarea, template, [hidden], [aria-hidden='true']")) return false;
          if (visCache && visCache.has(el)) return visCache.get(el);
          var ok = true;
          try {
            var st = window.getComputedStyle(el);
            if (!st || st.display === "none" || st.visibility === "hidden" || Number(st.opacity) === 0) ok = false;
          } catch (_) {}
          if (ok && el.parentElement && el.parentElement !== document.documentElement) {
            ok = isElemVisible(el.parentElement);
          }
          if (visCache) visCache.set(el, ok);
          return ok;
        }

        var groups = [];
        var groupMap = typeof WeakMap === "function" ? new WeakMap() : null;
        var walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, null);
        var node;
        while ((node = walker.nextNode())) {
          if (!node.nodeValue || !node.parentElement || !isElemVisible(node.parentElement)) continue;
          var host = getBlockHost(node.parentElement);
          var bucket = groupMap ? groupMap.get(host) : null;
          if (!bucket) {
            bucket = [];
            if (groupMap) groupMap.set(host, bucket);
            groups.push(bucket);
          }
          bucket.push(node);
        }

        for (var g = 0; g < groups.length; g++) {
          var nodes = groups[g];
          var joined = "";
          var spans = [];
          for (var n = 0; n < nodes.length; n++) {
            var val = nodes[n].nodeValue;
            spans.push({ node: nodes[n], start: joined.length, end: joined.length + val.length });
            joined += val;
          }
          var hay = findMatchCase ? joined : joined.toLocaleLowerCase();
          var pos = 0;
          while (pos <= hay.length - needle.length) {
            var hit = hay.indexOf(needle, pos);
            if (hit < 0) break;
            var hitEnd = hit + needle.length;
            var sNode = null, sOff = 0, eNode = null, eOff = 0;
            for (var k = 0; k < spans.length; k++) {
              var sp = spans[k];
              if (!sNode && hit >= sp.start && hit < sp.end) {
                sNode = sp.node;
                sOff = hit - sp.start;
              }
              if (hitEnd > sp.start && hitEnd <= sp.end) {
                eNode = sp.node;
                eOff = hitEnd - sp.start;
                break;
              }
            }
            if (sNode && eNode) {
              try {
                var r = document.createRange();
                r.setStart(sNode, sOff);
                r.setEnd(eNode, eOff);
                out.push(r);
                if (out.length >= 1000) return out;
              } catch (_) {}
            }
            pos = hit + Math.max(1, needle.length);
          }
        }
        return out;
      }

      function updateFindCountUi() {
        var q = findInp.value || "";
        if (!q) {
          findCount.textContent = "";
          findBar.classList.remove("no-match");
          return;
        }
        if (findMatches.length === 0) {
          findCount.textContent = "0/0";
          findBar.classList.add("no-match");
        } else {
          findBar.classList.remove("no-match");
          findCount.textContent = String(findCurrentIdx + 1) + "/" + String(findMatches.length);
        }
      }

      function focusFindMatch(idx) {
        if (!findMatches.length) {
          findCurrentIdx = -1;
          updateFindCountUi();
          return;
        }
        findCurrentIdx = ((idx % findMatches.length) + findMatches.length) % findMatches.length;
        var r = findMatches[findCurrentIdx];
        var highlightedViaCss = false;
        try {
          if (typeof CSS !== "undefined" && CSS.highlights && typeof Highlight === "function") {
            CSS.highlights.set("tp-find-active", new Highlight(r));
            highlightedViaCss = true;
          }
        } catch (_) {}
        try {
          if (!highlightedViaCss && document.activeElement !== findInp) {
            var sel = window.getSelection();
            if (sel) {
              sel.removeAllRanges();
              sel.addRange(r);
            }
          }
          var anchor = r.startContainer && r.startContainer.parentElement;
          if (anchor && typeof anchor.scrollIntoView === "function") {
            anchor.scrollIntoView({ behavior: "smooth", block: "center", inline: "nearest" });
          }
        } catch (_) {}
        updateFindCountUi();
      }

      function refreshFindMatches(keepSelection) {
        var q = findInp.value || "";
        clearFindHighlights();
        if (!q) {
          findMatches = [];
          findCurrentIdx = -1;
          updateFindCountUi();
          return;
        }
        findMatches = collectFindMatches(q);
        try {
          if (findMatches.length && typeof CSS !== "undefined" && CSS.highlights && typeof Highlight === "function") {
            var hl = new Highlight();
            for (var hi = 0; hi < findMatches.length; hi++) hl.add(findMatches[hi]);
            CSS.highlights.set("tp-find", hl);
          }
        } catch (_) {}
        if (!findMatches.length) {
          findCurrentIdx = -1;
          updateFindCountUi();
          return;
        }
        if (keepSelection) {
          focusFindMatch(0);
        } else {
          findCurrentIdx = 0;
          updateFindCountUi();
        }
      }

      function doFind(backwards) {
        var q = findInp.value || "";
        if (!q) {
          refreshFindMatches(false);
          return;
        }
        if (!findMatches.length) {
          refreshFindMatches(true);
          return;
        }
        focusFindMatch(findCurrentIdx + (backwards ? -1 : 1));
      }

      window.__tpFindStep = doFind;
      window.__tpCloseFindBar = closeFindBar;
      findNext.addEventListener("click", function () { doFind(false); });
      findPrev.addEventListener("click", function () { doFind(true); });
      findClose.addEventListener("click", closeFindBar);
      findCase.addEventListener("click", function () {
        findMatchCase = !findMatchCase;
        findCase.classList.toggle("active", findMatchCase);
        findCase.setAttribute("aria-pressed", String(findMatchCase));
        refreshFindMatches(true);
        findInp.focus();
      });
      findInp.addEventListener("input", function () {
        if (findDebounceTimer) clearTimeout(findDebounceTimer);
        findDebounceTimer = setTimeout(function () { refreshFindMatches(true); }, 45);
      });
      findInp.addEventListener("keydown", function (ev) {
        if (ev.key === "Enter") { ev.preventDefault(); doFind(ev.shiftKey); }
        else if (ev.key === "Escape") { ev.preventDefault(); closeFindBar(); }
      });

      window.__tpToggleFindBar = function (forceOpen, uiTheme) {
        mountTpBubbleUi();
        var rootEl = document.getElementById("__tp_bubble_root");
        var bar = document.getElementById("__tp_find_bar");
        var inp = document.getElementById("__tp_find_input");
        if (!bar || !inp) return;
        if (rootEl && uiTheme) {
          rootEl.setAttribute("data-theme", uiTheme);
        } else if (rootEl && lastBubbleState && lastBubbleState.ui_theme) {
          rootEl.setAttribute("data-theme", lastBubbleState.ui_theme);
        }
        var willOpen = forceOpen === true ? true : bar.style.display !== "flex";
        if (!willOpen) {
          closeFindBar();
          return;
        }
        bar.style.display = "flex";
        try {
          var selectedText = String((window.getSelection && window.getSelection().toString()) || "").trim();
          if (selectedText && selectedText.length <= 80 && selectedText.indexOf("\n") === -1) {
            inp.value = selectedText;
          }
        } catch (_) {}
        try { bar.animate([{ opacity: 0, transform: "translateY(-8px) scale(0.97)" }, { opacity: 1, transform: "translateY(0) scale(1)" }], { duration: 180, easing: "cubic-bezier(0.22,1,0.36,1)" }); } catch (_) {}
        inp.focus();
        inp.select();
        refreshFindMatches(!!inp.value);
      };

      var cachedBookmarks = [];
      var activeColFolder = "__all__";
      var editingBookmarkIdx = -1;

      function closeBookmarkEditor() {
        editingBookmarkIdx = -1;
        colEditor.classList.remove("open");
      }

      function openBookmarkEditor(bmIdx) {
        var bm = cachedBookmarks[bmIdx];
        if (!bm) return;
        editingBookmarkIdx = bmIdx;
        colEdTitle.value = bm.title || "";
        colEdFolder.value = bm.folder || "";
        colEdTags.value = Array.isArray(bm.tags) ? bm.tags.join(", ") : (bm.tags || "");
        colEdKeyword.value = bm.keyword || "";
        colEditor.classList.add("open");
        colEdFolder.focus();
      }

      colEdCancel.addEventListener("click", closeBookmarkEditor);
      colEdSave.addEventListener("click", function () {
        if (editingBookmarkIdx < 0 || !cachedBookmarks[editingBookmarkIdx]) return;
        var target = cachedBookmarks[editingBookmarkIdx];
        var nextTitle = (colEdTitle.value || "").trim() || target.url;
        var nextFolder = (colEdFolder.value || "").trim();
        var nextKeyword = (colEdKeyword.value || "").trim().replace(/\s+/g, "").toLowerCase();
        var nextTags = (colEdTags.value || "")
          .split(",")
          .map(function (t) { return t.trim().replace(/^#+/, ""); })
          .filter(function (t) { return !!t; });
        cachedBookmarks[editingBookmarkIdx] = {
          title: nextTitle,
          url: target.url,
          folder: nextFolder,
          tags: nextTags,
          keyword: nextKeyword
        };
        sendRust({ cmd: "save_bookmarks", bookmarks_json: JSON.stringify(cachedBookmarks) });
        closeBookmarkEditor();
        renderCollectionsRows();
      });

      function renderFolderFilters() {
        while (colFolders.firstChild) colFolders.removeChild(colFolders.firstChild);
        var folders = [];
        var seen = Object.create(null);
        for (var i = 0; i < cachedBookmarks.length; i++) {
          var f = String((cachedBookmarks[i] && cachedBookmarks[i].folder) || "").trim();
          if (f && !seen[f]) {
            seen[f] = true;
            folders.push(f);
          }
        }
        if (folders.length === 0) {
          activeColFolder = "__all__";
          colFolders.style.display = "none";
          return;
        }
        colFolders.style.setProperty("display", "flex", "important");
        var defs = [{ id: "__all__", label: "Tất cả" }].concat(folders.map(function (f) { return { id: f, label: "📁 " + f }; }));
        for (var d = 0; d < defs.length; d++) {
          (function (def) {
            var btn = domEl("button", {
              type: "button",
              className: "tp-col-folder-pill" + (activeColFolder === def.id ? " active" : ""),
              text: def.label
            });
            btn.addEventListener("click", function () {
              activeColFolder = def.id;
              renderCollectionsRows();
            });
            colFolders.appendChild(btn);
          })(defs[d]);
        }
      }

      function renderCollectionsRows() {
        renderFolderFilters();
        while (colList.firstChild) colList.removeChild(colList.firstChild);
        var kw = (colSearch.value || "").toLowerCase().trim();
        var indexed = [];
        for (var i = 0; i < cachedBookmarks.length; i++) {
          var b = cachedBookmarks[i];
          if (!b) continue;
          var folder = String(b.folder || "").trim();
          if (activeColFolder !== "__all__" && folder !== activeColFolder) continue;
          var tags = Array.isArray(b.tags) ? b.tags : [];
          var bKw = String(b.keyword || "").trim();
          if (kw) {
            var hay = [b.title || "", b.url || "", folder, bKw, "@" + bKw, tags.map(function (t) { return "#" + t + " " + t; }).join(" ")].join(" ").toLowerCase();
            if (hay.indexOf(kw) === -1) continue;
          }
          indexed.push({ idx: i, bm: b });
        }
        if (indexed.length === 0) {
          colList.appendChild(domEl("div", {
            style: "padding:18px !important; text-align:center !important; font-size:12px !important; opacity:0.75 !important;",
            text: "Chưa có trang nào khớp trong Bookmarks."
          }));
          return;
        }
        for (var rIdx = 0; rIdx < indexed.length; rIdx++) {
          (function (entry) {
            var bm = entry.bm;
            var metaChips = [];
            if (bm.folder) metaChips.push(domEl("span", { className: "tp-col-chip", text: "📁 " + bm.folder }));
            if (bm.keyword) metaChips.push(domEl("span", { className: "tp-col-chip", text: "@" + bm.keyword }));
            if (Array.isArray(bm.tags)) {
              for (var ti = 0; ti < bm.tags.length; ti++) {
                if (bm.tags[ti]) metaChips.push(domEl("span", { className: "tp-col-chip", text: "#" + bm.tags[ti] }));
              }
            }
            var infoChildren = [
              domEl("div", { style: "font-size:12.5px !important; font-weight:700 !important; white-space:nowrap !important; overflow:hidden !important; text-overflow:ellipsis !important;", text: bm.title || bm.url }),
              domEl("div", { style: "font-size:10.5px !important; opacity:0.7 !important; white-space:nowrap !important; overflow:hidden !important; text-overflow:ellipsis !important;", text: bm.url })
            ];
            if (metaChips.length) infoChildren.push(domEl("div", { className: "tp-col-meta" }, metaChips));
            var info = domEl("div", { style: "min-width:0 !important; flex:1 !important;" }, infoChildren);
            var editBtn = domEl("button", { type: "button", className: "tp-find-btn", title: "Sửa thư mục, tags, keyword", text: "✎" });
            editBtn.addEventListener("click", function (ev) {
              ev.stopPropagation();
              openBookmarkEditor(entry.idx);
            });
            var openTabBtn = domEl("button", { type: "button", className: "tp-find-btn", title: "Mở trong Tab mới", text: "+ Tab" });
            openTabBtn.addEventListener("click", function (ev) {
              ev.stopPropagation();
              colModal.style.display = "none";
              sendRust({ cmd: "new_tab", url: bm.url, isolated: false });
            });
            var delBtn = domEl("button", { type: "button", className: "tp-find-btn", title: "Xóa bookmark", text: "✕" });
            delBtn.addEventListener("click", function (ev) {
              ev.stopPropagation();
              cachedBookmarks.splice(entry.idx, 1);
              closeBookmarkEditor();
              sendRust({ cmd: "save_bookmarks", bookmarks_json: JSON.stringify(cachedBookmarks) });
              renderCollectionsRows();
            });
            var rowActions = domEl("div", { style: "display:flex !important; gap:4px !important; align-items:center !important; flex-shrink:0 !important;" }, [editBtn, openTabBtn, delBtn]);
            var row = domEl("div", { className: "tp-col-row" }, [info, rowActions]);
            row.addEventListener("click", function () {
              colModal.style.display = "none";
              sendRust({ cmd: "navigate_active", url: bm.url });
            });
            colList.appendChild(row);
          })(indexed[rIdx]);
        }
      }
      colSearch.addEventListener("input", renderCollectionsRows);
      colSearch.addEventListener("keydown", function (ev) {
        if (ev.key === "Escape") { ev.preventDefault(); colModal.style.display = "none"; }
        else if (ev.key === "Enter") {
          var firstRow = colList.querySelector(".tp-col-row");
          if (firstRow) { ev.preventDefault(); firstRow.click(); }
        }
      });
      colCloseBtn.addEventListener("click", function () { colModal.style.display = "none"; });
      colPinBtn.addEventListener("click", function () {
        colModal.style.display = "none";
        sendRust({ cmd: "bookmark_active" });
      });
      colModal.addEventListener("click", function (ev) {
        if (ev.target === colModal) colModal.style.display = "none";
      });
      window.__tpToggleCollections = function (payload) {
        mountTpBubbleUi();
        var m = document.getElementById("__tp_collections_modal");
        var r = document.getElementById("__tp_bubble_root");
        var s = document.getElementById("__tp_col_search");
        if (!m) return;
        if (payload && payload.ui_theme && r) r.setAttribute("data-theme", payload.ui_theme);
        if (payload && Array.isArray(payload.bookmarks)) cachedBookmarks = payload.bookmarks;
        var willOpen = m.style.display !== "flex";
        if (willOpen) {
          if (s) s.value = "";
          renderCollectionsRows();
          m.style.display = "flex";
          if (s) s.focus();
        } else {
          m.style.display = "none";
        }
      };

      var toastTimer = null;
      window.__tpShowToast = function (msg) {
        mountTpBubbleUi();
        var t = document.getElementById("__tp_toast");
        if (!t) return;
        t.textContent = String(msg || "");
        t.style.display = "block";
        if (toastTimer) clearTimeout(toastTimer);
        toastTimer = setTimeout(function () { t.style.display = "none"; }, 1800);
      };

      // ===== Zen Boosts (Custom CSS theo domain + Zap ẩn phần tử rác) =====
      function currentBoostDomain() {
        return (window.location.hostname || "").toLowerCase().replace(/^www\./, "");
      }

      function getBoostEntry(domain) {
        var map = (window.__tpBoostsMap && typeof window.__tpBoostsMap === "object") ? window.__tpBoostsMap : {};
        var d = (domain || currentBoostDomain()).toLowerCase().replace(/^www\./, "");
        if (d && map[d]) return { domain: d, entry: map[d] };
        return { domain: d, entry: { css: "", zap_selectors: [], enabled: true } };
      }

      function applyDomainBoostCss() {
        if (!document.documentElement) return;
        var res = getBoostEntry();
        var entry = res.entry || {};
        var existing = document.getElementById("__tp_boost_css");
        var zaps = Array.isArray(entry.zap_selectors) ? entry.zap_selectors.filter(Boolean) : [];
        var customCss = String(entry.css || "").trim();
        if (entry.enabled === false || (!zaps.length && !customCss)) {
          if (existing && existing.parentNode) existing.parentNode.removeChild(existing);
          return;
        }
        var cssParts = [];
        if (zaps.length) {
          cssParts.push(zaps.join(",\n") + " {\n  display: none !important;\n  pointer-events: none !important;\n}");
        }
        if (customCss) cssParts.push(customCss);
        if (!existing) {
          existing = document.createElement("style");
          existing.id = "__tp_boost_css";
          (document.head || document.documentElement).appendChild(existing);
        }
        existing.textContent = cssParts.join("\n\n");
      }

      window.__tpApplyBoosts = function (map) {
        if (map && typeof map === "object") {
          window.__tpBoostsMap = map;
        }
        applyDomainBoostCss();
      };

      var zapHighlight = domEl("div", { id: "__tp_zap_highlight" });
      var zapExitBtn = domEl("button", { type: "button", className: "tp-find-btn", text: "Thoát (Esc)" });
      var zapBanner = domEl("div", { id: "__tp_zap_banner" }, [
        domEl("span", { text: "⚡ Chế độ Zap: Click phần tử rác để ẩn" }),
        zapExitBtn
      ]);

      var boostDomainLbl = domEl("span", { style: "font-size:12px !important; font-weight:700 !important; color:#E2BE68 !important;" });
      var boostToggleBtn = domEl("button", { type: "button", className: "tp-find-btn", text: "Đang bật" });
      var boostCloseBtn = domEl("button", { type: "button", className: "tp-find-btn", text: "✕" });
      var boostHead = domEl("div", { style: "display:flex !important; align-items:center !important; justify-content:space-between !important; gap:8px !important;" }, [
        boostDomainLbl,
        domEl("div", { style: "display:flex !important; gap:6px !important;" }, [boostToggleBtn, boostCloseBtn])
      ]);
      var boostZapsBox = domEl("div", { id: "__tp_boost_zaps" });
      var boostCssInp = domEl("textarea", { id: "__tp_boost_css_inp", placeholder: "/* Custom CSS cho domain này, ví dụ: */\nheader { opacity: 0.92; }" });
      var boostZapBtn = domEl("button", { type: "button", className: "tp-find-btn", text: "⚡ Zap phần tử" });
      var boostResetBtn = domEl("button", { type: "button", className: "tp-find-btn", text: "Xóa Boost" });
      var boostSaveBtn = domEl("button", { type: "button", className: "tp-find-btn", text: "Lưu CSS" });
      var boostActions = domEl("div", { style: "display:flex !important; justify-content:space-between !important; gap:8px !important; flex-wrap:wrap !important;" }, [
        domEl("div", { style: "display:flex !important; gap:6px !important;" }, [boostZapBtn, boostResetBtn]),
        boostSaveBtn
      ]);
      var boostCard = domEl("div", { id: "__tp_boost_card" }, [
        boostHead,
        domEl("div", { style: "font-size:11px !important; opacity:0.75 !important;", text: "Phần tử đã Zap (bấm để khôi phục):" }),
        boostZapsBox,
        domEl("div", { style: "font-size:11px !important; opacity:0.75 !important;", text: "Custom CSS theo domain:" }),
        boostCssInp,
        boostActions
      ]);
      var boostModal = domEl("div", { id: "__tp_boost_modal" }, [boostCard]);

      root.appendChild(zapHighlight);
      root.appendChild(zapBanner);
      root.appendChild(boostModal);

      var zapActive = false;
      var boostModalEnabled = true;
      var boostModalZaps = [];

      function buildZapSelector(el) {
        if (!el || el.nodeType !== 1) return "";
        if (el.id && !/\d{5,}/.test(el.id)) return "#" + CSS.escape(el.id);
        var parts = [];
        var cur = el;
        for (var depth = 0; cur && cur.nodeType === 1 && cur !== document.body && cur !== document.documentElement && depth < 4; depth++) {
          var seg = cur.tagName.toLowerCase();
          if (cur.id && !/\d{5,}/.test(cur.id)) {
            parts.unshift("#" + CSS.escape(cur.id));
            break;
          }
          var clsList = [];
          if (cur.classList) {
            for (var c = 0; c < cur.classList.length && clsList.length < 2; c++) {
              var cname = cur.classList[c];
              if (cname && cname.indexOf("tp-") !== 0 && !/^\d/.test(cname)) {
                clsList.push("." + CSS.escape(cname));
              }
            }
          }
          if (clsList.length) {
            seg += clsList.join("");
          } else if (cur.parentElement) {
            var siblings = cur.parentElement.children;
            var sameTagCount = 0;
            var myIdx = 1;
            for (var s = 0; s < siblings.length; s++) {
              if (siblings[s].tagName === cur.tagName) {
                sameTagCount++;
                if (siblings[s] === cur) myIdx = sameTagCount;
              }
            }
            if (sameTagCount > 1) seg += ":nth-of-type(" + myIdx + ")";
          }
          parts.unshift(seg);
          cur = cur.parentElement;
        }
        return parts.join(" > ");
      }

      function stopZapMode() {
        zapActive = false;
        zapHighlight.style.display = "none";
        zapBanner.style.display = "none";
      }

      function onZapPointerMove(ev) {
        if (!zapActive) return;
        var t = ev.target;
        if (!t || t === document.body || t === document.documentElement || (t.closest && t.closest("#__tp_bubble_root"))) {
          zapHighlight.style.display = "none";
          return;
        }
        var r = t.getBoundingClientRect();
        zapHighlight.style.display = "block";
        zapHighlight.style.top = r.top + "px";
        zapHighlight.style.left = r.left + "px";
        zapHighlight.style.width = r.width + "px";
        zapHighlight.style.height = r.height + "px";
      }

      function onZapClick(ev) {
        if (!zapActive) return;
        var t = ev.target;
        if (t && t.closest && t.closest("#__tp_zap_banner")) return;
        ev.preventDefault();
        ev.stopImmediatePropagation();
        if (!t || t === document.body || t === document.documentElement || (t.closest && t.closest("#__tp_bubble_root"))) return;
        var sel = buildZapSelector(t);
        if (!sel) return;
        var res = getBoostEntry();
        if (!res.domain) return;
        var nextZaps = Array.isArray(res.entry.zap_selectors) ? res.entry.zap_selectors.slice() : [];
        if (nextZaps.indexOf(sel) === -1) nextZaps.push(sel);
        var nextCss = String(res.entry.css || "");
        if (!window.__tpBoostsMap || typeof window.__tpBoostsMap !== "object") window.__tpBoostsMap = {};
        window.__tpBoostsMap[res.domain] = { css: nextCss, zap_selectors: nextZaps, enabled: true };
        applyDomainBoostCss();
        sendRust({ cmd: "save_domain_boost", domain: res.domain, css: nextCss, zap_selectors: nextZaps, enabled: true });
        if (window.__tpShowToast) window.__tpShowToast("⚡ Đã ẩn phần tử trên " + res.domain);
        zapHighlight.style.display = "none";
      }

      function onZapKeydown(ev) {
        if (zapActive && ev.key === "Escape") {
          ev.preventDefault();
          ev.stopImmediatePropagation();
          stopZapMode();
        } else if (!zapActive && boostModal.style.display === "flex" && ev.key === "Escape") {
          ev.preventDefault();
          boostModal.style.display = "none";
        }
      }

      window.addEventListener("pointermove", onZapPointerMove, true);
      window.addEventListener("click", onZapClick, true);
      window.addEventListener("keydown", onZapKeydown, true);
      zapExitBtn.addEventListener("click", stopZapMode);

      window.__tpStartZapMode = function () {
        mountTpBubbleUi();
        boostModal.style.display = "none";
        if (!currentBoostDomain()) {
          if (window.__tpShowToast) window.__tpShowToast("Trang nội bộ không hỗ trợ Zap");
          return;
        }
        zapActive = true;
        zapBanner.style.setProperty("display", "flex", "important");
      };

      function renderBoostModalZaps() {
        while (boostZapsBox.firstChild) boostZapsBox.removeChild(boostZapsBox.firstChild);
        if (!boostModalZaps.length) {
          boostZapsBox.appendChild(domEl("span", { style: "font-size:11px !important; opacity:0.65 !important;", text: "Chưa có phần tử nào bị Zap." }));
          return;
        }
        for (var i = 0; i < boostModalZaps.length; i++) {
          (function (idx, sel) {
            var chip = domEl("button", { type: "button", className: "tp-find-btn", title: "Bấm để khôi phục: " + sel, text: sel.slice(0, 36) + " ✕" });
            chip.addEventListener("click", function () {
              boostModalZaps.splice(idx, 1);
              renderBoostModalZaps();
            });
            boostZapsBox.appendChild(chip);
          })(i, boostModalZaps[i]);
        }
      }

      window.__tpOpenBoosts = function () {
        mountTpBubbleUi();
        stopZapMode();
        var res = getBoostEntry();
        if (!res.domain) {
          if (window.__tpShowToast) window.__tpShowToast("Chỉ hỗ trợ Zen Boosts trên trang web");
          return;
        }
        boostDomainLbl.textContent = "✦ Zen Boosts: " + res.domain;
        boostModalEnabled = res.entry.enabled !== false;
        boostToggleBtn.textContent = boostModalEnabled ? "Đang bật" : "Đang tắt";
        boostModalZaps = Array.isArray(res.entry.zap_selectors) ? res.entry.zap_selectors.slice() : [];
        boostCssInp.value = String(res.entry.css || "");
        renderBoostModalZaps();
        boostModal.style.display = "flex";
        boostCssInp.focus();
      };

      boostToggleBtn.addEventListener("click", function () {
        boostModalEnabled = !boostModalEnabled;
        boostToggleBtn.textContent = boostModalEnabled ? "Đang bật" : "Đang tắt";
      });
      boostCloseBtn.addEventListener("click", function () { boostModal.style.display = "none"; });
      boostModal.addEventListener("click", function (ev) {
        if (ev.target === boostModal) boostModal.style.display = "none";
      });
      boostZapBtn.addEventListener("click", function () { window.__tpStartZapMode(); });
      boostResetBtn.addEventListener("click", function () {
        var d = currentBoostDomain();
        if (!d) return;
        if (window.__tpBoostsMap && typeof window.__tpBoostsMap === "object") delete window.__tpBoostsMap[d];
        applyDomainBoostCss();
        sendRust({ cmd: "reset_domain_boost", domain: d });
        boostModal.style.display = "none";
        if (window.__tpShowToast) window.__tpShowToast("Đã xóa Zen Boosts của " + d);
      });
      boostSaveBtn.addEventListener("click", function () {
        var d = currentBoostDomain();
        if (!d) return;
        var nextCss = String(boostCssInp.value || "").trim();
        if (!window.__tpBoostsMap || typeof window.__tpBoostsMap !== "object") window.__tpBoostsMap = {};
        window.__tpBoostsMap[d] = { css: nextCss, zap_selectors: boostModalZaps.slice(), enabled: boostModalEnabled };
        applyDomainBoostCss();
        sendRust({ cmd: "save_domain_boost", domain: d, css: nextCss, zap_selectors: boostModalZaps.slice(), enabled: boostModalEnabled });
        boostModal.style.display = "none";
        if (window.__tpShowToast) window.__tpShowToast("✦ Đã lưu Zen Boosts cho " + d);
      });

      applyDomainBoostCss();
    } catch (err) {
      window.__tpAgentErr = String(err && err.message ? err.message : err);
    }
  }

  mountTpBubbleUi();
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", function () {
      mountTpBubbleUi();
      if (lastBubbleState && window.__tpUpdateBubbleUi) window.__tpUpdateBubbleUi(lastBubbleState);
    });
  }
  setInterval(function () {
    tryDetectGoogleAccount();
    if (document.body && !document.getElementById("__tp_bubble_root")) {
      mountTpBubbleUi();
      if (lastBubbleState && window.__tpUpdateBubbleUi) window.__tpUpdateBubbleUi(lastBubbleState);
    }
  }, 1500);

  window.__tpUpdateBubbleUi = function (state) {
    if (!state) return;
    lastBubbleState = state;
    mountTpBubbleUi();
    var root = document.getElementById("__tp_bubble_root");
    var orbsWrap = document.getElementById("__tp_web_orbs");
    var pill = document.getElementById("__tp_bubble_pill");
    var statusSpan = document.getElementById("__tp_pill_status");
    var ring = document.getElementById("__tp_active_ring");
    var stopRecBtn = document.getElementById("__tp_pill_stop_rec");

    if (root && state.ui_theme) {
      root.setAttribute("data-theme", state.ui_theme);
    }
    if (orbsWrap) {
      orbsWrap.style.setProperty("display", state.always_show_floating_orbs === false ? "none" : "flex", "important");
    }
    if (Array.isArray(state.floating_orbs)) {
      renderWebOrbs(state.floating_orbs);
    }
    if (state.boosts && typeof state.boosts === "object" && window.__tpApplyBoosts) {
      window.__tpApplyBoosts(state.boosts);
    }
    if (state.context_menu_prefs && typeof state.context_menu_prefs === "object") {
      window.__tpContextMenuPrefs = state.context_menu_prefs;
    }

    var orbAi = document.getElementById("__tp_orb_ai");
    var orbFb = document.getElementById("__tp_orb_fb");
    var orbZalo = document.getElementById("__tp_orb_zalo");

    window.__tpRecordingSkill = !!state.recording;

    if (ring) {
      ring.classList.toggle("recording-ring", !!state.recording);
      ring.classList.toggle("split-active", !state.recording && !!state.show_split_ring);
    }

    var isOpen = state.ai_mode && state.ai_mode !== "bubble";
    var ch = state.chat_channel || "ai";
    if (orbAi) {
      orbAi.classList.toggle("active-orb", isOpen && ch === "ai");
      orbAi.classList.toggle("ai-flash-pulse", !!state.ai_finished_flash);
    }
    if (orbFb) orbFb.classList.toggle("active-orb", isOpen && ch === "fb");
    if (orbZalo) orbZalo.classList.toggle("active-orb", isOpen && ch === "zalo");

    if (typeof state.chime_seq === "number" && state.chime_seq > lastChimeSeq) {
      if (lastChimeSeq > 0 || state.ai_finished_flash) {
        playGentleNotifyChime(state.chime_kind || "ai_done");
      }
      lastChimeSeq = state.chime_seq;
    }

    function setCnt(id, val) {
      var el = document.getElementById(id);
      if (!el) return;
      var n = parseInt(val || 0, 10) || 0;
      if (n > 0) {
        el.textContent = n > 99 ? "99+" : String(n);
        el.classList.add("show");
      } else {
        el.textContent = "";
        el.classList.remove("show");
      }
    }
    var showBadges = state.show_unread_badges !== false;
    setCnt("__tp_cnt_ai", showBadges ? state.unread_ai : 0);
    setCnt("__tp_cnt_fb", showBadges ? state.unread_fb : 0);
    setCnt("__tp_cnt_zalo", showBadges ? state.unread_zalo : 0);

    if (typeof state.adblock_enabled === "boolean") {
      window.__tpAdblockEnabled = state.adblock_enabled;
      if (!state.adblock_enabled) {
        var existingShield = document.getElementById("__tp_adshield_css");
        if (existingShield) existingShield.remove();
      } else {
        mountAdShieldCss();
      }
    }

    if (!pill) return;
    if (state.recording || state.working) {
      pill.classList.add("show-pill");
    } else {
      pill.classList.remove("show-pill");
    }

    pill.classList.toggle("recording", !!state.recording);
    pill.classList.toggle("working", !state.recording && !!state.working);

    if (stopRecBtn) {
      stopRecBtn.style.display = state.recording ? "inline-block" : "none";
    }

    if (state.status_text) {
      statusSpan.textContent = state.status_text;
    } else {
      statusSpan.textContent = state.recording
        ? "🔴 Đang ghi mẫu Skill..."
        : state.working
          ? "⚡ AI đang làm việc..."
          : "🤖 AI Cowork";
    }
  };

  window.__tpBuildDomContext = function (maxChars) {
    var limit = typeof maxChars === "number" && maxChars > 0 ? maxChars : 9000;
    var elements = [];
    var nodes = document.querySelectorAll(
      "input, textarea, button, select, a[href], [role='button'], [contenteditable='true']"
    );
    var nextId = 1;
    for (var i = 0; i < nodes.length && elements.length < 80; i++) {
      var el = nodes[i];
      if (el.closest && el.closest("#__tp_bubble_root")) continue;
      var rect = el.getBoundingClientRect();
      if (rect.width < 10 || rect.height < 10) continue;
      el.setAttribute("data-tp-ai-id", String(nextId));
      var info = describeElement(el);
      elements.push({
        id: nextId,
        tag: info ? info.tag : el.tagName.toLowerCase(),
        type: el.getAttribute("type") || "",
        label: info ? info.label : "",
        selector: info ? info.selector : ""
      });
      nextId++;
    }

    var bodyText = "";
    try {
      bodyText = (document.body && document.body.innerText) || "";
    } catch (_) {}

    return {
      title: document.title || "",
      url: window.location.href || "",
      visible_text: bodyText.slice(0, limit),
      elements: elements
    };
  };

  window.__tpCollectDomForAi = function () {
    var ctx = window.__tpBuildDomContext(9000);
    sendRust({
      cmd: "dom_context_result",
      title: ctx.title,
      url: ctx.url,
      visible_text: ctx.visible_text,
      elements_json: JSON.stringify(ctx.elements)
    });
  };

  function findTargetElement(act) {
    if (act.target_id) {
      var byId = document.querySelector('[data-tp-ai-id="' + act.target_id + '"]');
      if (byId) return byId;
    }
    if (act.selector) {
      try {
        var bySel = document.querySelector(act.selector);
        if (bySel) return bySel;
      } catch (_) {}
    }
    if (act.label) {
      var needle = String(act.label).toLowerCase().trim();
      if (needle) {
        var candidates = document.querySelectorAll(
          "button, a, input, textarea, select, [role='button'], [contenteditable='true']"
        );
        for (var i = 0; i < candidates.length; i++) {
          var c = candidates[i];
          if (c.closest && c.closest("#__tp_bubble_root")) continue;
          var info = describeElement(c);
          if (info && info.label && info.label.toLowerCase().includes(needle)) {
            return c;
          }
        }
      }
    }
    if (act.type === "fill") {
      return document.querySelector(
        "input[type='search'], input[type='text'], input:not([type='hidden']), textarea, [contenteditable='true']"
      );
    }
    return null;
  }

  window.__tpExecuteDomAction = function (act) {
    if (!act || !act.type) return;
    if (act.type === "scroll") {
      var dy = act.direction === "up" ? -520 : 520;
      window.scrollBy({ top: dy, behavior: "smooth" });
      return;
    }
    if (act.type === "navigate" && act.url) {
      sendRust({ cmd: "navigate_active", url: act.url });
      return;
    }
    if (act.type === "new_tab" && act.url) {
      sendRust({ cmd: "new_tab", url: act.url, isolated: !!act.isolated });
      return;
    }
    if (act.type === "split_view") {
      sendRust({ cmd: "cycle_view_mode" });
      return;
    }

    var target = findTargetElement(act);
    if (!target) return;

    var prevOutline = target.style.outline;
    target.style.outline = "3px solid #10b981";
    target.scrollIntoView({ behavior: "smooth", block: "center" });
    setTimeout(function () {
      target.style.outline = prevOutline;
    }, 1800);

    if (act.type === "click") {
      target.focus();
      target.click();
    } else if (act.type === "fill") {
      target.focus();
      var textVal = act.value || "";
      if (target.isContentEditable) {
        document.execCommand("insertText", false, textVal);
        target.dispatchEvent(new InputEvent("input", { bubbles: true, data: textVal }));
      } else {
        target.value = textVal;
        target.dispatchEvent(new Event("input", { bubbles: true }));
        target.dispatchEvent(new Event("change", { bubbles: true }));
        if (act.submit && target.form) {
          try {
            target.form.requestSubmit ? target.form.requestSubmit() : target.form.submit();
          } catch (_) {}
        }
      }
    }
  };
  // ================= BỘ CHẶN QUẢNG CÁO 3 LỚP (WEB / XEM PHIM / YOUTUBE) =================
  function mountAdShieldCss() {
    if (!document.documentElement || document.getElementById("__tp_adshield_css")) return;
    var st = document.createElement("style");
    st.id = "__tp_adshield_css";
    st.textContent = `
      /* 1. Chặn Banner, Catfish, Popup & Link Popunder trên Web / Trang Xem Phim (Chống Anti-Adblock bằng 0px + pointer-events:none) */
      .Adv, .ad-center-header, .header-ads-pc, .Ads, .ads_player, .bellow_ads_player, .below-player,
      .ads-300, #pc-catfixx, .pc-catfixx, .pc_catfix_adv, [id^="_preload-ads"], .ads-banner,
      .popup-icon-close, a#bb0, a[href*="popunder"], a[href*="chinhhang"], a[href*="adqc.net"],
      a[href*="topbanner"], a[href*="catfish"], a[href*="underplayer"], a[href*="right-banner"],
      ins.adsbygoogle, [id^="div-gpt-ad"], iframe[src*="doubleclick"], iframe[src*="googlesyndication"],
      [class*="quangcao"], [id*="quangcao"], .float-ck, #float_content_left, #float_content_right {
        opacity: 0 !important;
        pointer-events: none !important;
        max-height: 0px !important;
        min-height: 0px !important;
        height: 0px !important;
        width: 0px !important;
        margin: 0px !important;
        padding: 0px !important;
        border: none !important;
        overflow: hidden !important;
        visibility: hidden !important;
        z-index: -99999 !important;
      }

      /* 2. Chặn Banner, Thẻ tài trợ & Khung phủ Quảng cáo trên YouTube */
      ytd-ad-slot-renderer, ytd-in-feed-ad-layout-renderer, ytd-banner-promo-renderer,
      ytd-statement-banner-renderer, ytd-EngagementPanelSectionListRenderer[target-id="engagement-panel-ads"],
      #masthead-ad, #player-ads, .ytp-ad-overlay-container, .ytp-ad-message-container,
      ytd-promoted-sparkles-web-renderer, ytd-display-ad-renderer, ytd-compact-promoted-video-renderer,
      .ytd-mealbar-promo-renderer, ytd-popup-container:has(a[href*="premium"]) {
        display: none !important;
      }
    `;
    (document.head || document.documentElement).appendChild(st);
  }

  window.__tpAdblockEnabled = true;

  function isYouTubeProtectedSurface(el) {
    if (!el || el === document.documentElement || el === document.body) return true;
    if (el.tagName === "YTD-APP" || el.id === "movie_player" || el.id === "masthead") return true;
    if (el.closest && el.closest("#__tp_bubble_root, #movie_player, #masthead, tp-yt-app-drawer, ytd-miniplayer, ytd-multi-page-menu-renderer, yt-live-chat-renderer, tp-yt-iron-dropdown")) {
      return true;
    }
    return false;
  }

  function hasVisibleYouTubeDialog() {
    var dialogs = document.querySelectorAll("tp-yt-paper-dialog, ytd-popup-container [role='dialog'], yt-dialog-view-model");
    for (var i = 0; i < dialogs.length; i++) {
      var d = dialogs[i];
      var popupHost = d.closest && d.closest("ytd-popup-container");
      if (popupHost && window.getComputedStyle(popupHost).display === "none") continue;
      var ds = window.getComputedStyle(d);
      if (ds.display === "none" || ds.visibility === "hidden" || ds.opacity === "0") continue;
      if (d.getAttribute("aria-hidden") === "true") continue;
      var dr = d.getBoundingClientRect();
      if (dr.width > 40 && dr.height > 40) return true;
    }
    return false;
  }

  function releaseLayerPointerEvents(el) {
    if (!el || el.nodeType !== 1) return;
    if (el.style.getPropertyValue("pointer-events") !== "none") {
      el.style.setProperty("pointer-events", "none", "important");
    }
    el.setAttribute("data-tp-youtube-overlay-released", "1");
  }

  function releaseYouTubeClickSurface() {
    // 1. Release ad / about:blank iframes anywhere on YouTube (including inside ytd-app)
    var iframes = document.getElementsByTagName("iframe");
    for (var n = 0; n < iframes.length; n++) {
      var frame = iframes[n];
      if (isYouTubeProtectedSurface(frame)) continue;
      var src = (frame.getAttribute("src") || "").toLowerCase();
      if (
        src.indexOf("youtube.com/live_chat") !== -1 ||
        src.indexOf("youtube.com/embed") !== -1 ||
        src.indexOf("studio.youtube.com") !== -1 ||
        src.indexOf("accounts.google.com") !== -1
      ) {
        continue;
      }
      var isSuspiciousSrc =
        src === "" ||
        src === "about:blank" ||
        src.indexOf("doubleclick") !== -1 ||
        src.indexOf("googlesyndication") !== -1 ||
        src.indexOf("googleadservices") !== -1 ||
        src.indexOf("pagead") !== -1 ||
        src.indexOf("adservice") !== -1;
      var rect = frame.getBoundingClientRect();
      var isLargeOverlay = rect.width >= window.innerWidth * 0.5 && rect.height >= window.innerHeight * 0.35;
      if (isSuspiciousSrc || isLargeOverlay) {
        releaseLayerPointerEvents(frame);
      }
    }

    // 2. Release orphaned YouTube modal backdrops (e.g. when popup was hidden by adshield CSS)
    var visibleDialog = hasVisibleYouTubeDialog();
    if (!visibleDialog) {
      var backdrops = document.querySelectorAll("tp-yt-iron-overlay-backdrop, #scrim");
      for (var b = 0; b < backdrops.length; b++) {
        var bd = backdrops[b];
        if (isYouTubeProtectedSurface(bd)) continue;
        var bds = window.getComputedStyle(bd);
        if (bds.position === "fixed" || bds.position === "absolute" || bd.classList.contains("opened")) {
          releaseLayerPointerEvents(bd);
        }
      }
      var ytdApp = document.querySelector("ytd-app");
      if (ytdApp && ytdApp.hasAttribute("inert")) {
        ytdApp.removeAttribute("inert");
      }
    }

    // 3. O(1) hit-test across viewport points to catch any invisible/high-z overlay blocking clicks
    if (typeof document.elementsFromPoint === "function" && window.innerWidth > 0 && window.innerHeight > 0) {
      var pts = [
        [window.innerWidth * 0.5, window.innerHeight * 0.5],
        [window.innerWidth * 0.3, window.innerHeight * 0.45],
        [window.innerWidth * 0.7, window.innerHeight * 0.45]
      ];
      for (var p = 0; p < pts.length; p++) {
        var stack = document.elementsFromPoint(pts[p][0], pts[p][1]);
        for (var s = 0; s < stack.length; s++) {
          var el = stack[s];
          if (isYouTubeProtectedSurface(el)) break;
          if (el.closest && el.closest("a, button, video, input, textarea, ytd-rich-item-renderer, ytd-video-renderer, ytd-compact-video-renderer, ytd-grid-video-renderer, tp-yt-paper-dialog, yt-dialog-view-model")) {
            break;
          }
          var st = window.getComputedStyle(el);
          if (st.position !== "fixed" && st.position !== "absolute") continue;
          var er = el.getBoundingClientRect();
          if (er.width < window.innerWidth * 0.45 || er.height < window.innerHeight * 0.3) continue;
          var highZ = Number(st.zIndex || 0) >= 10;
          var transparentBg = st.opacity === "0" || st.backgroundColor === "rgba(0, 0, 0, 0)" || st.backgroundColor === "transparent";
          var emptyOverlay = el.children.length === 0 && !(el.textContent || "").trim();
          if (transparentBg || emptyOverlay || (highZ && !visibleDialog)) {
            releaseLayerPointerEvents(el);
          }
        }
      }
    }
  }

  function runActiveAdCleanerAndSkipper() {
    var host = (window.location.hostname || "").toLowerCase();
    var isYouTube = host === "youtube.com" || host.endsWith(".youtube.com");
    if (isYouTube) {
      try { releaseYouTubeClickSurface(); } catch (_) {}
    }
    if (window.__tpAdblockEnabled === false) return;
    try {
      mountAdShieldCss();

      // A. Vô hiệu hóa các link popunder ẩn & tự bấm nút đóng popup trên trang xem phim
      var hijackLinks = document.querySelectorAll(
        'a#bb0, a[href*="popunder"], a[href*="chinhhang"], a[href*="adqc.net"], a[href*="catfish"], a[href*="topbanner"]'
      );
      for (var i = 0; i < hijackLinks.length; i++) {
        var a = hijackLinks[i];
        a.removeAttribute("href");
        a.removeAttribute("target");
        a.style.setProperty("display", "none", "important");
        a.style.setProperty("pointer-events", "none", "important");
      }

      var closeBtns = document.querySelectorAll(
        '#close-_preload-ads-1, #close-_preload-ads-2, .popup-icon-close, #pc-catfixx .close, .ads-close'
      );
      for (var j = 0; j < closeBtns.length; j++) {
        try { closeBtns[j].click(); } catch (_) {}
      }

      // B. Tự động tua & bấm Skip Quảng cáo Video trên trang xem phim (JWPlayer / VideoJS VAST)
      var vastSkip = document.querySelector(
        '.jw-skip, .jw-skippable, .vjs-skip-button, .vast-skip-button, [class*="skip-ad"]'
      );
      if (vastSkip) {
        try { vastSkip.click(); } catch (_) {}
      }
      var adPlayer = document.querySelector('.jw-flag-ads video, .vjs-ad-playing video');
      if (adPlayer && adPlayer.duration && isFinite(adPlayer.duration)) {
        adPlayer.muted = true;
        adPlayer.currentTime = adPlayer.duration;
      }

      // C. Tự động Tua Nhanh & Bấm Bỏ Qua (Auto-Skip < 0.1s) Quảng cáo Video trên YouTube
      if (host.includes("youtube.com")) {
        var moviePlayer = document.getElementById("movie_player");
        var isYtAd =
          (moviePlayer && (moviePlayer.classList.contains("ad-showing") || moviePlayer.classList.contains("ad-interrupting"))) ||
          document.querySelector(".ad-showing, .ad-interrupting");

        var skipBtns = document.querySelectorAll(
          ".ytp-skip-ad-button, .ytp-ad-skip-button, .ytp-ad-skip-button-modern, .ytp-ad-skip-button-slot button, .ytp-ad-overlay-close-button"
        );
        for (var k = 0; k < skipBtns.length; k++) {
          try { skipBtns[k].click(); } catch (_) {}
        }

        var ytVideo = document.querySelector("video.html5-main-video");
        if (ytVideo) {
          if (isYtAd) {
            ytVideo.muted = true;
            ytVideo.playbackRate = 16.0;
            if (ytVideo.duration && isFinite(ytVideo.duration) && ytVideo.duration > 0.5) {
              ytVideo.currentTime = Math.max(0, ytVideo.duration - 0.05);
            }
          } else if (ytVideo.playbackRate === 16.0) {
            ytVideo.playbackRate = 1.0;
            ytVideo.muted = false;
          }
        }
      }
    } catch (_) {}
  }

  // ================= 4. NHẮC VIỆC TRỰC TIẾP (REMINDER ENGINE) & BẮT VIỆC TỪ VĂN BẢN (PHASE 4) =================
  function playReminderChime() {
    try {
      var ctx = new (window.AudioContext || window.webkitAudioContext)();
      var now = ctx.currentTime;
      var o1 = ctx.createOscillator(); var g1 = ctx.createGain();
      o1.type = "sine"; o1.frequency.setValueAtTime(659.25, now);
      g1.gain.setValueAtTime(0.08, now); g1.gain.exponentialRampToValueAtTime(0.001, now + 0.35);
      o1.connect(g1); g1.connect(ctx.destination);
      o1.start(now); o1.stop(now + 0.35);

      var o2 = ctx.createOscillator(); var g2 = ctx.createGain();
      o2.type = "sine"; o2.frequency.setValueAtTime(987.77, now + 0.18);
      g2.gain.setValueAtTime(0.12, now + 0.18); g2.gain.exponentialRampToValueAtTime(0.001, now + 0.7);
      o2.connect(g2); g2.connect(ctx.destination);
      o2.start(now + 0.18); o2.stop(now + 0.7);
    } catch (_) {}
  }

  window.__tpOnReminderTrigger = function(task) {
    if (!task) return;
    playReminderChime();
    var existing = document.getElementById("__tp_reminder_banner");
    if (existing && existing.parentNode) existing.parentNode.removeChild(existing);

    var srcStr = task.source === "zalo" ? "Zalo Web" : (task.source === "messenger" ? "Messenger" : "Kế hoạch");
    if (task.source_contact) srcStr += " • " + task.source_contact;

    var remStyle = domEl("style", { text: "@keyframes tpBannerSlideIn { from { transform:translateX(60px); opacity:0; } to { transform:translateX(0); opacity:1; } }" });
    var closeBtn = domEl("button", { type: "button", id: "__tp_rem_close", style: "background:transparent; border:none; color:#C8C0B0; font-size:14px; cursor:pointer;", text: "✕" });
    var hdr = domEl("div", { style: "display:flex; justify-content:space-between; align-items:flex-start; margin-bottom:8px;" }, [
      domEl("div", { style: "display:flex; align-items:center; gap:6px;" }, [
        domEl("span", { style: "background:#EF4444; width:8px; height:8px; border-radius:50%; box-shadow:0 0 8px #EF4444;" }),
        domEl("b", { style: "font-size:11px; text-transform:uppercase; letter-spacing:0.5px; color:#E2BE68;", text: "Nhắc Việc Đến Hạn" })
      ]),
      closeBtn
    ]);
    var titleDiv = domEl("div", { style: "font-size:14px; font-weight:700; line-height:1.3; margin-bottom:6px; color:#FFFFFF;", text: task.title || "Công việc đến hạn" });
    var srcDiv = domEl("div", { style: "font-size:11px; color:#C8C0B0; margin-bottom:12px;" }, [
      document.createTextNode("Nguồn: "),
      domEl("span", { style: "color:#E2BE68; font-weight:600;", text: srcStr })
    ]);
    var actBtn = domEl("button", { type: "button", id: "__tp_rem_act", style: "background:linear-gradient(135deg, #E2BE68 0%, #B5872E 100%); color:#140F05; border:none; padding:6px 14px; border-radius:999px; font-size:11px; font-weight:700; cursor:pointer;", text: "🚀 Làm ngay" });
    var snz10 = domEl("button", { type: "button", id: "__tp_rem_snooze10", style: "background:rgba(255,255,255,0.06); border:1px solid rgba(226,190,104,0.3); color:#F5F2EB; padding:6px 10px; border-radius:999px; font-size:11px; font-weight:600; cursor:pointer;", text: "⏱️ +10p" });
    var snz30 = domEl("button", { type: "button", id: "__tp_rem_snooze30", style: "background:rgba(255,255,255,0.06); border:1px solid rgba(226,190,104,0.3); color:#F5F2EB; padding:6px 10px; border-radius:999px; font-size:11px; font-weight:600; cursor:pointer;", text: "⏱️ +30p" });
    var doneBtn = domEl("button", { type: "button", id: "__tp_rem_done", style: "background:rgba(16,185,129,0.2); border:1px solid #10B981; color:#34D399; padding:6px 10px; border-radius:999px; font-size:11px; font-weight:700; cursor:pointer;", text: "✅ Xong" });
    var btnRow = domEl("div", { style: "display:flex; gap:5px; flex-wrap:wrap;" }, [actBtn, snz10, snz30, doneBtn]);

    var banner = domEl("div", {
      id: "__tp_reminder_banner",
      style: "position:fixed; top:20px; right:20px; z-index:2147483647; width:min(380px, 92vw); background:radial-gradient(circle at 20% 20%, #1c202a, #0e1015 88%); border:1.5px solid #E2BE68; border-radius:16px; padding:16px 18px; box-shadow:0 14px 40px rgba(0,0,0,0.7), 0 0 16px rgba(226,190,104,0.3); font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif; color:#F5F2EB; user-select:none; animation:tpBannerSlideIn 0.28s cubic-bezier(0.22,1,0.36,1);"
    }, [remStyle, hdr, titleDiv, srcDiv, btnRow]);

    document.body.appendChild(banner);

    function closeBanner() {
      if (banner.parentNode) banner.parentNode.removeChild(banner);
    }
    closeBtn.onclick = closeBanner;
    actBtn.onclick = function() {
      closeBanner();
      sendIpc({ cmd: "open_chat_channel", channel: "ai" });
      sendIpc({ cmd: "set_ai_mode", mode: "popup" });
    };
    snz10.onclick = function() {
      closeBanner();
      sendIpc({ cmd: "snooze_reminder", id: task.id, minutes: 10 });
    };
    snz30.onclick = function() {
      closeBanner();
      sendIpc({ cmd: "snooze_reminder", id: task.id, minutes: 30 });
    };
    doneBtn.onclick = function() {
      closeBanner();
      sendIpc({ cmd: "complete_task", id: task.id });
    };
  };

  if (!IS_INTERNAL_HTML_PAGE) {
    mountAdShieldCss();
    setInterval(runActiveAdCleanerAndSkipper, 220);
  }

  // ===== Audio/Mute Indicator + Mini Media Player =====
  window.__tpTabMuted = false;
  var lastMediaSig = "";

  function findActiveMediaElement() {
    var nodes = document.querySelectorAll("video, audio");
    var playingEl = null;
    var fallbackEl = null;
    for (var i = 0; i < nodes.length; i++) {
      var el = nodes[i];
      if (!fallbackEl) fallbackEl = el;
      if (!el.paused && !el.ended && el.readyState >= 2) {
        playingEl = el;
        break;
      }
    }
    return playingEl || fallbackEl;
  }

  function reportMediaState() {
    try {
      var nodes = document.querySelectorAll("video, audio");
      var playing = false;
      for (var i = 0; i < nodes.length; i++) {
        var el = nodes[i];
        if (window.__tpTabMuted && !el.muted) el.muted = true;
        if (!el.paused && !el.ended && el.readyState >= 2) playing = true;
      }
      var md = (navigator.mediaSession && navigator.mediaSession.metadata) || null;
      var title = (md && md.title) || (playing ? (document.title || "") : "");
      var artist = (md && md.artist) || (playing ? (window.location.hostname || "") : "");
      var sig = (playing ? "1" : "0") + "|" + (window.__tpTabMuted ? "1" : "0") + "|" + title.slice(0, 60) + "|" + artist.slice(0, 40);
      if (sig === lastMediaSig) return;
      lastMediaSig = sig;
      sendRust({
        cmd: "tab_media_state",
        playing: playing,
        muted: !!window.__tpTabMuted,
        title: title.slice(0, 80),
        artist: artist.slice(0, 50)
      });
    } catch (_) {}
  }

  window.__tpSetTabMuted = function (mute) {
    window.__tpTabMuted = !!mute;
    var nodes = document.querySelectorAll("video, audio");
    for (var i = 0; i < nodes.length; i++) {
      try { nodes[i].muted = window.__tpTabMuted; } catch (_) {}
    }
    reportMediaState();
  };

  window.__tpMediaControl = function (action) {
    var el = findActiveMediaElement();
    if (action === "toggle_play") {
      if (el) {
        if (el.paused) { try { el.play(); } catch (_) {} }
        else { try { el.pause(); } catch (_) {} }
      }
      setTimeout(reportMediaState, 120);
    } else if (action === "next") {
      var nextBtn = document.querySelector(".ytp-next-button, [data-testid='control-button-skip-forward'], [aria-label*='Next' i], [aria-label*='Tiếp' i]");
      if (nextBtn) try { nextBtn.click(); } catch (_) {}
    } else if (action === "prev") {
      var prevBtn = document.querySelector(".ytp-prev-button, [data-testid='control-button-skip-back'], [aria-label*='Previous' i], [aria-label*='Trước' i]");
      if (prevBtn) try { prevBtn.click(); } catch (_) {}
    } else if (action === "pip") {
      var vid = document.querySelector("video");
      if (vid && document.pictureInPictureEnabled) {
        if (document.pictureInPictureElement) document.exitPictureInPicture().catch(function(){});
        else vid.requestPictureInPicture().catch(function(){});
      }
    }
  };

  if (!IS_INTERNAL_HTML_PAGE) {
    setInterval(reportMediaState, 1200);
  }

  // ===== Bonus: Floating Link Status Pill góc dưới trái =====
  var linkPillHideTimer = null;
  document.addEventListener("mouseover", function(e) {
    try {
      var a = e.target && e.target.closest && e.target.closest("a[href]");
      if (!a) return;
      var href = a.getAttribute("href") || "";
      if (!href || href.indexOf("http://") !== 0 && href.indexOf("https://") !== 0) {
        try {
          var resolved = new URL(href, window.location.href).href;
          if (resolved.indexOf("http://") !== 0 && resolved.indexOf("https://") !== 0) return;
          href = resolved;
        } catch (_) { return; }
      }
      var pillEl = document.getElementById("__tp_link_pill");
      if (!pillEl) return;
      if (linkPillHideTimer) { clearTimeout(linkPillHideTimer); linkPillHideTimer = null; }
      pillEl.textContent = href.length > 90 ? href.slice(0, 90) + "…" : href;
      pillEl.classList.add("show-pill");
    } catch (_) {}
  });
  document.addEventListener("mouseout", function(e) {
    try {
      var a = e.target && e.target.closest && e.target.closest("a[href]");
      if (!a) return;
      var pillEl = document.getElementById("__tp_link_pill");
      if (!pillEl) return;
      if (linkPillHideTimer) clearTimeout(linkPillHideTimer);
      linkPillHideTimer = setTimeout(function() {
        pillEl.classList.remove("show-pill");
      }, 120);
    } catch (_) {}
  });
  // ===== Zen Auto Picture-in-Picture (Auto PiP) =====
  window.__tpAutoPip = function(enter) {
    try {
      if (enter) {
        var vid = document.querySelector("video");
        if (vid && !vid.paused && !vid.ended && vid.readyState >= 2 && vid.videoWidth > 180 && document.pictureInPictureEnabled && !document.pictureInPictureElement) {
          vid.requestPictureInPicture().catch(function(){});
        }
      } else {
        if (document.pictureInPictureElement) {
          document.exitPictureInPicture().catch(function(){});
        }
      }
    } catch (_) {}
  };

  // ==========================================
  // Zen Translations: Dịch Bôi Đen & Dịch Toàn Trang
  // (Tuân thủ Trusted Types: 100% DOM createElement / textContent)
  // ==========================================
  var __tpTranslateCounter = 0;
  var __tpTranslatePending = {};
  var __tpOriginalTexts = new WeakMap();
  var __tpTranslatedNodes = [];
  var __tpIsPageTranslated = false;

  window.__tpOnTranslated = function(reqId, translations) {
    if (__tpTranslatePending[reqId]) {
      try {
        __tpTranslatePending[reqId](translations);
      } catch (e) {
        console.warn("[TPbrowser Translation] callback error:", e);
      }
      delete __tpTranslatePending[reqId];
    }
  };

  var selBtn = null;
  var selCard = null;

  function ensureTranslateStyles() {
    if (document.getElementById("__tp_translate_css")) return;
    var st = document.createElement("style");
    st.id = "__tp_translate_css";
    st.textContent = [
      "#__tp_sel_btn { position: fixed; z-index: 2147483640; width: 28px; height: 28px; border-radius: 8px; background: #171B24; border: 1px solid rgba(212,175,55,0.45); color: #E2BE68; display: flex; align-items: center; justify-content: center; cursor: pointer; box-shadow: 0 4px 16px rgba(0,0,0,0.5); font-size: 13px; font-weight: bold; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Inter, sans-serif; transition: transform 0.15s, background 0.15s; }",
      "#__tp_sel_btn:hover { background: #222734; transform: scale(1.08); border-color: #E2BE68; }",
      "#__tp_sel_card { position: fixed; z-index: 2147483641; max-width: 380px; min-width: 240px; background: #171B24; border: 1px solid rgba(212,175,55,0.35); border-radius: 12px; box-shadow: 0 12px 36px rgba(0,0,0,0.65); padding: 12px; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Inter, sans-serif; color: #F5F2EB; font-size: 13px; line-height: 1.45; }",
      "#__tp_sel_card .tp-card-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 8px; font-size: 11px; font-weight: 600; color: #E2BE68; letter-spacing: 0.5px; text-transform: uppercase; }",
      "#__tp_sel_card .tp-card-close { background: none; border: none; color: #9A9284; cursor: pointer; font-size: 14px; padding: 0 4px; line-height: 1; border-radius: 4px; }",
      "#__tp_sel_card .tp-card-close:hover { color: #FFF; background: rgba(255,255,255,0.1); }",
      "#__tp_sel_card .tp-card-body { color: #F5F2EB; word-break: break-word; max-height: 220px; overflow-y: auto; padding-right: 4px; }",
      "#__tp_sel_card .tp-card-copy { margin-top: 10px; display: inline-flex; align-items: center; gap: 4px; padding: 4px 10px; border-radius: 6px; background: rgba(212,175,55,0.15); border: 1px solid rgba(212,175,55,0.3); color: #E2BE68; font-size: 11.5px; font-weight: 500; cursor: pointer; }",
      "#__tp_sel_card .tp-card-copy:hover { background: rgba(212,175,55,0.25); }",
      "#__tp_page_banner { position: fixed; top: 12px; right: 20px; z-index: 2147483642; background: #171B24; border: 1px solid rgba(212,175,55,0.35); border-radius: 10px; padding: 9px 14px; display: flex; align-items: center; gap: 10px; box-shadow: 0 8px 32px rgba(0,0,0,0.55); font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Inter, sans-serif; font-size: 12.5px; color: #F5F2EB; transition: opacity 0.25s, transform 0.25s; }",
      "#__tp_page_banner .tp-btn { border: none; border-radius: 6px; padding: 4px 10px; font-size: 11.5px; font-weight: 600; cursor: pointer; transition: all 0.15s; }",
      "#__tp_page_banner .tp-btn-primary { background: linear-gradient(135deg, #E2BE68 0%, #B5872E 100%); color: #140F05; }",
      "#__tp_page_banner .tp-btn-primary:hover { filter: brightness(1.1); }",
      "#__tp_page_banner .tp-btn-ghost { background: rgba(255,255,255,0.06); color: #E5E7EB; border: 1px solid rgba(255,255,255,0.1); }",
      "#__tp_page_banner .tp-btn-ghost:hover { background: rgba(255,255,255,0.12); }",
      "#__tp_page_banner .tp-close { background: none; border: none; color: #9CA3AF; cursor: pointer; font-size: 14px; padding: 2px 4px; }"
    ].join("\n");
    (document.head || document.documentElement).appendChild(st);
  }

  document.addEventListener("mouseup", function(e) {
    if (e.target && e.target.closest && (e.target.closest("#__tp_sel_btn") || e.target.closest("#__tp_sel_card"))) {
      return;
    }
    setTimeout(function() {
      var sel = window.getSelection();
      var text = sel ? sel.toString().trim() : "";
      if (!text || text.length < 2 || text.length > 2000) {
        if (selBtn && selBtn.parentNode) selBtn.parentNode.removeChild(selBtn);
        selBtn = null;
        return;
      }
      ensureTranslateStyles();
      if (!selBtn) {
        selBtn = document.createElement("div");
        selBtn.id = "__tp_sel_btn";
        selBtn.title = "Dịch đoạn bôi đen bằng TPbrowser";
        selBtn.textContent = "文A";
        (document.body || document.documentElement).appendChild(selBtn);
      }
      var range = sel.getRangeAt(0);
      var rect = range.getBoundingClientRect();
      var top = rect.bottom + 6;
      var left = rect.right - 28;
      if (top + 40 > window.innerHeight) top = rect.top - 36;
      if (left < 10) left = 10;
      selBtn.style.top = Math.max(8, top) + "px";
      selBtn.style.left = Math.min(window.innerWidth - 38, Math.max(8, left)) + "px";

      selBtn.onmousedown = function(be) {
        be.preventDefault();
        be.stopPropagation();
      };
      selBtn.onclick = function(be) {
        be.preventDefault();
        be.stopPropagation();
        var curText = text;
        if (selBtn && selBtn.parentNode) selBtn.parentNode.removeChild(selBtn);
        selBtn = null;
        openSelectionTranslationCard(curText, rect);
      };
    }, 10);
  });

  function openSelectionTranslationCard(text, anchorRect) {
    ensureTranslateStyles();
    if (selCard && selCard.parentNode) selCard.parentNode.removeChild(selCard);
    selCard = document.createElement("div");
    selCard.id = "__tp_sel_card";

    var head = document.createElement("div");
    head.className = "tp-card-head";
    var headTitle = document.createElement("span");
    headTitle.textContent = "Bản dịch TPbrowser";
    var closeBtn = document.createElement("button");
    closeBtn.className = "tp-card-close";
    closeBtn.textContent = "✕";
    closeBtn.onclick = function() {
      if (selCard && selCard.parentNode) selCard.parentNode.removeChild(selCard);
      selCard = null;
    };
    head.appendChild(headTitle);
    head.appendChild(closeBtn);

    var body = document.createElement("div");
    body.className = "tp-card-body";
    body.textContent = "Đang dịch...";

    var copyBtn = document.createElement("button");
    copyBtn.className = "tp-card-copy";
    copyBtn.textContent = "Sao chép";
    copyBtn.style.display = "none";
    copyBtn.onclick = function() {
      navigator.clipboard.writeText(body.textContent).then(function() {
        copyBtn.textContent = "Đã chép ✓";
        setTimeout(function() { copyBtn.textContent = "Sao chép"; }, 1500);
      }).catch(function() {});
    };

    selCard.appendChild(head);
    selCard.appendChild(body);
    selCard.appendChild(copyBtn);

    var top = (anchorRect ? anchorRect.bottom : 100) + 8;
    var left = anchorRect ? anchorRect.left : 100;
    if (top + 200 > window.innerHeight) top = (anchorRect ? anchorRect.top : 200) - 210;
    if (left + 300 > window.innerWidth) left = window.innerWidth - 310;
    selCard.style.top = Math.max(10, top) + "px";
    selCard.style.left = Math.max(10, left) + "px";
    (document.body || document.documentElement).appendChild(selCard);

    var reqId = ++__tpTranslateCounter;
    __tpTranslatePending[reqId] = function(translations) {
      if (translations && translations[0]) {
        body.textContent = translations[0];
        copyBtn.style.display = "inline-flex";
      } else {
        body.textContent = text;
      }
    };
    sendRust({
      cmd: "translate_req",
      req_id: reqId,
      target_lang: window.__tpTranslateTargetLang || "vi",
      texts: [text]
    });
  }

  function translateFullPage() {
    if (__tpIsPageTranslated) return;
    ensureTranslateStyles();

    var walker = document.createTreeWalker(
      document.body,
      NodeFilter.SHOW_TEXT,
      {
        acceptNode: function(node) {
          if (!node || !node.parentElement) return NodeFilter.FILTER_REJECT;
          var tag = node.parentElement.tagName.toLowerCase();
          if (tag === "script" || tag === "style" || tag === "noscript" || tag === "code" || tag === "pre" || tag === "textarea") {
            return NodeFilter.FILTER_REJECT;
          }
          if (node.parentElement.closest && (node.parentElement.closest("#__tp_sel_card, #__tp_sel_btn, #__tp_page_banner, #__tp_bubble_root, #__tp_link_pill") || node.parentElement.isContentEditable)) {
            return NodeFilter.FILTER_REJECT;
          }
          var val = node.nodeValue ? node.nodeValue.trim() : "";
          if (val.length < 2 || /^[\d\s.,:;!?%&*()_\-+=/\\|<>[\]{}#@$~^`"']+$/.test(val)) {
            return NodeFilter.FILTER_REJECT;
          }
          return NodeFilter.FILTER_ACCEPT;
        }
      }
    );

    var nodes = [];
    var n;
    while ((n = walker.nextNode())) {
      nodes.push(n);
    }

    if (nodes.length === 0) return;

    __tpTranslatedNodes = nodes;
    for (var i = 0; i < nodes.length; i++) {
      if (!__tpOriginalTexts.has(nodes[i])) {
        __tpOriginalTexts.set(nodes[i], nodes[i].nodeValue);
      }
    }

    var batchSize = 35;
    var batches = [];
    for (var i = 0; i < nodes.length; i += batchSize) {
      batches.push(nodes.slice(i, i + batchSize));
    }

    batches.forEach(function(batch) {
      var reqId = ++__tpTranslateCounter;
      var rawTexts = batch.map(function(node) { return node.nodeValue; });
      __tpTranslatePending[reqId] = function(translations) {
        if (!Array.isArray(translations)) return;
        for (var b = 0; b < batch.length; b++) {
          if (typeof translations[b] === "string" && translations[b].trim()) {
            batch[b].nodeValue = translations[b];
          }
        }
      };
      sendRust({
        cmd: "translate_req",
        req_id: reqId,
        target_lang: window.__tpTranslateTargetLang || "vi",
        texts: rawTexts
      });
    });

    __tpIsPageTranslated = true;
    updatePageTranslateBanner();
  }

  function restoreOriginalPage() {
    if (!__tpIsPageTranslated) return;
    for (var i = 0; i < __tpTranslatedNodes.length; i++) {
      var node = __tpTranslatedNodes[i];
      if (__tpOriginalTexts.has(node)) {
        node.nodeValue = __tpOriginalTexts.get(node);
      }
    }
    __tpIsPageTranslated = false;
    updatePageTranslateBanner();
  }

  var pageBannerEl = null;
  function updatePageTranslateBanner() {
    if (!pageBannerEl) return;
    while (pageBannerEl.firstChild) {
      pageBannerEl.removeChild(pageBannerEl.firstChild);
    }
    var msg = document.createElement("span");
    if (__tpIsPageTranslated) {
      msg.textContent = "Đã dịch sang Tiếng Việt";
      var revertBtn = document.createElement("button");
      revertBtn.className = "tp-btn tp-btn-ghost";
      revertBtn.textContent = "Xem bản gốc";
      revertBtn.onclick = restoreOriginalPage;
      pageBannerEl.appendChild(msg);
      pageBannerEl.appendChild(revertBtn);
    } else {
      msg.textContent = "Dịch trang này sang Tiếng Việt?";
      var transBtn = document.createElement("button");
      transBtn.className = "tp-btn tp-btn-primary";
      transBtn.textContent = "Dịch toàn trang";
      transBtn.onclick = translateFullPage;
      pageBannerEl.appendChild(msg);
      pageBannerEl.appendChild(transBtn);
    }
    var closeBtn = document.createElement("button");
    closeBtn.className = "tp-close";
    closeBtn.textContent = "✕";
    closeBtn.onclick = function() {
      if (pageBannerEl && pageBannerEl.parentNode) pageBannerEl.parentNode.removeChild(pageBannerEl);
      pageBannerEl = null;
    };
    pageBannerEl.appendChild(closeBtn);
  }

  function checkAndPromptPageTranslation() {
    try {
      if (!window.__tpTranslateAutoPrompt) return;
      var host = (window.location.hostname || "").toLowerCase();
      var never = (window.__tpTranslateNeverDomains || "").toLowerCase().split(",");
      for (var i = 0; i < never.length; i++) {
        var d = never[i].trim();
        if (d && host.indexOf(d) !== -1) return;
      }
      var lang = (document.documentElement.lang || "").toLowerCase();
      var isVietnamese = lang.indexOf("vi") === 0;
      if (isVietnamese) return;

      setTimeout(function() {
        if (pageBannerEl) return;
        ensureTranslateStyles();
        pageBannerEl = document.createElement("div");
        pageBannerEl.id = "__tp_page_banner";
        updatePageTranslateBanner();
        (document.body || document.documentElement).appendChild(pageBannerEl);
      }, 1500);
    } catch (_) {}
  }

  if (!IS_INTERNAL_HTML_PAGE) {
    if (document.readyState === "complete" || document.readyState === "interactive") {
      checkAndPromptPageTranslation();
    } else {
      document.addEventListener("DOMContentLoaded", checkAndPromptPageTranslation);
    }
  }

  // ===== Zen Web Context Menu (Trusted Types safe, Shift+RightClick / Input / Img -> Native Edge Menu) =====
  var webCtxStyleMounted = false;
  var webCtxMenuEl = null;

  function cleanWebTrackingUrl(raw) {
    var str = String(raw || "").trim();
    if (!/^https?:\/\//i.test(str)) return str;
    try {
      var u = new URL(str);
      var strip = ["fbclid","gclid","dclid","msclkid","mc_cid","mc_eid","si","igsh","ref_src","_hsenc","_hsmi"];
      var toDel = [];
      u.searchParams.forEach(function (_, k) {
        var kl = k.toLowerCase();
        if (kl.indexOf("utm_") === 0 || strip.indexOf(kl) !== -1) toDel.push(k);
      });
      toDel.forEach(function (k) { u.searchParams.delete(k); });
      return u.toString();
    } catch (_) {
      return str;
    }
  }

  function closeWebContextMenu() {
    if (webCtxMenuEl && webCtxMenuEl.parentNode) {
      webCtxMenuEl.parentNode.removeChild(webCtxMenuEl);
    }
    webCtxMenuEl = null;
  }

  function shouldBypassZenWebContextMenu(e) {
    var prefs = (window.__tpContextMenuPrefs && typeof window.__tpContextMenuPrefs === "object")
      ? window.__tpContextMenuPrefs
      : {};
    if (prefs.web_menu_enabled === false) return true;
    if (e.shiftKey) return true;
    var target = e.target;
    if (!target || !target.tagName) return false;
    var tag = target.tagName.toLowerCase();
    if (tag === "textarea" || tag === "select") {
      return true;
    }
    if (tag === "input") {
      var inputType = String(target.getAttribute("type") || "text").toLowerCase();
      if (inputType !== "button" && inputType !== "submit" && inputType !== "reset" && inputType !== "checkbox" && inputType !== "radio") {
        return true;
      }
    }
    if (target.isContentEditable) return true;
    if (target.closest && target.closest("input:not([type='button']):not([type='submit']):not([type='reset']):not([type='checkbox']):not([type='radio']), textarea, select, [contenteditable='true'], [contenteditable=''], #__tp_bubble_root")) {
      return true;
    }
    return false;
  }

  function ensureWebCtxStyles() {
    if (webCtxStyleMounted || !document.head) return;
    webCtxStyleMounted = true;
    var css = [
      "#__tp_web_ctx_menu { position: fixed !important; z-index: 2147483647 !important; min-width: 204px !important; max-width: 268px !important; max-height: 78vh !important; overflow-y: auto !important; padding: 6px !important; border-radius: 12px !important; border: 1px solid rgba(226, 190, 104, 0.28) !important; background: #141720 !important; color: #F5F2EB !important; box-shadow: 0 14px 36px rgba(0, 0, 0, 0.56) !important; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Inter, sans-serif !important; display: flex !important; flex-direction: column !important; gap: 2px !important; user-select: none !important; }",
      "#__tp_web_ctx_menu[data-theme='carrara-light'] { background: #FAF6EE !important; color: #18130C !important; border-color: rgba(122, 81, 11, 0.26) !important; box-shadow: 0 12px 30px rgba(48, 34, 12, 0.18) !important; }",
      "#__tp_web_ctx_menu .tp-wctx-nav { display: flex !important; gap: 4px !important; padding: 2px 2px 4px !important; }",
      "#__tp_web_ctx_menu .tp-wctx-nav-btn { flex: 1 !important; border: 1px solid rgba(255,255,255,0.1) !important; border-radius: 8px !important; background: rgba(255,255,255,0.05) !important; color: inherit !important; padding: 5px 0 !important; font-size: 12px !important; font-weight: 600 !important; cursor: pointer !important; text-align: center !important; }",
      "#__tp_web_ctx_menu[data-theme='carrara-light'] .tp-wctx-nav-btn { border-color: rgba(24,24,27,0.12) !important; background: rgba(24,24,27,0.04) !important; }",
      "#__tp_web_ctx_menu .tp-wctx-btn { border: 0 !important; border-radius: 8px !important; background: transparent !important; color: inherit !important; padding: 7px 11px !important; font-size: 12px !important; font-weight: 500 !important; text-align: left !important; cursor: pointer !important; white-space: nowrap !important; overflow: hidden !important; text-overflow: ellipsis !important; width: 100% !important; }",
      "#__tp_web_ctx_menu .tp-wctx-btn:hover, #__tp_web_ctx_menu .tp-wctx-nav-btn:hover { background: rgba(226, 190, 104, 0.14) !important; color: #E2BE68 !important; }",
      "#__tp_web_ctx_menu[data-theme='carrara-light'] .tp-wctx-btn:hover, #__tp_web_ctx_menu[data-theme='carrara-light'] .tp-wctx-nav-btn:hover { background: rgba(122, 81, 11, 0.12) !important; color: #7A510B !important; }",
      "#__tp_web_ctx_menu .tp-wctx-sep { height: 1px !important; background: rgba(255,255,255,0.1) !important; margin: 3px 6px !important; }",
      "#__tp_web_ctx_menu[data-theme='carrara-light'] .tp-wctx-sep { background: rgba(24,24,27,0.1) !important; }",
      "#__tp_web_ctx_menu .tp-wctx-hint { font-size: 10px !important; opacity: 0.56 !important; padding: 3px 10px 2px !important; text-align: center !important; }"
    ].join("\n");
    document.head.appendChild(domEl("style", { text: css }));
  }

  function detectContextTargets(e) {
    var linkEl = e.target && e.target.closest ? e.target.closest("a[href]") : null;
    var imgEl = e.target && e.target.tagName && e.target.tagName.toLowerCase() === "img" ? e.target : null;
    var videoEl = e.target && e.target.tagName && e.target.tagName.toLowerCase() === "video" ? e.target : null;
    if (typeof document.elementsFromPoint === "function") {
      try {
        var stack = document.elementsFromPoint(e.clientX, e.clientY) || [];
        for (var i = 0; i < stack.length; i++) {
          var node = stack[i];
          if (!linkEl && node.closest) linkEl = node.closest("a[href]");
          if (!imgEl && node.tagName && node.tagName.toLowerCase() === "img") imgEl = node;
          if (!videoEl && node.tagName && node.tagName.toLowerCase() === "video") videoEl = node;
        }
      } catch (_) {}
    }
    var rawLinkHref = "";
    if (linkEl) {
      try { rawLinkHref = new URL(linkEl.getAttribute("href") || "", window.location.href).href; } catch (_) {}
      if (!/^https?:\/\//i.test(rawLinkHref)) rawLinkHref = "";
    }
    var imgSrc = "";
    if (imgEl && imgEl.currentSrc) imgSrc = imgEl.currentSrc;
    else if (imgEl && imgEl.getAttribute) imgSrc = imgEl.getAttribute("src") || "";
    if (imgSrc && !/^https?:\/\//i.test(imgSrc)) {
      try { imgSrc = new URL(imgSrc, window.location.href).href; } catch (_) {}
    }
    if (!/^https?:\/\//i.test(imgSrc)) imgSrc = "";
    return { linkEl: linkEl, rawLinkHref: rawLinkHref, imgSrc: imgSrc, videoEl: videoEl };
  }

  function openZenWebContextMenu(e) {
    closeWebContextMenu();
    ensureWebCtxStyles();
    var prefs = (window.__tpContextMenuPrefs && typeof window.__tpContextMenuPrefs === "object")
      ? window.__tpContextMenuPrefs
      : {};
    var hideSep = prefs.hide_separators !== false;
    var cleanCopy = prefs.clean_copy_link !== false;
    var theme = (lastBubbleState && lastBubbleState.ui_theme)
      || document.documentElement.getAttribute("data-theme")
      || "obsidian-gold";

    var ctx = detectContextTargets(e);
    var linkEl = ctx.linkEl;
    var rawLinkHref = ctx.rawLinkHref;
    var imgSrc = ctx.imgSrc;
    var videoEl = ctx.videoEl;
    var sel = window.getSelection ? window.getSelection() : null;
    var selText = (sel ? sel.toString() : "").trim();

    var menu = domEl("div", { id: "__tp_web_ctx_menu", role: "menu", "data-theme": theme });

    function addBtn(label, onClick) {
      var b = domEl("button", { type: "button", className: "tp-wctx-btn", role: "menuitem", text: label });
      b.addEventListener("click", function (ev) {
        ev.stopPropagation();
        closeWebContextMenu();
        onClick();
      });
      menu.appendChild(b);
    }
    function addSep() {
      if (hideSep) return;
      menu.appendChild(domEl("div", { className: "tp-wctx-sep" }));
    }

    if (!IS_INTERNAL_HTML_PAGE) {
      var navRow = domEl("div", { className: "tp-wctx-nav" });
      var bBack = domEl("button", { type: "button", className: "tp-wctx-nav-btn", title: "Quay lại", text: "←" });
      bBack.addEventListener("click", function () { closeWebContextMenu(); window.history.back(); });
      var bFwd = domEl("button", { type: "button", className: "tp-wctx-nav-btn", title: "Đi tới", text: "→" });
      bFwd.addEventListener("click", function () { closeWebContextMenu(); window.history.forward(); });
      var bReload = domEl("button", { type: "button", className: "tp-wctx-nav-btn", title: "Tải lại", text: "↻" });
      bReload.addEventListener("click", function () { closeWebContextMenu(); sendRust({ cmd: "reload" }); });
      navRow.appendChild(bBack);
      navRow.appendChild(bFwd);
      navRow.appendChild(bReload);
      menu.appendChild(navRow);
    }

    if (rawLinkHref) {
      addBtn("Mở link ở Tab mới", function () {
        sendRust({ cmd: "new_tab", url: rawLinkHref, isolated: false });
      });
      addBtn("◫ Mở nhanh Zen Glance", function () {
        sendRust({ cmd: "open_glance", url: rawLinkHref, title: (linkEl && linkEl.textContent ? linkEl.textContent : "").trim().slice(0, 80) || rawLinkHref });
      });
      addBtn(cleanCopy ? "Sao chép link sạch" : "Sao chép link", function () {
        sendRust({ cmd: "copy_to_clipboard", text: cleanCopy ? cleanWebTrackingUrl(rawLinkHref) : rawLinkHref });
      });
      addSep();
    }

    if (imgSrc) {
      addBtn("Mở ảnh ở Tab mới", function () {
        sendRust({ cmd: "new_tab", url: imgSrc, isolated: false });
      });
      addBtn("Sao chép URL ảnh", function () {
        sendRust({ cmd: "copy_to_clipboard", text: imgSrc });
      });
      addSep();
    }

    if (videoEl) {
      if (document.pictureInPictureEnabled && typeof videoEl.requestPictureInPicture === "function") {
        addBtn("◰ Hình trong hình (PiP)", function () {
          if (document.pictureInPictureElement) {
            document.exitPictureInPicture().catch(function () {});
          } else {
            videoEl.requestPictureInPicture().catch(function () {});
          }
        });
      }
      addBtn(videoEl.muted ? "🔊 Bật tiếng video" : "🔇 Tắt tiếng video", function () {
        try { videoEl.muted = !videoEl.muted; } catch (_) {}
      });
      addSep();
    }

    if (selText.length >= 2) {
      addBtn("Sao chép đoạn chọn", function () {
        sendRust({ cmd: "copy_to_clipboard", text: selText });
      });
      addBtn("✦ Hỏi AI đoạn chọn", function () {
        sendRust({ cmd: "open_chat_channel", channel: "ai" });
        sendRust({ cmd: "run_ai_prompt", prompt: "Giải thích hoặc tóm tắt ngắn gọn đoạn sau:\n\n" + selText.slice(0, 1200) });
      });
      addBtn("Tìm Web: \"" + (selText.length > 18 ? selText.slice(0, 18) + "…" : selText) + "\"", function () {
        sendRust({ cmd: "new_tab", url: "https://www.google.com/search?q=" + encodeURIComponent(selText.slice(0, 200)), isolated: false });
      });
      addSep();
    }

    if (!IS_INTERNAL_HTML_PAGE) {
      addBtn(cleanCopy ? "Sao chép URL trang (Sạch)" : "Sao chép URL trang", function () {
        var href = window.location.href || "";
        sendRust({ cmd: "copy_to_clipboard", text: cleanCopy ? cleanWebTrackingUrl(href) : href });
      });
      addBtn("⚡ Zap ẩn phần tử trên trang", function () {
        if (typeof window.__tpStartZapMode === "function") window.__tpStartZapMode();
      });
    }
    addBtn("Kiểm tra phần tử (DevTools)", function () {
      sendRust({ cmd: "open_devtools" });
    });
    menu.appendChild(domEl("div", { className: "tp-wctx-hint", text: "Shift + Chuột phải: Menu gốc" }));

    (document.body || document.documentElement).appendChild(menu);
    webCtxMenuEl = menu;
    var mw = menu.offsetWidth || 220;
    var mh = menu.offsetHeight || 220;
    var left = Math.max(8, Math.min(e.clientX, window.innerWidth - mw - 8));
    var top = Math.max(8, Math.min(e.clientY, window.innerHeight - mh - 8));
    menu.style.setProperty("left", left + "px", "important");
    menu.style.setProperty("top", top + "px", "important");
  }

  window.addEventListener("contextmenu", function (e) {
    if (shouldBypassZenWebContextMenu(e)) {
      closeWebContextMenu();
      return;
    }
    e.preventDefault();
    e.stopImmediatePropagation();
    openZenWebContextMenu(e);
  }, true);
  document.addEventListener("mousedown", function (e) {
    if (webCtxMenuEl && !webCtxMenuEl.contains(e.target)) closeWebContextMenu();
  }, true);
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape") closeWebContextMenu();
  }, true);
  window.addEventListener("keyup", function (e) {
    if (e.key === "Control") {
      sendRust({ cmd: "commit_mru_tab" });
    }
  }, true);
  window.addEventListener("blur", closeWebContextMenu);
  window.addEventListener("scroll", closeWebContextMenu, true);
})();
