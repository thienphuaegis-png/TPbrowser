#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod json_mini;
mod layout;
mod mcp_server;

use std::{
    collections::HashMap,
    env, fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, OnceLock,
    },
};

use json_mini::{parse_json, JsonVal};
use mcp_server::McpBridgeMsg;

use layout::{
    compute_content_panes, compute_fb_chat_popup_rect_with_hdr, compute_shell_layout,
    AiDisplayMode, ContentViewMode, TabOrientation, DEFAULT_LEFT_SIDEBAR_WIDTH,
    FB_CHAT_HEADER_HEIGHT, RIGHT_AI_DOCK_WIDTH,
};
#[cfg(target_os = "windows")]
use tao::platform::windows::{WindowBuilderExtWindows, WindowExtWindows};
use tao::{
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    keyboard::{KeyCode, ModifiersState},
    window::{Icon, Window, WindowBuilder},
};
use wry::{
    MemoryUsageLevel, NewWindowResponse, PageLoadEvent, PermissionKind, PermissionResponse, Rect,
    Theme, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows,
};

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Clone, Copy)]
struct WinPoint {
    x: i32,
    y: i32,
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Clone, Copy)]
#[allow(dead_code)]
struct WinRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct FLASHWINFO {
    cb_size: u32,
    hwnd: isize,
    dw_flags: u32,
    u_count: u32,
    dw_timeout: u32,
}
const FLASHW_ALL: u32 = 0x00000003;
const FLASHW_TIMERNOFG: u32 = 0x0000000C;

#[cfg(target_os = "windows")]
#[link(name = "user32")]
#[allow(dead_code)]
extern "system" {
    fn SetWindowPos(
        hwnd: isize,
        hwnd_insert_after: isize,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> i32;
    fn GetCursorPos(lp_point: *mut WinPoint) -> i32;
    fn GetClientRect(hwnd: isize, lp_rect: *mut WinRect) -> i32;
    fn ClientToScreen(hwnd: isize, lp_point: *mut WinPoint) -> i32;
    fn WindowFromPoint(point: WinPoint) -> isize;
    fn GetAncestor(hwnd: isize, ga_flags: u32) -> isize;
    fn GetAsyncKeyState(v_key: i32) -> i16;
    fn IsIconic(hwnd: isize) -> i32;
    fn IsWindow(hwnd: isize) -> i32;
    fn GetDpiForWindow(hwnd: isize) -> u32;
    fn FlashWindowEx(pfwi: *const FLASHWINFO) -> i32;
    fn CreateIconFromResourceEx(
        presbits: *const u8,
        dw_res_size: u32,
        f_icon: i32,
        dw_ver: u32,
        cx_desired: i32,
        cy_desired: i32,
        flags: u32,
    ) -> isize;
    fn DestroyIcon(hicon: isize) -> i32;
    fn SendMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryA(lp_lib_file_name: *const u8) -> isize;
    fn GetProcAddress(h_module: isize, lp_proc_name: *const u8) -> *const std::ffi::c_void;
}

const CHROME_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

const TAB_SHELL_HTML: &str = include_str!("tab_shell.html");
const NEWTAB_HTML: &str = include_str!("newtab.html");
const HISTORY_HTML: &str = include_str!("history.html");
const WORKSPACE_HTML: &str = include_str!("workspace.html");
const AI_PANEL_HTML: &str = include_str!("ai_panel.html");
const CHAT_HEADER_HTML: &str = include_str!("chat_header.html");
const TITLEBAR_HTML: &str = include_str!("titlebar.html");
const DOCK_RESIZER_HTML: &str = include_str!("dock_resizer.html");
const SIDEBAR_RESIZER_HTML: &str = include_str!("sidebar_resizer.html");
const SETTINGS_MODAL_HTML: &str = include_str!("settings_modal.html");
const CONTENT_AGENT_JS: &str = include_str!("content_agent.js");
const ECOM_AGENTS_JS: &str = include_str!("ecom_agents.js");
const MOTION_JS: &str = include_str!("../assets/vendor/motion.js");
const TITLEBAR_HEIGHT: u32 = 40;

const TIKTOK_PASSIVE_SCAN_JS: &str = r##"(function () {
  try {
    var bodyTxt0 = (document.body && document.body.innerText) || "";
    if (/login|sign in|đăng nhập/i.test(window.location.pathname)) {
      if (window.ipc && typeof window.ipc.postMessage === "function") {
        window.ipc.postMessage(JSON.stringify({ cmd: "update_platform_data", platform: "tiktok", data_json: JSON.stringify({ logged_in: false, need_login: true, ts: new Date().toLocaleTimeString("vi-VN") }) }));
      }
      return;
    }
    var captcha = /xác minh để tiếp tục|kéo mảnh ghép vào vị trí/i.test(bodyTxt0);
    if (captcha) {
      if (window.ipc && typeof window.ipc.postMessage === "function") {
        window.ipc.postMessage(JSON.stringify({ cmd: "update_platform_data", platform: "tiktok", data_json: JSON.stringify({ logged_in: true, captcha_active: true, ts: new Date().toLocaleTimeString("vi-VN") }) }));
      }
      return;
    }

    function parseVn(s) {
      if (!s) return 0;
      var t = String(s).trim().toLowerCase().replace(/[^\d.,-]/g, "");
      if (!t) return 0;
      if (t.indexOf(".") !== -1 && t.indexOf(",") !== -1) {
        t = t.lastIndexOf(",") > t.lastIndexOf(".") ? t.replace(/\./g, "").replace(",", ".") : t.replace(/,/g, "");
      } else if (t.indexOf(",") !== -1) {
        var p = t.split(",");
        t = p.length === 2 && p[1].length <= 2 ? t.replace(",", ".") : t.replace(/,/g, "");
      } else if (t.indexOf(".") !== -1) {
        var p2 = t.split(".");
        if ((p2.length === 2 && p2[1].length === 3) || p2.length > 2) t = t.replace(/\./g, "");
      }
      var v = parseFloat(t);
      return isNaN(v) ? 0 : v;
    }

    function fromOverviewCard(matchFn) {
      var nodes = document.querySelectorAll("div, span, p");
      for (var i = 0; i < nodes.length; i++) {
        var rawLabel = (nodes[i].innerText || "").trim();
        if (!rawLabel || rawLabel.length > 65 || rawLabel.indexOf("\n") !== -1) continue;
        if (!matchFn(rawLabel.toLowerCase())) continue;
        var card = nodes[i].parentElement;
        for (var step = 0; step < 6 && card; step++) {
          var cTxt = card.innerText || "";
          var parts = cTxt.split(/so với/i);
          if (parts.length === 2) {
            var lines = parts[0].split(/
?\n/).map(function (x) { return x.trim(); }).filter(Boolean);
            for (var j = 0; j < lines.length; j++) {
              var mm = lines[j].match(/^([\d]+(?:[.,]\d+)*)/);
              if (mm) {
                var v = parseVn(mm[1]);
                if (v > 0) return v;
              }
            }
            break;
          }
          card = card.parentElement;
        }
      }
      return 0;
    }

    function getDateInputs() {
      return [].slice.call(document.querySelectorAll("input")).filter(function (el) {
        return /^\d{4}-\d{2}-\d{2}$/.test((el.value || "").trim());
      });
    }

    function isTodaySelected() {
      var inps = getDateInputs();
      if (inps.length >= 2) return inps[0].value.trim() === inps[1].value.trim();
      var bTxt = (document.body && document.body.innerText) || "";
      return bTxt.indexOf("so với ngày hôm trước") !== -1 || bTxt.indexOf("so với ngày") !== -1;
    }

    var retryCount = 0;
    function extractAndSend() {
      var bodyTxt = (document.body && document.body.innerText) || "";
      var isToday = isTodaySelected();
      var inps = getDateInputs();
      var costAds = fromOverviewCard(function (t) { return t === "chi phí" || (t.indexOf("chi phí") === 0 && t.indexOf("mỗi") === -1 && t.indexOf("ròng") === -1); });
      var skuOrders = fromOverviewCard(function (t) { return t.indexOf("số lượng đơn hàng") === 0 || t.indexOf("số lượng") === 0; });
      var cpaAds = fromOverviewCard(function (t) { return t.indexOf("chi phí cho mỗi") === 0 || t.indexOf("chi phí mỗi") === 0; });
      var grossRev = fromOverviewCard(function (t) { return t.indexOf("doanh thu gộp") === 0 || t.indexOf("doanh thu") === 0; });
      var roi = fromOverviewCard(function (t) { return t === "roi" || t.indexOf("roi ") === 0; });

      if ((!isToday || (costAds === 0 && skuOrders === 0)) && retryCount < 4) {
        retryCount++;
        setTimeout(extractAndSend, 1100);
        return;
      }

      var campaigns = [];
      var tables = document.querySelectorAll("table");
      if (tables.length > 0) {
        var tbl = tables[tables.length - 1];
        var ths = [].slice.call(tbl.querySelectorAll("thead th, tr:first-child th")).map(function (th) {
          return (th.innerText || "").trim().toLowerCase().replace(/\s+/g, " ");
        });
        var idxName = ths.findIndex(function (t) { return t.indexOf("tên chiến dịch") !== -1 || t.indexOf("tên sản phẩm") !== -1; });
        var idxStatus = ths.findIndex(function (t) { return t.indexOf("trạng thái") !== -1; });
        var idxBudget = ths.findIndex(function (t) { return t.indexOf("ngân sách") !== -1; });
        var idxCost = ths.findIndex(function (t) { return t === "chi phí" || (t.indexOf("chi phí") === 0 && t.indexOf("ròng") === -1 && t.indexOf("mỗi") === -1); });
        var idxOrders = ths.findIndex(function (t) { return t.indexOf("số lượng đơn hàng") !== -1; });
        var idxRev = ths.findIndex(function (t) { return t.indexOf("doanh thu gộp") !== -1 || t.indexOf("doanh thu") !== -1; });
        var idxRoi = ths.findIndex(function (t) { return t === "roi" || t.indexOf("roi ") === 0; });
        if (idxName < 0) idxName = 2;
        if (idxStatus < 0) idxStatus = 3;
        if (idxBudget < 0) idxBudget = 5;

        [].slice.call(tbl.querySelectorAll("tbody tr")).forEach(function (tr) {
          var cells = [].slice.call(tr.querySelectorAll("td, th")).map(function (c) { return (c.innerText || "").trim().replace(/\s+/g, " "); });
          if (cells.length >= 4) {
            var name = (cells[idxName] || "").trim();
            if (name && !/tên chiến dịch|tên sản phẩm|bật\/tắt/i.test(name)) {
              var cost = idxCost >= 0 && cells[idxCost] ? parseVn(cells[idxCost]) : 0;
              var rev = idxRev >= 0 && cells[idxRev] ? parseVn(cells[idxRev]) : 0;
              campaigns.push({
                name: name,
                status: idxStatus >= 0 && cells[idxStatus] ? cells[idxStatus] : "Đang hoạt động",
                budget: idxBudget >= 0 && cells[idxBudget] ? parseVn(cells[idxBudget]) : 0,
                cost: cost,
                orders: idxOrders >= 0 && cells[idxOrders] ? parseVn(cells[idxOrders]) : 0,
                sales: rev,
                roas: idxRoi >= 0 && cells[idxRoi] ? parseVn(cells[idxRoi]) : (cost > 0 && rev > 0 ? Number((rev / cost).toFixed(2)) : 0)
              });
            }
          }
        });
      }
      if (grossRev === 0 && costAds > 0 && roi > 0) grossRev = Math.round(costAds * roi);
      if (roi === 0 && costAds > 0 && grossRev > 0) roi = Number((grossRev / costAds).toFixed(2));
      var payload = {
        ts: new Date().toLocaleTimeString("vi-VN"),
        logged_in: true,
        captcha_active: false,
        period_label: "Hôm nay (" + (inps[0] ? inps[0].value.trim() : "") + ")",
        orders_ads: skuOrders,
        cpa_ads: cpaAds,
        sales_ads: grossRev,
        cost_ads: costAds,
        roas_ads: roi,
        campaigns: campaigns,
        live_streaming: /đang live|live now/i.test(bodyTxt)
      };
      if (window.ipc && typeof window.ipc.postMessage === "function") {
        window.ipc.postMessage(JSON.stringify({ cmd: "update_platform_data", platform: "tiktok", data_json: JSON.stringify(payload) }));
      }
    }

    if (!isTodaySelected()) {
      var inps = getDateInputs();
      var targetInp = inps[0] || document.querySelector(".dashboard-date-picker-F08X input") || document.querySelector(".dashboard-date-picker-F08X");
      if (targetInp) {
        targetInp.click();
        setTimeout(function () {
          var el = [].slice.call(document.querySelectorAll("*")).find(function (e) {
            return e.children.length <= 1 && (e.innerText || "").trim() === "Hôm nay";
          });
          if (el) el.click();
          setTimeout(extractAndSend, 2400);
        }, 800);
        return;
      }
    }
    extractAndSend();
  } catch (_) {}
})();"##;

const SHELL_SHORTCUTS_JS: &str = r##"(function(){
  if (window.__tpShellShortcuts) return;
  window.__tpShellShortcuts = true;
  function send(payload) {
    try {
      if (window.ipc && typeof window.ipc.postMessage === "function") {
        window.ipc.postMessage(JSON.stringify(payload));
      }
    } catch (_) {}
  }
  window.addEventListener("keydown", function(e) {
    if (e.isComposing || e.keyCode === 229) return;
    var code = e.code;
    if (e.key === "F5" && !e.altKey && !e.shiftKey) {
      e.preventDefault(); e.stopImmediatePropagation();
      send({ cmd: e.ctrlKey ? "hard_reload" : "reload" });
      return;
    }
    if (e.key === "F3" && !e.ctrlKey && !e.altKey) {
      e.preventDefault(); e.stopImmediatePropagation();
      send({ cmd: "toggle_find_bar" });
      return;
    }
    if (e.key === "F6" && !e.ctrlKey && !e.altKey && !e.shiftKey) {
      e.preventDefault(); e.stopImmediatePropagation();
      send({ cmd: "focus_url_bar" });
      return;
    }
    if (e.ctrlKey && !e.altKey && !e.metaKey) {
      if (code === "KeyT" && e.shiftKey) {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "reopen_closed_tab" });
      } else if (code === "KeyT" && !e.shiftKey) {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "new_tab", url: "tp://newtab", isolated: false });
      } else if ((code === "KeyW" || code === "F4") && !e.shiftKey) {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "close_active_tab" });
      } else if (code === "Tab") {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: e.shiftKey ? "prev_tab" : "next_tab" });
      } else if (code === "PageDown" && !e.shiftKey) {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "next_tab" });
      } else if (code === "PageUp" && !e.shiftKey) {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "prev_tab" });
      } else if (code === "KeyR") {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: e.shiftKey ? "hard_reload" : "reload" });
      } else if (code === "KeyL" && !e.shiftKey) {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "focus_url_bar" });
      } else if (code === "KeyD" && !e.shiftKey) {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "bookmark_active" });
      } else if (code === "KeyF" && !e.shiftKey) {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "toggle_find_bar" });
      } else if (code === "KeyH" && !e.shiftKey) {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "open_history" });
      } else if (code === "Equal" || code === "NumpadAdd") {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "zoom_step", delta: 10 });
      } else if (code === "Minus" || code === "NumpadSubtract") {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "zoom_step", delta: -10 });
      } else if ((code === "Digit0" || code === "Numpad0") && !e.shiftKey) {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "zoom_reset" });
      } else if (!e.shiftKey && /^(Digit|Numpad)[1-8]$/.test(code)) {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "focus_index", index: parseInt(code.slice(-1), 10) - 1 });
      } else if (!e.shiftKey && (code === "Digit9" || code === "Numpad9")) {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "focus_last_tab" });
      }
      return;
    }
    if (!e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
    if (code === "ArrowLeft") {
      e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "nav_back" });
    } else if (code === "ArrowRight") {
      e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "nav_forward" });
    } else if (code === "Home") {
      e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "go_home" });
    } else if (code === "KeyD") {
      e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "focus_url_bar" });
    } else if (code === "Comma") {
      e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "toggle_settings_modal" });
    } else if (code === "KeyA") {
      e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "toggle_chat_channel", channel: "ai" });
    } else if (code === "KeyZ") {
      e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "toggle_chat_channel", channel: "zalo" });
    } else if (code === "KeyF") {
      e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "toggle_chat_channel", channel: "fb" });
    } else if (code === "KeyM") {
      e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "cycle_view_mode" });
    } else if (code === "KeyT") {
      e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "toggle_tab_orientation" });
    } else if (code === "KeyS") {
      e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "toggle_sidebar_collapse" });
    } else if (code === "KeyB") {
      e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "toggle_collections_drawer" });
    } else if (code === "KeyP") {
      e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "new_tab", url: "tp://newtab", isolated: true });
    } else if (code === "KeyW") {
      e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "open_workspace" });
    }
  }, true);
})();"##;

const CHANNEL_UNREAD_WATCHER_JS: &str = r##"(function(){
  if (window.__tpUnreadWatcher) return;
  window.__tpUnreadWatcher = true;
  var ch = "__CH__";
  var lastCount = -1;

  function getActiveContact() {
    try {
      if (ch === "zalo") {
        var el = document.querySelector(".header-title, .conv-item.active .conv-item-title, [data-id*='chat-name'], .card-name");
        if (el && el.textContent.trim()) return el.textContent.trim();
        return (document.title || "").replace(/ - Zalo.*$/i, "").replace(/^\(\d+\)\s*/, "").trim() || "Zalo";
      } else {
        var fbEl = document.querySelector("h1, [role='main'] h2, [aria-label*='Conversation']");
        if (fbEl && fbEl.textContent.trim()) return fbEl.textContent.trim();
        return (document.title || "").replace(/ - Messenger.*$/i, "").replace(/^\(\d+\)\s*/, "").trim() || "Messenger";
      }
    } catch (_) { return ch === "zalo" ? "Zalo" : "Messenger"; }
  }

  // --- FLOATING SELECTION PILL ---
  var selPill = null;
  function ensurePill() {
    if (selPill) return selPill;
    selPill = document.createElement("div");
    selPill.id = "__tp_chat_sel_pill";
    selPill.style.cssText = "position:absolute; display:none; z-index:99999999; background:linear-gradient(135deg, #E2BE68 0%, #B5872E 100%); color:#140F05; font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif; font-size:11px; font-weight:700; padding:5px 12px; border-radius:999px; box-shadow:0 6px 20px rgba(0,0,0,0.5), inset 0 1px 0 rgba(255,255,255,0.4); cursor:pointer; user-select:none; transition:transform 0.14s ease;";
    selPill.textContent = "✨ Lập Plan Cowork";
    selPill.onmousedown = function(e) { e.preventDefault(); e.stopPropagation(); };
    selPill.onclick = function(e) {
      e.preventDefault();
      e.stopPropagation();
      var sel = window.getSelection();
      var text = (sel ? sel.toString() : "").trim();
      if (!text && selPill.__pendingText) text = selPill.__pendingText;
      if (text) {
        var contact = getActiveContact();
        try {
          if (window.ipc && typeof window.ipc.postMessage === "function") {
            window.ipc.postMessage(JSON.stringify({
              cmd: "create_task_from_selection",
              text: text,
              source: ch,
              contact: contact
            }));
          }
        } catch (_) {}
      }
      hidePill();
    };
    document.body.appendChild(selPill);
    return selPill;
  }

  function hidePill() {
    if (selPill) selPill.style.display = "none";
  }

  document.addEventListener("selectionchange", function() {
    try {
      var sel = window.getSelection();
      var txt = (sel ? sel.toString() : "").trim();
      if (txt.length < 4) {
        hidePill();
      }
    } catch (_) {}
  });

  document.addEventListener("mouseup", function(e) {
    try {
      var sel = window.getSelection();
      var txt = (sel ? sel.toString() : "").trim();
      if (txt.length >= 4) {
        var pill = ensurePill();
        pill.__pendingText = txt;
        var r = sel.getRangeAt(0).getBoundingClientRect();
        var top = window.scrollY + r.top - 32;
        if (top < window.scrollY + 10) top = window.scrollY + r.bottom + 8;
        var left = Math.max(10, window.scrollX + r.left + (r.width / 2) - 60);
        pill.style.top = top + "px";
        pill.style.left = left + "px";
        pill.style.display = "block";
      } else {
        hidePill();
      }
    } catch (_) {}
  });

  // --- CONVERSATION SCANNER HELPER ---
  window.__tpScanChatConversation = function() {
    try {
      var sel = ch === "zalo" ? ".chat-message, .message-view, .text-message, .bubble-content, [data-id*='bubble']" : "[role='row'] [dir='auto'], [data-scope='messages_table']";
      var nodes = document.querySelectorAll(sel);
      var collected = [];
      var start = Math.max(0, nodes.length - 6);
      for (var i = start; i < nodes.length; i++) {
        var t = (nodes[i].textContent || "").trim();
        if (t && t.length > 2) collected.push(t);
      }
      var fullText = collected.join("\n");
      if (!fullText) fullText = "Đoạn chat Zalo với " + getActiveContact();
      if (window.ipc && typeof window.ipc.postMessage === "function") {
        window.ipc.postMessage(JSON.stringify({
          cmd: "create_task_from_selection",
          text: fullText,
          source: ch,
          contact: getActiveContact()
        }));
      }
    } catch (_) {}
  };

  // --- DRAFT MESSAGE INSERTER ---
  window.__tpInsertDraftMessage = function(text) {
    try {
      var inp = document.querySelector("#richInput, [contenteditable='true'], .chat-input, .input-area, textarea");
      if (inp) {
        inp.focus();
        if (inp.getAttribute("contenteditable") === "true") {
          document.execCommand("selectAll", false, null);
          document.execCommand("insertText", false, text);
        } else {
          inp.value = text;
          inp.dispatchEvent(new Event("input", { bubbles: true }));
        }
        var toast = document.createElement("div");
        toast.style.cssText = "position:fixed; top:16px; left:50%; transform:translateX(-50%); z-index:99999999; background:rgba(17,19,24,0.96); border:1.5px solid #E2BE68; color:#F5F2EB; font-family:-apple-system,sans-serif; font-size:12px; font-weight:700; padding:8px 16px; border-radius:999px; box-shadow:0 10px 30px rgba(0,0,0,0.6); pointer-events:none;";
        toast.textContent = "✨ Đã điền nháp báo cáo vào ô chat!";
        document.body.appendChild(toast);
        setTimeout(function(){ if (toast.parentNode) toast.parentNode.removeChild(toast); }, 2200);
      }
    } catch (_) {}
  };

  function detectCount() {
    var count = 0;
    try {
      var m = String(document.title || "").match(/^\s*[\(\[](\d+)[\)\]]/);
      if (m) count = Math.max(count, parseInt(m[1], 10) || 0);
      if (ch === "fb") {
        var nodes = document.querySelectorAll('[aria-label*="unread" i], [aria-label*="chưa đọc" i], [data-testid*="unread"]');
        for (var i = 0; i < nodes.length; i++) {
          var lb = (nodes[i].getAttribute("aria-label") || nodes[i].textContent || "").trim();
          var nm = lb.match(/(\d+)/);
          if (nm) count = Math.max(count, parseInt(nm[1], 10) || 0);
        }
      } else if (ch === "zalo") {
        var badges = document.querySelectorAll('.conv-unread, .unread-count, .badge-count, .z-badge, [data-id*="Unread"], [data-id*="div_Main_Badge"], .conv-item .unread');
        var sum = 0;
        for (var j = 0; j < badges.length; j++) {
          var txt = (badges[j].textContent || "").trim();
          if (/^\d+$/.test(txt)) sum += parseInt(txt, 10);
          else if (txt.indexOf("+") !== -1) sum += 5;
        }
        if (sum === 0) {
          var dots = document.querySelectorAll('.unread-dot, .tab-red-dot, .conv-item.unread');
          if (dots.length > 0) sum = dots.length;
        }
        if (sum > count) count = sum;
      }
    } catch (_) {}
    if (count !== lastCount) {
      lastCount = count;
      try {
        if (window.ipc && typeof window.ipc.postMessage === "function") {
          window.ipc.postMessage(JSON.stringify({ cmd: "channel_unread", channel: ch, count: count }));
        }
      } catch (_) {}
    }
  }
  setInterval(detectCount, 1200);
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", function(){ setTimeout(detectCount, 600); });
  } else {
    setTimeout(detectCount, 600);
  }
})();"##;

const HEADING_FONT_ROMAN_WOFF2: &[u8] =
    include_bytes!("../assets/fonts/CormorantGaramondGnosis-Variable-Roman.woff2");
const HEADING_FONT_ITALIC_WOFF2: &[u8] =
    include_bytes!("../assets/fonts/CormorantGaramondGnosis-Variable-Italic.woff2");

const LOGO_RIBBON_LIGHT_PNG: &[u8] = include_bytes!("../assets/icons/logo_ribbon_light.png");
const LOGO_RIBBON_DARK_PNG: &[u8] = include_bytes!("../assets/icons/logo_ribbon_dark.png");
const LOGO_ARROW_LIGHT_PNG: &[u8] = include_bytes!("../assets/icons/logo_arrow_light.png");
const LOGO_ARROW_DARK_PNG: &[u8] = include_bytes!("../assets/icons/logo_arrow_dark.png");
const APP_RIBBON_LIGHT_PNG: &[u8] = include_bytes!("../assets/icons/app_ribbon_light.png");
const APP_RIBBON_DARK_PNG: &[u8] = include_bytes!("../assets/icons/app_ribbon_dark.png");
const APP_ARROW_LIGHT_PNG: &[u8] = include_bytes!("../assets/icons/app_arrow_light.png");
const APP_ARROW_DARK_PNG: &[u8] = include_bytes!("../assets/icons/app_arrow_dark.png");

const WIN_RIBBON_LIGHT_RGBA: &[u8] = include_bytes!("../assets/icons/win_ribbon_light.rgba");
const WIN_RIBBON_DARK_RGBA: &[u8] = include_bytes!("../assets/icons/win_ribbon_dark.rgba");
const WIN_ARROW_LIGHT_RGBA: &[u8] = include_bytes!("../assets/icons/win_arrow_light.rgba");
const WIN_ARROW_DARK_RGBA: &[u8] = include_bytes!("../assets/icons/win_arrow_dark.rgba");

static HEADING_FONT_CSS: OnceLock<String> = OnceLock::new();
static BRAND_ASSETS_JS: OnceLock<String> = OnceLock::new();

fn encode_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(((input.len() + 2) / 3) * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16)
            | ((input[i + 1] as u32) << 8)
            | (input[i + 2] as u32);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        out.push(TABLE[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let n = (input[i] as u32) << 16;
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }
    out
}

fn heading_font_css() -> &'static str {
    HEADING_FONT_CSS
        .get_or_init(|| {
            let roman_b64 = encode_base64(HEADING_FONT_ROMAN_WOFF2);
            let italic_b64 = encode_base64(HEADING_FONT_ITALIC_WOFF2);
            format!(
                concat!(
                    "@font-face {{\n",
                    "  font-family: \"Cormorant Garamond Gnosis\";\n",
                    "  src: url(\"data:font/woff2;base64,{}\") format(\"woff2\");\n",
                    "  font-weight: 300 700;\n",
                    "  font-style: normal;\n",
                    "  font-display: swap;\n",
                    "}}\n",
                    "@font-face {{\n",
                    "  font-family: \"Cormorant Garamond Gnosis\";\n",
                    "  src: url(\"data:font/woff2;base64,{}\") format(\"woff2\");\n",
                    "  font-weight: 300 700;\n",
                    "  font-style: italic;\n",
                    "  font-display: swap;\n",
                    "}}"
                ),
                roman_b64, italic_b64
            )
        })
        .as_str()
}

fn brand_assets_js() -> &'static str {
    BRAND_ASSETS_JS
        .get_or_init(|| {
            format!(
                concat!(
                    "window.__TP_BRAND_ASSETS = {{\n",
                    "  logo_ribbon_light: \"data:image/png;base64,{}\",\n",
                    "  logo_ribbon_dark: \"data:image/png;base64,{}\",\n",
                    "  logo_arrow_light: \"data:image/png;base64,{}\",\n",
                    "  logo_arrow_dark: \"data:image/png;base64,{}\",\n",
                    "  app_ribbon_light: \"data:image/png;base64,{}\",\n",
                    "  app_ribbon_dark: \"data:image/png;base64,{}\",\n",
                    "  app_arrow_light: \"data:image/png;base64,{}\",\n",
                    "  app_arrow_dark: \"data:image/png;base64,{}\"\n",
                    "}};\n",
                    "window.__tpResolveBrandAsset = function(kind, logoStyle, uiTheme) {{\n",
                    "  var k = (kind === \"logo\") ? \"logo\" : \"app\";\n",
                    "  var s = (logoStyle === \"arrow\") ? \"arrow\" : \"ribbon\";\n",
                    "  var isLight = (uiTheme === \"carrara-light\" || uiTheme === \"warm-paper\" || uiTheme === \"light\");\n",
                    "  var t = isLight ? \"light\" : \"dark\";\n",
                    "  var key = k + \"_\" + s + \"_\" + t;\n",
                    "  return (window.__TP_BRAND_ASSETS && window.__TP_BRAND_ASSETS[key]) || \"\";\n",
                    "}};\n",
                    "window.__tpSetFavicon = function(src) {{\n",
                    "  if (!src) return;\n",
                    "  var link = document.getElementById(\"tp_favicon\") || document.querySelector(\"link[rel~='icon']\");\n",
                    "  if (link && link.getAttribute(\"href\") !== src) link.setAttribute(\"href\", src);\n",
                    "}};"
                ),
                encode_base64(LOGO_RIBBON_LIGHT_PNG),
                encode_base64(LOGO_RIBBON_DARK_PNG),
                encode_base64(LOGO_ARROW_LIGHT_PNG),
                encode_base64(LOGO_ARROW_DARK_PNG),
                encode_base64(APP_RIBBON_LIGHT_PNG),
                encode_base64(APP_RIBBON_DARK_PNG),
                encode_base64(APP_ARROW_LIGHT_PNG),
                encode_base64(APP_ARROW_DARK_PNG),
            )
        })
        .as_str()
}

fn inject_ui_html(html: &str, ui_theme: &str) -> String {
    let normalized_theme = if ui_theme.eq_ignore_ascii_case("carrara-light")
        || ui_theme.eq_ignore_ascii_case("warm-paper")
        || ui_theme.eq_ignore_ascii_case("light")
    {
        "carrara-light"
    } else {
        "obsidian-gold"
    };
    let themed = if normalized_theme != "obsidian-gold" {
        html.replacen(
            "data-theme=\"obsidian-gold\"",
            &format!("data-theme=\"{}\"", normalized_theme),
            1,
        )
    } else {
        html.to_string()
    };
    themed
        .replace("/*__TP_FONT_FACE__*/", heading_font_css())
        .replace("/*__TP_MOTION_JS__*/", MOTION_JS)
        .replace("/*__TP_BRAND_ASSETS_JS__*/", brand_assets_js())
}

#[cfg(test)]
fn inject_ui_fonts(html: &str) -> String {
    inject_ui_html(html, "obsidian-gold")
}

static NEXT_TAB_ID: AtomicU64 = AtomicU64::new(1);
static ADBLOCK_ENABLED: AtomicBool = AtomicBool::new(true);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChatChannel {
    Ai,
    Messenger,
    Zalo,
    Custom,
}

impl ChatChannel {
    fn as_str(self) -> &'static str {
        match self {
            ChatChannel::Ai => "ai",
            ChatChannel::Messenger => "fb",
            ChatChannel::Zalo => "zalo",
            ChatChannel::Custom => "custom",
        }
    }

    fn from_str(s: &str) -> Self {
        if s.eq_ignore_ascii_case("fb") || s.eq_ignore_ascii_case("messenger") {
            ChatChannel::Messenger
        } else if s.eq_ignore_ascii_case("zalo") {
            ChatChannel::Zalo
        } else if s.eq_ignore_ascii_case("custom") {
            ChatChannel::Custom
        } else {
            ChatChannel::Ai
        }
    }
}

#[derive(Debug, Clone)]
enum AppCmd {
    ToggleTabOrientation,
    ToggleSidebarCollapse,
    #[allow(dead_code)]
    SetTitlebarHover(bool),
    ToggleTitlebarPin,
    DragWindow,
    WinMinimize,
    WinToggleMaximize,
    WinClose,
    CycleViewMode,
    ToggleAiMode,
    SetAiMode(AiDisplayMode),
    OpenChatChannel(ChatChannel),
    ToggleChatChannel(ChatChannel),
    ToggleChatDockStyle,
    StartDockResize,
    SetDockWidth(u32),
    ResetDockWidth,
    FinishDockResize,
    StartSidebarResize,
    SetSidebarWidth(u32),
    ResetSidebarWidth,
    FinishSidebarResize,
    UpdateTabMeta {
        tab_id: u64,
        title: String,
        favicon: String,
    },
    PreloadChatChannels,
    ChannelUnread {
        channel: ChatChannel,
        count: u32,
    },
    OpenChannelInTab,
    SyncAiPanelInit,
    SyncSettingsModalInit,
    UiShellReady(String),
    SetSettingsSection(String),
    OpenSettingsModal(String),
    CloseSettingsModal,
    ToggleSettingsModal,
    SaveSetting {
        key: String,
        value: String,
    },
    OpenAiSettings,
    FocusAiPanel,
    OpenGoogleLogin,
    SetGoogleUser(String),
    SetUiTheme(String),
    NewTab {
        url: String,
        isolated: bool,
    },
    FocusIndex(usize),
    FocusLastTab,
    NextTab,
    PrevTab,
    FocusTabById(u64),
    CloseIndex(usize),
    CloseActiveTab,
    NavigateActive(String),
    NavBack,
    NavForward,
    ReloadActive,
    HardReloadActive,
    GoHome,
    FocusUrlBar,
    ToggleFindBar,
    ToggleCollectionsDrawer,
    OpenHistory,
    GetHistory,
    ClearHistory,
    OpenWorkspace,
    RunEcomScan {
        scope: String,
    },
    SetEcomAutoScan(bool),
    UpdatePlatformData {
        platform: String,
        data_json: String,
    },
    ExportShopeeCsv,
    CloseTabById(u64),
    ReopenClosedTab,
    ZoomStep(i32),
    ZoomReset,
    SaveFloatingOrbs(String),
    OpenCustomOrb {
        id: String,
        kind: String,
        url: String,
        title: String,
    },
    BookmarkActive,
    SaveBookmarks(String),
    SaveAiConfig {
        provider: String,
        api_key: String,
        model: String,
        base_url: String,
        auto_minimize_on_task: bool,
    },
    SaveSkillsJson(String),
    StartSkillRecording,
    StopSkillRecording,
    RecordedSkillStep(String),
    RunAiPrompt(String),
    RequestDomContextForAi {
        use_worker: bool,
        prompt: String,
    },
    DomContextResult {
        tab_id: u64,
        title: String,
        url: String,
        visible_text: String,
        elements_json: String,
    },
    ExecuteDomAction {
        target_tab_id: Option<u64>,
        action_json: String,
    },
    SyncBubbleStatus {
        working: bool,
        recording: bool,
        status_text: String,
        last_reply: String,
    },
    PageFinished {
        tab_id: u64,
        url: String,
    },
    CheckReminders,
    GetTasks,
    SaveTasksJson(String),
    SnoozeReminder {
        id: String,
        minutes: u64,
    },
    CompleteTask(String),
    CreateTaskFromSelection {
        text: String,
        source: String,
        contact: String,
    },
    ScanZaloChat,
    PasteToChat {
        channel: String,
        text: String,
    },
    McpDispatch(McpBridgeMsg),
    McpJsCallback {
        req_id: u64,
        ok: bool,
        payload_json: String,
    },
}

struct BrowserTab {
    id: u64,
    profile_name: String,
    url: String,
    title: String,
    favicon_url: String,
    worker_kind: Option<String>,
    worker_status: String,
    webview: WebView,
}

struct TpBrowserApp {
    window: Window,
    proxy: EventLoopProxy<AppCmd>,
    web_context: WebContext,
    exe_dir: PathBuf,
    titlebar_shell: WebView,
    tab_shell: WebView,
    sidebar_resizer_shell: WebView,
    chat_header_shell: WebView,
    dock_resizer_shell: WebView,
    settings_shell: WebView,
    ai_shell: WebView,
    fb_shell: Option<WebView>,
    zalo_shell: Option<WebView>,
    custom_popup_shell: Option<WebView>,
    custom_popup_id: String,
    custom_popup_title: String,
    custom_popup_url: String,
    chat_channel: ChatChannel,
    tabs: Vec<BrowserTab>,
    active_idx: usize,
    tab_orientation: TabOrientation,
    sidebar_collapsed: bool,
    titlebar_hover: bool,
    titlebar_pinned: bool,
    settings_open: bool,
    settings_section: String,
    should_exit: bool,
    ai_dock_width: u32,
    unread_ai: u32,
    unread_fb: u32,
    unread_zalo: u32,
    ai_finished_flash: bool,
    chime_seq: u64,
    chime_kind: String,
    zoom_pct: u32,
    closed_tabs: Vec<String>,
    history_json: String,
    view_mode: ContentViewMode,
    ai_mode: AiDisplayMode,
    ai_working: bool,
    ai_recording: bool,
    ai_status_text: String,
    ai_last_reply: String,
    master_ratio: f32,
    profile_counter: u32,
    // Config fields (persisted in config.json)
    floating_omnibox: bool,
    compact_titlebar: bool,
    sidebar_width: u32,
    show_unread_badges: bool,
    always_show_floating_orbs: bool,
    floating_orbs_json: String,
    adblock_enabled: bool,
    ai_provider: String,
    ai_api_key: String,
    ai_model: String,
    ai_base_url: String,
    auto_minimize_on_task: bool,
    google_user: String,
    ui_theme: String,
    logo_style: String,
    startup_mode: String,
    #[cfg(target_os = "windows")]
    hicon_big: isize,
    #[cfg(target_os = "windows")]
    hicon_small: isize,
    bookmarks_json: String,
    skills_json: String,
    tasks_json: String,
    ai_worker_tab_id: Option<u64>,
    shopee_orders_json: String,
    shopee_pas_json: String,
    tiktok_json: String,
    ecom_auto_scan: bool,
    ecom_status_text: String,
    ecom_logs: Vec<String>,
    last_ecom_auto_epoch: u64,
    next_mcp_req_id: u64,
    mcp_pending: HashMap<u64, mpsc::Sender<Result<String, String>>>,
}

impl TpBrowserApp {
    fn normalize_theme(raw: &str) -> String {
        let s = raw.trim().to_lowercase();
        if s == "carrara-light" || s == "warm-paper" || s == "light" || s == "airy-cloud" || s == "zen-sand" {
            "carrara-light".to_string()
        } else {
            "obsidian-gold".to_string()
        }
    }

    fn normalize_logo_style(raw: &str) -> String {
        if raw.trim().eq_ignore_ascii_case("arrow") {
            "arrow".to_string()
        } else {
            "ribbon".to_string()
        }
    }

    fn build_window_icon(logo_style: &str, ui_theme: &str) -> Option<Icon> {
        let is_light = Self::normalize_theme(ui_theme) == "carrara-light";
        let is_arrow = Self::normalize_logo_style(logo_style) == "arrow";
        let bytes = match (is_arrow, is_light) {
            (false, false) => WIN_RIBBON_DARK_RGBA,
            (false, true) => WIN_RIBBON_LIGHT_RGBA,
            (true, false) => WIN_ARROW_DARK_RGBA,
            (true, true) => WIN_ARROW_LIGHT_RGBA,
        };
        Icon::from_rgba(bytes.to_vec(), 64, 64).ok()
    }

    fn sync_window_icon(&mut self) {
        if let Some(ico) = Self::build_window_icon(&self.logo_style, &self.ui_theme) {
            self.window.set_window_icon(Some(ico));
        }
        #[cfg(target_os = "windows")]
        {
            const WM_SETICON: u32 = 0x0080;
            const ICON_SMALL: usize = 0;
            const ICON_BIG: usize = 1;
            let is_light = Self::normalize_theme(&self.ui_theme) == "carrara-light";
            let is_arrow = Self::normalize_logo_style(&self.logo_style) == "arrow";
            let png = match (is_arrow, is_light) {
                (false, false) => APP_RIBBON_DARK_PNG,
                (false, true) => APP_RIBBON_LIGHT_PNG,
                (true, false) => APP_ARROW_DARK_PNG,
                (true, true) => APP_ARROW_LIGHT_PNG,
            };
            unsafe {
                let h_big = CreateIconFromResourceEx(
                    png.as_ptr(),
                    png.len() as u32,
                    1,
                    0x00030000,
                    48,
                    48,
                    0,
                );
                let h_small = CreateIconFromResourceEx(
                    png.as_ptr(),
                    png.len() as u32,
                    1,
                    0x00030000,
                    24,
                    24,
                    0,
                );
                let hwnd = self.window.hwnd();
                if h_big != 0 {
                    SendMessageW(hwnd, WM_SETICON, ICON_BIG, h_big);
                    if self.hicon_big != 0 {
                        DestroyIcon(self.hicon_big);
                    }
                    self.hicon_big = h_big;
                }
                if h_small != 0 {
                    SendMessageW(hwnd, WM_SETICON, ICON_SMALL, h_small);
                    if self.hicon_small != 0 {
                        DestroyIcon(self.hicon_small);
                    }
                    self.hicon_small = h_small;
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn sync_window_titlebar_theme(&self) {
        const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
        const DWMWA_BORDER_COLOR: u32 = 34;
        const DWMWCP_ROUND: u32 = 2;

        let hwnd = self.window.hwnd();
        let is_light = self.ui_theme == "carrara-light";
        let corner_pref: u32 = DWMWCP_ROUND;
        // COLORREF uses 0x00BBGGRR
        let border_color: u32 = if is_light { 0x0089B0C5 } else { 0x002D4B68 };

        unsafe {
            let hmod = LoadLibraryA(b"dwmapi.dll\0".as_ptr());
            if hmod == 0 {
                return;
            }
            let proc = GetProcAddress(hmod, b"DwmSetWindowAttribute\0".as_ptr());
            if proc.is_null() {
                return;
            }
            type DwmSetWindowAttributeFn =
                unsafe extern "system" fn(isize, u32, *const std::ffi::c_void, u32) -> i32;
            let dwm_set: DwmSetWindowAttributeFn = std::mem::transmute(proc);

            let _ = dwm_set(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner_pref as *const u32 as *const std::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            );
            let _ = dwm_set(
                hwnd,
                DWMWA_BORDER_COLOR,
                &border_color as *const u32 as *const std::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            );
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn sync_window_titlebar_theme(&self) {}

    #[cfg(target_os = "windows")]
    #[allow(dead_code)]
    fn spawn_titlebar_hover_watcher(&self) {
        let hwnd = self.window.hwnd();
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let mut shown = false;
            let mut last_inside = std::time::Instant::now();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(45));
                unsafe {
                    if IsWindow(hwnd) == 0 {
                        break;
                    }
                    if IsIconic(hwnd) != 0 {
                        if shown {
                            shown = false;
                            let _ = proxy.send_event(AppCmd::SetTitlebarHover(false));
                        }
                        continue;
                    }
                    let mut pt = WinPoint { x: 0, y: 0 };
                    if GetCursorPos(&mut pt) == 0 {
                        continue;
                    }
                    let mut origin = WinPoint { x: 0, y: 0 };
                    if ClientToScreen(hwnd, &mut origin) == 0 {
                        continue;
                    }
                    let mut crect = WinRect {
                        left: 0,
                        top: 0,
                        right: 0,
                        bottom: 0,
                    };
                    if GetClientRect(hwnd, &mut crect) == 0 {
                        continue;
                    }

                    let dpi = GetDpiForWindow(hwnd);
                    let scale = if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 };
                    let top_trigger_px = (8.0 * scale).round() as i32;
                    let keep_open_px = ((TITLEBAR_HEIGHT as f64 + 8.0) * scale).round() as i32;

                    let dx = pt.x - origin.x;
                    let dy = pt.y - origin.y;
                    let in_x = dx >= 0 && dx < crect.right;

                    let wfp = WindowFromPoint(pt);
                    let root = if wfp != 0 { GetAncestor(wfp, 2) } else { 0 };
                    let lbtn_down = (GetAsyncKeyState(0x01) & (0x8000u16 as i16)) != 0;

                    if !shown {
                        if in_x && dy >= 0 && dy <= top_trigger_px && root == hwnd {
                            shown = true;
                            last_inside = std::time::Instant::now();
                            let _ = proxy.send_event(AppCmd::SetTitlebarHover(true));
                        }
                    } else if lbtn_down
                        || (in_x
                            && dy >= -2
                            && dy <= keep_open_px
                            && (root == hwnd || dy <= top_trigger_px))
                    {
                        last_inside = std::time::Instant::now();
                    } else if last_inside.elapsed() >= std::time::Duration::from_millis(240) {
                        shown = false;
                        let _ = proxy.send_event(AppCmd::SetTitlebarHover(false));
                    }
                }
            }
        });
    }

    #[cfg(not(target_os = "windows"))]
    fn spawn_titlebar_hover_watcher(&self) {}

    #[cfg(target_os = "windows")]
    fn bring_webview_to_front(wv: &WebView) {
        let hwnd = wv.hwnd().0 as isize;
        unsafe {
            // HWND_TOP = 0, SWP_NOSIZE (0x0001) | SWP_NOMOVE (0x0002) | SWP_NOACTIVATE (0x0010)
            SetWindowPos(hwnd, 0, 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0010);
        }
    }

    #[cfg(target_os = "windows")]
    fn send_webview_to_back(wv: &WebView) {
        let hwnd = wv.hwnd().0 as isize;
        unsafe {
            // HWND_BOTTOM = 1, SWP_NOSIZE (0x0001) | SWP_NOMOVE (0x0002) | SWP_NOACTIVATE (0x0010)
            SetWindowPos(hwnd, 1, 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0010);
        }
    }

    fn new(window: Window, proxy: EventLoopProxy<AppCmd>, exe_dir: PathBuf) -> wry::Result<Self> {
        let data_dir = exe_dir.join("user_data");
        let _ = fs::create_dir_all(&data_dir);
        let skills_dir = exe_dir.join("skills");
        let _ = fs::create_dir_all(&skills_dir);
        let mut web_context = WebContext::new(Some(data_dir));

        let extensions_dir = exe_dir.join("extensions");
        let _ = fs::create_dir_all(&extensions_dir);

        let pre_cfg = fs::read_to_string(exe_dir.join("config.json")).unwrap_or_default();
        let init_theme = Self::extract_field_str(&pre_cfg, "ui_theme")
            .map(|s| Self::normalize_theme(&s))
            .unwrap_or_else(|| "obsidian-gold".to_string());
        let init_logo = Self::extract_field_str(&pre_cfg, "logo_style")
            .map(|s| Self::normalize_logo_style(&s))
            .unwrap_or_else(|| "ribbon".to_string());
        let init_bg = if init_theme == "carrara-light" {
            (246, 242, 234, 255)
        } else {
            (17, 19, 24, 255)
        };
        let init_tab_orient = Self::extract_field_str(&pre_cfg, "tab_orientation")
            .map(|s| TabOrientation::from_str(&s))
            .unwrap_or(TabOrientation::VerticalLeft);
        let init_sidebar_collapsed = Self::extract_bool(&pre_cfg, "sidebar_collapsed").unwrap_or(false);
        let init_sidebar_width = Self::extract_u64(&pre_cfg, "\"sidebar_width\":")
            .map(|w| (w as u32).clamp(140, 440))
            .unwrap_or(228);
        let init_ai_mode = Self::extract_field_str(&pre_cfg, "ai_mode")
            .map(|s| AiDisplayMode::from_str(&s))
            .unwrap_or(AiDisplayMode::BubbleMinimized);
        let init_ai_dock_width = Self::extract_u64(&pre_cfg, "\"ai_dock_width\":")
            .map(|w| (w as u32).clamp(260, 1200))
            .unwrap_or(RIGHT_AI_DOCK_WIDTH);
        let init_view_mode = Self::extract_field_str(&pre_cfg, "view_mode")
            .map(|s| ContentViewMode::from_str(&s))
            .unwrap_or(ContentViewMode::SingleTab);
        let init_google_user = Self::extract_field_str(&pre_cfg, "google_user").unwrap_or_default();
        let init_adblock = Self::extract_bool(&pre_cfg, "adblock_enabled").unwrap_or(true);
        let init_startup = Self::extract_field_str(&pre_cfg, "startup_mode")
            .map(|s| if s.eq_ignore_ascii_case("newtab") { "newtab" } else { "restore" })
            .unwrap_or("restore");

        let scale = window.scale_factor();
        let win_sz = window.inner_size().to_logical::<u32>(scale);
        let win_w = win_sz.width.max(320);
        let win_h = win_sz.height.max(240);
        let init_shell = compute_shell_layout(
            win_w,
            win_h.saturating_sub(TITLEBAR_HEIGHT),
            init_tab_orient,
            init_sidebar_collapsed,
            init_sidebar_width,
            init_ai_mode,
            init_ai_dock_width,
        );

        let mut init_tabs_json = Vec::new();
        let mut init_active_idx = 0usize;
        if init_startup == "restore" {
            if let Ok(raw_sess) = fs::read_to_string(exe_dir.join("session.json")) {
                if let Ok(val) = parse_json(&raw_sess) {
                    init_active_idx = val.get("active_idx").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    if let Some(arr) = val.get("tabs").and_then(|v| v.as_arr()) {
                        for (i, item) in arr.iter().enumerate() {
                            if let Some(u) = item.get("url").and_then(|v| v.as_str()) {
                                let norm = Self::normalize_url(u);
                                let title = Self::title_from_url(&norm);
                                let isolated = item
                                    .get("profile")
                                    .and_then(|v| v.as_str())
                                    .map(|p| p != "default")
                                    .unwrap_or(false);
                                init_tabs_json.push(format!(
                                    "{{\"id\":{},\"url\":\"{}\",\"title\":\"{}\",\"favicon_url\":\"\",\"isolated\":{}}}",
                                    i + 1,
                                    Self::escape_json(&norm),
                                    Self::escape_json(&title),
                                    isolated
                                ));
                            }
                        }
                    }
                }
            }
        }
        if init_tabs_json.is_empty() {
            init_tabs_json.push("{\"id\":1,\"url\":\"tp://newtab\",\"title\":\"TPbrowser — Không Gian Khởi Đầu\",\"favicon_url\":\"\",\"isolated\":false}".to_string());
        }
        if init_active_idx >= init_tabs_json.len() {
            init_active_idx = init_tabs_json.len() - 1;
        }
        let bootstrap_js = format!(
            "window.__TP_BOOTSTRAP__ = {{\"orientation\":\"{}\",\"sidebar_collapsed\":{},\"view_mode\":\"{}\",\"ai_mode\":\"{}\",\"chat_channel\":\"ai\",\"ai_working\":false,\"active_idx\":{},\"google_user\":\"{}\",\"ui_theme\":\"{}\",\"logo_style\":\"{}\",\"adblock_enabled\":{},\"tab_orientation\":\"{}\",\"url\":\"\",\"title\":\"TPbrowser\",\"is_pinned\":true,\"is_maximized\":{},\"tabs\":[{}]}};",
            init_tab_orient.as_str(),
            init_sidebar_collapsed,
            init_view_mode.as_str(),
            init_ai_mode.as_str(),
            init_active_idx,
            Self::escape_json(&init_google_user),
            Self::escape_json(&init_theme),
            Self::escape_json(&init_logo),
            init_adblock,
            init_tab_orient.as_str(),
            window.is_maximized(),
            init_tabs_json.join(",")
        );

        let proxy_tab = proxy.clone();
        let tab_shell_html = inject_ui_html(TAB_SHELL_HTML, &init_theme)
            .replace("/*__TP_BOOTSTRAP_JS__*/", &bootstrap_js);
        let tab_shell = WebViewBuilder::new_with_web_context(&mut web_context)
            .with_browser_extensions_enabled(true)
            .with_extensions_path(extensions_dir.clone())
            .with_profile_name("default".to_string())
            .with_theme(Theme::Light)
            .with_background_color(init_bg)
            .with_visible(init_tab_orient == TabOrientation::VerticalLeft)
            .with_initialization_script(SHELL_SHORTCUTS_JS)
            .with_html(tab_shell_html)
            .with_ipc_handler(move |req| {
                Self::handle_ipc(&proxy_tab, req.body());
            })
            .with_bounds(Rect {
                position: LogicalPosition::new(init_shell.tab_bar.x, init_shell.tab_bar.y + TITLEBAR_HEIGHT as i32).into(),
                size: LogicalSize::new(init_shell.tab_bar.width.max(54), init_shell.tab_bar.height.max(120)).into(),
            })
            .build_as_child(&window)?;

        let shared_env = tab_shell.environment();

        let proxy_ch = proxy.clone();
        let chat_header_html = inject_ui_html(CHAT_HEADER_HTML, &init_theme);
        let chat_header_shell = WebViewBuilder::new_with_web_context(&mut web_context)
            .with_environment(shared_env.clone())
            .with_profile_name("default".to_string())
            .with_theme(Theme::Light)
            .with_background_color(init_bg)
            .with_visible(false)
            .with_initialization_script(SHELL_SHORTCUTS_JS)
            .with_html(chat_header_html)
            .with_ipc_handler(move |req| {
                Self::handle_ipc(&proxy_ch, req.body());
            })
            .with_bounds(Rect {
                position: LogicalPosition::new(1000, 0).into(),
                size: LogicalSize::new(RIGHT_AI_DOCK_WIDTH, FB_CHAT_HEADER_HEIGHT).into(),
            })
            .build_as_child(&window)?;

        let proxy_ai = proxy.clone();
        let ai_panel_html = inject_ui_html(AI_PANEL_HTML, &init_theme);
        let ai_shell = WebViewBuilder::new_with_web_context(&mut web_context)
            .with_environment(shared_env.clone())
            .with_profile_name("default".to_string())
            .with_theme(Theme::Light)
            .with_background_color(init_bg)
            .with_visible(false)
            .with_initialization_script(SHELL_SHORTCUTS_JS)
            .with_html(ai_panel_html)
            .with_ipc_handler(move |req| {
                Self::handle_ipc(&proxy_ai, req.body());
            })
            .with_bounds(Rect {
                position: LogicalPosition::new(1000, FB_CHAT_HEADER_HEIGHT as i32).into(),
                size: LogicalSize::new(RIGHT_AI_DOCK_WIDTH, 760).into(),
            })
            .build_as_child(&window)?;

        let proxy_tb = proxy.clone();
        let titlebar_html = inject_ui_html(TITLEBAR_HTML, &init_theme)
            .replace("/*__TP_BOOTSTRAP_JS__*/", &bootstrap_js);
        let titlebar_shell = WebViewBuilder::new_with_web_context(&mut web_context)
            .with_environment(shared_env.clone())
            .with_profile_name("default".to_string())
            .with_theme(Theme::Light)
            .with_background_color(init_bg)
            .with_initialization_script(SHELL_SHORTCUTS_JS)
            .with_html(titlebar_html)
            .with_ipc_handler(move |req| {
                Self::handle_ipc(&proxy_tb, req.body());
            })
            .with_bounds(Rect {
                position: LogicalPosition::new(0, 0).into(),
                size: LogicalSize::new(win_w, TITLEBAR_HEIGHT).into(),
            })
            .build_as_child(&window)?;
        let _ = titlebar_shell.set_visible(true);

        let proxy_rsz = proxy.clone();
        let dock_resizer_html = inject_ui_html(DOCK_RESIZER_HTML, &init_theme);
        let dock_resizer_shell = WebViewBuilder::new_with_web_context(&mut web_context)
            .with_environment(shared_env.clone())
            .with_profile_name("default".to_string())
            .with_theme(Theme::Light)
            .with_background_color(init_bg)
            .with_visible(false)
            .with_html(dock_resizer_html)
            .with_ipc_handler(move |req| {
                Self::handle_ipc(&proxy_rsz, req.body());
            })
            .with_bounds(Rect {
                position: LogicalPosition::new(900, 0).into(),
                size: LogicalSize::new(6, 800).into(),
            })
            .build_as_child(&window)?;
        let _ = dock_resizer_shell.set_visible(false);

        let proxy_sbr = proxy.clone();
        let sidebar_resizer_html = inject_ui_html(SIDEBAR_RESIZER_HTML, &init_theme);
        let sidebar_resizer_shell = WebViewBuilder::new_with_web_context(&mut web_context)
            .with_environment(shared_env.clone())
            .with_profile_name("default".to_string())
            .with_theme(Theme::Light)
            .with_background_color(init_bg)
            .with_visible(false)
            .with_html(sidebar_resizer_html)
            .with_ipc_handler(move |req| {
                Self::handle_ipc(&proxy_sbr, req.body());
            })
            .with_bounds(Rect {
                position: LogicalPosition::new(225, 40).into(),
                size: LogicalSize::new(6, 800).into(),
            })
            .build_as_child(&window)?;
        let _ = sidebar_resizer_shell.set_visible(false);

        let proxy_st = proxy.clone();
        let settings_modal_html = inject_ui_html(SETTINGS_MODAL_HTML, &init_theme);
        let settings_shell = WebViewBuilder::new_with_web_context(&mut web_context)
            .with_environment(shared_env)
            .with_profile_name("default".to_string())
            .with_transparent(true)
            .with_theme(Theme::Light)
            .with_html(settings_modal_html)
            .with_ipc_handler(move |req| {
                Self::handle_ipc(&proxy_st, req.body());
            })
            .with_bounds(Rect {
                position: LogicalPosition::new(0, 0).into(),
                size: LogicalSize::new(win_w, win_h).into(),
            })
            .build_as_child(&window)?;
        let _ = settings_shell.set_visible(false);

        let _ = ai_shell.set_memory_usage_level(MemoryUsageLevel::Normal);

        let default_bookmarks = r#"[{"title":"Google Sheets","url":"https://docs.google.com/spreadsheets"},{"title":"Gmail Cá Nhân","url":"https://mail.google.com"},{"title":"Google Drive","url":"https://drive.google.com"},{"title":"FB Messenger","url":"https://www.messenger.com"},{"title":"Zalo Web","url":"https://chat.zalo.me"},{"title":"YouTube","url":"https://www.youtube.com"},{"title":"Google Gemini","url":"https://gemini.google.com"},{"title":"Tài Khoản Google","url":"https://myaccount.google.com"}]"#.to_string();
        let default_skills = r#"[{"name":"Kiểm tra & Tóm tắt nhanh trang","domain":"*","notes":"Đọc tiêu đề, các mục chính và số liệu quan trọng trên trang đang mở, trình bày ngắn gọn gạch đầu dòng.","steps":[]},{"name":"Chia đôi màn hình so sánh 2 Tab","domain":"*","notes":"Chuyển sang chế độ chia khung Xoắn ốc BSP để đối chiếu 2 trang web cạnh nhau.","steps":[{"type":"split_view"}]}]"#.to_string();
        let default_floating_orbs = r##"[{"id":"ai","label":"AI","title":"AI Cowork","color":"#E2BE68","kind":"channel","channel":"ai","url":"","enabled":true},{"id":"fb","label":"FB","title":"Facebook Messenger","color":"#38BDF8","kind":"channel","channel":"fb","url":"https://www.messenger.com","enabled":true},{"id":"zalo","label":"Zalo","title":"Zalo Web","color":"#60A5FA","kind":"channel","channel":"zalo","url":"https://chat.zalo.me","enabled":true}]"##.to_string();

        let mut app = Self {
            window,
            proxy,
            web_context,
            exe_dir,
            titlebar_shell,
            tab_shell,
            sidebar_resizer_shell,
            chat_header_shell,
            dock_resizer_shell,
            settings_shell,
            ai_shell,
            fb_shell: None,
            zalo_shell: None,
            custom_popup_shell: None,
            custom_popup_id: String::new(),
            custom_popup_title: String::new(),
            custom_popup_url: String::new(),
            chat_channel: ChatChannel::Ai,
            tabs: Vec::new(),
            active_idx: 0,
            tab_orientation: TabOrientation::VerticalLeft,
            sidebar_collapsed: false,
            titlebar_hover: false,
            titlebar_pinned: true,
            settings_open: false,
            settings_section: "look_feel".to_string(),
            should_exit: false,
            ai_dock_width: RIGHT_AI_DOCK_WIDTH,
            unread_ai: 0,
            unread_fb: 0,
            unread_zalo: 0,
            ai_finished_flash: false,
            chime_seq: 0,
            chime_kind: "ai_done".to_string(),
            zoom_pct: 100,
            closed_tabs: Vec::new(),
            history_json: "[]".to_string(),
            view_mode: ContentViewMode::SingleTab,
            ai_mode: AiDisplayMode::FbPopup,
            ai_working: false,
            ai_recording: false,
            ai_status_text: "🤖 AI Cowork sẵn sàng".to_string(),
            ai_last_reply: String::new(),
            master_ratio: 0.54,
            profile_counter: 1,
            floating_omnibox: true,
            compact_titlebar: false,
            sidebar_width: 228,
            show_unread_badges: true,
            always_show_floating_orbs: true,
            floating_orbs_json: default_floating_orbs,
            adblock_enabled: true,
            ai_provider: "openai".to_string(),
            ai_api_key: String::new(),
            ai_model: "gemini-3.8-flash".to_string(),
            ai_base_url: "http://127.0.0.1:8045/v1".to_string(),
            auto_minimize_on_task: true,
            google_user: String::new(),
            ui_theme: "obsidian-gold".to_string(),
            logo_style: "ribbon".to_string(),
            startup_mode: "restore".to_string(),
            #[cfg(target_os = "windows")]
            hicon_big: 0,
            #[cfg(target_os = "windows")]
            hicon_small: 0,
            bookmarks_json: default_bookmarks,
            skills_json: default_skills,
            tasks_json: "[]".to_string(),
            ai_worker_tab_id: None,
            shopee_orders_json: String::new(),
            shopee_pas_json: String::new(),
            tiktok_json: String::new(),
            ecom_auto_scan: false,
            ecom_status_text: "Sẵn sàng — bấm \"Quét Tất Cả Ngầm\" để bắt đầu".to_string(),
            ecom_logs: vec!["• Khởi tạo Không Gian Làm Việc TMĐT TPbrowser.".to_string()],
            last_ecom_auto_epoch: 0,
            next_mcp_req_id: 1,
            mcp_pending: HashMap::new(),
        };

        app.load_config();
        app.compact_titlebar = false;
        app.titlebar_pinned = true;
        app.ui_theme = Self::normalize_theme(&app.ui_theme);
        app.logo_style = Self::normalize_logo_style(&app.logo_style);
        app.sync_window_titlebar_theme();
        app.sync_window_icon();
        app.load_skills();
        app.load_tasks();
        app.load_history();
        app.load_exports_cache();
        app.load_session_or_default();
        let preload_proxy = app.proxy.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(250));
            let _ = preload_proxy.send_event(AppCmd::UiShellReady("boot_settle".to_string()));
            std::thread::sleep(std::time::Duration::from_millis(950));
            let _ = preload_proxy.send_event(AppCmd::PreloadChatChannels);
        });
        Ok(app)
    }

    fn ensure_fb_shell(&mut self) {
        if self.fb_shell.is_some() {
            return;
        }
        let shared_env = self.tab_shell.environment();
        let proxy_ipc = self.proxy.clone();
        let fb_watch_js = format!(
            "{}\n{}",
            SHELL_SHORTCUTS_JS,
            CHANNEL_UNREAD_WATCHER_JS.replace("__CH__", "fb")
        );
        if let Ok(wv) = WebViewBuilder::new_with_web_context(&mut self.web_context)
            .with_environment(shared_env)
            .with_profile_name("default".to_string())
            .with_user_agent(CHROME_USER_AGENT)
            .with_autoplay(true)
            .with_clipboard(true)
            .with_theme(Theme::Light)
            .with_visible(false)
            .with_initialization_script(&fb_watch_js)
            .with_url("https://www.messenger.com")
            .with_ipc_handler(move |req| {
                Self::handle_ipc(&proxy_ipc, req.body());
            })
            .with_bounds(Rect {
                position: LogicalPosition::new(980, 360).into(),
                size: LogicalSize::new(392, 480).into(),
            })
            .build_as_child(&self.window)
        {
            let _ = wv.set_memory_usage_level(MemoryUsageLevel::Normal);
            self.fb_shell = Some(wv);
        }
    }

    fn ensure_zalo_shell(&mut self) {
        if self.zalo_shell.is_some() {
            return;
        }
        let shared_env = self.tab_shell.environment();
        let proxy_ipc = self.proxy.clone();
        let zalo_watch_js = format!(
            "{}\n{}",
            SHELL_SHORTCUTS_JS,
            CHANNEL_UNREAD_WATCHER_JS.replace("__CH__", "zalo")
        );
        if let Ok(wv) = WebViewBuilder::new_with_web_context(&mut self.web_context)
            .with_environment(shared_env)
            .with_profile_name("default".to_string())
            .with_user_agent(CHROME_USER_AGENT)
            .with_autoplay(true)
            .with_clipboard(true)
            .with_theme(Theme::Light)
            .with_visible(false)
            .with_initialization_script(&zalo_watch_js)
            .with_url("https://chat.zalo.me")
            .with_ipc_handler(move |req| {
                Self::handle_ipc(&proxy_ipc, req.body());
            })
            .with_bounds(Rect {
                position: LogicalPosition::new(980, 360).into(),
                size: LogicalSize::new(392, 480).into(),
            })
            .build_as_child(&self.window)
        {
            let _ = wv.set_memory_usage_level(MemoryUsageLevel::Normal);
            self.zalo_shell = Some(wv);
        }
    }

    fn ensure_custom_popup_shell(&mut self, url: &str) {
        if let Some(wv) = &self.custom_popup_shell {
            if self.custom_popup_url != url && !url.is_empty() {
                let _ = wv.load_url(url);
                self.custom_popup_url = url.to_string();
            }
            return;
        }
        let shared_env = self.tab_shell.environment();
        let proxy_ipc = self.proxy.clone();
        let target_url = if url.is_empty() { "https://www.google.com" } else { url };
        if let Ok(wv) = WebViewBuilder::new_with_web_context(&mut self.web_context)
            .with_environment(shared_env)
            .with_profile_name("default".to_string())
            .with_user_agent(CHROME_USER_AGENT)
            .with_autoplay(true)
            .with_clipboard(true)
            .with_theme(Theme::Light)
            .with_visible(false)
            .with_initialization_script(SHELL_SHORTCUTS_JS)
            .with_url(target_url)
            .with_ipc_handler(move |req| {
                Self::handle_ipc(&proxy_ipc, req.body());
            })
            .with_bounds(Rect {
                position: LogicalPosition::new(980, 360).into(),
                size: LogicalSize::new(392, 480).into(),
            })
            .build_as_child(&self.window)
        {
            let _ = wv.set_memory_usage_level(MemoryUsageLevel::Normal);
            self.custom_popup_url = target_url.to_string();
            self.custom_popup_shell = Some(wv);
        }
    }

    fn normalize_url(input: &str) -> String {
        let trimmed = input.trim();
        if trimmed.is_empty()
            || trimmed.eq_ignore_ascii_case("tp://newtab")
            || trimmed.eq_ignore_ascii_case("about:blank")
        {
            return "tp://newtab".to_string();
        }
        if trimmed.eq_ignore_ascii_case("tp://history") {
            return "tp://history".to_string();
        }
        if trimmed.eq_ignore_ascii_case("tp://workspace") {
            return "tp://workspace".to_string();
        }
        if trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
            || trimmed.starts_with("chrome-extension://")
            || trimmed.starts_with("edge://")
        {
            return trimmed.to_string();
        }
        if trimmed.contains('.') && !trimmed.contains(' ') {
            return format!("https://{}", trimmed);
        }
        let encoded: String = trimmed
            .bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                    (b as char).to_string()
                }
                b' ' => "+".to_string(),
                _ => format!("%{:02X}", b),
            })
            .collect();
        format!("https://www.google.com/search?q={}", encoded)
    }

    fn title_from_url(url: &str) -> String {
        if url == "tp://newtab" {
            return "Không gian bắt đầu".to_string();
        }
        if url == "tp://history" {
            return "Lịch sử duyệt web".to_string();
        }
        if url == "tp://workspace" {
            return "Không gian TMĐT".to_string();
        }
        if url.starts_with("chrome-extension://") && url.contains("dashboard.html") {
            return "🛡️ uBlock Origin Dashboard".to_string();
        }
        let clean = url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("www.");
        clean.split('/').next().unwrap_or(url).to_string()
    }

    fn add_tab(&mut self, raw_url: &str, isolated: bool) {
        self.add_tab_internal(raw_url, isolated, true, None, None);
    }

    fn add_worker_tab(&mut self, raw_url: &str, worker_kind: &str, custom_title: Option<&str>) -> Option<u64> {
        self.add_tab_internal(
            raw_url,
            false,
            true,
            Some(worker_kind.to_string()),
            custom_title.map(|s| s.to_string()),
        )
    }

    fn add_tab_internal(
        &mut self,
        raw_url: &str,
        isolated: bool,
        sync_now: bool,
        worker_kind: Option<String>,
        custom_title: Option<String>,
    ) -> Option<u64> {
        if self.tabs.len() >= 14 {
            return None;
        }
        let tab_id = NEXT_TAB_ID.fetch_add(1, Ordering::SeqCst);
        let profile_name = if isolated {
            self.profile_counter += 1;
            format!("acc_{}", self.profile_counter)
        } else {
            "default".to_string()
        };

        let url = Self::normalize_url(raw_url);
        let is_tiktok = url.to_lowercase().contains("tiktok.com");
        let is_worker = worker_kind.is_some();

        let proxy_ipc = self.proxy.clone();
        let proxy_popup = self.proxy.clone();
        let proxy_load = self.proxy.clone();
        let shared_env = self.tab_shell.environment();

        let extensions_dir = self.exe_dir.join("extensions");
        let mut builder = WebViewBuilder::new_with_web_context(&mut self.web_context)
            .with_environment(shared_env)
            .with_browser_extensions_enabled(true)
            .with_autoplay(true)
            .with_clipboard(true)
            .with_devtools(true)
            .with_theme(Theme::Light)
            .with_visible(sync_now && !is_worker)
            .with_background_color(if self.ui_theme == "carrara-light" {
                (246, 242, 234, 255)
            } else {
                (14, 16, 20, 255)
            })
            .with_profile_name(profile_name.clone());

        if !is_tiktok {
            let init_js = format!(
                "{}\n{}",
                CONTENT_AGENT_JS.replace("__TAB_ID__", &tab_id.to_string()),
                ECOM_AGENTS_JS
            );
            builder = builder.with_initialization_script(&init_js);
        }

        let mut builder = builder
            .with_on_page_load_handler(move |event, loaded_url| {
                if matches!(event, PageLoadEvent::Finished) {
                    let _ = proxy_load.send_event(AppCmd::PageFinished {
                        tab_id,
                        url: loaded_url,
                    });
                }
            })
            .with_permission_handler(|kind| match kind {
                PermissionKind::Microphone
                | PermissionKind::Camera
                | PermissionKind::ClipboardRead
                | PermissionKind::Notifications
                | PermissionKind::Autoplay
                | PermissionKind::WindowManagement => PermissionResponse::Allow,
                _ => PermissionResponse::Default,
            })
            .with_new_window_req_handler(move |popup_url, features| {
                let lower = popup_url.to_lowercase();
                // Chặn đứng mọi cửa sổ/tab quảng cáo, cá cược, popunder tự nhảy ra
                let is_ad_or_popunder = lower.contains("chinhhang")
                    || lower.contains("adqc.")
                    || lower.contains("popunder")
                    || lower.contains("doubleclick")
                    || lower.contains("googlesyndication")
                    || lower.contains("adservice")
                    || lower.contains("popads")
                    || lower.contains("hitclub")
                    || lower.contains("sunwin")
                    || lower.contains("rikvip")
                    || lower.contains("gemwin")
                    || lower.contains("win79")
                    || lower.contains("yo88")
                    || lower.contains("uk88")
                    || lower.contains("bom88")
                    || lower.contains("tic88")
                    || lower.contains("haywin")
                    || lower.contains("zowin")
                    || lower.contains("zo88");

                if ADBLOCK_ENABLED.load(Ordering::Relaxed) && is_ad_or_popunder {
                    return NewWindowResponse::Deny;
                }

                let is_call_or_auth = features.size.is_some()
                    || popup_url.is_empty()
                    || popup_url == "about:blank"
                    || popup_url.contains("messenger.com")
                    || popup_url.contains("facebook.com")
                    || popup_url.contains("accounts.google.com")
                    || popup_url.contains("oauth")
                    || popup_url.contains("login");

                if is_call_or_auth {
                    NewWindowResponse::Allow
                } else {
                    let _ = proxy_popup.send_event(AppCmd::NewTab {
                        url: popup_url,
                        isolated: false,
                    });
                    NewWindowResponse::Deny
                }
            })
            .with_ipc_handler(move |req| {
                Self::handle_ipc(&proxy_ipc, req.body());
            })
            .with_bounds(Rect {
                position: LogicalPosition::new(228, 0).into(),
                size: LogicalSize::new(800, 800).into(),
            });

        if isolated {
            builder = builder.with_extensions_path(extensions_dir);
        }

        let is_ext_url = url.starts_with("chrome-extension://");
        builder = if url == "tp://newtab" {
            builder.with_html(inject_ui_html(NEWTAB_HTML, &self.ui_theme))
        } else if url == "tp://history" {
            builder.with_html(inject_ui_html(HISTORY_HTML, &self.ui_theme))
        } else if url == "tp://workspace" {
            builder.with_html(inject_ui_html(WORKSPACE_HTML, &self.ui_theme))
        } else if is_ext_url {
            builder.with_html("<html><body style='background:#f8fafc;'></body></html>")
        } else {
            builder.with_url(&url)
        };

        if let Ok(webview) = builder.build_as_child(&self.window) {
            if is_ext_url {
                let ext_target = url.clone();
                let proxy_delayed = self.proxy.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(650));
                    let _ = proxy_delayed.send_event(AppCmd::NavigateActive(ext_target));
                });
            }
            let title = custom_title.unwrap_or_else(|| Self::title_from_url(&url));
            self.tabs.push(BrowserTab {
                id: tab_id,
                profile_name,
                url,
                title,
                favicon_url: String::new(),
                worker_kind,
                worker_status: if is_worker { "Đang chạy ngầm".to_string() } else { String::new() },
                webview,
            });
            if !is_worker {
                self.active_idx = self.tabs.len() - 1;
            }
            if sync_now {
                self.relayout();
                self.sync_all_ui();
                self.save_session();
            }
            return Some(tab_id);
        }
        None
    }

    fn relayout(&self) {
        let scale = self.window.scale_factor();
        let logical = self.window.inner_size().to_logical::<u32>(scale);
        let top_reserved = TITLEBAR_HEIGHT;
        let mut shell = compute_shell_layout(
            logical.width,
            logical.height.saturating_sub(top_reserved),
            self.tab_orientation,
            self.sidebar_collapsed,
            self.sidebar_width,
            self.ai_mode,
            self.ai_dock_width,
        );
        shell.tab_bar.y += top_reserved as i32;
        shell.content_area.y += top_reserved as i32;
        if let Some(ref mut dock) = shell.ai_dock {
            dock.y += top_reserved as i32;
        }

        if self.tab_orientation == TabOrientation::VerticalLeft {
            let _ = self.tab_shell.set_visible(true);
            let _ = self.tab_shell.set_bounds(Rect {
                position: LogicalPosition::new(shell.tab_bar.x, shell.tab_bar.y).into(),
                size: LogicalSize::new(shell.tab_bar.width, shell.tab_bar.height).into(),
            });
            if !self.sidebar_collapsed && shell.tab_bar.width > 0 {
                let _ = self.sidebar_resizer_shell.set_visible(true);
                let _ = self.sidebar_resizer_shell.set_bounds(Rect {
                    position: LogicalPosition::new(
                        (shell.tab_bar.x + shell.tab_bar.width as i32 - 3).max(0),
                        shell.tab_bar.y,
                    )
                    .into(),
                    size: LogicalSize::new(6, shell.tab_bar.height.max(120)).into(),
                });
                #[cfg(target_os = "windows")]
                Self::bring_webview_to_front(&self.sidebar_resizer_shell);
            } else {
                let _ = self.sidebar_resizer_shell.set_visible(false);
            }
        } else {
            let _ = self.tab_shell.set_visible(false);
            let _ = self.sidebar_resizer_shell.set_visible(false);
        }

        let total = self.tabs.len();
        if total > 0 {
            if self.view_mode == ContentViewMode::SingleTab || total == 1 {
                if let Some(active_tab) = self.tabs.get(self.active_idx) {
                    let _ = active_tab.webview.set_visible(true);
                    let _ = active_tab.webview.set_bounds(Rect {
                        position: LogicalPosition::new(
                            shell.content_area.x,
                            shell.content_area.y,
                        )
                        .into(),
                        size: LogicalSize::new(
                            shell.content_area.width,
                            shell.content_area.height,
                        )
                        .into(),
                    });
                    let _ = active_tab
                        .webview
                        .set_memory_usage_level(MemoryUsageLevel::Normal);
                    #[cfg(target_os = "windows")]
                    Self::send_webview_to_back(&active_tab.webview);
                }
                for (idx, tab) in self.tabs.iter().enumerate() {
                    if idx == self.active_idx {
                        continue;
                    }
                    if tab.worker_kind.is_some() {
                        // Giữ Worker Tab sống ở kích thước đầy đủ lớp dưới (HWND_BOTTOM sau tab đang xem) để SPA/API chạy mượt mà không chiếm màn hình hay chuột
                        let _ = tab.webview.set_visible(true);
                        let _ = tab.webview.set_bounds(Rect {
                            position: LogicalPosition::new(
                                shell.content_area.x,
                                shell.content_area.y,
                            )
                            .into(),
                            size: LogicalSize::new(
                                shell.content_area.width,
                                shell.content_area.height,
                            )
                            .into(),
                        });
                        let _ = tab.webview.set_memory_usage_level(MemoryUsageLevel::Normal);
                        #[cfg(target_os = "windows")]
                        Self::send_webview_to_back(&tab.webview);
                    } else {
                        let _ = tab.webview.set_visible(false);
                        let _ = tab.webview.set_memory_usage_level(MemoryUsageLevel::Low);
                    }
                }
            } else {
                let rects = compute_content_panes(
                    shell.content_area,
                    self.view_mode,
                    total,
                    self.master_ratio,
                    4,
                );
                for (idx, tab) in self.tabs.iter().enumerate() {
                    if let Some(r) = rects.get(idx) {
                        let _ = tab.webview.set_visible(true);
                        let _ = tab.webview.set_bounds(Rect {
                            position: LogicalPosition::new(r.x, r.y).into(),
                            size: LogicalSize::new(r.width, r.height).into(),
                        });
                        let _ = tab.webview.set_memory_usage_level(if idx == self.active_idx {
                            MemoryUsageLevel::Normal
                        } else {
                            MemoryUsageLevel::Low
                        });
                        #[cfg(target_os = "windows")]
                        Self::send_webview_to_back(&tab.webview);
                    }
                }
            }
        }

        // Bố trí Cửa sổ Chat: AI Cowork bỏ hẳn thanh trên (0px); Messenger / Zalo dùng thanh mini icon 28px
        let need_chat_header = self.chat_channel != ChatChannel::Ai;
        let chat_hdr_h = if need_chat_header {
            FB_CHAT_HEADER_HEIGHT
        } else {
            0
        };
        let chat_rects = match self.ai_mode {
            AiDisplayMode::RightDock => shell.ai_dock.map(|r| {
                let hdr_h = chat_hdr_h.min(r.height / 2);
                (
                    layout::PaneRect {
                        x: r.x,
                        y: r.y,
                        width: r.width,
                        height: hdr_h,
                    },
                    layout::PaneRect {
                        x: r.x,
                        y: r.y + hdr_h as i32,
                        width: r.width,
                        height: r.height.saturating_sub(hdr_h),
                    },
                )
            }),
            AiDisplayMode::FbPopup => Some(compute_fb_chat_popup_rect_with_hdr(
                logical.width,
                logical.height,
                chat_hdr_h,
            )),
            AiDisplayMode::BubbleMinimized => None,
        };

        if let Some((hdr_r, body_r)) = chat_rects {
            if need_chat_header && hdr_r.height > 0 {
                let _ = self.chat_header_shell.set_visible(true);
                let _ = self.chat_header_shell.set_bounds(Rect {
                    position: LogicalPosition::new(hdr_r.x, hdr_r.y).into(),
                    size: LogicalSize::new(hdr_r.width, hdr_r.height).into(),
                });
                #[cfg(target_os = "windows")]
                Self::bring_webview_to_front(&self.chat_header_shell);
            } else {
                let _ = self.chat_header_shell.set_visible(false);
            }

            let body_rect = Rect {
                position: LogicalPosition::new(body_r.x, body_r.y).into(),
                size: LogicalSize::new(body_r.width, body_r.height).into(),
            };

            match self.chat_channel {
                ChatChannel::Ai => {
                    let _ = self.ai_shell.set_visible(true);
                    let _ = self.ai_shell.set_bounds(body_rect);
                    #[cfg(target_os = "windows")]
                    Self::bring_webview_to_front(&self.ai_shell);
                    if let Some(fb) = &self.fb_shell {
                        let _ = fb.set_visible(false);
                    }
                    if let Some(zl) = &self.zalo_shell {
                        let _ = zl.set_visible(false);
                    }
                    if let Some(cs) = &self.custom_popup_shell {
                        let _ = cs.set_visible(false);
                    }
                }
                ChatChannel::Messenger => {
                    let _ = self.ai_shell.set_visible(false);
                    if let Some(zl) = &self.zalo_shell {
                        let _ = zl.set_visible(false);
                    }
                    if let Some(cs) = &self.custom_popup_shell {
                        let _ = cs.set_visible(false);
                    }
                    if let Some(fb) = &self.fb_shell {
                        let _ = fb.set_visible(true);
                        let _ = fb.set_bounds(body_rect);
                        #[cfg(target_os = "windows")]
                        Self::bring_webview_to_front(fb);
                    }
                }
                ChatChannel::Zalo => {
                    let _ = self.ai_shell.set_visible(false);
                    if let Some(fb) = &self.fb_shell {
                        let _ = fb.set_visible(false);
                    }
                    if let Some(cs) = &self.custom_popup_shell {
                        let _ = cs.set_visible(false);
                    }
                    if let Some(zl) = &self.zalo_shell {
                        let _ = zl.set_visible(true);
                        let _ = zl.set_bounds(body_rect);
                        #[cfg(target_os = "windows")]
                        Self::bring_webview_to_front(zl);
                    }
                }
                ChatChannel::Custom => {
                    let _ = self.ai_shell.set_visible(false);
                    if let Some(fb) = &self.fb_shell {
                        let _ = fb.set_visible(false);
                    }
                    if let Some(zl) = &self.zalo_shell {
                        let _ = zl.set_visible(false);
                    }
                    if let Some(cs) = &self.custom_popup_shell {
                        let _ = cs.set_visible(true);
                        let _ = cs.set_bounds(body_rect);
                        #[cfg(target_os = "windows")]
                        Self::bring_webview_to_front(cs);
                    }
                }
            }
        } else {
            let _ = self.chat_header_shell.set_visible(false);
            let _ = self.ai_shell.set_visible(false);
            let _ = self
                .ai_shell
                .set_memory_usage_level(MemoryUsageLevel::Normal);
            if let Some(fb) = &self.fb_shell {
                let _ = fb.set_visible(false);
            }
            if let Some(zl) = &self.zalo_shell {
                let _ = zl.set_visible(false);
            }
            if let Some(cs) = &self.custom_popup_shell {
                let _ = cs.set_visible(false);
            }
        }

        if self.ai_mode == AiDisplayMode::RightDock {
            if let Some(dock_r) = shell.ai_dock {
                let _ = self.dock_resizer_shell.set_visible(true);
                let _ = self.dock_resizer_shell.set_bounds(Rect {
                    position: LogicalPosition::new((dock_r.x - 3).max(0), dock_r.y).into(),
                    size: LogicalSize::new(6, dock_r.height.max(120)).into(),
                });
                #[cfg(target_os = "windows")]
                Self::bring_webview_to_front(&self.dock_resizer_shell);
            } else {
                let _ = self.dock_resizer_shell.set_visible(false);
            }
        } else {
            let _ = self.dock_resizer_shell.set_visible(false);
        }

        let _ = self.titlebar_shell.set_visible(true);
        let _ = self.titlebar_shell.set_bounds(Rect {
            position: LogicalPosition::new(0, 0).into(),
            size: LogicalSize::new(logical.width.max(320), TITLEBAR_HEIGHT).into(),
        });
        #[cfg(target_os = "windows")]
        Self::bring_webview_to_front(&self.titlebar_shell);

        if self.settings_open {
            let _ = self.settings_shell.set_visible(true);
            let _ = self.settings_shell.set_bounds(Rect {
                position: LogicalPosition::new(0, 0).into(),
                size: LogicalSize::new(logical.width.max(320), logical.height.max(240)).into(),
            });
            #[cfg(target_os = "windows")]
            Self::bring_webview_to_front(&self.settings_shell);
        } else {
            let _ = self.settings_shell.set_visible(false);
        }
    }

    fn build_config_payload_json(&self, open_section: Option<&str>) -> String {
        let sec = open_section.unwrap_or("");
        let orbs_raw = if self.floating_orbs_json.trim().is_empty() {
            "[]"
        } else {
            &self.floating_orbs_json
        };
        format!(
            "{{\"ui_theme\":\"{}\",\"logo_style\":\"{}\",\"startup_mode\":\"{}\",\"floating_omnibox\":{},\"compact_titlebar\":{},\"sidebar_width\":{},\"tab_orientation\":\"{}\",\"sidebar_collapsed\":{},\"view_mode\":\"{}\",\"ai_provider\":\"{}\",\"ai_api_key\":\"{}\",\"ai_model\":\"{}\",\"ai_base_url\":\"{}\",\"auto_minimize_on_task\":{},\"ai_mode\":\"{}\",\"show_unread_badges\":{},\"always_show_floating_orbs\":{},\"floating_orbs\":{},\"ai_dock_width\":{},\"adblock_enabled\":{},\"google_user\":\"{}\",\"chat_channel\":\"{}\",\"unread_ai\":{},\"unread_fb\":{},\"unread_zalo\":{},\"open_section\":\"{}\"}}",
            Self::escape_json(&self.ui_theme),
            Self::escape_json(&self.logo_style),
            Self::escape_json(&self.startup_mode),
            self.floating_omnibox,
            self.compact_titlebar,
            self.sidebar_width,
            self.tab_orientation.as_str(),
            self.sidebar_collapsed,
            self.view_mode.as_str(),
            Self::escape_json(&self.ai_provider),
            Self::escape_json(&self.ai_api_key),
            Self::escape_json(&self.ai_model),
            Self::escape_json(&self.ai_base_url),
            self.auto_minimize_on_task,
            self.ai_mode.as_str(),
            self.show_unread_badges,
            self.always_show_floating_orbs,
            orbs_raw,
            self.ai_dock_width,
            self.adblock_enabled,
            Self::escape_json(&self.google_user),
            self.chat_channel.as_str(),
            self.unread_ai,
            self.unread_fb,
            self.unread_zalo,
            Self::escape_json(sec)
        )
    }

    fn sync_settings_modal(&self, open_section: Option<&str>) {
        let payload = self.build_config_payload_json(open_section);
        let _ = self.settings_shell.evaluate_script(&format!(
            "if (window.__tpSyncSettingsModal) window.__tpSyncSettingsModal({});",
            payload
        ));
    }

    fn broadcast_config_changed(&self) {
        let payload = self.build_config_payload_json(None);
        let ev_js = format!(
            "window.dispatchEvent(new CustomEvent('tp_config_changed', {{ detail: {} }}));",
            payload
        );
        let _ = self.tab_shell.evaluate_script(&ev_js);
        let _ = self.chat_header_shell.evaluate_script(&ev_js);
        let _ = self.ai_shell.evaluate_script(&ev_js);
        let _ = self.titlebar_shell.evaluate_script(&ev_js);
        let _ = self.settings_shell.evaluate_script(&ev_js);
        for t in &self.tabs {
            let _ = t.webview.evaluate_script(&ev_js);
        }
    }

    fn sync_all_ui(&self) {
        let mut tabs_json = Vec::new();
        for t in &self.tabs {
            tabs_json.push(format!(
                "{{\"id\":{},\"url\":\"{}\",\"title\":\"{}\",\"favicon\":\"{}\",\"profile\":\"{}\",\"worker_kind\":\"{}\"}}",
                t.id,
                Self::escape_json(&t.url),
                Self::escape_json(&t.title),
                Self::escape_json(&t.favicon_url),
                Self::escape_json(&t.profile_name),
                Self::escape_json(t.worker_kind.as_deref().unwrap_or(""))
            ));
        }
        let tabs_arr_str = tabs_json.join(",");

        // -1. Đồng bộ Thanh Trên Hợp Nhất (Unified Top Bar)
        let active_title = self
            .tabs
            .get(self.active_idx)
            .map(|t| t.title.as_str())
            .unwrap_or("Không gian bắt đầu");
        let active_url = self
            .tabs
            .get(self.active_idx)
            .map(|t| t.url.as_str())
            .unwrap_or("tp://newtab");
        let is_bookmarked = !active_url.starts_with("tp://") && self.bookmarks_json.contains(active_url);
        let tb_title = format!("TPbrowser — {}", active_title);
        let tb_state = format!(
            "{{\"ui_theme\":\"{}\",\"logo_style\":\"{}\",\"orientation\":\"{}\",\"active_idx\":{},\"tabs\":[{}],\"title\":\"{}\",\"active_url\":\"{}\",\"is_bookmarked\":{},\"view_mode\":\"{}\",\"zoom_pct\":{},\"pinned\":{},\"maximized\":{}}}",
            Self::escape_json(&self.ui_theme),
            Self::escape_json(&self.logo_style),
            self.tab_orientation.as_str(),
            self.active_idx,
            tabs_arr_str,
            Self::escape_json(&tb_title),
            Self::escape_json(active_url),
            is_bookmarked,
            self.view_mode.as_str(),
            self.zoom_pct,
            self.titlebar_pinned,
            self.window.is_maximized()
        );
        let _ = self.titlebar_shell.evaluate_script(&format!(
            "if (window.__tpSyncTitlebar) window.__tpSyncTitlebar({});",
            tb_state
        ));

        // 0. Đồng bộ Thanh Tiêu Đề Cửa Sổ Chat & Thanh Kéo Độ Rộng
        let custom_title = if !self.custom_popup_title.is_empty() {
            self.custom_popup_title.clone()
        } else if self.custom_popup_url.is_empty() {
            "App".to_string()
        } else {
            Self::title_from_url(&self.custom_popup_url)
        };
        let ch_state = format!(
            "{{\"channel\":\"{}\",\"custom_title\":\"{}\",\"ai_mode\":\"{}\",\"ui_theme\":\"{}\",\"show_unread_badges\":{},\"unread_ai\":{},\"unread_fb\":{},\"unread_zalo\":{}}}",
            self.chat_channel.as_str(),
            Self::escape_json(&custom_title),
            self.ai_mode.as_str(),
            Self::escape_json(&self.ui_theme),
            self.show_unread_badges,
            self.unread_ai,
            self.unread_fb,
            self.unread_zalo
        );
        let _ = self.chat_header_shell.evaluate_script(&format!(
            "if (window.__tpSyncChatHeader) window.__tpSyncChatHeader({});",
            ch_state
        ));
        let _ = self.dock_resizer_shell.evaluate_script(&format!(
            "if (window.__tpSyncDockResizer) window.__tpSyncDockResizer({{\"ui_theme\":\"{}\"}});",
            Self::escape_json(&self.ui_theme)
        ));
        let _ = self.sidebar_resizer_shell.evaluate_script(&format!(
            "if (window.__tpSyncSidebarResizer) window.__tpSyncSidebarResizer({{\"ui_theme\":\"{}\"}});",
            Self::escape_json(&self.ui_theme)
        ));

        // 1. Đồng bộ Thanh Quản Lý Tab (Dọc)
        let shell_state = format!(
            "{{\"orientation\":\"{}\",\"sidebar_collapsed\":{},\"view_mode\":\"{}\",\"ai_mode\":\"{}\",\"chat_channel\":\"{}\",\"ai_working\":{},\"active_idx\":{},\"google_user\":\"{}\",\"ui_theme\":\"{}\",\"logo_style\":\"{}\",\"adblock_enabled\":{},\"tabs\":[{}]}}",
            self.tab_orientation.as_str(),
            self.sidebar_collapsed,
            self.view_mode.as_str(),
            self.ai_mode.as_str(),
            self.chat_channel.as_str(),
            self.ai_working,
            self.active_idx,
            Self::escape_json(&self.google_user),
            Self::escape_json(&self.ui_theme),
            Self::escape_json(&self.logo_style),
            self.adblock_enabled,
            tabs_arr_str
        );
        let _ = self.tab_shell.evaluate_script(&format!(
            "if (window.__tpSyncShellState) window.__tpSyncShellState({});",
            shell_state
        ));

        // 2. Đồng bộ Cụm Bong Bóng Góc Phải (AI / FB / Zalo / Custom) & Trang Chủ trên mọi Tab
        let is_split = self.view_mode != ContentViewMode::SingleTab && self.tabs.len() > 1;
        let orbs_raw = if self.floating_orbs_json.trim().is_empty() {
            "[]"
        } else {
            &self.floating_orbs_json
        };
        for (idx, t) in self.tabs.iter().enumerate() {
            let show_ring = is_split && idx == self.active_idx;
            let bubble_state = format!(
                "{{\"ai_mode\":\"{}\",\"chat_channel\":\"{}\",\"ui_theme\":\"{}\",\"working\":{},\"recording\":{},\"status_text\":\"{}\",\"last_reply\":\"{}\",\"show_split_ring\":{},\"show_unread_badges\":{},\"always_show_floating_orbs\":{},\"floating_orbs\":{},\"adblock_enabled\":{},\"unread_ai\":{},\"unread_fb\":{},\"unread_zalo\":{},\"ai_finished_flash\":{},\"chime_seq\":{},\"chime_kind\":\"{}\"}}",
                self.ai_mode.as_str(),
                self.chat_channel.as_str(),
                Self::escape_json(&self.ui_theme),
                self.ai_working,
                self.ai_recording,
                Self::escape_json(&self.ai_status_text),
                Self::escape_json(&self.ai_last_reply),
                show_ring,
                self.show_unread_badges,
                self.always_show_floating_orbs,
                orbs_raw,
                self.adblock_enabled,
                self.unread_ai,
                self.unread_fb,
                self.unread_zalo,
                self.ai_finished_flash,
                self.chime_seq,
                Self::escape_json(&self.chime_kind)
            );
            let _ = t.webview.evaluate_script(&format!(
                "if (window.__tpUpdateBubbleUi) window.__tpUpdateBubbleUi({});",
                bubble_state
            ));

            if t.url == "tp://newtab" {
                let nt_cfg = format!(
                    "{{\"google_user\":\"{}\",\"ui_theme\":\"{}\",\"logo_style\":\"{}\",\"floating_omnibox\":{},\"adblock_enabled\":{},\"show_unread_badges\":{},\"ai_mode\":\"{}\",\"chat_channel\":\"{}\",\"unread_ai\":{},\"unread_fb\":{},\"unread_zalo\":{},\"bookmarks\":{}}}",
                    Self::escape_json(&self.google_user),
                    Self::escape_json(&self.ui_theme),
                    Self::escape_json(&self.logo_style),
                    self.floating_omnibox,
                    self.adblock_enabled,
                    self.show_unread_badges,
                    self.ai_mode.as_str(),
                    self.chat_channel.as_str(),
                    self.unread_ai,
                    self.unread_fb,
                    self.unread_zalo,
                    if self.bookmarks_json.trim().is_empty() {
                        "[]"
                    } else {
                        &self.bookmarks_json
                    }
                );
                let _ = t.webview.evaluate_script(&format!(
                    "if (window.__tpSyncNewTab) window.__tpSyncNewTab({});",
                    nt_cfg
                ));
            }
        }

        if self.settings_open {
            self.sync_settings_modal(None);
        }

        // 3. Cập nhật tiêu đề cửa sổ chính
        let tab_mode_lbl = match self.tab_orientation {
            TabOrientation::VerticalLeft => "Tab Dọc Trái",
            TabOrientation::HorizontalTop => "Tab Ngang Trên",
        };
        let ai_lbl = match self.ai_mode {
            AiDisplayMode::RightDock => "Chat Ngăn Phải",
            AiDisplayMode::FbPopup => "Cửa Sổ Chat Nổi",
            AiDisplayMode::BubbleMinimized => {
                if self.ai_recording {
                    "🔴 Đang Ghi Mẫu Skill"
                } else if self.ai_working {
                    "AI Bong Bóng (Đang chạy...)"
                } else {
                    "Chat Thu Gọn"
                }
            }
        };
        self.window.set_title(&format!(
            "TPbrowser [Tab {}/{} • {} • {}]",
            self.active_idx + 1,
            self.tabs.len().max(1),
            tab_mode_lbl,
            ai_lbl
        ));
    }

    fn sync_ai_config_to_panel(&self, open_drawer: bool) {
        let skills_raw = if self.skills_json.trim().is_empty() {
            "[]"
        } else {
            &self.skills_json
        };
        let cfg_js = format!(
            "if (window.__tpLoadAiConfigFromRust) window.__tpLoadAiConfigFromRust({{\"provider\":\"{}\",\"api_key\":\"{}\",\"model\":\"{}\",\"base_url\":\"{}\",\"auto_minimize_on_task\":{},\"open_drawer\":{},\"ui_theme\":\"{}\",\"skills\":{}}});",
            Self::escape_json(&self.ai_provider),
            Self::escape_json(&self.ai_api_key),
            Self::escape_json(&self.ai_model),
            Self::escape_json(&self.ai_base_url),
            self.auto_minimize_on_task,
            open_drawer,
            Self::escape_json(&self.ui_theme),
            skills_raw
        );
        let _ = self.ai_shell.evaluate_script(&cfg_js);
    }

    fn save_skills(&self) {
        let skills_dir = self.exe_dir.join("skills");
        let _ = fs::create_dir_all(&skills_dir);
        let path = skills_dir.join("skills.json");
        let _ = fs::write(path, &self.skills_json);
    }

    fn load_skills(&mut self) {
        let path = self.exe_dir.join("skills").join("skills.json");
        if let Ok(raw) = fs::read_to_string(path) {
            let trimmed = raw.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                self.skills_json = trimmed.to_string();
            }
        } else {
            self.save_skills();
        }
    }

    fn flash_window(&self) {
        #[cfg(target_os = "windows")]
        unsafe {
            let hwnd = self.window.hwnd() as isize;
            let mut fwi = FLASHWINFO {
                cb_size: std::mem::size_of::<FLASHWINFO>() as u32,
                hwnd,
                dw_flags: FLASHW_ALL | FLASHW_TIMERNOFG,
                u_count: 5,
                dw_timeout: 0,
            };
            FlashWindowEx(&mut fwi);
        }
    }

    fn save_tasks(&self) {
        let tasks_dir = self.exe_dir.join("tasks");
        let _ = fs::create_dir_all(&tasks_dir);
        let path = tasks_dir.join("tasks.json");
        let _ = fs::write(path, &self.tasks_json);
    }

    fn load_tasks(&mut self) {
        let path = self.exe_dir.join("tasks").join("tasks.json");
        if let Ok(raw) = fs::read_to_string(path) {
            let trimmed = raw.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                self.tasks_json = trimmed.to_string();
                return;
            }
        }
        self.tasks_json = "[]".to_string();
        self.save_tasks();
    }

    fn sync_tasks_to_views(&self) {
        let raw = if self.tasks_json.trim().is_empty() { "[]" } else { &self.tasks_json };
        let js = format!("if (window.__tpSyncTasks) window.__tpSyncTasks({});", raw);
        let _ = self.ai_shell.evaluate_script(&js);
        let _ = self.tab_shell.evaluate_script(&js);
        for tab in &self.tabs {
            let _ = tab.webview.evaluate_script(&js);
        }
    }

    fn broadcast_reminder_trigger(&self, task_json: &str) {
        self.flash_window();
        let js = format!(
            "if (window.__tpOnReminderTrigger) window.__tpOnReminderTrigger({});",
            task_json
        );
        let _ = self.tab_shell.evaluate_script(&js);
        let _ = self.ai_shell.evaluate_script(&js);
        for tab in &self.tabs {
            let _ = tab.webview.evaluate_script(&js);
        }
    }

    fn check_reminders(&mut self) {
        let now_secs = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_secs(),
            Err(_) => return,
        };
        if self.ecom_auto_scan && now_secs.saturating_sub(self.last_ecom_auto_epoch) >= 900 {
            self.last_ecom_auto_epoch = now_secs;
            self.execute(AppCmd::RunEcomScan {
                scope: "all".to_string(),
            });
        }
        let tasks_raw = self.tasks_json.clone();
        if !tasks_raw.starts_with('[') || !tasks_raw.ends_with(']') {
            return;
        }
        let mut triggered = Vec::new();
        let mut depth = 0;
        let mut start_idx = None;
        for (i, c) in tasks_raw.char_indices() {
            if c == '{' {
                if depth == 0 {
                    start_idx = Some(i);
                }
                depth += 1;
            } else if c == '}' {
                depth -= 1;
                if depth == 0 {
                    if let Some(start) = start_idx {
                        let obj_slice = &tasks_raw[start..=i];
                        let is_done = obj_slice.contains("\"status\":\"done\"") || obj_slice.contains("\"status\": \"done\"");
                        let is_reminded = obj_slice.contains("\"reminded\":true") || obj_slice.contains("\"reminded\": true");
                        let remind_epoch = Self::extract_u64(obj_slice, "\"remind_at_epoch\":").unwrap_or(0);
                        if !is_done && !is_reminded && remind_epoch > 0 && remind_epoch <= now_secs {
                            triggered.push(obj_slice.to_string());
                        }
                    }
                    start_idx = None;
                }
            }
        }

        if !triggered.is_empty() {
            for t in &triggered {
                if self.tasks_json.contains("\"reminded\":false") {
                    self.tasks_json = self.tasks_json.replacen("\"reminded\":false", "\"reminded\":true", 1);
                } else if self.tasks_json.contains("\"reminded\": false") {
                    self.tasks_json = self.tasks_json.replacen("\"reminded\": false", "\"reminded\":true", 1);
                }
                self.broadcast_reminder_trigger(t);
            }
            self.save_tasks();
            self.sync_tasks_to_views();
        }
    }

    fn save_config(&self) {
        let cfg_path = self.exe_dir.join("config.json");
        let orbs_raw = if self.floating_orbs_json.trim().is_empty() {
            "[]"
        } else {
            &self.floating_orbs_json
        };
        let body = format!(
            "{{\n  \"ui_theme\": \"{}\",\n  \"logo_style\": \"{}\",\n  \"startup_mode\": \"{}\",\n  \"floating_omnibox\": {},\n  \"auto_hide_titlebar\": {},\n  \"sidebar_width\": {},\n  \"tab_orientation\": \"{}\",\n  \"sidebar_collapsed\": {},\n  \"view_mode\": \"{}\",\n  \"ai_mode\": \"{}\",\n  \"ai_dock_width\": {},\n  \"show_unread_badges\": {},\n  \"always_show_floating_orbs\": {},\n  \"floating_orbs\": {},\n  \"adblock_enabled\": {},\n  \"ai_provider\": \"{}\",\n  \"ai_api_key\": \"{}\",\n  \"ai_model\": \"{}\",\n  \"ai_base_url\": \"{}\",\n  \"auto_minimize_on_task\": {},\n  \"google_user\": \"{}\",\n  \"bookmarks\": {}\n}}",
            Self::escape_json(&self.ui_theme),
            Self::escape_json(&self.logo_style),
            Self::escape_json(&self.startup_mode),
            self.floating_omnibox,
            self.compact_titlebar,
            self.sidebar_width,
            self.tab_orientation.as_str(),
            self.sidebar_collapsed,
            self.view_mode.as_str(),
            self.ai_mode.as_str(),
            self.ai_dock_width,
            self.show_unread_badges,
            self.always_show_floating_orbs,
            orbs_raw,
            self.adblock_enabled,
            Self::escape_json(&self.ai_provider),
            Self::escape_json(&self.ai_api_key),
            Self::escape_json(&self.ai_model),
            Self::escape_json(&self.ai_base_url),
            self.auto_minimize_on_task,
            Self::escape_json(&self.google_user),
            if self.bookmarks_json.trim().is_empty() {
                "[]"
            } else {
                &self.bookmarks_json
            }
        );
        let _ = fs::write(cfg_path, body);
    }

    fn load_config(&mut self) {
        let cfg_path = self.exe_dir.join("config.json");
        let Ok(raw) = fs::read_to_string(cfg_path) else {
            return;
        };
        if let Some(v) = Self::extract_field_str(&raw, "tab_orientation") {
            self.tab_orientation = TabOrientation::from_str(&v);
        }
        if let Some(b) = Self::extract_bool(&raw, "sidebar_collapsed") {
            self.sidebar_collapsed = b;
        }
        if let Some(b) = Self::extract_bool(&raw, "floating_omnibox") {
            self.floating_omnibox = b;
        }
        if let Some(b) = Self::extract_bool(&raw, "auto_hide_titlebar") {
            self.compact_titlebar = b;
        }
        if let Some(w) = Self::extract_u64(&raw, "\"sidebar_width\":") {
            if (140..=440).contains(&w) {
                self.sidebar_width = w as u32;
            }
        }
        if let Some(v) = Self::extract_field_str(&raw, "view_mode") {
            self.view_mode = ContentViewMode::from_str(&v);
        }
        if let Some(v) = Self::extract_field_str(&raw, "ai_mode") {
            self.ai_mode = AiDisplayMode::from_str(&v);
        }
        if let Some(w) = Self::extract_u64(&raw, "\"ai_dock_width\":") {
            if (260..=1200).contains(&w) {
                self.ai_dock_width = w as u32;
            }
        }
        if let Some(b) = Self::extract_bool(&raw, "show_unread_badges") {
            self.show_unread_badges = b;
        }
        if let Some(b) = Self::extract_bool(&raw, "always_show_floating_orbs") {
            self.always_show_floating_orbs = b;
        }
        if let Some(b) = Self::extract_bool(&raw, "adblock_enabled") {
            self.adblock_enabled = b;
            ADBLOCK_ENABLED.store(b, Ordering::Relaxed);
        }
        if let Some(v) = Self::extract_field_str(&raw, "ai_provider") {
            self.ai_provider = v;
        }
        if let Some(v) = Self::extract_field_str(&raw, "ai_api_key") {
            self.ai_api_key = v;
        }
        if let Some(v) = Self::extract_field_str(&raw, "ai_model") {
            if !v.is_empty() {
                self.ai_model = v;
            }
        }
        if let Some(v) = Self::extract_field_str(&raw, "ai_base_url") {
            if !v.is_empty() {
                self.ai_base_url = v;
            }
        }
        if let Some(b) = Self::extract_bool(&raw, "auto_minimize_on_task") {
            self.auto_minimize_on_task = b;
        }
        if let Some(v) = Self::extract_field_str(&raw, "google_user") {
            self.google_user = v;
        }
        if let Some(v) = Self::extract_field_str(&raw, "ui_theme") {
            if !v.is_empty() {
                self.ui_theme = v;
            }
        }
        if let Some(v) = Self::extract_field_str(&raw, "logo_style") {
            if !v.is_empty() {
                self.logo_style = Self::normalize_logo_style(&v);
            }
        }
        if let Some(v) = Self::extract_field_str(&raw, "startup_mode") {
            self.startup_mode = if v.eq_ignore_ascii_case("newtab") {
                "newtab".to_string()
            } else {
                "restore".to_string()
            };
        }
        if let Ok(parsed) = parse_json(&raw) {
            if let Some(bm) = parsed.get("bookmarks") {
                if matches!(bm, JsonVal::Arr(_)) {
                    self.bookmarks_json = bm.to_json();
                }
            }
            if let Some(orbs) = parsed.get("floating_orbs") {
                if let Some(arr) = orbs.as_arr() {
                    if !arr.is_empty() {
                        self.floating_orbs_json = orbs.to_json();
                    }
                }
            }
        }
    }

    fn save_history(&self) {
        let hist_path = self.exe_dir.join("history.json");
        let _ = fs::write(hist_path, &self.history_json);
    }

    fn load_history(&mut self) {
        let hist_path = self.exe_dir.join("history.json");
        if let Ok(raw) = fs::read_to_string(hist_path) {
            if let Ok(JsonVal::Arr(_)) = parse_json(&raw) {
                self.history_json = raw;
            }
        }
    }

    fn record_history(&mut self, title: &str, url: &str) {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return;
        }
        let Ok(JsonVal::Arr(mut items)) = parse_json(&self.history_json) else {
            return;
        };
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if let Some(last) = items.last() {
            if last.get("url").and_then(|v| v.as_str()) == Some(url) {
                return;
            }
        }
        items.push(JsonVal::obj(vec![
            ("title", JsonVal::Str(title.to_string())),
            ("url", JsonVal::Str(url.to_string())),
            ("ts", JsonVal::Num(ts as f64)),
        ]));
        if items.len() > 200 {
            let drop_n = items.len() - 200;
            items.drain(0..drop_n);
        }
        self.history_json = JsonVal::Arr(items).to_json();
        self.save_history();
    }

    fn sync_history_view(&self) {
        let closed_arr: Vec<JsonVal> = self
            .closed_tabs
            .iter()
            .map(|u| JsonVal::Str(u.clone()))
            .collect();
        let closed_json = JsonVal::Arr(closed_arr).to_json();
        let payload = format!(
            "{{\"ui_theme\":\"{}\",\"logo_style\":\"{}\",\"history\":{},\"closed_tabs\":{}}}",
            Self::escape_json(&self.ui_theme),
            Self::escape_json(&self.logo_style),
            if self.history_json.trim().is_empty() { "[]" } else { &self.history_json },
            closed_json
        );
        let js = format!("if (window.__tpSyncHistory) window.__tpSyncHistory({});", payload);
        for t in &self.tabs {
            if t.url == "tp://history" {
                let _ = t.webview.evaluate_script(&js);
            }
        }
    }

    fn load_exports_cache(&mut self) {
        let dir = self.exe_dir.join("exports");
        if let Ok(s) = fs::read_to_string(dir.join("shopee_orders_latest.json")) {
            if !s.trim().is_empty() {
                self.shopee_orders_json = s;
            }
        }
        if let Ok(s) = fs::read_to_string(dir.join("shopee_pas_latest.json")) {
            if !s.trim().is_empty() {
                self.shopee_pas_json = s;
            }
        }
        if let Ok(s) = fs::read_to_string(dir.join("tiktok_latest.json")) {
            if !s.trim().is_empty() {
                self.tiktok_json = s;
            }
        }
    }

    fn save_export_file(&self, filename: &str, content: &str) -> Option<PathBuf> {
        let dir = self.exe_dir.join("exports");
        if fs::create_dir_all(&dir).is_err() {
            return None;
        }
        let path = dir.join(filename);
        if fs::write(&path, content).is_ok() {
            Some(path)
        } else {
            None
        }
    }

    fn push_ecom_log(&mut self, msg: &str) {
        self.ecom_logs.push(format!("• {}", msg));
        if self.ecom_logs.len() > 60 {
            let drop_n = self.ecom_logs.len() - 60;
            self.ecom_logs.drain(0..drop_n);
        }
    }

    fn sync_workspace_view(&self) {
        let mut workers = Vec::new();
        for t in &self.tabs {
            if let Some(kind) = &t.worker_kind {
                workers.push(JsonVal::obj(vec![
                    ("id", JsonVal::Num(t.id as f64)),
                    ("worker_kind", JsonVal::Str(kind.clone())),
                    ("title", JsonVal::Str(t.title.clone())),
                    ("url", JsonVal::Str(t.url.clone())),
                    (
                        "status",
                        JsonVal::Str(if t.worker_status.is_empty() {
                            "Đang chạy ngầm".to_string()
                        } else {
                            t.worker_status.clone()
                        }),
                    ),
                ]));
            }
        }
        let logs_arr: Vec<JsonVal> = self
            .ecom_logs
            .iter()
            .map(|l| JsonVal::Str(l.clone()))
            .collect();

        let sp_ord = if self.shopee_orders_json.trim().is_empty() {
            "null"
        } else {
            &self.shopee_orders_json
        };
        let sp_pas = if self.shopee_pas_json.trim().is_empty() {
            "null"
        } else {
            &self.shopee_pas_json
        };
        let tt = if self.tiktok_json.trim().is_empty() {
            "null"
        } else {
            &self.tiktok_json
        };

        let payload = format!(
            "{{\"ui_theme\":\"{}\",\"logo_style\":\"{}\",\"auto_scan\":{},\"status_text\":\"{}\",\"worker_tabs\":{},\"shopee_orders\":{},\"shopee_pas\":{},\"tiktok\":{},\"logs\":{}}}",
            Self::escape_json(&self.ui_theme),
            Self::escape_json(&self.logo_style),
            self.ecom_auto_scan,
            Self::escape_json(&self.ecom_status_text),
            JsonVal::Arr(workers).to_json(),
            sp_ord,
            sp_pas,
            tt,
            JsonVal::Arr(logs_arr).to_json()
        );
        let js = format!("if (window.__tpSyncWorkspace) window.__tpSyncWorkspace({});", payload);
        for t in &self.tabs {
            if t.url == "tp://workspace" {
                let _ = t.webview.evaluate_script(&js);
            }
        }
    }

    fn export_shopee_csv(&mut self) {
        let Ok(val) = parse_json(&self.shopee_orders_json) else {
            self.ecom_status_text = "Chưa có dữ liệu đơn Shopee để xuất CSV — hãy bấm Quét Đơn Shopee trước.".to_string();
            self.push_ecom_log(&self.ecom_status_text.clone());
            self.sync_workspace_view();
            return;
        };
        let mut lines = vec!["\u{FEFF}MaDon,KhachHang,TrangThai,SanPham,PhanLoai,SoLuong,DonGia,TongDon,DonViVanChuyen".to_string()];
        let csv_esc = |s: &str| -> String {
            format!("\"{}\"", s.replace('"', "\"\""))
        };
        if let Some(arr) = val.get("raw_rows").and_then(|v| v.as_arr()) {
            for r in arr {
                let sn = r.get("order_sn").and_then(|v| v.as_str()).unwrap_or("");
                let buyer = r.get("buyer").and_then(|v| v.as_str()).unwrap_or("");
                let st = r.get("status").and_then(|v| v.as_str()).unwrap_or("");
                let item = r.get("item_name").and_then(|v| v.as_str()).unwrap_or("");
                let model = r.get("model_name").and_then(|v| v.as_str()).unwrap_or("");
                let qty = r.get("qty").and_then(|v| v.as_i64()).unwrap_or(1);
                let ip = r.get("item_price").and_then(|v| v.as_str()).unwrap_or("");
                let tp = r.get("total_price").and_then(|v| v.as_str()).unwrap_or("");
                let ship = r.get("shipping").and_then(|v| v.as_str()).unwrap_or("");
                lines.push(format!(
                    "{},{},{},{},{},{},{},{},{}",
                    csv_esc(sn),
                    csv_esc(buyer),
                    csv_esc(st),
                    csv_esc(item),
                    csv_esc(model),
                    qty,
                    csv_esc(ip),
                    csv_esc(tp),
                    csv_esc(ship)
                ));
            }
        }
        let count = lines.len().saturating_sub(1);
        if let Some(p) = self.save_export_file("shopee_don_hang.csv", &lines.join("
\n")) {
            self.ecom_status_text = format!("✅ Đã xuất {} dòng ra {}", count, p.display());
        } else {
            self.ecom_status_text = "Không ghi được file exports/shopee_don_hang.csv".to_string();
        }
        self.push_ecom_log(&self.ecom_status_text.clone());
        self.sync_workspace_view();
    }

    fn ensure_ecom_worker(&mut self, domain_hint: &str, url: &str, title: &str) -> Option<u64> {
        if let Some(t) = self.tabs.iter_mut().find(|t| {
            t.worker_kind.as_deref() == Some("bot") && t.url.contains(domain_hint)
        }) {
            t.worker_status = "Đang quét...".to_string();
            let id = t.id;
            return Some(id);
        }
        self.add_worker_tab(url, "bot", Some(title))
    }

    fn trigger_worker_scan_by_id(&mut self, tab_id: u64, url: &str) {
        if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) {
            if url.contains("banhang.shopee.vn/portal/marketing/pas") {
                let _ = tab.webview.evaluate_script(
                    "setTimeout(function(){ if (window.__tpRunShopeeScan) window.__tpRunShopeeScan('pas'); }, 1500);",
                );
            } else if url.contains("banhang.shopee.vn") {
                let _ = tab.webview.evaluate_script(
                    "setTimeout(function(){ if (window.__tpRunShopeeScan) window.__tpRunShopeeScan('orders'); }, 1000);",
                );
            } else if url.contains("seller-vn.tiktok.com") {
                let js = format!("setTimeout(function(){{ {} }}, 2200);", TIKTOK_PASSIVE_SCAN_JS);
                let _ = tab.webview.evaluate_script(&js);
            }
        }
    }

    fn save_session(&self) {
        let session_path = self.exe_dir.join("session.json");
        let mut tabs_arr = Vec::new();
        let mut saved_active_idx = 0usize;
        let mut user_idx = 0usize;
        for (idx, t) in self.tabs.iter().enumerate() {
            if t.worker_kind.is_some() {
                continue;
            }
            if idx == self.active_idx {
                saved_active_idx = user_idx;
            }
            tabs_arr.push(format!(
                "{{\"url\":\"{}\",\"profile\":\"{}\"}}",
                Self::escape_json(&t.url),
                Self::escape_json(&t.profile_name)
            ));
            user_idx += 1;
        }
        if tabs_arr.is_empty() {
            tabs_arr.push("{\"url\":\"tp://newtab\",\"profile\":\"default\"}".to_string());
        }
        let body = format!(
            "{{\n  \"view_mode\": \"{}\",\n  \"active_idx\": {},\n  \"tabs\": [\n    {}\n  ]\n}}",
            self.view_mode.as_str(),
            saved_active_idx,
            tabs_arr.join(",\n    ")
        );
        let _ = fs::write(session_path, body);
    }

    fn load_session_or_default(&mut self) {
        if self.startup_mode == "newtab" {
            self.add_tab("tp://newtab", false);
            return;
        }
        let cfg_has_vm = fs::read_to_string(self.exe_dir.join("config.json"))
            .map(|c| c.contains("\"view_mode\""))
            .unwrap_or(false);
        let session_path = self.exe_dir.join("session.json");
        let mut saved_active_idx: Option<usize> = None;
        if let Ok(raw) = fs::read_to_string(session_path) {
            if let Ok(val) = parse_json(&raw) {
                if !cfg_has_vm {
                    if let Some(vm) = val.get("view_mode").and_then(|v| v.as_str()) {
                        self.view_mode = ContentViewMode::from_str(vm);
                    }
                }
                saved_active_idx = val.get("active_idx").and_then(|v| v.as_u64()).map(|n| n as usize);
                if let Some(arr) = val.get("tabs").and_then(|v| v.as_arr()) {
                    for item in arr {
                        if let Some(u) = item.get("url").and_then(|v| v.as_str()) {
                            let isolated = item
                                .get("profile")
                                .and_then(|v| v.as_str())
                                .map(|p| p != "default")
                                .unwrap_or(false);
                            self.add_tab_internal(u, isolated, false, None, None);
                        }
                    }
                }
            } else {
                if !cfg_has_vm {
                    if let Some(vm) = Self::extract_field_str(&raw, "view_mode") {
                        self.view_mode = ContentViewMode::from_str(&vm);
                    }
                }
                for line in raw.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("{\"url\":\"") {
                        if let Some(u) = Self::extract_string(trimmed, "\"url\":\"") {
                            self.add_tab_internal(&u, false, false, None, None);
                        }
                    }
                }
            }
        }
        if self.tabs.is_empty() {
            self.add_tab_internal("tp://newtab", false, false, None, None);
        }
        if let Some(idx) = saved_active_idx {
            if idx < self.tabs.len() {
                self.active_idx = idx;
            }
        }
        self.relayout();
        self.sync_all_ui();
        self.save_session();
    }

    fn escape_json(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\r', "")
            .replace('\n', "\\n")
    }

    fn parse_bool_str(val: &str, default: bool) -> bool {
        let t = val.trim();
        if t.eq_ignore_ascii_case("true") || t == "1" {
            true
        } else if t.eq_ignore_ascii_case("false") || t == "0" {
            false
        } else {
            default
        }
    }

    fn extract_bool(json: &str, field: &str) -> Option<bool> {
        let needle = format!("\"{}\":", field);
        let start = json.find(&needle)? + needle.len();
        let rest = json[start..].trim_start();
        if rest.starts_with("true") {
            Some(true)
        } else if rest.starts_with("false") {
            Some(false)
        } else {
            None
        }
    }

    fn extract_field_str(json: &str, field: &str) -> Option<String> {
        let needle_tight = format!("\"{}\":\"", field);
        let needle_spaced = format!("\"{}\": \"", field);
        Self::extract_string(json, &needle_tight).or_else(|| Self::extract_string(json, &needle_spaced))
    }

    fn extract_setting_value(json: &str) -> Option<String> {
        if let Some(s) = Self::extract_field_str(json, "value") {
            return Some(s);
        }
        let needle = "\"value\":";
        let start = json.find(needle)? + needle.len();
        let rest = json[start..].trim_start();
        if rest.starts_with("true") {
            Some("true".to_string())
        } else if rest.starts_with("false") {
            Some("false".to_string())
        } else {
            let digits: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
                .collect();
            if digits.is_empty() {
                None
            } else {
                Some(digits)
            }
        }
    }

    fn extract_u64(json: &str, key: &str) -> Option<u64> {
        let start = json.find(key)? + key.len();
        let rest = json[start..].trim_start();
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse().ok()
    }

    fn extract_string(json: &str, key: &str) -> Option<String> {
        let start = json.find(key)? + key.len();
        let rest = &json[start..];
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(n) = chars.next() {
                    match n {
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        _ => out.push(n),
                    }
                }
            } else if ch == '"' {
                break;
            } else {
                out.push(ch);
            }
        }
        Some(out)
    }

    fn handle_ipc(proxy: &EventLoopProxy<AppCmd>, body: &str) {
        if body.contains("\"cmd\":\"mcp_callback\"") {
            if let Ok(val) = parse_json(body) {
                if val.get("cmd").and_then(|v| v.as_str()) == Some("mcp_callback") {
                    let req_id = val.get("req_id").and_then(|v| v.as_u64()).unwrap_or(0);
                    let ok = val.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                    let payload_json = if ok {
                        val.get("data")
                            .map(|v| v.to_json())
                            .unwrap_or_else(|| "null".to_string())
                    } else {
                        val.get("error")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "JS execution error".to_string())
                    };
                    let _ = proxy.send_event(AppCmd::McpJsCallback {
                        req_id,
                        ok,
                        payload_json,
                    });
                    return;
                }
            }
        }
        let tab_id = Self::extract_u64(body, "\"tab_id\":").unwrap_or(0);
        if body.contains("\"cmd\":\"toggle_tab_orientation\"") {
            let _ = proxy.send_event(AppCmd::ToggleTabOrientation);
        } else if body.contains("\"cmd\":\"toggle_sidebar_collapse\"") {
            let _ = proxy.send_event(AppCmd::ToggleSidebarCollapse);
        } else if body.contains("\"cmd\":\"toggle_titlebar_pin\"") {
            let _ = proxy.send_event(AppCmd::ToggleTitlebarPin);
        } else if body.contains("\"cmd\":\"drag_window\"") {
            let _ = proxy.send_event(AppCmd::DragWindow);
        } else if body.contains("\"cmd\":\"win_minimize\"") {
            let _ = proxy.send_event(AppCmd::WinMinimize);
        } else if body.contains("\"cmd\":\"win_toggle_maximize\"") {
            let _ = proxy.send_event(AppCmd::WinToggleMaximize);
        } else if body.contains("\"cmd\":\"win_close\"") {
            let _ = proxy.send_event(AppCmd::WinClose);
        } else if body.contains("\"cmd\":\"cycle_view_mode\"") {
            let _ = proxy.send_event(AppCmd::CycleViewMode);
        } else if body.contains("\"cmd\":\"toggle_ai_mode\"") {
            let _ = proxy.send_event(AppCmd::ToggleAiMode);
        } else if body.contains("\"cmd\":\"set_ai_mode\"") {
            if let Some(m) = Self::extract_string(body, "\"mode\":\"") {
                let _ = proxy.send_event(AppCmd::SetAiMode(AiDisplayMode::from_str(&m)));
            }
        } else if body.contains("\"cmd\":\"open_chat_channel\"") {
            let ch = Self::extract_string(body, "\"channel\":\"").unwrap_or_else(|| "ai".to_string());
            let _ = proxy.send_event(AppCmd::OpenChatChannel(ChatChannel::from_str(&ch)));
        } else if body.contains("\"cmd\":\"toggle_chat_channel\"") {
            let ch = Self::extract_string(body, "\"channel\":\"").unwrap_or_else(|| "ai".to_string());
            let _ = proxy.send_event(AppCmd::ToggleChatChannel(ChatChannel::from_str(&ch)));
        } else if body.contains("\"cmd\":\"toggle_chat_dock_style\"") {
            let _ = proxy.send_event(AppCmd::ToggleChatDockStyle);
        } else if body.contains("\"cmd\":\"start_dock_resize\"") {
            let _ = proxy.send_event(AppCmd::StartDockResize);
        } else if body.contains("\"cmd\":\"reset_dock_width\"") {
            let _ = proxy.send_event(AppCmd::ResetDockWidth);
        } else if body.contains("\"cmd\":\"start_sidebar_resize\"") {
            let _ = proxy.send_event(AppCmd::StartSidebarResize);
        } else if body.contains("\"cmd\":\"reset_sidebar_width\"") {
            let _ = proxy.send_event(AppCmd::ResetSidebarWidth);
        } else if body.contains("\"cmd\":\"tab_meta\"") {
            if let Ok(val) = parse_json(body) {
                let title = val.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let favicon = val.get("favicon").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let _ = proxy.send_event(AppCmd::UpdateTabMeta {
                    tab_id,
                    title,
                    favicon,
                });
            }
        } else if body.contains("\"cmd\":\"channel_unread\"") {
            let ch = Self::extract_string(body, "\"channel\":\"").unwrap_or_else(|| "ai".to_string());
            let count = Self::extract_u64(body, "\"count\":").unwrap_or(0) as u32;
            let _ = proxy.send_event(AppCmd::ChannelUnread {
                channel: ChatChannel::from_str(&ch),
                count,
            });
        } else if body.contains("\"cmd\":\"open_channel_in_tab\"") {
            let _ = proxy.send_event(AppCmd::OpenChannelInTab);
        } else if body.contains("\"cmd\":\"ai_panel_ready\"") {
            let _ = proxy.send_event(AppCmd::SyncAiPanelInit);
        } else if body.contains("\"cmd\":\"ui_shell_ready\"") {
            let shell = Self::extract_field_str(body, "shell").unwrap_or_default();
            let _ = proxy.send_event(AppCmd::UiShellReady(shell));
        } else if body.contains("\"cmd\":\"set_settings_section\"") {
            if let Some(sec) = Self::extract_field_str(body, "section") {
                let _ = proxy.send_event(AppCmd::SetSettingsSection(sec));
            }
        } else if body.contains("\"cmd\":\"settings_modal_ready\"") {
            let _ = proxy.send_event(AppCmd::SyncSettingsModalInit);
        } else if body.contains("\"cmd\":\"open_settings_modal\"") {
            let sec = Self::extract_field_str(body, "section")
                .unwrap_or_else(|| "look_feel".to_string());
            let _ = proxy.send_event(AppCmd::OpenSettingsModal(sec));
        } else if body.contains("\"cmd\":\"close_settings_modal\"") {
            let _ = proxy.send_event(AppCmd::CloseSettingsModal);
        } else if body.contains("\"cmd\":\"toggle_settings_modal\"") {
            let _ = proxy.send_event(AppCmd::ToggleSettingsModal);
        } else if body.contains("\"cmd\":\"save_setting\"") {
            if let Ok(val) = parse_json(body) {
                if let Some(key) = val.get("key").and_then(|v| v.as_str()) {
                    let value = match val.get("value") {
                        Some(JsonVal::Str(s)) => s.clone(),
                        Some(JsonVal::Bool(b)) => b.to_string(),
                        Some(JsonVal::Num(n)) => {
                            if n.fract() == 0.0 {
                                (*n as i64).to_string()
                            } else {
                                n.to_string()
                            }
                        }
                        _ => String::new(),
                    };
                    let _ = proxy.send_event(AppCmd::SaveSetting {
                        key: key.to_string(),
                        value,
                    });
                }
            } else if let Some(key) = Self::extract_field_str(body, "key") {
                let value = Self::extract_setting_value(body).unwrap_or_default();
                let _ = proxy.send_event(AppCmd::SaveSetting { key, value });
            }
        } else if body.contains("\"cmd\":\"open_ai_settings\"") {
            let _ = proxy.send_event(AppCmd::OpenAiSettings);
        } else if body.contains("\"cmd\":\"focus_ai_panel\"") {
            let _ = proxy.send_event(AppCmd::FocusAiPanel);
        } else if body.contains("\"cmd\":\"open_google_login\"") {
            let _ = proxy.send_event(AppCmd::OpenGoogleLogin);
        } else if body.contains("\"cmd\":\"set_google_user\"") {
            if let Some(u) = Self::extract_string(body, "\"google_user\":\"") {
                let _ = proxy.send_event(AppCmd::SetGoogleUser(u));
            }
        } else if body.contains("\"cmd\":\"set_ui_theme\"") {
            if let Some(th) = Self::extract_string(body, "\"theme\":\"") {
                let _ = proxy.send_event(AppCmd::SetUiTheme(th));
            }
        } else if body.contains("\"cmd\":\"new_tab\"") {
            let url = Self::extract_string(body, "\"url\":\"").unwrap_or_else(|| "tp://newtab".to_string());
            let isolated = body.contains("\"isolated\":true");
            let _ = proxy.send_event(AppCmd::NewTab { url, isolated });
        } else if body.contains("\"cmd\":\"focus_index\"") {
            if let Some(idx) = Self::extract_u64(body, "\"index\":") {
                let _ = proxy.send_event(AppCmd::FocusIndex(idx as usize));
            }
        } else if body.contains("\"cmd\":\"focus_last_tab\"") {
            let _ = proxy.send_event(AppCmd::FocusLastTab);
        } else if body.contains("\"cmd\":\"next_tab\"") {
            let _ = proxy.send_event(AppCmd::NextTab);
        } else if body.contains("\"cmd\":\"prev_tab\"") {
            let _ = proxy.send_event(AppCmd::PrevTab);
        } else if body.contains("\"cmd\":\"focus_this_tab\"") {
            let _ = proxy.send_event(AppCmd::FocusTabById(tab_id));
        } else if body.contains("\"cmd\":\"focus_tab_by_id\"") {
            if let Some(tid) = Self::extract_u64(body, "\"tab_id\":") {
                let _ = proxy.send_event(AppCmd::FocusTabById(tid));
            }
        } else if body.contains("\"cmd\":\"close_tab_by_id\"") {
            if let Some(tid) = Self::extract_u64(body, "\"tab_id\":") {
                let _ = proxy.send_event(AppCmd::CloseTabById(tid));
            }
        } else if body.contains("\"cmd\":\"close_index\"") {
            if let Some(idx) = Self::extract_u64(body, "\"index\":") {
                let _ = proxy.send_event(AppCmd::CloseIndex(idx as usize));
            }
        } else if body.contains("\"cmd\":\"close_active_tab\"") {
            let _ = proxy.send_event(AppCmd::CloseActiveTab);
        } else if body.contains("\"cmd\":\"navigate_active\"") {
            if let Some(u) = Self::extract_string(body, "\"url\":\"") {
                let _ = proxy.send_event(AppCmd::NavigateActive(u));
            }
        } else if body.contains("\"cmd\":\"nav_back\"") {
            let _ = proxy.send_event(AppCmd::NavBack);
        } else if body.contains("\"cmd\":\"nav_forward\"") {
            let _ = proxy.send_event(AppCmd::NavForward);
        } else if body.contains("\"cmd\":\"reload\"") {
            let _ = proxy.send_event(AppCmd::ReloadActive);
        } else if body.contains("\"cmd\":\"hard_reload\"") {
            let _ = proxy.send_event(AppCmd::HardReloadActive);
        } else if body.contains("\"cmd\":\"go_home\"") {
            let _ = proxy.send_event(AppCmd::GoHome);
        } else if body.contains("\"cmd\":\"focus_url_bar\"") {
            let _ = proxy.send_event(AppCmd::FocusUrlBar);
        } else if body.contains("\"cmd\":\"toggle_find_bar\"") {
            let _ = proxy.send_event(AppCmd::ToggleFindBar);
        } else if body.contains("\"cmd\":\"toggle_collections_drawer\"") {
            let _ = proxy.send_event(AppCmd::ToggleCollectionsDrawer);
        } else if body.contains("\"cmd\":\"open_history\"") {
            let _ = proxy.send_event(AppCmd::OpenHistory);
        } else if body.contains("\"cmd\":\"get_history\"") {
            let _ = proxy.send_event(AppCmd::GetHistory);
        } else if body.contains("\"cmd\":\"clear_history\"") {
            let _ = proxy.send_event(AppCmd::ClearHistory);
        } else if body.contains("\"cmd\":\"open_workspace\"") {
            let _ = proxy.send_event(AppCmd::OpenWorkspace);
        } else if body.contains("\"cmd\":\"run_ecom_scan\"") {
            let scope = Self::extract_field_str(body, "scope").unwrap_or_else(|| "all".to_string());
            let _ = proxy.send_event(AppCmd::RunEcomScan { scope });
        } else if body.contains("\"cmd\":\"set_ecom_auto_scan\"") {
            let enabled = body.contains("\"enabled\":true");
            let _ = proxy.send_event(AppCmd::SetEcomAutoScan(enabled));
        } else if body.contains("\"cmd\":\"update_platform_data\"") {
            if let Ok(val) = parse_json(body) {
                let platform = val.get("platform").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let data_json = val.get("data_json").and_then(|v| v.as_str()).unwrap_or("{}").to_string();
                if !platform.is_empty() {
                    let _ = proxy.send_event(AppCmd::UpdatePlatformData { platform, data_json });
                }
            }
        } else if body.contains("\"cmd\":\"export_shopee_csv\"") {
            let _ = proxy.send_event(AppCmd::ExportShopeeCsv);
        } else if body.contains("\"cmd\":\"reopen_closed_tab\"") {
            let _ = proxy.send_event(AppCmd::ReopenClosedTab);
        } else if body.contains("\"cmd\":\"zoom_step\"") {
            let delta = parse_json(body)
                .ok()
                .and_then(|v| v.get("delta").and_then(|d| d.as_i64()))
                .unwrap_or(10) as i32;
            let _ = proxy.send_event(AppCmd::ZoomStep(delta));
        } else if body.contains("\"cmd\":\"zoom_reset\"") {
            let _ = proxy.send_event(AppCmd::ZoomReset);
        } else if body.contains("\"cmd\":\"save_floating_orbs\"") {
            if let Ok(val) = parse_json(body) {
                if let Some(oj) = val.get("orbs_json").and_then(|v| v.as_str()) {
                    let _ = proxy.send_event(AppCmd::SaveFloatingOrbs(oj.to_string()));
                }
            }
        } else if body.contains("\"cmd\":\"open_custom_orb\"") {
            if let Ok(val) = parse_json(body) {
                let id = val.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let kind = val.get("kind").and_then(|v| v.as_str()).unwrap_or("popup").to_string();
                let url = val.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let title = val.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let _ = proxy.send_event(AppCmd::OpenCustomOrb { id, kind, url, title });
            }
        } else if body.contains("\"cmd\":\"bookmark_active\"") {
            let _ = proxy.send_event(AppCmd::BookmarkActive);
        } else if body.contains("\"cmd\":\"save_bookmarks\"") {
            if let Some(bm) = Self::extract_string(body, "\"bookmarks_json\":\"") {
                let _ = proxy.send_event(AppCmd::SaveBookmarks(bm));
            }
        } else if body.contains("\"cmd\":\"save_ai_config\"") {
            if let Ok(val) = parse_json(body) {
                let provider = val.get("provider").and_then(|v| v.as_str()).unwrap_or("gemini").to_string();
                let api_key = val.get("api_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let model = val.get("model").and_then(|v| v.as_str()).unwrap_or("gemini-2.5-flash").to_string();
                let base_url = val.get("base_url").and_then(|v| v.as_str()).unwrap_or("http://127.0.0.1:20128/v1").to_string();
                let auto_min = val.get("auto_minimize_on_task").and_then(|v| v.as_bool()).unwrap_or(true);
                let _ = proxy.send_event(AppCmd::SaveAiConfig {
                    provider,
                    api_key,
                    model,
                    base_url,
                    auto_minimize_on_task: auto_min,
                });
            } else {
                let provider = Self::extract_string(body, "\"provider\":\"").unwrap_or_else(|| "gemini".to_string());
                let api_key = Self::extract_string(body, "\"api_key\":\"").unwrap_or_default();
                let model = Self::extract_string(body, "\"model\":\"").unwrap_or_else(|| "gemini-2.5-flash".to_string());
                let base_url = Self::extract_string(body, "\"base_url\":\"").unwrap_or_else(|| "http://127.0.0.1:20128/v1".to_string());
                let auto_min = !body.contains("\"auto_minimize_on_task\":false");
                let _ = proxy.send_event(AppCmd::SaveAiConfig {
                    provider,
                    api_key,
                    model,
                    base_url,
                    auto_minimize_on_task: auto_min,
                });
            }
        } else if body.contains("\"cmd\":\"save_skills_json\"") {
            if let Some(sk) = Self::extract_string(body, "\"skills_json\":\"") {
                let _ = proxy.send_event(AppCmd::SaveSkillsJson(sk));
            }
        } else if body.contains("\"cmd\":\"get_tasks\"") {
            let _ = proxy.send_event(AppCmd::GetTasks);
        } else if body.contains("\"cmd\":\"save_tasks_json\"") {
            if let Some(tj) = Self::extract_string(body, "\"tasks_json\":\"") {
                let _ = proxy.send_event(AppCmd::SaveTasksJson(tj));
            }
        } else if body.contains("\"cmd\":\"snooze_reminder\"") {
            let id = Self::extract_field_str(body, "id").unwrap_or_default();
            let mins = Self::extract_u64(body, "\"minutes\":").unwrap_or(10);
            let _ = proxy.send_event(AppCmd::SnoozeReminder { id, minutes: mins });
        } else if body.contains("\"cmd\":\"complete_task\"") {
            let id = Self::extract_field_str(body, "id").unwrap_or_default();
            let _ = proxy.send_event(AppCmd::CompleteTask(id));
        } else if body.contains("\"cmd\":\"create_task_from_selection\"") {
            let text = Self::extract_field_str(body, "text").unwrap_or_default();
            let source = Self::extract_field_str(body, "source").unwrap_or_else(|| "web".to_string());
            let contact = Self::extract_field_str(body, "contact").unwrap_or_default();
            let _ = proxy.send_event(AppCmd::CreateTaskFromSelection { text, source, contact });
        } else if body.contains("\"cmd\":\"scan_zalo_chat\"") {
            let _ = proxy.send_event(AppCmd::ScanZaloChat);
        } else if body.contains("\"cmd\":\"paste_to_chat\"") {
            let channel = Self::extract_field_str(body, "channel").unwrap_or_else(|| "zalo".to_string());
            let text = Self::extract_field_str(body, "text").unwrap_or_default();
            let _ = proxy.send_event(AppCmd::PasteToChat { channel, text });
        } else if body.contains("\"cmd\":\"start_skill_recording\"") {
            let _ = proxy.send_event(AppCmd::StartSkillRecording);
        } else if body.contains("\"cmd\":\"stop_skill_recording\"") {
            let _ = proxy.send_event(AppCmd::StopSkillRecording);
        } else if body.contains("\"cmd\":\"recorded_skill_step\"") {
            if let Some(st) = Self::extract_string(body, "\"step_json\":\"") {
                let _ = proxy.send_event(AppCmd::RecordedSkillStep(st));
            }
        } else if body.contains("\"cmd\":\"run_ai_prompt\"") {
            if let Some(p) = Self::extract_string(body, "\"prompt\":\"") {
                let _ = proxy.send_event(AppCmd::RunAiPrompt(p));
            }
        } else if body.contains("\"cmd\":\"request_dom_context_for_ai\"") {
            let use_worker = parse_json(body)
                .ok()
                .and_then(|v| v.get("use_worker").and_then(|b| b.as_bool()))
                .unwrap_or(true);
            let prompt = Self::extract_field_str(body, "prompt").unwrap_or_default();
            let _ = proxy.send_event(AppCmd::RequestDomContextForAi { use_worker, prompt });
        } else if body.contains("\"cmd\":\"dom_context_result\"") {
            let title = Self::extract_string(body, "\"title\":\"").unwrap_or_default();
            let url = Self::extract_string(body, "\"url\":\"").unwrap_or_default();
            let visible_text = Self::extract_string(body, "\"visible_text\":\"").unwrap_or_default();
            let elements_json = Self::extract_string(body, "\"elements_json\":\"").unwrap_or_else(|| "[]".to_string());
            let _ = proxy.send_event(AppCmd::DomContextResult {
                tab_id,
                title,
                url,
                visible_text,
                elements_json,
            });
        } else if body.contains("\"cmd\":\"execute_dom_action\"") {
            if let Ok(val) = parse_json(body) {
                let target_tab_id = val.get("target_tab_id").and_then(|v| v.as_u64());
                let action_json = val
                    .get("action_json")
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}")
                    .to_string();
                let _ = proxy.send_event(AppCmd::ExecuteDomAction {
                    target_tab_id,
                    action_json,
                });
            } else if let Some(act_json) = Self::extract_string(body, "\"action_json\":\"") {
                let _ = proxy.send_event(AppCmd::ExecuteDomAction {
                    target_tab_id: None,
                    action_json: act_json,
                });
            }
        } else if body.contains("\"cmd\":\"sync_bubble_status\"") {
            let working = body.contains("\"working\":true");
            let recording = body.contains("\"recording\":true");
            let status_text = Self::extract_string(body, "\"status_text\":\"").unwrap_or_default();
            let last_reply = Self::extract_string(body, "\"last_reply\":\"").unwrap_or_default();
            let _ = proxy.send_event(AppCmd::SyncBubbleStatus {
                working,
                recording,
                status_text,
                last_reply,
            });
        }
    }

    fn execute(&mut self, cmd: AppCmd) {
        match cmd {
            AppCmd::ToggleTabOrientation => {
                self.tab_orientation = self.tab_orientation.toggle();
                self.relayout();
                self.sync_all_ui();
                self.save_config();
                self.broadcast_config_changed();
            }
            AppCmd::ToggleSidebarCollapse => {
                self.sidebar_collapsed = !self.sidebar_collapsed;
                self.relayout();
                self.sync_all_ui();
                self.save_config();
                self.broadcast_config_changed();
            }
            AppCmd::SetTitlebarHover(show) => {
                if !self.titlebar_pinned && self.titlebar_hover != show {
                    self.titlebar_hover = show;
                    self.relayout();
                    self.sync_all_ui();
                }
            }
            AppCmd::ToggleTitlebarPin => {
                self.titlebar_pinned = !self.titlebar_pinned;
                self.compact_titlebar = !self.titlebar_pinned;
                self.relayout();
                self.sync_all_ui();
                self.save_config();
                self.broadcast_config_changed();
            }
            AppCmd::DragWindow => {
                let _ = self.window.drag_window();
            }
            AppCmd::WinMinimize => {
                self.window.set_minimized(true);
            }
            AppCmd::WinToggleMaximize => {
                self.window.set_maximized(!self.window.is_maximized());
                self.sync_all_ui();
            }
            AppCmd::WinClose => {
                self.save_config();
                self.save_skills();
                self.save_session();
                self.should_exit = true;
            }
            AppCmd::CycleViewMode => {
                self.view_mode = self.view_mode.next();
                self.relayout();
                self.sync_all_ui();
                self.save_session();
                self.save_config();
                self.broadcast_config_changed();
            }
            AppCmd::ToggleAiMode => {
                self.ai_mode = match self.ai_mode {
                    AiDisplayMode::RightDock | AiDisplayMode::FbPopup => {
                        AiDisplayMode::BubbleMinimized
                    }
                    AiDisplayMode::BubbleMinimized => {
                        if self.chat_channel == ChatChannel::Ai {
                            self.unread_ai = 0;
                            self.ai_finished_flash = false;
                        }
                        AiDisplayMode::FbPopup
                    }
                };
                self.relayout();
                self.sync_all_ui();
                self.save_config();
                self.broadcast_config_changed();
            }
            AppCmd::SetAiMode(mode) => {
                self.ai_mode = mode;
                if self.ai_mode != AiDisplayMode::BubbleMinimized && self.chat_channel == ChatChannel::Ai {
                    self.unread_ai = 0;
                    self.ai_finished_flash = false;
                }
                self.relayout();
                self.sync_all_ui();
                self.save_config();
                self.broadcast_config_changed();
            }
            AppCmd::OpenChatChannel(ch) => {
                self.chat_channel = ch;
                match ch {
                    ChatChannel::Messenger => self.ensure_fb_shell(),
                    ChatChannel::Zalo => self.ensure_zalo_shell(),
                    ChatChannel::Ai => {
                        self.unread_ai = 0;
                        self.ai_finished_flash = false;
                    }
                    ChatChannel::Custom => {}
                }
                if self.ai_mode == AiDisplayMode::BubbleMinimized {
                    self.ai_mode = AiDisplayMode::FbPopup;
                }
                self.relayout();
                self.sync_all_ui();
                self.save_config();
                self.broadcast_config_changed();
            }
            AppCmd::ToggleChatChannel(ch) => {
                if self.ai_mode != AiDisplayMode::BubbleMinimized && self.chat_channel == ch {
                    self.ai_mode = AiDisplayMode::BubbleMinimized;
                } else {
                    self.chat_channel = ch;
                    match ch {
                        ChatChannel::Messenger => self.ensure_fb_shell(),
                        ChatChannel::Zalo => self.ensure_zalo_shell(),
                        ChatChannel::Ai => {
                            self.unread_ai = 0;
                            self.ai_finished_flash = false;
                        }
                        ChatChannel::Custom => {}
                    }
                    if self.ai_mode == AiDisplayMode::BubbleMinimized {
                        self.ai_mode = AiDisplayMode::FbPopup;
                    }
                }
                self.relayout();
                self.sync_all_ui();
                self.save_config();
                self.broadcast_config_changed();
            }
            AppCmd::ToggleChatDockStyle => {
                self.ai_mode = match self.ai_mode {
                    AiDisplayMode::RightDock => AiDisplayMode::FbPopup,
                    AiDisplayMode::FbPopup | AiDisplayMode::BubbleMinimized => {
                        AiDisplayMode::RightDock
                    }
                };
                self.relayout();
                self.sync_all_ui();
                self.save_config();
                self.broadcast_config_changed();
            }
            AppCmd::StartDockResize => {
                #[cfg(target_os = "windows")]
                {
                    let hwnd = self.window.hwnd();
                    let proxy = self.proxy.clone();
                    let scale = self.window.scale_factor();
                    let win_w = self.window.inner_size().to_logical::<u32>(scale).width;
                    std::thread::spawn(move || {
                        let mut last_w = 0u32;
                        loop {
                            unsafe {
                                let down = (GetAsyncKeyState(0x01) & (0x8000u16 as i16)) != 0;
                                if !down || IsWindow(hwnd) == 0 {
                                    break;
                                }
                                let mut pt = WinPoint { x: 0, y: 0 };
                                let mut origin = WinPoint { x: 0, y: 0 };
                                if GetCursorPos(&mut pt) != 0 && ClientToScreen(hwnd, &mut origin) != 0 {
                                    let dpi = GetDpiForWindow(hwnd);
                                    let s = if dpi == 0 { scale } else { dpi as f64 / 96.0 };
                                    let cur_x = ((pt.x - origin.x) as f64 / s).round() as i32;
                                    let new_w = (win_w as i32 - cur_x).max(260) as u32;
                                    if new_w != last_w {
                                        last_w = new_w;
                                        let _ = proxy.send_event(AppCmd::SetDockWidth(new_w));
                                    }
                                }
                            }
                            std::thread::sleep(std::time::Duration::from_millis(16));
                        }
                        let _ = proxy.send_event(AppCmd::FinishDockResize);
                    });
                }
            }
            AppCmd::SetDockWidth(w) => {
                self.ai_dock_width = w.clamp(260, 1200);
                self.relayout();
            }
            AppCmd::ResetDockWidth => {
                self.ai_dock_width = RIGHT_AI_DOCK_WIDTH;
                self.relayout();
                self.save_config();
                self.broadcast_config_changed();
            }
            AppCmd::FinishDockResize => {
                let _ = self.dock_resizer_shell.evaluate_script(
                    "if (window.__tpSyncDockResizer) window.__tpSyncDockResizer({dragging:false});",
                );
                self.save_config();
                self.broadcast_config_changed();
            }
            AppCmd::StartSidebarResize => {
                #[cfg(target_os = "windows")]
                {
                    let hwnd = self.window.hwnd();
                    let proxy = self.proxy.clone();
                    let scale = self.window.scale_factor();
                    let win_w = self.window.inner_size().to_logical::<u32>(scale).width;
                    std::thread::spawn(move || {
                        let mut last_w = 0u32;
                        let max_w = (win_w / 2).clamp(140, 440);
                        loop {
                            unsafe {
                                let down = (GetAsyncKeyState(0x01) & (0x8000u16 as i16)) != 0;
                                if !down || IsWindow(hwnd) == 0 {
                                    break;
                                }
                                let mut pt = WinPoint { x: 0, y: 0 };
                                let mut origin = WinPoint { x: 0, y: 0 };
                                if GetCursorPos(&mut pt) != 0 && ClientToScreen(hwnd, &mut origin) != 0 {
                                    let dpi = GetDpiForWindow(hwnd);
                                    let s = if dpi == 0 { scale } else { dpi as f64 / 96.0 };
                                    let cur_x = ((pt.x - origin.x) as f64 / s).round() as i32;
                                    let new_w = cur_x.clamp(140, max_w as i32) as u32;
                                    if new_w != last_w {
                                        last_w = new_w;
                                        let _ = proxy.send_event(AppCmd::SetSidebarWidth(new_w));
                                    }
                                }
                            }
                            std::thread::sleep(std::time::Duration::from_millis(16));
                        }
                        let _ = proxy.send_event(AppCmd::FinishSidebarResize);
                    });
                }
            }
            AppCmd::SetSidebarWidth(w) => {
                self.sidebar_width = w.clamp(140, 440);
                self.relayout();
            }
            AppCmd::ResetSidebarWidth => {
                self.sidebar_width = DEFAULT_LEFT_SIDEBAR_WIDTH;
                self.relayout();
                self.save_config();
                self.broadcast_config_changed();
            }
            AppCmd::FinishSidebarResize => {
                let _ = self.sidebar_resizer_shell.evaluate_script(
                    "if (window.__tpSyncSidebarResizer) window.__tpSyncSidebarResizer({dragging:false});",
                );
                self.save_config();
                self.broadcast_config_changed();
            }
            AppCmd::UpdateTabMeta {
                tab_id,
                title,
                favicon,
            } => {
                let mut changed = false;
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                    if tab.url.starts_with("http://") || tab.url.starts_with("https://") {
                        let clean_t = title.trim();
                        if !clean_t.is_empty() && tab.title != clean_t {
                            tab.title = clean_t.to_string();
                            changed = true;
                        }
                        let clean_f = favicon.trim();
                        if !clean_f.is_empty() && tab.favicon_url != clean_f {
                            tab.favicon_url = clean_f.to_string();
                            changed = true;
                        }
                    }
                }
                if changed {
                    self.sync_all_ui();
                }
            }
            AppCmd::PreloadChatChannels => {
                self.ensure_fb_shell();
                self.ensure_zalo_shell();
                self.relayout();
                self.sync_all_ui();
            }
            AppCmd::ChannelUnread { channel, count } => {
                let mut increased = false;
                match channel {
                    ChatChannel::Ai => {
                        if count > self.unread_ai {
                            increased = true;
                        }
                        self.unread_ai = count;
                    }
                    ChatChannel::Messenger => {
                        if count > self.unread_fb {
                            increased = true;
                        }
                        self.unread_fb = count;
                    }
                    ChatChannel::Zalo => {
                        if count > self.unread_zalo {
                            increased = true;
                        }
                        self.unread_zalo = count;
                    }
                    ChatChannel::Custom => {}
                }
                if increased && self.show_unread_badges {
                    self.chime_seq = self.chime_seq.saturating_add(1);
                    self.chime_kind = "message".to_string();
                }
                self.sync_all_ui();
            }
            AppCmd::OpenChannelInTab => {
                let target_url = match self.chat_channel {
                    ChatChannel::Messenger => "https://www.messenger.com".to_string(),
                    ChatChannel::Zalo => "https://chat.zalo.me".to_string(),
                    ChatChannel::Custom => {
                        if self.custom_popup_url.is_empty() {
                            "tp://newtab".to_string()
                        } else {
                            self.custom_popup_url.clone()
                        }
                    }
                    ChatChannel::Ai => "tp://newtab".to_string(),
                };
                self.ai_mode = AiDisplayMode::BubbleMinimized;
                self.add_tab(&target_url, false);
            }
            AppCmd::SyncAiPanelInit => {
                self.sync_ai_config_to_panel(false);
                self.sync_all_ui();
            }
            AppCmd::UiShellReady(shell) => {
                self.relayout();
                self.sync_all_ui();
                if shell == "history" {
                    self.sync_history_view();
                } else if shell == "workspace" {
                    self.sync_workspace_view();
                }
            }
            AppCmd::SetSettingsSection(sec) => {
                if !sec.trim().is_empty() {
                    self.settings_section = sec.trim().to_string();
                }
            }
            AppCmd::SyncSettingsModalInit => {
                self.sync_settings_modal(Some(&self.settings_section));
            }
            AppCmd::OpenSettingsModal(section) => {
                let target_sec = if section.trim().is_empty() {
                    "look_feel".to_string()
                } else {
                    section
                };
                self.settings_section = target_sec.clone();
                self.settings_open = true;
                self.relayout();
                self.sync_settings_modal(Some(&target_sec));
                let _ = self.settings_shell.focus();
            }
            AppCmd::CloseSettingsModal => {
                if self.settings_open {
                    self.settings_open = false;
                    self.relayout();
                    if let Some(t) = self.tabs.get(self.active_idx) {
                        let _ = t.webview.focus();
                    }
                }
            }
            AppCmd::ToggleSettingsModal => {
                if self.settings_open {
                    self.execute(AppCmd::CloseSettingsModal);
                } else {
                    let sec = self.settings_section.clone();
                    self.execute(AppCmd::OpenSettingsModal(sec));
                }
            }
            AppCmd::SaveSetting { key, value } => {
                match key.as_str() {
                    "ui_theme" => {
                        self.ui_theme = Self::normalize_theme(&value);
                        self.sync_window_titlebar_theme();
                        self.sync_window_icon();
                    }
                    "logo_style" => {
                        self.logo_style = Self::normalize_logo_style(&value);
                        self.sync_window_icon();
                    }
                    "startup_mode" => {
                        self.startup_mode = if value.eq_ignore_ascii_case("newtab") {
                            "newtab".to_string()
                        } else {
                            "restore".to_string()
                        };
                    }
                    "floating_omnibox" => {
                        self.floating_omnibox = Self::parse_bool_str(&value, true);
                    }
                    "compact_titlebar" => {
                        self.compact_titlebar = Self::parse_bool_str(&value, true);
                        self.titlebar_pinned = !self.compact_titlebar;
                    }
                    "sidebar_width" => {
                        if let Ok(w) = value.trim().parse::<u32>() {
                            self.sidebar_width = w.clamp(140, 440);
                        }
                    }
                    "tab_orientation" => {
                        self.tab_orientation = TabOrientation::from_str(&value);
                    }
                    "sidebar_collapsed" => {
                        self.sidebar_collapsed = Self::parse_bool_str(&value, false);
                    }
                    "view_mode" => {
                        self.view_mode = ContentViewMode::from_str(&value);
                        self.save_session();
                    }
                    "ai_provider" => {
                        self.ai_provider = if value.eq_ignore_ascii_case("gemini") {
                            "gemini".to_string()
                        } else {
                            "openai".to_string()
                        };
                    }
                    "ai_api_key" => {
                        self.ai_api_key = value;
                    }
                    "ai_model" => {
                        if !value.trim().is_empty() {
                            self.ai_model = value.trim().to_string();
                        }
                    }
                    "ai_base_url" => {
                        if !value.trim().is_empty() {
                            self.ai_base_url = value.trim().to_string();
                        }
                    }
                    "auto_minimize_on_task" => {
                        self.auto_minimize_on_task = Self::parse_bool_str(&value, true);
                    }
                    "ai_mode" => {
                        self.ai_mode = AiDisplayMode::from_str(&value);
                        if self.ai_mode != AiDisplayMode::BubbleMinimized
                            && self.chat_channel == ChatChannel::Ai
                        {
                            self.unread_ai = 0;
                        }
                    }
                    "show_unread_badges" => {
                        self.show_unread_badges = Self::parse_bool_str(&value, true);
                    }
                    "always_show_floating_orbs" => {
                        self.always_show_floating_orbs = Self::parse_bool_str(&value, true);
                    }
                    "ai_dock_width" => {
                        if let Ok(w) = value.trim().parse::<u32>() {
                            self.ai_dock_width = w.clamp(260, 1200);
                        }
                    }
                    "adblock_enabled" => {
                        let b = Self::parse_bool_str(&value, true);
                        self.adblock_enabled = b;
                        ADBLOCK_ENABLED.store(b, Ordering::Relaxed);
                    }
                    "google_user" => {
                        self.google_user = value;
                    }
                    _ => {}
                }
                self.save_config();
                self.relayout();
                self.sync_all_ui();
                self.sync_ai_config_to_panel(false);
                self.broadcast_config_changed();
            }
            AppCmd::OpenAiSettings => {
                self.execute(AppCmd::OpenSettingsModal("ai_cowork".to_string()));
            }
            AppCmd::FocusAiPanel => {
                self.chat_channel = ChatChannel::Ai;
                if self.ai_mode == AiDisplayMode::BubbleMinimized {
                    self.ai_mode = AiDisplayMode::FbPopup;
                }
                self.relayout();
                self.sync_all_ui();
                let _ = self.ai_shell.focus();
            }
            AppCmd::OpenGoogleLogin => {
                if self.settings_open {
                    self.settings_open = false;
                    self.relayout();
                }
                self.execute(AppCmd::NavigateActive(
                    "https://accounts.google.com".to_string(),
                ));
            }
            AppCmd::SetGoogleUser(user) => {
                self.google_user = user;
                self.save_config();
                self.sync_all_ui();
                self.broadcast_config_changed();
            }
            AppCmd::SetUiTheme(th) => {
                self.ui_theme = Self::normalize_theme(&th);
                self.sync_window_titlebar_theme();
                self.sync_window_icon();
                self.save_config();
                self.sync_all_ui();
                self.sync_ai_config_to_panel(false);
                self.broadcast_config_changed();
            }
            AppCmd::NewTab { url, isolated } => {
                self.add_tab(&url, isolated);
            }
            AppCmd::FocusIndex(idx) => {
                if idx < self.tabs.len() {
                    self.active_idx = idx;
                    self.relayout();
                    if let Some(t) = self.tabs.get(self.active_idx) {
                        let _ = t.webview.focus();
                    }
                    self.sync_all_ui();
                    self.save_session();
                }
            }
            AppCmd::FocusLastTab => {
                if !self.tabs.is_empty() {
                    let last = self.tabs.len() - 1;
                    self.execute(AppCmd::FocusIndex(last));
                }
            }
            AppCmd::NextTab => {
                if self.tabs.len() > 1 {
                    let next = (self.active_idx + 1) % self.tabs.len();
                    self.execute(AppCmd::FocusIndex(next));
                }
            }
            AppCmd::PrevTab => {
                if self.tabs.len() > 1 {
                    let prev = if self.active_idx == 0 {
                        self.tabs.len() - 1
                    } else {
                        self.active_idx - 1
                    };
                    self.execute(AppCmd::FocusIndex(prev));
                }
            }
            AppCmd::FocusTabById(tab_id) => {
                if let Some(pos) = self.tabs.iter().position(|t| t.id == tab_id) {
                    if self.active_idx != pos {
                        self.active_idx = pos;
                        self.relayout();
                        if let Some(t) = self.tabs.get(self.active_idx) {
                            let _ = t.webview.focus();
                        }
                        self.sync_all_ui();
                        self.save_session();
                    }
                }
            }
            AppCmd::CloseTabById(tab_id) => {
                if let Some(pos) = self.tabs.iter().position(|t| t.id == tab_id) {
                    self.execute(AppCmd::CloseIndex(pos));
                    self.sync_workspace_view();
                }
            }
            AppCmd::CloseIndex(idx) => {
                if self.tabs.len() > 1 && idx < self.tabs.len() {
                    let removed_id = self.tabs[idx].id;
                    if self.ai_worker_tab_id == Some(removed_id) {
                        self.ai_worker_tab_id = None;
                    }
                    let closed_url = self.tabs[idx].url.clone();
                    if closed_url.starts_with("http://") || closed_url.starts_with("https://") {
                        self.closed_tabs.push(closed_url);
                        if self.closed_tabs.len() > 25 {
                            self.closed_tabs.remove(0);
                        }
                    }
                    self.tabs.remove(idx);
                    if self.active_idx >= self.tabs.len() {
                        self.active_idx = self.tabs.len() - 1;
                    }
                    self.relayout();
                    self.sync_all_ui();
                    self.save_session();
                }
            }
            AppCmd::CloseActiveTab => {
                let idx = self.active_idx;
                self.execute(AppCmd::CloseIndex(idx));
            }
            AppCmd::NavigateActive(raw_url) => {
                let normalized = Self::normalize_url(&raw_url);
                if let Some(tab) = self.tabs.get_mut(self.active_idx) {
                    tab.url = normalized.clone();
                    tab.title = Self::title_from_url(&normalized);
                    if normalized == "tp://newtab" {
                        let _ = tab.webview.load_html(&inject_ui_html(NEWTAB_HTML, &self.ui_theme));
                    } else if normalized == "tp://history" {
                        let _ = tab.webview.load_html(&inject_ui_html(HISTORY_HTML, &self.ui_theme));
                    } else if normalized == "tp://workspace" {
                        let _ = tab.webview.load_html(&inject_ui_html(WORKSPACE_HTML, &self.ui_theme));
                    } else {
                        let _ = tab.webview.load_url(&normalized);
                    }
                }
                if normalized.starts_with("http://") || normalized.starts_with("https://") {
                    let t = Self::title_from_url(&normalized);
                    self.record_history(&t, &normalized);
                } else if normalized == "tp://history" {
                    self.sync_history_view();
                } else if normalized == "tp://workspace" {
                    self.sync_workspace_view();
                }
                self.sync_all_ui();
                self.save_session();
            }
            AppCmd::NavBack => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    let _ = tab.webview.evaluate_script("window.history.back();");
                }
            }
            AppCmd::NavForward => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    let _ = tab.webview.evaluate_script("window.history.forward();");
                }
            }
            AppCmd::ReloadActive => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    if tab.url == "tp://newtab" {
                        let _ = tab.webview.load_html(&inject_ui_html(NEWTAB_HTML, &self.ui_theme));
                    } else if tab.url == "tp://history" {
                        let _ = tab.webview.load_html(&inject_ui_html(HISTORY_HTML, &self.ui_theme));
                    } else if tab.url == "tp://workspace" {
                        let _ = tab.webview.load_html(&inject_ui_html(WORKSPACE_HTML, &self.ui_theme));
                    } else {
                        let _ = tab.webview.reload();
                    }
                }
            }
            AppCmd::HardReloadActive => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    if tab.url == "tp://newtab" {
                        let _ = tab.webview.load_html(&inject_ui_html(NEWTAB_HTML, &self.ui_theme));
                    } else if tab.url == "tp://history" {
                        let _ = tab.webview.load_html(&inject_ui_html(HISTORY_HTML, &self.ui_theme));
                    } else if tab.url == "tp://workspace" {
                        let _ = tab.webview.load_html(&inject_ui_html(WORKSPACE_HTML, &self.ui_theme));
                    } else {
                        let _ = tab.webview.evaluate_script(
                            "try { if (window.caches) caches.keys().then(function(ks){ ks.forEach(function(k){ caches.delete(k); }); }); fetch(location.href, { cache: 'reload', credentials: 'same-origin' }).catch(function(){}).finally(function(){ location.reload(); }); } catch(_) { location.reload(); }",
                        );
                    }
                }
            }
            AppCmd::GoHome => {
                self.execute(AppCmd::NavigateActive("tp://newtab".to_string()));
            }
            AppCmd::FocusUrlBar => {
                if !self.titlebar_pinned && !self.titlebar_hover {
                    self.titlebar_hover = true;
                    self.relayout();
                }
                let _ = self.titlebar_shell.focus();
                let _ = self.titlebar_shell.evaluate_script(
                    "if (window.__tpFocusTitlebarUrl) window.__tpFocusTitlebarUrl();",
                );
            }
            AppCmd::ToggleFindBar => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    let _ = tab.webview.focus();
                    let _ = tab.webview.evaluate_script(
                        "if (window.__tpToggleFindBar) window.__tpToggleFindBar();",
                    );
                }
            }
            AppCmd::ToggleCollectionsDrawer => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    let _ = tab.webview.focus();
                    let bm_raw = if self.bookmarks_json.trim().is_empty() {
                        "[]"
                    } else {
                        &self.bookmarks_json
                    };
                    let js = format!(
                        "if (window.__tpToggleCollections) window.__tpToggleCollections({{\"ui_theme\":\"{}\",\"bookmarks\":{}}});",
                        Self::escape_json(&self.ui_theme),
                        bm_raw
                    );
                    let _ = tab.webview.evaluate_script(&js);
                }
            }
            AppCmd::OpenHistory => {
                if let Some(pos) = self.tabs.iter().position(|t| t.url == "tp://history") {
                    self.execute(AppCmd::FocusIndex(pos));
                } else {
                    self.add_tab("tp://history", false);
                }
                self.sync_history_view();
            }
            AppCmd::GetHistory => {
                self.sync_history_view();
            }
            AppCmd::ClearHistory => {
                self.history_json = "[]".to_string();
                self.save_history();
                self.sync_history_view();
            }
            AppCmd::OpenWorkspace => {
                if let Some(pos) = self.tabs.iter().position(|t| t.url == "tp://workspace") {
                    self.execute(AppCmd::FocusIndex(pos));
                } else {
                    self.add_tab("tp://workspace", false);
                }
                self.sync_workspace_view();
            }
            AppCmd::RunEcomScan { scope } => {
                let s = scope.to_lowercase();
                let run_orders = s == "all" || s == "shopee_orders";
                let run_pas = s == "all" || s == "shopee_pas";
                let run_tiktok = s == "all" || s == "tiktok";
                self.ecom_status_text = format!("⏳ Đang mở Tab ngầm để quét ({})...", scope);
                self.push_ecom_log(&format!("Khởi chạy quét ngầm [{}] — giữ nguyên Tab đang xem.", scope));

                if run_orders {
                    if let Some(tid) = self.ensure_ecom_worker(
                        "banhang.shopee.vn/portal/sale",
                        "https://banhang.shopee.vn/portal/sale/order",
                        "Shopee Đơn Hàng",
                    ) {
                        self.trigger_worker_scan_by_id(tid, "https://banhang.shopee.vn/portal/sale/order");
                    }
                }
                if run_pas {
                    if let Some(tid) = self.ensure_ecom_worker(
                        "banhang.shopee.vn/portal/marketing/pas",
                        "https://banhang.shopee.vn/portal/marketing/pas/index?source_page_id=1&type=new_cpc_homepage&group=today",
                        "Shopee PAS",
                    ) {
                        self.trigger_worker_scan_by_id(tid, "https://banhang.shopee.vn/portal/marketing/pas/index");
                    }
                }
                if run_tiktok {
                    if let Some(tid) = self.ensure_ecom_worker(
                        "seller-vn.tiktok.com",
                        "https://seller-vn.tiktok.com/ads-creation/dashboard",
                        "TikTok Seller Ads",
                    ) {
                        self.trigger_worker_scan_by_id(tid, "https://seller-vn.tiktok.com/ads-creation/dashboard");
                    }
                }
                self.sync_workspace_view();
            }
            AppCmd::SetEcomAutoScan(enabled) => {
                self.ecom_auto_scan = enabled;
                if enabled {
                    self.last_ecom_auto_epoch = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    self.ecom_status_text = "⏱ Đã bật tự động quét ngầm mỗi 15 phút.".to_string();
                } else {
                    self.ecom_status_text = "Đã tắt tự động quét ngầm.".to_string();
                }
                self.push_ecom_log(&self.ecom_status_text.clone());
                self.sync_workspace_view();
            }
            AppCmd::UpdatePlatformData { platform, data_json } => {
                match platform.as_str() {
                    "shopee_orders" => {
                        self.shopee_orders_json = data_json.clone();
                        self.save_export_file("shopee_orders_latest.json", &data_json);
                        self.ecom_status_text = "✅ Đã cập nhật dữ liệu Đơn hàng Shopee (Tab ngầm).".to_string();
                        self.push_ecom_log("Shopee Đơn hàng: đã cập nhật JSON & kiểm toán giá.");
                        for t in &mut self.tabs {
                            if t.worker_kind.as_deref() == Some("bot") && t.url.contains("banhang.shopee.vn/portal/sale") {
                                t.worker_status = if data_json.contains("\"need_login\":true") {
                                    "Cần đăng nhập".to_string()
                                } else {
                                    "Đã lấy xong".to_string()
                                };
                            }
                        }
                    }
                    "shopee_pas" => {
                        self.shopee_pas_json = data_json.clone();
                        self.save_export_file("shopee_pas_latest.json", &data_json);
                        self.ecom_status_text = "✅ Đã cập nhật dữ liệu Quảng cáo Shopee PAS (Tab ngầm).".to_string();
                        self.push_ecom_log("Shopee PAS: đã cập nhật danh sách chiến dịch & ROAS/ACOS.");
                        for t in &mut self.tabs {
                            if t.worker_kind.as_deref() == Some("bot") && t.url.contains("/portal/marketing/pas") {
                                t.worker_status = if data_json.contains("\"need_login\":true") {
                                    "Cần đăng nhập".to_string()
                                } else {
                                    "Đã lấy xong".to_string()
                                };
                            }
                        }
                    }
                    "tiktok" => {
                        self.tiktok_json = data_json.clone();
                        self.save_export_file("tiktok_latest.json", &data_json);
                        let cap = data_json.contains("\"captcha_active\":true");
                        let login = data_json.contains("\"need_login\":true");
                        self.ecom_status_text = if cap {
                            "🧩 TikTok đang yêu cầu kéo CAPTCHA — bấm Mở xem Tab để kéo 1 lần.".to_string()
                        } else if login {
                            "⚠️ TikTok chưa đăng nhập — bấm Mở xem Tab để đăng nhập.".to_string()
                        } else {
                            "✅ Đã cập nhật dữ liệu TikTok Seller Hôm nay (Tab ngầm).".to_string()
                        };
                        self.push_ecom_log(&self.ecom_status_text.clone());
                        for t in &mut self.tabs {
                            if t.worker_kind.as_deref() == Some("bot") && t.url.contains("seller-vn.tiktok.com") {
                                t.worker_status = if cap {
                                    "Cần kéo CAPTCHA".to_string()
                                } else if login {
                                    "Cần đăng nhập".to_string()
                                } else {
                                    "Đã lấy xong".to_string()
                                };
                            }
                        }
                    }
                    _ => {}
                }
                self.sync_workspace_view();
            }
            AppCmd::ExportShopeeCsv => {
                self.export_shopee_csv();
            }
            AppCmd::ReopenClosedTab => {
                if let Some(url) = self.closed_tabs.pop() {
                    self.add_tab(&url, false);
                    self.sync_history_view();
                }
            }
            AppCmd::ZoomStep(delta) => {
                let next = (self.zoom_pct as i32 + delta).clamp(50, 250) as u32;
                self.zoom_pct = next;
                let factor = (next as f64) / 100.0;
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    let _ = tab.webview.zoom(factor);
                }
                self.sync_all_ui();
            }
            AppCmd::ZoomReset => {
                self.zoom_pct = 100;
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    let _ = tab.webview.zoom(1.0);
                }
                self.sync_all_ui();
            }
            AppCmd::SaveFloatingOrbs(orbs_json) => {
                if let Ok(JsonVal::Arr(_)) = parse_json(&orbs_json) {
                    self.floating_orbs_json = orbs_json;
                    self.save_config();
                    self.sync_all_ui();
                    self.broadcast_config_changed();
                }
            }
            AppCmd::OpenCustomOrb { id, kind, url, title } => {
                if kind == "tab" {
                    if !url.is_empty() {
                        if let Some(pos) = self.tabs.iter().position(|t| t.url.starts_with(&url)) {
                            self.execute(AppCmd::FocusIndex(pos));
                        } else {
                            self.add_tab(&url, false);
                        }
                    }
                } else {
                    let same_open = self.ai_mode != AiDisplayMode::BubbleMinimized
                        && self.chat_channel == ChatChannel::Custom
                        && self.custom_popup_id == id;
                    if same_open {
                        self.ai_mode = AiDisplayMode::BubbleMinimized;
                    } else {
                        self.custom_popup_id = id;
                        self.custom_popup_title = title;
                        self.ensure_custom_popup_shell(&url);
                        self.chat_channel = ChatChannel::Custom;
                        if self.ai_mode == AiDisplayMode::BubbleMinimized {
                            self.ai_mode = AiDisplayMode::FbPopup;
                        }
                    }
                    self.relayout();
                    self.sync_all_ui();
                    self.save_config();
                    self.broadcast_config_changed();
                }
            }
            AppCmd::BookmarkActive => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    if tab.url != "tp://newtab" && tab.url != "tp://history" {
                        let target_url = tab.url.clone();
                        let target_title = tab.title.clone();
                        let mut items = match parse_json(&self.bookmarks_json) {
                            Ok(JsonVal::Arr(arr)) => arr,
                            _ => Vec::new(),
                        };
                        let existing_pos = items.iter().position(|it| {
                            it.get("url").and_then(|v| v.as_str()) == Some(target_url.as_str())
                        });
                        let toast_msg = if let Some(pos) = existing_pos {
                            items.remove(pos);
                            "☆ Đã bỏ lưu trang khỏi Bookmarks"
                        } else {
                            items.push(JsonVal::obj(vec![
                                ("title", JsonVal::Str(target_title)),
                                ("url", JsonVal::Str(target_url)),
                            ]));
                            "★ Đã lưu trang vào Bookmarks"
                        };
                        self.bookmarks_json = JsonVal::Arr(items).to_json();
                        let _ = tab.webview.evaluate_script(&format!(
                            "if (window.__tpShowToast) window.__tpShowToast(\"{}\");",
                            Self::escape_json(toast_msg)
                        ));
                        self.save_config();
                        self.sync_all_ui();
                        self.broadcast_config_changed();
                    }
                }
            }
            AppCmd::SaveBookmarks(bm_json) => {
                self.bookmarks_json = bm_json;
                self.save_config();
                self.sync_all_ui();
            }
            AppCmd::SaveAiConfig {
                provider,
                api_key,
                model,
                base_url,
                auto_minimize_on_task,
            } => {
                self.ai_provider = provider;
                self.ai_api_key = api_key;
                self.ai_model = model;
                self.ai_base_url = base_url;
                self.auto_minimize_on_task = auto_minimize_on_task;
                self.save_config();
                self.sync_ai_config_to_panel(false);
                self.sync_all_ui();
                self.broadcast_config_changed();
            }
            AppCmd::SaveSkillsJson(skills_json) => {
                self.skills_json = skills_json;
                self.save_skills();
            }
            AppCmd::CheckReminders => {
                self.check_reminders();
            }
            AppCmd::GetTasks => {
                self.sync_tasks_to_views();
            }
            AppCmd::SaveTasksJson(tasks_json) => {
                self.tasks_json = tasks_json;
                self.save_tasks();
                self.sync_tasks_to_views();
            }
            AppCmd::SnoozeReminder { id, minutes } => {
                let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                let next_epoch = now_secs + minutes * 60;
                let js = format!(
                    "if (window.__tpSnoozeTaskLocal) window.__tpSnoozeTaskLocal(\"{}\", {});",
                    Self::escape_json(&id),
                    next_epoch
                );
                let _ = self.ai_shell.evaluate_script(&js);
            }
            AppCmd::CompleteTask(id) => {
                let js = format!(
                    "if (window.__tpCompleteTaskLocal) window.__tpCompleteTaskLocal(\"{}\");",
                    Self::escape_json(&id)
                );
                let _ = self.ai_shell.evaluate_script(&js);
            }
            AppCmd::CreateTaskFromSelection { text, source, contact } => {
                if self.ai_mode == AiDisplayMode::BubbleMinimized {
                    self.execute(AppCmd::SetAiMode(AiDisplayMode::FbPopup));
                }
                let js = format!(
                    "if (window.__tpReceiveTaskFromSelection) window.__tpReceiveTaskFromSelection({{\\\"text\\\":\\\"{}\\\",\\\"source\\\":\\\"{}\\\",\\\"contact\\\":\\\"{}\\\"}});",
                    Self::escape_json(&text),
                    Self::escape_json(&source),
                    Self::escape_json(&contact)
                );
                let _ = self.ai_shell.evaluate_script(&js);
            }
            AppCmd::ScanZaloChat => {
                if let Some(zl) = &self.zalo_shell {
                    let _ = zl.evaluate_script(
                        "if (window.__tpScanChatConversation) window.__tpScanChatConversation();"
                    );
                } else if let Some(fb) = &self.fb_shell {
                    let _ = fb.evaluate_script(
                        "if (window.__tpScanChatConversation) window.__tpScanChatConversation();"
                    );
                }
            }
            AppCmd::PasteToChat { channel, text } => {
                let js = format!(
                    "if (window.__tpInsertDraftMessage) window.__tpInsertDraftMessage(\"{}\");",
                    Self::escape_json(&text)
                );
                if channel == "zalo" {
                    if let Some(zl) = &self.zalo_shell {
                        let _ = zl.evaluate_script(&js);
                    }
                } else if let Some(fb) = &self.fb_shell {
                    let _ = fb.evaluate_script(&js);
                }
            }
            AppCmd::StartSkillRecording => {
                let _ = self.ai_shell.evaluate_script(
                    "if (window.__tpStartSkillRecordingFromRust) window.__tpStartSkillRecordingFromRust();",
                );
            }
            AppCmd::StopSkillRecording => {
                let _ = self.ai_shell.evaluate_script(
                    "if (window.__tpStopSkillRecordingFromRust) window.__tpStopSkillRecordingFromRust();",
                );
            }
            AppCmd::RecordedSkillStep(step_json) => {
                let js = format!(
                    "if (window.__tpAddRecordedStep) window.__tpAddRecordedStep({});",
                    step_json
                );
                let _ = self.ai_shell.evaluate_script(&js);
            }
            AppCmd::RunAiPrompt(prompt) => {
                let js = format!(
                    "if (window.triggerCoworkTask) window.triggerCoworkTask(\"{}\");",
                    Self::escape_json(&prompt)
                );
                let _ = self.ai_shell.evaluate_script(&js);
            }
            AppCmd::RequestDomContextForAi { use_worker, prompt } => {
                let target_id = if use_worker {
                    let existing = self.ai_worker_tab_id.filter(|id| self.tabs.iter().any(|t| t.id == *id));
                    if let Some(id) = existing {
                        id
                    } else {
                        let lower = prompt.trim().to_lowercase();
                        let initial_url = if lower.starts_with("mở trang ") || lower.starts_with("vào trang ") || lower.starts_with("open ") {
                            let raw = prompt
                                .split_once(' ')
                                .map(|(_, rest)| rest)
                                .unwrap_or("https://www.google.com")
                                .trim();
                            let cleaned = raw.trim_start_matches("trang ").trim();
                            if cleaned.is_empty() {
                                "https://www.google.com".to_string()
                            } else {
                                Self::normalize_url(cleaned)
                            }
                        } else {
                            "https://www.google.com".to_string()
                        };
                        let created = self.add_worker_tab(&initial_url, "ai", Some("AI Cowork"));
                        self.ai_worker_tab_id = created;
                        created.unwrap_or_else(|| self.tabs.get(self.active_idx).map(|t| t.id).unwrap_or(0))
                    }
                } else {
                    self.tabs.get(self.active_idx).map(|t| t.id).unwrap_or(0)
                };
                if let Some(tab) = self.tabs.iter().find(|t| t.id == target_id) {
                    let _ = tab.webview.evaluate_script(
                        "if (window.__tpCollectDomForAi) window.__tpCollectDomForAi();",
                    );
                }
            }
            AppCmd::DomContextResult {
                tab_id,
                title,
                url,
                visible_text,
                elements_json,
            } => {
                let target_idx = self
                    .tabs
                    .iter()
                    .position(|t| t.id == tab_id)
                    .unwrap_or(self.active_idx);
                let mut is_worker = false;
                if let Some(tab) = self.tabs.get_mut(target_idx) {
                    is_worker = tab.worker_kind.is_some();
                    if !title.is_empty() && tab.worker_kind.is_none() {
                        tab.title = title.clone();
                    }
                }
                let mut all_tabs_json = Vec::new();
                for t in &self.tabs {
                    all_tabs_json.push(format!(
                        "{{\"title\":\"{}\",\"url\":\"{}\",\"profile\":\"{}\",\"worker_kind\":\"{}\"}}",
                        Self::escape_json(&t.title),
                        Self::escape_json(&t.url),
                        Self::escape_json(&t.profile_name),
                        Self::escape_json(t.worker_kind.as_deref().unwrap_or(""))
                    ));
                }
                let payload = format!(
                    "{{\"target_tab_id\":{},\"is_worker\":{},\"title\":\"{}\",\"url\":\"{}\",\"visible_text\":\"{}\",\"elements\":{},\"all_tabs\":[{}]}}",
                    tab_id,
                    is_worker,
                    Self::escape_json(&title),
                    Self::escape_json(&url),
                    Self::escape_json(&visible_text),
                    if elements_json.trim().is_empty() {
                        "[]"
                    } else {
                        &elements_json
                    },
                    all_tabs_json.join(",")
                );
                let _ = self.ai_shell.evaluate_script(&format!(
                    "if (window.__tpReceiveDomContext) window.__tpReceiveDomContext({});",
                    payload
                ));
                self.sync_all_ui();
            }
            AppCmd::ExecuteDomAction {
                target_tab_id,
                action_json,
            } => {
                let from_worker = target_tab_id.is_some()
                    && target_tab_id
                        .and_then(|id| self.tabs.iter().find(|t| t.id == id))
                        .map(|t| t.worker_kind.is_some())
                        .unwrap_or(false);
                if from_worker {
                    if let Ok(val) = parse_json(&action_json) {
                        if val.get("type").and_then(|v| v.as_str()) == Some("new_tab") {
                            let new_u = val
                                .get("url")
                                .and_then(|v| v.as_str())
                                .unwrap_or("https://www.google.com");
                            if let Some(new_id) = self.add_worker_tab(new_u, "ai", Some("AI Cowork")) {
                                self.ai_worker_tab_id = Some(new_id);
                            }
                            return;
                        }
                    }
                }
                let target_tab = target_tab_id
                    .and_then(|id| self.tabs.iter().find(|t| t.id == id))
                    .or_else(|| self.tabs.get(self.active_idx));
                if let Some(tab) = target_tab {
                    let js = format!(
                        "if (window.__tpExecuteDomAction) window.__tpExecuteDomAction({});",
                        action_json
                    );
                    let _ = tab.webview.evaluate_script(&js);
                }
            }
            AppCmd::SyncBubbleStatus {
                working,
                recording,
                status_text,
                last_reply,
            } => {
                let was_working = self.ai_working;
                self.ai_working = working;
                self.ai_recording = recording;
                if was_working && !working && !recording {
                    self.ai_finished_flash = true;
                    if self.ai_mode == AiDisplayMode::BubbleMinimized
                        || self.chat_channel != ChatChannel::Ai
                    {
                        self.unread_ai = self.unread_ai.saturating_add(1);
                    }
                    if self.show_unread_badges {
                        self.chime_seq = self.chime_seq.saturating_add(1);
                        self.chime_kind = "ai_done".to_string();
                    }
                }
                if !status_text.is_empty() {
                    self.ai_status_text = status_text;
                }
                if !last_reply.is_empty() {
                    self.ai_last_reply = last_reply;
                }
                self.sync_all_ui();
            }
            AppCmd::PageFinished { tab_id, url } => {
                let mut finished_title = String::new();
                let mut is_bot_worker = false;
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                    is_bot_worker = tab.worker_kind.as_deref() == Some("bot");
                    if url.starts_with("http://") || url.starts_with("https://") {
                        let host_changed = Self::title_from_url(&tab.url) != Self::title_from_url(&url);
                        tab.url = url.clone();
                        if !is_bot_worker && (host_changed || tab.title.is_empty()) {
                            tab.title = Self::title_from_url(&url);
                            tab.favicon_url.clear();
                        }
                        if tab.worker_kind.is_none() {
                            finished_title = tab.title.clone();
                        }
                    }
                }
                if is_bot_worker {
                    self.trigger_worker_scan_by_id(tab_id, &url);
                    self.sync_workspace_view();
                }
                if !finished_title.is_empty() {
                    self.record_history(&finished_title, &url);
                }
                if url == "tp://history" {
                    self.sync_history_view();
                } else if url == "tp://workspace" {
                    self.sync_workspace_view();
                }
                self.sync_ai_config_to_panel(false);
                self.sync_all_ui();
                self.save_session();
            }
            AppCmd::McpDispatch(msg) => {
                self.handle_mcp_msg(msg);
            }
            AppCmd::McpJsCallback {
                req_id,
                ok,
                payload_json,
            } => {
                if let Some(reply) = self.mcp_pending.remove(&req_id) {
                    let res = if ok {
                        Ok(payload_json)
                    } else {
                        Err(payload_json)
                    };
                    let _ = reply.send(res);
                }
            }
        }
    }

    fn resolve_mcp_webview(&mut self, target: &str) -> Result<&WebView, String> {
        let t = target.trim().to_ascii_lowercase();
        if t == "zalo" {
            self.ensure_zalo_shell();
            return self
                .zalo_shell
                .as_ref()
                .ok_or_else(|| "Zalo WebView unavailable".to_string());
        }
        if t == "fb" || t == "messenger" {
            self.ensure_fb_shell();
            return self
                .fb_shell
                .as_ref()
                .ok_or_else(|| "Messenger WebView unavailable".to_string());
        }
        if t == "titlebar" {
            return Ok(&self.titlebar_shell);
        }
        if t == "tab_shell" {
            return Ok(&self.tab_shell);
        }
        if t == "settings" {
            return Ok(&self.settings_shell);
        }
        if t == "ai" {
            return Ok(&self.ai_shell);
        }
        if t == "workspace" {
            return self
                .tabs
                .iter()
                .find(|tab| tab.url == "tp://workspace")
                .map(|tab| &tab.webview)
                .ok_or_else(|| "Workspace tab not open".to_string());
        }
        if t.is_empty() || t == "active" {
            return self
                .tabs
                .get(self.active_idx)
                .map(|tab| &tab.webview)
                .ok_or_else(|| "No active tab".to_string());
        }
        if let Ok(id) = t.parse::<u64>() {
            return self
                .tabs
                .iter()
                .find(|tab| tab.id == id)
                .map(|tab| &tab.webview)
                .ok_or_else(|| format!("Tab id {} not found", id));
        }
        Err(format!("Unknown target '{}'. Use 'active', 'zalo', 'fb', or numeric tab_id.", target))
    }

    fn dispatch_mcp_eval(
        &mut self,
        target: &str,
        js_body: &str,
        reply: mpsc::Sender<Result<String, String>>,
    ) {
        let req_id = self.next_mcp_req_id;
        self.next_mcp_req_id = self.next_mcp_req_id.wrapping_add(1).max(1);
        let wrapped = format!(
            "(async function(){{ try {{ var __res = await (async function(){{ {} }})(); var __out = (__res === undefined) ? null : __res; window.ipc.postMessage(JSON.stringify({{ cmd: \"mcp_callback\", req_id: {}, ok: true, data: __out }})); }} catch (err) {{ window.ipc.postMessage(JSON.stringify({{ cmd: \"mcp_callback\", req_id: {}, ok: false, error: String(err && err.stack ? err.stack : err) }})); }} }})();",
            js_body, req_id, req_id
        );
        self.mcp_pending.insert(req_id, reply.clone());
        let eval_res = match self.resolve_mcp_webview(target) {
            Ok(wv) => wv
                .evaluate_script(&wrapped)
                .map_err(|e| format!("evaluate_script failed: {:?}", e)),
            Err(e) => Err(e),
        };
        if let Err(e) = eval_res {
            self.mcp_pending.remove(&req_id);
            let _ = reply.send(Err(e));
        }
    }

    fn mcp_tabs_snapshot(&self) -> String {
        let arr: Vec<JsonVal> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                JsonVal::obj(vec![
                    ("index", JsonVal::Num(i as f64)),
                    ("tab_id", JsonVal::Num(t.id as f64)),
                    ("title", JsonVal::Str(t.title.clone())),
                    ("url", JsonVal::Str(t.url.clone())),
                    ("profile", JsonVal::Str(t.profile_name.clone())),
                    ("active", JsonVal::Bool(i == self.active_idx)),
                ])
            })
            .collect();
        JsonVal::obj(vec![
            ("active_index", JsonVal::Num(self.active_idx as f64)),
            ("tab_count", JsonVal::Num(self.tabs.len() as f64)),
            ("view_mode", JsonVal::Str(self.view_mode.as_str().to_string())),
            ("ai_mode", JsonVal::Str(self.ai_mode.as_str().to_string())),
            (
                "chat_channel",
                JsonVal::Str(self.chat_channel.as_str().to_string()),
            ),
            ("tabs", JsonVal::Arr(arr)),
        ])
        .to_json()
    }

    fn handle_mcp_msg(&mut self, msg: McpBridgeMsg) {
        match msg {
            McpBridgeMsg::HealthCheck { reply } => {
                let info = JsonVal::obj(vec![
                    ("ok", JsonVal::Bool(true)),
                    ("app", JsonVal::Str("TPbrowser".to_string())),
                    ("version", JsonVal::Str("0.4.0".to_string())),
                    ("tabs", JsonVal::Num(self.tabs.len() as f64)),
                ])
                .to_json();
                let _ = reply.send(info);
            }
            McpBridgeMsg::ReadResource { uri, reply } => {
                let res = match uri.as_str() {
                    "tpbrowser://tabs" => Ok(self.mcp_tabs_snapshot()),
                    "tpbrowser://tasks" => Ok(self.tasks_json.clone()),
                    "tpbrowser://skills" => Ok(self.skills_json.clone()),
                    _ => Err(format!("Unknown resource URI: {}", uri)),
                };
                let _ = reply.send(res);
            }
            McpBridgeMsg::ToolCall {
                tool,
                args_json,
                reply,
            } => {
                let args = parse_json(&args_json).unwrap_or_else(|_| JsonVal::obj(vec![]));
                match tool.as_str() {
                    "tp_list_tabs" => {
                        let _ = reply.send(Ok(self.mcp_tabs_snapshot()));
                    }
                    "tp_open_tab" => {
                        let url = args
                            .get("url")
                            .and_then(|v| v.as_str())
                            .unwrap_or("tp://newtab")
                            .to_string();
                        let isolated = args
                            .get("isolated")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        self.add_tab(&url, isolated);
                        let created = self.tabs.last().map(|t| {
                            JsonVal::obj(vec![
                                ("ok", JsonVal::Bool(true)),
                                ("index", JsonVal::Num((self.tabs.len() - 1) as f64)),
                                ("tab_id", JsonVal::Num(t.id as f64)),
                                ("url", JsonVal::Str(t.url.clone())),
                                ("title", JsonVal::Str(t.title.clone())),
                            ])
                            .to_json()
                        });
                        let _ = reply.send(created.ok_or_else(|| "Failed to open tab".to_string()));
                    }
                    "tp_focus_tab" => {
                        let idx_opt = args
                            .get("tab_id")
                            .and_then(|v| v.as_u64())
                            .and_then(|tid| self.tabs.iter().position(|t| t.id == tid))
                            .or_else(|| args.get("index").and_then(|v| v.as_u64()).map(|i| i as usize));
                        match idx_opt {
                            Some(idx) if idx < self.tabs.len() => {
                                self.execute(AppCmd::FocusIndex(idx));
                                let _ = reply.send(Ok(self.mcp_tabs_snapshot()));
                            }
                            _ => {
                                let _ = reply.send(Err("Tab not found (provide valid index or tab_id)".to_string()));
                            }
                        }
                    }
                    "tp_close_tab" => {
                        if self.tabs.len() <= 1 {
                            let _ = reply.send(Err("Cannot close the last remaining tab".to_string()));
                            return;
                        }
                        let idx = args
                            .get("tab_id")
                            .and_then(|v| v.as_u64())
                            .and_then(|tid| self.tabs.iter().position(|t| t.id == tid))
                            .or_else(|| args.get("index").and_then(|v| v.as_u64()).map(|i| i as usize))
                            .unwrap_or(self.active_idx);
                        if idx < self.tabs.len() {
                            self.execute(AppCmd::CloseIndex(idx));
                            let _ = reply.send(Ok(self.mcp_tabs_snapshot()));
                        } else {
                            let _ = reply.send(Err("Invalid tab index or tab_id".to_string()));
                        }
                    }
                    "tp_navigate" => {
                        let target_idx = args
                            .get("tab_id")
                            .and_then(|v| v.as_u64())
                            .and_then(|tid| self.tabs.iter().position(|t| t.id == tid))
                            .unwrap_or(self.active_idx);
                        if target_idx >= self.tabs.len() {
                            let _ = reply.send(Err("Target tab not found".to_string()));
                            return;
                        }
                        if target_idx != self.active_idx {
                            self.execute(AppCmd::FocusIndex(target_idx));
                        }
                        let action = args
                            .get("action")
                            .and_then(|v| v.as_str())
                            .unwrap_or("goto")
                            .to_ascii_lowercase();
                        match action.as_str() {
                            "back" => self.execute(AppCmd::NavBack),
                            "forward" => self.execute(AppCmd::NavForward),
                            "reload" => self.execute(AppCmd::ReloadActive),
                            "home" => self.execute(AppCmd::GoHome),
                            _ => {
                                let Some(url) = args.get("url").and_then(|v| v.as_str()) else {
                                    let _ = reply.send(Err("Missing 'url' for navigation".to_string()));
                                    return;
                                };
                                self.execute(AppCmd::NavigateActive(url.to_string()));
                            }
                        }
                        let _ = reply.send(Ok(self.mcp_tabs_snapshot()));
                    }
                    "tp_get_page_context" => {
                        let target = args
                            .get("target")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .or_else(|| args.get("target").and_then(|v| v.as_u64()).map(|n| n.to_string()))
                            .unwrap_or_else(|| "active".to_string());
                        let max_chars = args
                            .get("max_chars")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(9000)
                            .clamp(200, 50000);
                        let js = format!(
                            "var __max={}; if (window.__tpBuildDomContext) {{ return window.__tpBuildDomContext(__max); }} var __els=[], __nodes=document.querySelectorAll(\"input, textarea, button, select, a[href], [role='button'], [contenteditable='true']\"), __nid=1; for (var i=0; i<__nodes.length && __els.length<80; i++) {{ var el=__nodes[i]; var r=el.getBoundingClientRect(); if (r.width<8||r.height<8) continue; el.setAttribute('data-tp-ai-id', String(__nid)); var lbl=(el.getAttribute('aria-label')||el.getAttribute('placeholder')||el.innerText||el.value||'').replace(/\\s+/g,' ').trim().slice(0,65); __els.push({{id:__nid,tag:el.tagName.toLowerCase(),type:el.getAttribute('type')||'',label:lbl}}); __nid++; }} return {{ title:document.title||'', url:window.location.href||'', visible_text:((document.body&&document.body.innerText)||'').slice(0,__max), elements:__els }};",
                            max_chars
                        );
                        self.dispatch_mcp_eval(&target, &js, reply);
                    }
                    "tp_execute_action" => {
                        let target = args
                            .get("tab_id")
                            .and_then(|v| v.as_u64())
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| "active".to_string());
                        let act_literal = JsonVal::Str(args_json.clone()).to_json();
                        let js = format!(
                            "var __act = JSON.parse({}); if (!window.__tpExecuteDomAction) {{ throw new Error('content_agent.js is not active on this page'); }} window.__tpExecuteDomAction(__act); return {{ ok: true, executed: __act.type || '' }};",
                            act_literal
                        );
                        self.dispatch_mcp_eval(&target, &js, reply);
                    }
                    "tp_eval_js" => {
                        let Some(code) = args.get("code").and_then(|v| v.as_str()) else {
                            let _ = reply.send(Err("Missing required 'code' argument".to_string()));
                            return;
                        };
                        let target = args
                            .get("target")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .or_else(|| args.get("target").and_then(|v| v.as_u64()).map(|n| n.to_string()))
                            .unwrap_or_else(|| "active".to_string());
                        let code_lit = JsonVal::Str(code.to_string()).to_json();
                        let js = format!(
                            "var __src = {}; try {{ var __fnExpr = new Function('return (async function(){{ return (' + __src + '); }})()'); return await __fnExpr(); }} catch (__e) {{ if (__e instanceof SyntaxError) {{ var __fnBlock = new Function('return (async function(){{ ' + __src + ' }})()'); return await __fnBlock(); }} throw __e; }}",
                            code_lit
                        );
                        self.dispatch_mcp_eval(&target, &js, reply);
                    }
                    "tp_tasks_list" => {
                        let _ = reply.send(Ok(self.tasks_json.clone()));
                    }
                    "tp_tasks_save" => {
                        let Some(tj) = args.get("tasks_json").and_then(|v| v.as_str()) else {
                            let _ = reply.send(Err("Missing 'tasks_json' string".to_string()));
                            return;
                        };
                        match parse_json(tj) {
                            Ok(JsonVal::Arr(_)) => {
                                self.execute(AppCmd::SaveTasksJson(tj.to_string()));
                                let _ = reply.send(Ok(r#"{"ok":true}"#.to_string()));
                            }
                            _ => {
                                let _ = reply.send(Err("tasks_json must be a valid JSON array".to_string()));
                            }
                        }
                    }
                    "tp_task_complete" => {
                        let Some(id) = args.get("id").and_then(|v| v.as_str()) else {
                            let _ = reply.send(Err("Missing 'id'".to_string()));
                            return;
                        };
                        if self.update_task_via_mcp(id, None) {
                            let _ = reply.send(Ok(JsonVal::obj(vec![
                                ("ok", JsonVal::Bool(true)),
                                ("id", JsonVal::Str(id.to_string())),
                                ("status", JsonVal::Str("done".to_string())),
                            ])
                            .to_json()));
                        } else {
                            let _ = reply.send(Err(format!("Task id '{}' not found", id)));
                        }
                    }
                    "tp_task_snooze" => {
                        let Some(id) = args.get("id").and_then(|v| v.as_str()) else {
                            let _ = reply.send(Err("Missing 'id'".to_string()));
                            return;
                        };
                        let mins = args.get("minutes").and_then(|v| v.as_u64()).unwrap_or(10);
                        let now_secs = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        let next_epoch = now_secs + mins * 60;
                        if self.update_task_via_mcp(id, Some(next_epoch)) {
                            let _ = reply.send(Ok(JsonVal::obj(vec![
                                ("ok", JsonVal::Bool(true)),
                                ("id", JsonVal::Str(id.to_string())),
                                ("remind_at_epoch", JsonVal::Num(next_epoch as f64)),
                            ])
                            .to_json()));
                        } else {
                            let _ = reply.send(Err(format!("Task id '{}' not found", id)));
                        }
                    }
                    "tp_chat_open" => {
                        let ch = args
                            .get("channel")
                            .and_then(|v| v.as_str())
                            .unwrap_or("zalo");
                        self.execute(AppCmd::OpenChatChannel(ChatChannel::from_str(ch)));
                        let _ = reply.send(Ok(JsonVal::obj(vec![
                            ("ok", JsonVal::Bool(true)),
                            ("channel", JsonVal::Str(ch.to_string())),
                        ])
                        .to_json()));
                    }
                    "tp_chat_scan" => {
                        let ch = args
                            .get("channel")
                            .and_then(|v| v.as_str())
                            .unwrap_or("zalo");
                        let target = if ch.eq_ignore_ascii_case("fb")
                            || ch.eq_ignore_ascii_case("messenger")
                        {
                            "fb"
                        } else {
                            "zalo"
                        };
                        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(15).clamp(1, 80);
                        let js = format!(
                            "var __ch='{}', __lim={}; var contact=''; if (__ch==='zalo') {{ var h=document.querySelector(\".header-title, .conv-item.active .conv-item-title, [data-id*='chat-name'], .card-name\"); contact=(h&&h.textContent.trim())||(document.title||'').replace(/ - Zalo.*$/i,'').trim()||'Zalo'; }} else {{ var fh=document.querySelector(\"h1, [role='main'] h2, [aria-label*='Conversation']\"); contact=(fh&&fh.textContent.trim())||(document.title||'').replace(/ - Messenger.*$/i,'').trim()||'Messenger'; }} var sel=__ch==='zalo' ? \".chat-message, .message-view, .text-message, .bubble-content, [data-id*='bubble']\" : \"[role='row'] [dir='auto'], [data-scope='messages_table']\"; var nodes=document.querySelectorAll(sel); var msgs=[]; var start=Math.max(0, nodes.length - __lim); for (var i=start; i<nodes.length; i++) {{ var t=(nodes[i].textContent||'').trim(); if (t && t.length>1) msgs.push(t); }} return {{ channel:__ch, contact:contact, url:window.location.href||'', count:msgs.length, messages:msgs }};",
                            target, limit
                        );
                        self.dispatch_mcp_eval(target, &js, reply);
                    }
                    "tp_chat_draft" => {
                        let ch = args
                            .get("channel")
                            .and_then(|v| v.as_str())
                            .unwrap_or("zalo");
                        let Some(text) = args.get("text").and_then(|v| v.as_str()) else {
                            let _ = reply.send(Err("Missing 'text'".to_string()));
                            return;
                        };
                        if ch.eq_ignore_ascii_case("fb") || ch.eq_ignore_ascii_case("messenger") {
                            self.ensure_fb_shell();
                        } else {
                            self.ensure_zalo_shell();
                        }
                        self.execute(AppCmd::PasteToChat {
                            channel: ch.to_string(),
                            text: text.to_string(),
                        });
                        let _ = reply.send(Ok(JsonVal::obj(vec![
                            ("ok", JsonVal::Bool(true)),
                            ("channel", JsonVal::Str(ch.to_string())),
                            ("drafted_chars", JsonVal::Num(text.chars().count() as f64)),
                        ])
                        .to_json()));
                    }
                    _ => {
                        let _ = reply.send(Err(format!("Unknown tool: {}", tool)));
                    }
                }
            }
        }
    }

    fn update_task_via_mcp(&mut self, id: &str, snooze_epoch: Option<u64>) -> bool {
        let Ok(JsonVal::Arr(mut items)) = parse_json(&self.tasks_json) else {
            return false;
        };
        let mut found = false;
        for item in &mut items {
            if let JsonVal::Obj(pairs) = item {
                let matches = pairs
                    .iter()
                    .any(|(k, v)| k == "id" && v.as_str() == Some(id));
                if matches {
                    found = true;
                    match snooze_epoch {
                        Some(ep) => {
                            Self::set_json_pair(pairs, "remind_at_epoch", JsonVal::Num(ep as f64));
                            Self::set_json_pair(pairs, "reminded", JsonVal::Bool(false));
                        }
                        None => {
                            Self::set_json_pair(pairs, "status", JsonVal::Str("done".to_string()));
                            Self::set_json_pair(pairs, "reminded", JsonVal::Bool(true));
                        }
                    }
                }
            }
        }
        if found {
            self.tasks_json = JsonVal::Arr(items).to_json();
            self.save_tasks();
            self.sync_tasks_to_views();
        }
        found
    }

    fn set_json_pair(pairs: &mut Vec<(String, JsonVal)>, key: &str, val: JsonVal) {
        if let Some((_, existing)) = pairs.iter_mut().find(|(k, _)| k == key) {
            *existing = val;
        } else {
            pairs.push((key.to_string(), val));
        }
    }
}

fn main() -> wry::Result<()> {
    if env::args().any(|a| a == "--mcp") {
        std::process::exit(mcp_server::run_stdio_bridge());
    }

    let event_loop = EventLoopBuilder::<AppCmd>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    let window_builder = WindowBuilder::new()
        .with_title("TPbrowser — Trình Duyệt Cá Nhân & AI Cowork")
        .with_decorations(false)
        .with_window_icon(TpBrowserApp::build_window_icon("ribbon", "obsidian-gold"))
        .with_inner_size(LogicalSize::new(1480, 900))
        .with_min_inner_size(LogicalSize::new(860, 560));

    #[cfg(target_os = "windows")]
    let window_builder = window_builder.with_undecorated_shadow(true);

    let window = window_builder
        .build(&event_loop)
        .expect("Failed to create TPbrowser window");

    let mut app = TpBrowserApp::new(window, proxy, exe_dir)?;
    let mut modifiers = ModifiersState::empty();

    let proxy_mcp = app.proxy.clone();
    mcp_server::start_http_server(move |msg| {
        let _ = proxy_mcp.send_event(AppCmd::McpDispatch(msg));
    });

    let proxy_reminder = app.proxy.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(15));
            let _ = proxy_reminder.send_event(AppCmd::CheckReminders);
        }
    });

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    app.save_config();
                    app.save_skills();
                    app.save_tasks();
                    app.save_session();
                    *control_flow = ControlFlow::Exit;
                }
                WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                    app.relayout();
                }
                WindowEvent::ModifiersChanged(new_mods) => {
                    modifiers = new_mods;
                }
                WindowEvent::KeyboardInput {
                    event:
                        KeyEvent {
                            physical_key,
                            state: ElementState::Pressed,
                            ..
                        },
                    ..
                } => {
                    if physical_key == KeyCode::F5 && !modifiers.alt_key() && !modifiers.shift_key() {
                        if modifiers.control_key() {
                            app.execute(AppCmd::HardReloadActive);
                        } else {
                            app.execute(AppCmd::ReloadActive);
                        }
                    } else if physical_key == KeyCode::F3
                        && !modifiers.control_key()
                        && !modifiers.alt_key()
                    {
                        app.execute(AppCmd::ToggleFindBar);
                    } else if physical_key == KeyCode::F6
                        && !modifiers.control_key()
                        && !modifiers.alt_key()
                        && !modifiers.shift_key()
                    {
                        app.execute(AppCmd::FocusUrlBar);
                    } else if physical_key == KeyCode::Escape && app.settings_open {
                        app.execute(AppCmd::CloseSettingsModal);
                    } else if modifiers.control_key() && !modifiers.alt_key() {
                        match physical_key {
                            KeyCode::KeyT if modifiers.shift_key() => {
                                app.execute(AppCmd::ReopenClosedTab)
                            }
                            KeyCode::KeyT if !modifiers.shift_key() => app.execute(AppCmd::NewTab {
                                url: "tp://newtab".to_string(),
                                isolated: false,
                            }),
                            KeyCode::KeyW | KeyCode::F4 if !modifiers.shift_key() => {
                                app.execute(AppCmd::CloseActiveTab)
                            }
                            KeyCode::Tab if modifiers.shift_key() => app.execute(AppCmd::PrevTab),
                            KeyCode::Tab => app.execute(AppCmd::NextTab),
                            KeyCode::PageDown if !modifiers.shift_key() => {
                                app.execute(AppCmd::NextTab)
                            }
                            KeyCode::PageUp if !modifiers.shift_key() => {
                                app.execute(AppCmd::PrevTab)
                            }
                            KeyCode::KeyR if modifiers.shift_key() => {
                                app.execute(AppCmd::HardReloadActive)
                            }
                            KeyCode::KeyR => app.execute(AppCmd::ReloadActive),
                            KeyCode::KeyL if !modifiers.shift_key() => {
                                app.execute(AppCmd::FocusUrlBar)
                            }
                            KeyCode::KeyD if !modifiers.shift_key() => {
                                app.execute(AppCmd::BookmarkActive)
                            }
                            KeyCode::KeyF if !modifiers.shift_key() => {
                                app.execute(AppCmd::ToggleFindBar)
                            }
                            KeyCode::KeyH if !modifiers.shift_key() => {
                                app.execute(AppCmd::OpenHistory)
                            }
                            KeyCode::Equal | KeyCode::NumpadAdd => app.execute(AppCmd::ZoomStep(10)),
                            KeyCode::Minus | KeyCode::NumpadSubtract => {
                                app.execute(AppCmd::ZoomStep(-10))
                            }
                            KeyCode::Digit0 | KeyCode::Numpad0 if !modifiers.shift_key() => {
                                app.execute(AppCmd::ZoomReset)
                            }
                            KeyCode::Digit1 | KeyCode::Numpad1 if !modifiers.shift_key() => {
                                app.execute(AppCmd::FocusIndex(0))
                            }
                            KeyCode::Digit2 | KeyCode::Numpad2 if !modifiers.shift_key() => {
                                app.execute(AppCmd::FocusIndex(1))
                            }
                            KeyCode::Digit3 | KeyCode::Numpad3 if !modifiers.shift_key() => {
                                app.execute(AppCmd::FocusIndex(2))
                            }
                            KeyCode::Digit4 | KeyCode::Numpad4 if !modifiers.shift_key() => {
                                app.execute(AppCmd::FocusIndex(3))
                            }
                            KeyCode::Digit5 | KeyCode::Numpad5 if !modifiers.shift_key() => {
                                app.execute(AppCmd::FocusIndex(4))
                            }
                            KeyCode::Digit6 | KeyCode::Numpad6 if !modifiers.shift_key() => {
                                app.execute(AppCmd::FocusIndex(5))
                            }
                            KeyCode::Digit7 | KeyCode::Numpad7 if !modifiers.shift_key() => {
                                app.execute(AppCmd::FocusIndex(6))
                            }
                            KeyCode::Digit8 | KeyCode::Numpad8 if !modifiers.shift_key() => {
                                app.execute(AppCmd::FocusIndex(7))
                            }
                            KeyCode::Digit9 | KeyCode::Numpad9 if !modifiers.shift_key() => {
                                app.execute(AppCmd::FocusLastTab)
                            }
                            _ => {}
                        }
                    } else if modifiers.alt_key() && !modifiers.control_key() && !modifiers.shift_key()
                    {
                        match physical_key {
                            KeyCode::ArrowLeft => app.execute(AppCmd::NavBack),
                            KeyCode::ArrowRight => app.execute(AppCmd::NavForward),
                            KeyCode::Home => app.execute(AppCmd::GoHome),
                            KeyCode::KeyD => app.execute(AppCmd::FocusUrlBar),
                            KeyCode::Comma => app.execute(AppCmd::ToggleSettingsModal),
                            KeyCode::KeyA => {
                                app.execute(AppCmd::ToggleChatChannel(ChatChannel::Ai))
                            }
                            KeyCode::KeyZ => {
                                app.execute(AppCmd::ToggleChatChannel(ChatChannel::Zalo))
                            }
                            KeyCode::KeyF => {
                                app.execute(AppCmd::ToggleChatChannel(ChatChannel::Messenger))
                            }
                            KeyCode::KeyM => app.execute(AppCmd::CycleViewMode),
                            KeyCode::KeyT => app.execute(AppCmd::ToggleTabOrientation),
                            KeyCode::KeyS => app.execute(AppCmd::ToggleSidebarCollapse),
                            KeyCode::KeyB => app.execute(AppCmd::ToggleCollectionsDrawer),
                            KeyCode::KeyP => app.execute(AppCmd::NewTab {
                                url: "tp://newtab".to_string(),
                                isolated: true,
                            }),
                            KeyCode::KeyW => app.execute(AppCmd::OpenWorkspace),
                            _ => {}
                        }
                    }
                }
                _ => {}
            },
            Event::UserEvent(cmd) => {
                app.execute(cmd);
                if app.should_exit {
                    *control_flow = ControlFlow::Exit;
                }
            }
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_bool_and_field_str() {
        let sample = r#"{
  "ui_theme": "carrara-light",
  "floating_omnibox": false,
  "compact_titlebar": true,
  "adblock_enabled": false,
  "show_unread_badges": true,
  "ai_dock_width": 440
}"#;
        assert_eq!(
            TpBrowserApp::extract_field_str(sample, "ui_theme").as_deref(),
            Some("carrara-light")
        );
        assert_eq!(TpBrowserApp::extract_bool(sample, "floating_omnibox"), Some(false));
        assert_eq!(TpBrowserApp::extract_bool(sample, "compact_titlebar"), Some(true));
        assert_eq!(TpBrowserApp::extract_bool(sample, "adblock_enabled"), Some(false));
        assert_eq!(TpBrowserApp::extract_bool(sample, "show_unread_badges"), Some(true));
        assert_eq!(TpBrowserApp::extract_u64(sample, "\"ai_dock_width\":"), Some(440));
    }

    #[test]
    fn test_extract_setting_value_variants() {
        let s1 = r#"{"cmd":"save_setting","key":"ui_theme","value":"carrara-light"}"#;
        let s2 = r#"{"cmd":"save_setting","key":"floating_omnibox","value":false}"#;
        let s3 = r#"{"cmd":"save_setting","key":"ai_dock_width","value":480}"#;
        assert_eq!(
            TpBrowserApp::extract_setting_value(s1).as_deref(),
            Some("carrara-light")
        );
        assert_eq!(
            TpBrowserApp::extract_setting_value(s2).as_deref(),
            Some("false")
        );
        assert_eq!(
            TpBrowserApp::extract_setting_value(s3).as_deref(),
            Some("480")
        );
        assert!(!TpBrowserApp::parse_bool_str("false", true));
        assert!(TpBrowserApp::parse_bool_str("true", false));
    }

    #[test]
    fn test_settings_modal_html_has_font_marker_and_six_tabs() {
        assert!(SETTINGS_MODAL_HTML.contains("/*__TP_FONT_FACE__*/"));
        let injected = inject_ui_fonts(SETTINGS_MODAL_HTML);
        assert!(injected.contains("@font-face"));
        for tab in [
            "pane_look_feel",
            "pane_tabs",
            "pane_ai_cowork",
            "pane_side_panels",
            "pane_privacy",
            "pane_shortcuts",
        ] {
            assert!(SETTINGS_MODAL_HTML.contains(tab), "missing {}", tab);
        }
        assert!(SETTINGS_MODAL_HTML.contains("orb_manager_list"));
        assert!(SETTINGS_MODAL_HTML.contains("sw_always_show_orbs"));
        assert!(injected.contains("Motion"));
    }

    #[test]
    fn test_unified_topbar_and_history_assets() {
        assert!(TITLEBAR_HTML.contains("tb_url_input"));
        assert!(TITLEBAR_HTML.contains("/*__TP_MOTION_JS__*/"));
        let tb = inject_ui_fonts(TITLEBAR_HTML);
        assert!(tb.contains("Motion"));
        assert!(HISTORY_HTML.contains("/*__TP_MOTION_JS__*/"));
        assert!(NEWTAB_HTML.contains("/*__TP_MOTION_JS__*/"));
    }

    #[test]
    fn test_trusted_types_safety_and_standard_shortcuts() {
        assert!(!CONTENT_AGENT_JS.contains(".innerHTML"));
        assert!(!CONTENT_AGENT_JS.contains(".outerHTML"));
        assert!(!CONTENT_AGENT_JS.contains("insertAdjacentHTML"));
        assert!(CONTENT_AGENT_JS.contains("__tpToggleCollections"));
        assert!(CONTENT_AGENT_JS.contains("MutationObserver"));
        assert!(SHELL_SHORTCUTS_JS.contains("toggle_collections_drawer"));
        assert!(SHELL_SHORTCUTS_JS.contains("next_tab"));
        assert!(SHELL_SHORTCUTS_JS.contains("prev_tab"));
        assert!(SHELL_SHORTCUTS_JS.contains("hard_reload"));
        assert!(SETTINGS_MODAL_HTML.contains("Alt + ,"));
        assert!(SETTINGS_MODAL_HTML.contains("Alt + B"));
        assert!(SETTINGS_MODAL_HTML.contains("Ctrl + D"));
    }

    #[test]
    fn test_brand_logo_and_icon_assets() {
        assert_eq!(WIN_RIBBON_LIGHT_RGBA.len(), 64 * 64 * 4);
        assert_eq!(WIN_RIBBON_DARK_RGBA.len(), 64 * 64 * 4);
        assert_eq!(WIN_ARROW_LIGHT_RGBA.len(), 64 * 64 * 4);
        assert_eq!(WIN_ARROW_DARK_RGBA.len(), 64 * 64 * 4);

        for style in ["ribbon", "arrow"] {
            for theme in ["obsidian-gold", "carrara-light"] {
                assert!(
                    TpBrowserApp::build_window_icon(style, theme).is_some(),
                    "failed window icon for {} / {}",
                    style,
                    theme
                );
            }
        }
        assert_eq!(TpBrowserApp::normalize_logo_style("arrow"), "arrow");
        assert_eq!(TpBrowserApp::normalize_logo_style("unknown"), "ribbon");

        for html in [TITLEBAR_HTML, SETTINGS_MODAL_HTML, NEWTAB_HTML, HISTORY_HTML, TAB_SHELL_HTML] {
            assert!(html.contains("/*__TP_BRAND_ASSETS_JS__*/"));
            let injected = inject_ui_fonts(html);
            assert!(injected.contains("window.__TP_BRAND_ASSETS"));
            assert!(injected.contains("window.__tpResolveBrandAsset"));
        }
        assert!(TITLEBAR_HTML.contains("tb_brand_icon"));
        assert!(SETTINGS_MODAL_HTML.contains("zs_brand_icon"));
        assert!(SETTINGS_MODAL_HTML.contains("seg_logo_ribbon"));
        assert!(SETTINGS_MODAL_HTML.contains("seg_logo_arrow"));
        assert!(NEWTAB_HTML.contains("nt_brand_logo"));
        assert!(NEWTAB_HTML.contains("tp_favicon"));
        assert!(HISTORY_HTML.contains("tp_favicon"));
    }

    #[test]
    fn test_compact_icon_ui_and_sidebar_resizer() {
        assert!(SIDEBAR_RESIZER_HTML.contains("start_sidebar_resize"));
        assert!(SIDEBAR_RESIZER_HTML.contains("reset_sidebar_width"));
        assert!(TITLEBAR_HTML.contains("open_settings_modal"));
        assert!(TITLEBAR_HTML.contains("tb_horiz_tabs"));
        assert!(!TITLEBAR_HTML.contains(">Cài đặt<"));
        assert!(!CHAT_HEADER_HTML.contains("tab_ai"));
        assert!(AI_PANEL_HTML.contains("btn_ai_dock_toggle"));
        assert!(AI_PANEL_HTML.contains("skills_count_badge"));
        assert!(CONTENT_AGENT_JS.contains("tab_meta"));
        assert!(CONTENT_AGENT_JS.contains("pickTabFavicon"));
        assert!(TAB_SHELL_HTML.contains("tab-fav"));
    }

    #[test]
    fn test_settings_modal_and_startup_stability() {
        assert!(!SETTINGS_MODAL_HTML.contains("sw_compact_titlebar"));
        assert!(SETTINGS_MODAL_HTML.contains("seg_startup_restore"));
        assert!(SETTINGS_MODAL_HTML.contains("seg_startup_newtab"));
        assert!(SETTINGS_MODAL_HTML.contains("set_settings_section"));
        assert!(!SETTINGS_MODAL_HTML.contains("var(--border-col)"));
        assert!(!SETTINGS_MODAL_HTML.contains("pill-btn primary"));
        assert!(SETTINGS_MODAL_HTML.contains(".zs-input"));
        assert!(SETTINGS_MODAL_HTML.contains(".setting-info > b"));

        for html in [
            TITLEBAR_HTML,
            TAB_SHELL_HTML,
            NEWTAB_HTML,
            HISTORY_HTML,
            CHAT_HEADER_HTML,
            DOCK_RESIZER_HTML,
            SIDEBAR_RESIZER_HTML,
        ] {
            assert!(html.contains("ui_shell_ready"));
        }

        let light_tb = inject_ui_html(TITLEBAR_HTML, "carrara-light");
        assert!(light_tb.contains("data-theme=\"carrara-light\""));
        let light_nt = inject_ui_html(NEWTAB_HTML, "carrara-light");
        assert!(light_nt.contains("data-theme=\"carrara-light\""));
    }

    #[test]
    fn test_workspace_and_background_worker_tabs() {
        assert!(WORKSPACE_HTML.contains("__tpSyncWorkspace"));
        assert!(WORKSPACE_HTML.contains("run_ecom_scan"));
        assert!(WORKSPACE_HTML.contains("export_shopee_csv"));
        assert!(ECOM_AGENTS_JS.contains("__tpRunShopeeScan"));
        assert!(ECOM_AGENTS_JS.contains("search_order_list_index"));
        assert!(TIKTOK_PASSIVE_SCAN_JS.contains("update_platform_data"));
        assert!(TITLEBAR_HTML.contains("open_workspace"));
        assert!(TITLEBAR_HTML.contains("tb-worker-badge"));
        assert!(TAB_SHELL_HTML.contains("tab-worker-badge"));
        assert!(AI_PANEL_HTML.contains("quick_worker_tab"));
        assert_eq!(TpBrowserApp::normalize_url("tp://workspace"), "tp://workspace");
        assert_eq!(TpBrowserApp::title_from_url("tp://workspace"), "Không gian TMĐT");
    }
}
