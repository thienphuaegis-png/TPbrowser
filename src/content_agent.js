// Script tiêm vào mỗi Tab web của TPbrowser:
// 1. Chuyển tiếp phím tắt điều khiển trình duyệt (Alt+T, Alt+A, Alt+N, Alt+P, Alt+W, Alt+M).
// 2. Hiển thị Bong bóng Chat AI Nổi (Floating Bubble / Mini-Pill) khi cửa sổ AI thu gọn — vẫn hoạt động ngầm & có nút [🔴 Làm mẫu / ⏹ Lưu Skill].
// 3. Chế độ "🔴 Làm mẫu 1 lần (Record)": Ghi nhận thao tác Click / Điền chữ thật của anh Phú trên trang web (kèm nhãn ngữ nghĩa & selector bền vững) để lưu thành Skill.
// 4. Thực thi thao tác Cowork (hỗ trợ cả thẻ contenteditable của Zalo/Messenger/Docs và tìm nút theo Nhãn ngữ nghĩa khi trang đổi giao diện).
(function () {
  var TAB_ID = __TAB_ID__;
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
      var sig = t + "|" + f;
      if (sig === lastMetaSig) return;
      lastMetaSig = sig;
      sendRust({ cmd: "tab_meta", title: t, favicon: f });
    } catch (_) {}
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", function () {
      reportTabMeta();
      setTimeout(reportTabMeta, 500);
    });
  } else {
    reportTabMeta();
  }
  window.addEventListener("load", function () {
    reportTabMeta();
    setTimeout(reportTabMeta, 700);
  });
  setInterval(reportTabMeta, 2500);

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
          fbBar.style.display = "none";
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
          sendRust({ cmd: e.shiftKey ? "prev_tab" : "next_tab" });
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
        } else if (code === "KeyD" && !e.shiftKey) {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "bookmark_active" });
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
          sendRust({ cmd: "focus_index", index: idx });
          return;
        } else if (!e.shiftKey && (code === "Digit9" || code === "Numpad9")) {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "focus_last_tab" });
          return;
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
    "#__tp_find_bar { position: fixed !important; top: 14px !important; right: 20px !important; z-index: 2147483647 !important; display: none; align-items: center; gap: 6px; background: rgba(17, 19, 24, 0.96) !important; color: #F5F2EB !important; border: 1.5px solid #E2BE68 !important; border-radius: 999px !important; padding: 5px 10px 5px 14px !important; box-shadow: 0 12px 32px rgba(0, 0, 0, 0.55) !important; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Inter, sans-serif !important; }",
    "#__tp_find_input { background: transparent !important; border: none !important; outline: none !important; color: #F5F2EB !important; font-size: 12.5px !important; width: 190px !important; user-select: text !important; margin: 0 !important; padding: 2px 0 !important; }",
    "#__tp_bubble_root .tp-find-btn { background: rgba(255,255,255,0.08) !important; border: 1px solid rgba(226,190,104,0.3) !important; color: #F5F2EB !important; border-radius: 999px !important; padding: 3px 8px !important; font-size: 11px !important; font-weight: 700 !important; cursor: pointer !important; line-height: 1.2 !important; }",
    "#__tp_bubble_root .tp-find-btn:hover { background: rgba(226,190,104,0.25) !important; color: #E2BE68 !important; }",
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
    "#__tp_col_list { overflow-y: auto !important; padding: 10px 14px 14px 14px !important; display: flex !important; flex-direction: column !important; gap: 6px !important; }",
    "#__tp_bubble_root .tp-col-row { display: flex !important; align-items: center !important; justify-content: space-between !important; gap: 10px !important; padding: 8px 12px !important; border: 1px solid rgba(212, 175, 55, 0.22) !important; border-radius: 10px !important; cursor: pointer !important; background: rgba(255, 255, 255, 0.03) !important; }",
    "#__tp_bubble_root .tp-col-row:hover { border-color: #E2BE68 !important; background: rgba(212, 175, 55, 0.12) !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] .tp-col-row { background: #FFFFFF !important; border-color: rgba(138, 94, 16, 0.22) !important; }",
    "#__tp_bubble_root[data-theme='carrara-light'] .tp-col-row:hover { border-color: #8A5E10 !important; background: rgba(154, 107, 22, 0.08) !important; }",
    "#__tp_toast { position: fixed !important; top: 18px !important; left: 50% !important; transform: translateX(-50%) !important; z-index: 2147483647 !important; display: none; background: rgba(17, 19, 24, 0.96) !important; color: #E2BE68 !important; border: 1.5px solid #E2BE68 !important; border-radius: 999px !important; padding: 7px 16px !important; font-size: 12px !important; font-weight: 700 !important; box-shadow: 0 10px 28px rgba(0, 0, 0, 0.55) !important; pointer-events: none !important; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Inter, sans-serif !important; }"
  ].join("\n");

  function ensureSpaResilienceObserver() {
    if (tpObserverBound || !document.documentElement) return;
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

      var findInp = domEl("input", { id: "__tp_find_input", type: "text", placeholder: "Tìm chữ trong trang..." });
      var findPrev = domEl("button", { type: "button", className: "tp-find-btn", id: "__tp_find_prev", title: "Trước (Shift+F3)", text: "▲" });
      var findNext = domEl("button", { type: "button", className: "tp-find-btn", id: "__tp_find_next", title: "Tiếp (F3)", text: "▼" });
      var findClose = domEl("button", { type: "button", className: "tp-find-btn", id: "__tp_find_close", title: "Đóng (Esc)", text: "✕" });
      var findBar = domEl("div", { id: "__tp_find_bar" }, [findInp, findPrev, findNext, findClose]);

      var colTitle = domEl("span", { id: "__tp_col_title", text: "COLLECTIONS & BOOKMARKS (Alt+B)" });
      var colPinBtn = domEl("button", { type: "button", className: "tp-find-btn", text: "★ Ghim trang này" });
      var colCloseBtn = domEl("button", { type: "button", className: "tp-find-btn", text: "✕" });
      var colHeadActions = domEl("div", { style: "display:flex !important; gap:6px !important; align-items:center !important;" }, [colPinBtn, colCloseBtn]);
      var colHead = domEl("div", { id: "__tp_col_head" }, [colTitle, colHeadActions]);
      var colSearch = domEl("input", { id: "__tp_col_search", type: "text", placeholder: "Lọc trang đã lưu theo tên hoặc URL..." });
      var colSearchWrap = domEl("div", { id: "__tp_col_search_wrap" }, [colSearch]);
      var colList = domEl("div", { id: "__tp_col_list" });
      var colCard = domEl("div", { id: "__tp_collections_card" }, [colHead, colSearchWrap, colList]);
      var colModal = domEl("div", { id: "__tp_collections_modal" }, [colCard]);

      var toastEl = domEl("div", { id: "__tp_toast" });

      var orbsHost = domEl("div", { id: "__tp_web_orbs" });
      var pillDot = domEl("div", { className: "tp-pill-dot" });
      var statusSpan = domEl("span", { className: "tp-pill-text", id: "__tp_pill_status", text: "🤖 AI Cowork" });
      var stopRecBtn = domEl("button", { type: "button", className: "tp-pill-dock-btn stop-rec", id: "__tp_pill_stop_rec", style: "display:none;", text: "⏹ Lưu Skill" });
      var dockBtn = domEl("button", { type: "button", className: "tp-pill-dock-btn", id: "__tp_pill_dock_btn", text: "Mở Chat" });
      var pill = domEl("div", { id: "__tp_bubble_pill", title: "Trạng thái AI Cowork / Ghi mẫu Skill" }, [pillDot, statusSpan, stopRecBtn, dockBtn]);
      var bubbleWrap = domEl("div", { id: "__tp_bubble_wrap" }, [orbsHost, pill]);

      var root = domEl("div", { id: "__tp_bubble_root" }, [styleEl, activeRing, findBar, colModal, toastEl, bubbleWrap]);
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

      function doFind(backwards) {
        var q = (findInp.value || "").trim();
        if (!q) return;
        try { window.find(q, false, !!backwards, true, false, false, false); } catch (_) {}
      }
      window.__tpFindStep = doFind;
      findNext.addEventListener("click", function () { doFind(false); });
      findPrev.addEventListener("click", function () { doFind(true); });
      findClose.addEventListener("click", function () { findBar.style.display = "none"; });
      findInp.addEventListener("keydown", function (ev) {
        if (ev.key === "Enter") { ev.preventDefault(); doFind(ev.shiftKey); }
        else if (ev.key === "Escape") { ev.preventDefault(); findBar.style.display = "none"; }
      });

      window.__tpToggleFindBar = function (forceOpen) {
        mountTpBubbleUi();
        var bar = document.getElementById("__tp_find_bar");
        var inp = document.getElementById("__tp_find_input");
        if (!bar || !inp) return;
        var willOpen = forceOpen === true ? true : bar.style.display !== "flex";
        bar.style.display = willOpen ? "flex" : "none";
        if (willOpen) {
          try { bar.animate([{ opacity: 0, transform: "translateY(-8px) scale(0.97)" }, { opacity: 1, transform: "translateY(0) scale(1)" }], { duration: 180, easing: "cubic-bezier(0.22,1,0.36,1)" }); } catch (_) {}
          inp.focus();
          inp.select();
        }
      };

      var cachedBookmarks = [];
      function renderCollectionsRows() {
        while (colList.firstChild) colList.removeChild(colList.firstChild);
        var kw = (colSearch.value || "").toLowerCase().trim();
        var filtered = cachedBookmarks.filter(function (b) {
          if (!kw) return true;
          return String(b.title || "").toLowerCase().indexOf(kw) !== -1 || String(b.url || "").toLowerCase().indexOf(kw) !== -1;
        });
        if (filtered.length === 0) {
          colList.appendChild(domEl("div", {
            style: "padding:18px !important; text-align:center !important; font-size:12px !important; opacity:0.75 !important;",
            text: "Chưa có trang nào khớp trong Bookmarks."
          }));
          return;
        }
        for (var i = 0; i < filtered.length; i++) {
          (function (bm) {
            var info = domEl("div", { style: "min-width:0 !important; flex:1 !important;" }, [
              domEl("div", { style: "font-size:12.5px !important; font-weight:700 !important; white-space:nowrap !important; overflow:hidden !important; text-overflow:ellipsis !important;", text: bm.title || bm.url }),
              domEl("div", { style: "font-size:10.5px !important; opacity:0.7 !important; white-space:nowrap !important; overflow:hidden !important; text-overflow:ellipsis !important;", text: bm.url })
            ]);
            var openTabBtn = domEl("button", { type: "button", className: "tp-find-btn", title: "Mở trong Tab mới", text: "+ Tab" });
            openTabBtn.addEventListener("click", function (ev) {
              ev.stopPropagation();
              colModal.style.display = "none";
              sendRust({ cmd: "new_tab", url: bm.url, isolated: false });
            });
            var row = domEl("div", { className: "tp-col-row" }, [info, openTabBtn]);
            row.addEventListener("click", function () {
              colModal.style.display = "none";
              sendRust({ cmd: "navigate_active", url: bm.url });
            });
            colList.appendChild(row);
          })(filtered[i]);
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

  function runActiveAdCleanerAndSkipper() {
    if (window.__tpAdblockEnabled === false) return;
    try {
      mountAdShieldCss();
      var host = (window.location.hostname || "").toLowerCase();

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

  // --- FLOATING TEXT SELECTION PILL ON ALL WEB PAGES ---
  var webSelPill = null;
  function ensureWebSelPill() {
    if (webSelPill) return webSelPill;
    webSelPill = document.createElement("div");
    webSelPill.id = "__tp_web_sel_pill";
    webSelPill.style.cssText = "position:absolute; display:none; z-index:99999999; background:linear-gradient(135deg, #E2BE68 0%, #B5872E 100%); color:#140F05; font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif; font-size:11px; font-weight:700; padding:5px 12px; border-radius:999px; box-shadow:0 6px 20px rgba(0,0,0,0.5), inset 0 1px 0 rgba(255,255,255,0.4); cursor:pointer; user-select:none; transition:transform 0.14s ease;";
    webSelPill.textContent = "✨ Lập Plan Cowork";
    webSelPill.onmousedown = function(e) { e.preventDefault(); e.stopPropagation(); };
    webSelPill.onclick = function(e) {
      e.preventDefault();
      e.stopPropagation();
      var sel = window.getSelection();
      var text = (sel ? sel.toString() : "").trim();
      if (!text && webSelPill.__pendingText) text = webSelPill.__pendingText;
      if (text) {
        sendIpc({
          cmd: "create_task_from_selection",
          text: text,
          source: "web",
          contact: document.title || window.location.hostname
        });
      }
      hideWebSelPill();
    };
    document.body.appendChild(webSelPill);
    return webSelPill;
  }

  function hideWebSelPill() {
    if (webSelPill) webSelPill.style.display = "none";
  }

  document.addEventListener("selectionchange", function() {
    try {
      var sel = window.getSelection();
      var txt = (sel ? sel.toString() : "").trim();
      if (txt.length < 4) hideWebSelPill();
    } catch (_) {}
  });

  document.addEventListener("mouseup", function(e) {
    try {
      if (e.target && e.target.closest && e.target.closest("#__tp_bubble_root, #__tp_reminder_banner, #__tp_web_sel_pill")) return;
      var sel = window.getSelection();
      var txt = (sel ? sel.toString() : "").trim();
      if (txt.length >= 4) {
        var pill = ensureWebSelPill();
        pill.__pendingText = txt;
        var r = sel.getRangeAt(0).getBoundingClientRect();
        var top = window.scrollY + r.top - 32;
        if (top < window.scrollY + 10) top = window.scrollY + r.bottom + 8;
        var left = Math.max(10, window.scrollX + r.left + (r.width / 2) - 60);
        pill.style.top = top + "px";
        pill.style.left = left + "px";
        pill.style.display = "block";
      } else {
        hideWebSelPill();
      }
    } catch (_) {}
  });

  mountAdShieldCss();
  setInterval(runActiveAdCleanerAndSkipper, 220);
})();
