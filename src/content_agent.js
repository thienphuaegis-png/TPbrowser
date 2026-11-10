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

      if (e.key === "F5") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "reload" });
        return;
      }

      if (!e.altKey || e.ctrlKey) return;
      var code = e.code;

      if (code === "KeyT") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "toggle_tab_orientation" });
      } else if (code === "KeyA") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "toggle_ai_mode" });
      } else if (code === "KeyN") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "new_tab", url: "tp://newtab", isolated: false });
      } else if (code === "KeyP") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "new_tab", url: "tp://newtab", isolated: true });
      } else if (code === "KeyW") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "close_active_tab" });
      } else if (code === "KeyM") {
        e.preventDefault();
        e.stopImmediatePropagation();
        sendRust({ cmd: "cycle_view_mode" });
      } else if (code === "KeyZ") {
        e.preventDefault();
        e.stopImmediatePropagation();
        window.history.back();
      } else if (code === "KeyX") {
        e.preventDefault();
        e.stopImmediatePropagation();
        window.history.forward();
      } else if (code.startsWith("Digit")) {
        var n = parseInt(code.replace("Digit", ""), 10);
        if (n >= 1 && n <= 8) {
          e.preventDefault();
          e.stopImmediatePropagation();
          sendRust({ cmd: "focus_index", index: n - 1 });
        }
      }
    },
    true
  );

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

  // Gắn Bong bóng Chat AI Nổi (Floating Bubble) & Khung Viền Active
  function mountTpBubbleUi() {
    if (!document.documentElement || !document.body) {
      setTimeout(mountTpBubbleUi, 30);
      return;
    }
    if (document.getElementById("__tp_bubble_root")) return;

    var root = document.createElement("div");
    root.id = "__tp_bubble_root";
    root.innerHTML = `
      <style>
        #__tp_active_ring {
          position: fixed;
          inset: 0;
          border: 2px solid transparent;
          pointer-events: none;
          z-index: 2147483640;
          transition: border-color 0.12s;
        }
        #__tp_active_ring.split-active {
          border-color: #10b981;
        }
        #__tp_active_ring.recording-ring {
          border: 3px dashed #ef4444;
        }
        #__tp_bubble_wrap {
          position: fixed;
          bottom: 18px;
          right: 18px;
          z-index: 2147483646;
          font-family: system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
          display: none;
          flex-direction: column;
          align-items: flex-end;
          gap: 8px;
        }
        #__tp_bubble_wrap.visible {
          display: flex;
        }
        #__tp_bubble_popover {
          width: 340px;
          background: #ffffff;
          color: #0f172a;
          border: 2px solid #0f172a;
          box-shadow: 6px 6px 0px rgba(15, 23, 42, 0.25);
          padding: 12px;
          display: none;
          flex-direction: column;
          gap: 8px;
        }
        #__tp_bubble_popover.open {
          display: flex;
        }
        .tp-pop-hdr {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding-bottom: 6px;
          border-bottom: 1px solid #cbd5e1;
          font-size: 11.5px;
          font-weight: 800;
        }
        .tp-pop-reply {
          background: #f8fafc;
          border-left: 3px solid #10b981;
          padding: 8px;
          font-size: 11.5px;
          line-height: 1.45;
          color: #1e293b;
          max-height: 150px;
          overflow-y: auto;
          white-space: pre-wrap;
        }
        .tp-pop-form {
          display: flex;
          gap: 5px;
        }
        .tp-pop-form input {
          flex: 1;
          border: 1.5px solid #0f172a;
          padding: 6px 8px;
          font-size: 11.5px;
          color: #0f172a;
          background: #ffffff;
          outline: none;
        }
        .tp-pop-form button {
          background: #0f172a;
          color: #ffffff;
          border: none;
          padding: 0 10px;
          font-size: 11px;
          font-weight: 800;
          cursor: pointer;
        }
        #__tp_bubble_pill {
          display: flex;
          align-items: center;
          gap: 8px;
          background: #0f172a;
          color: #ffffff;
          border: 2px solid #10b981;
          box-shadow: 4px 4px 0px rgba(15, 23, 42, 0.28);
          padding: 7px 12px;
          cursor: pointer;
          max-width: 430px;
        }
        #__tp_bubble_pill.working {
          border-color: #f59e0b;
          background: #1e293b;
        }
        #__tp_bubble_pill.recording {
          border-color: #ef4444;
          background: #450a0a;
        }
        .tp-pill-dot {
          width: 9px;
          height: 9px;
          background: #10b981;
          flex-shrink: 0;
        }
        #__tp_bubble_pill.working .tp-pill-dot {
          background: #f59e0b;
        }
        #__tp_bubble_pill.recording .tp-pill-dot {
          background: #ef4444;
        }
        .tp-pill-text {
          font-size: 11.5px;
          font-weight: 700;
          white-space: nowrap;
          overflow: hidden;
          text-overflow: ellipsis;
          max-width: 245px;
        }
        .tp-pill-dock-btn {
          background: #10b981;
          color: #04130d;
          border: none;
          padding: 3px 7px;
          font-size: 10.5px;
          font-weight: 800;
          cursor: pointer;
          flex-shrink: 0;
        }
        .tp-pill-dock-btn.stop-rec {
          background: #ef4444;
          color: #ffffff;
        }
      </style>
      <div id="__tp_active_ring"></div>
      <div id="__tp_bubble_wrap">
        <div id="__tp_bubble_popover">
          <div class="tp-pop-hdr">
            <span>💬 BONG BÓNG AI & HỌC SKILL</span>
            <div style="display:flex; gap:4px;">
              <button type="button" style="background:#fef2f2;border:1px solid #ef4444;color:#b91c1c;padding:2px 6px;font-size:10px;font-weight:700;cursor:pointer;" id="__tp_btn_pop_record">🔴 Làm mẫu</button>
              <button type="button" style="background:#ecfdf5;border:1px solid #10b981;color:#065f46;padding:2px 6px;font-size:10px;font-weight:700;cursor:pointer;" id="__tp_btn_expand_dock">⇥ Ngăn Phải</button>
              <button type="button" style="background:#f1f5f9;border:1px solid #cbd5e1;color:#0f172a;padding:2px 6px;font-size:10px;cursor:pointer;" id="__tp_btn_close_pop">×</button>
            </div>
          </div>
          <div class="tp-pop-reply" id="__tp_pop_reply_box">Giao việc hoặc dạy AI: "Lần sau vào trang này làm X rồi lưu thành skill nhé", hoặc bấm [🔴 Làm mẫu] để tự thao tác 1 lần cho AI học!</div>
          <form class="tp-pop-form" id="__tp_pop_form">
            <input id="__tp_pop_inp" type="text" placeholder="Giao việc hoặc dạy Skill cho AI..." autocomplete="off" />
            <button type="submit">LÀM</button>
          </form>
        </div>

        <div id="__tp_bubble_pill" title="Bấm để mở/đóng hộp chat Bong bóng AI">
          <div class="tp-pill-dot"></div>
          <span class="tp-pill-text" id="__tp_pill_status">🤖 AI Cowork (Bong bóng)</span>
          <button type="button" class="tp-pill-dock-btn stop-rec" id="__tp_pill_stop_rec" style="display:none;">⏹ Lưu Skill</button>
          <button type="button" class="tp-pill-dock-btn" id="__tp_pill_dock_btn" title="Mở lại Ngăn AI bên tay phải">Ngăn Phải ⇥</button>
        </div>
      </div>
    `;
    document.body.appendChild(root);

    var pill = document.getElementById("__tp_bubble_pill");
    var pop = document.getElementById("__tp_bubble_popover");
    var dockBtn = document.getElementById("__tp_pill_dock_btn");
    var stopRecBtn = document.getElementById("__tp_pill_stop_rec");
    var popRecBtn = document.getElementById("__tp_btn_pop_record");
    var expandDockBtn = document.getElementById("__tp_btn_expand_dock");
    var closePopBtn = document.getElementById("__tp_btn_close_pop");
    var popForm = document.getElementById("__tp_pop_form");
    var popInp = document.getElementById("__tp_pop_inp");

    pill.addEventListener("click", function (ev) {
      if (ev.target === dockBtn || ev.target === stopRecBtn) return;
      pop.classList.toggle("open");
      if (pop.classList.contains("open")) {
        setTimeout(function () { popInp.focus(); }, 30);
      }
    });

    dockBtn.addEventListener("click", function (ev) {
      ev.stopPropagation();
      sendRust({ cmd: "set_ai_mode", mode: "dock" });
    });

    stopRecBtn.addEventListener("click", function (ev) {
      ev.stopPropagation();
      sendRust({ cmd: "stop_skill_recording" });
    });

    popRecBtn.addEventListener("click", function () {
      pop.classList.remove("open");
      sendRust({ cmd: "start_skill_recording" });
    });

    expandDockBtn.addEventListener("click", function () {
      sendRust({ cmd: "set_ai_mode", mode: "dock" });
    });

    closePopBtn.addEventListener("click", function () {
      pop.classList.remove("open");
    });

    popForm.addEventListener("submit", function (ev) {
      ev.preventDefault();
      var txt = popInp.value.trim();
      if (!txt) return;
      popInp.value = "";
      pop.classList.remove("open");
      sendRust({ cmd: "run_ai_prompt", prompt: txt });
    });
  }

  mountTpBubbleUi();
  setInterval(tryDetectGoogleAccount, 2500);

  window.__tpUpdateBubbleUi = function (state) {
    mountTpBubbleUi();
    var wrap = document.getElementById("__tp_bubble_wrap");
    var pill = document.getElementById("__tp_bubble_pill");
    var statusSpan = document.getElementById("__tp_pill_status");
    var replyBox = document.getElementById("__tp_pop_reply_box");
    var ring = document.getElementById("__tp_active_ring");
    var stopRecBtn = document.getElementById("__tp_pill_stop_rec");

    window.__tpRecordingSkill = !!state.recording;

    if (ring) {
      ring.classList.toggle("recording-ring", !!state.recording);
      ring.classList.toggle("split-active", !state.recording && !!state.show_split_ring);
    }

    if (!wrap) return;
    if (state.ai_mode === "bubble" || state.recording) {
      wrap.classList.add("visible");
    } else {
      wrap.classList.remove("visible");
    }

    pill.classList.toggle("recording", !!state.recording);
    pill.classList.toggle("working", !state.recording && !!state.working);

    if (stopRecBtn) {
      stopRecBtn.style.display = state.recording ? "inline-block" : "none";
    }

    if (state.working || state.recording) {
      var pop = document.getElementById("__tp_bubble_popover");
      if (pop) pop.classList.remove("open");
    }

    if (state.status_text) {
      statusSpan.textContent = state.status_text;
    } else {
      statusSpan.textContent = state.recording
        ? "🔴 Đang ghi mẫu Skill..."
        : state.working
          ? "⚡ AI đang làm việc..."
          : "🤖 AI Cowork (Bong bóng)";
    }

    if (state.last_reply && replyBox) {
      replyBox.textContent = state.last_reply;
    }
  };

  window.__tpCollectDomForAi = function () {
    var elements = [];
    var nodes = document.querySelectorAll(
      "input, textarea, button, select, a[href], [role='button'], [contenteditable='true']"
    );
    var nextId = 1;
    for (var i = 0; i < nodes.length && elements.length < 50; i++) {
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

    sendRust({
      cmd: "dom_context_result",
      title: document.title || "",
      url: window.location.href || "",
      visible_text: bodyText.slice(0, 9000),
      elements_json: JSON.stringify(elements)
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

  function runActiveAdCleanerAndSkipper() {
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

  mountAdShieldCss();
  setInterval(runActiveAdCleanerAndSkipper, 220);
})();
