#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod json_mini;
mod layout;
mod mcp_server;
mod shopee_discovery;

use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap, HashSet},
    env, fs,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
        mpsc, OnceLock,
    },
    time::Instant,
};

use json_mini::{parse_json, JsonVal};
use mcp_server::McpBridgeMsg;

#[cfg(test)]
fn parse_workspace_discovery_tab_id(body: &str) -> Result<Option<u64>, ()> {
    let value = parse_json(body).map_err(|_| ())?;
    let cmd = value.get("cmd").and_then(JsonVal::as_str);
    if cmd != Some("discover_shopee_shops") && cmd != Some("shopee_discover_shops") {
        return Err(());
    }
    shopee_discovery::parse_tab_id(value.get("tab_id"))
}

#[cfg(test)]
fn select_workspace_discovery_tab(
    tabs: &[(u64, bool, String, bool)],
    requested: Option<u64>,
) -> Option<u64> {
    tabs.iter()
        .find(|(id, worker, url, ready)| {
            requested.map_or(true, |wanted| *id == wanted)
                && !*worker
                && *ready
                && shopee_discovery::valid_url(url)
        })
        .map(|(id, _, _, _)| *id)
}

fn workspace_discovery_payload(normalized: &str) -> String {
    let Ok(value) = parse_json(normalized) else { return shopee_discovery::unavailable(); };
    let status = value.get("status").and_then(JsonVal::as_str).unwrap_or("unavailable");
    let identity = value.get("account_identity").and_then(JsonVal::as_str).unwrap_or("unavailable");
    let mut fields = vec![
        ("status", JsonVal::Str(status.to_string())),
        ("logged_in", JsonVal::Bool(value.get("logged_in").and_then(JsonVal::as_bool) == Some(true))),
        ("account_identity", JsonVal::Str(identity.to_string())),
    ];
    if let Some(shops) = value.get("shops").and_then(JsonVal::as_arr) {
        fields.push(("shops", JsonVal::Arr(shops.to_vec())));
    }
    JsonVal::obj(fields).to_json()
}

const TIKTOK_ADS_STATUSES: [&str; 7] = ["ok", "partial", "loading", "needs_login", "captcha", "unavailable", "error"];

/// Classify a TikTok Ads payload into the tp.tiktok.ads.v1 status vocabulary.
/// Fail-closed: unparseable -> "error"; unknown status -> "error"; legacy payloads
/// without `status` map login/CAPTCHA/error flags, otherwise "partial" (no provenance).
fn tiktok_ads_status(data_json: &str) -> &'static str {
    let Ok(value) = parse_json(data_json) else { return "error"; };
    if let Some(status) = value.get("status").and_then(JsonVal::as_str) {
        return TIKTOK_ADS_STATUSES.iter().copied().find(|s| *s == status).unwrap_or("error");
    }
    if value.get("captcha_active").and_then(JsonVal::as_bool) == Some(true) {
        "captcha"
    } else if value.get("need_login").and_then(JsonVal::as_bool) == Some(true) {
        "needs_login"
    } else if value.get("error").is_some() {
        "error"
    } else {
        "partial"
    }
}

/// Tag a TikTok payload restored from disk so the workspace never presents it as fresh:
/// `ok` is downgraded to `partial`, `restored_from_disk` is added to `status_reasons`,
/// and `period.is_today` is forced to false (the original period is kept for provenance).
fn mark_tiktok_cache_restored(data_json: &str) -> Option<String> {
    let Ok(JsonVal::Obj(mut pairs)) = parse_json(data_json) else { return None; };
    pairs.retain(|(k, _)| k != "cache_state");
    for (k, v) in pairs.iter_mut() {
        match k.as_str() {
            "status" if v.as_str() == Some("ok") => *v = JsonVal::Str("partial".to_string()),
            "status_reasons" => {
                if let JsonVal::Arr(items) = v {
                    if !items.iter().any(|i| i.as_str() == Some("restored_from_disk")) {
                        items.push(JsonVal::Str("restored_from_disk".to_string()));
                    }
                }
            }
            "period" => {
                if let JsonVal::Obj(p) = v {
                    p.retain(|(pk, _)| pk != "is_today");
                    p.push(("is_today".to_string(), JsonVal::Bool(false)));
                }
            }
            _ => {}
        }
    }
    pairs.push(("cache_state".to_string(), JsonVal::Str("restored_from_disk".to_string())));
    Some(JsonVal::Obj(pairs).to_json())
}

fn tiktok_status_messages(status: &str) -> (&'static str, &'static str) {
    match status {
        "ok" => ("✅ TikTok Ads: đã đọc đủ số liệu và kỳ Hôm nay từ DOM (Tab ngầm, chỉ đọc).", "Đã lấy xong"),
        "partial" => ("◐ TikTok Ads: chỉ đọc được một phần số liệu/kỳ báo cáo — xem nguồn trong Workspace.", "Dữ liệu một phần"),
        "loading" => ("⏳ TikTok Ads: trang chưa tải xong số liệu — chưa có dữ liệu mới.", "Đang tải trang"),
        "needs_login" => ("⚠️ TikTok chưa đăng nhập — bấm Mở xem Tab để đăng nhập.", "Cần đăng nhập"),
        "captcha" => ("🧩 TikTok đang yêu cầu kéo CAPTCHA — bấm Mở xem Tab để kéo 1 lần.", "Cần kéo CAPTCHA"),
        "unavailable" => ("⛔ TikTok Ads: không tìm thấy số liệu trên trang — không dùng dữ liệu cũ.", "Không có dữ liệu"),
        _ => ("❌ TikTok Ads: lỗi khi đọc trang — không dùng dữ liệu cũ.", "Lỗi quét"),
    }
}

use layout::{
    compute_content_panes, compute_fb_chat_popup_rect_with_hdr, compute_glance_rects,
    compute_shell_layout, AiDisplayMode, ContentViewMode, TabOrientation,
    DEFAULT_LEFT_SIDEBAR_WIDTH, FB_CHAT_HEADER_HEIGHT, GLANCE_HEADER_HEIGHT, RIGHT_AI_DOCK_WIDTH,
    SIDEBAR_COLLAPSE_THRESHOLD,
};
#[cfg(target_os = "windows")]
use tao::platform::windows::{WindowBuilderExtWindows, WindowExtWindows};
use tao::{
    dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize},
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget},
    keyboard::{KeyCode, ModifiersState},
    window::{Icon, Window, WindowBuilder, WindowId},
};
use wry::{
    MemoryUsageLevel, NewWindowResponse, PageLoadEvent, PermissionResponse, Rect,
    Theme, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows,
};
#[cfg(target_os = "windows")]
use webview2_com::{
    ContentLoadingEventHandler, NavigationCompletedEventHandler, NavigationStartingEventHandler,
    ProfileGetBrowserExtensionsCompletedHandler, take_pwstr,
    Microsoft::Web::WebView2::Win32::{
        ICoreWebView2, ICoreWebView2Profile, ICoreWebView2Profile7, ICoreWebView2_13,
    },
};
#[cfg(target_os = "windows")]
use windows_core::{BOOL, Interface, PWSTR};

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
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
struct MonitorInfo {
    cb_size: u32,
    monitor: WinRect,
    work: WinRect,
    flags: u32,
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
    fn MonitorFromPoint(point: WinPoint, flags: u32) -> isize;
    fn GetMonitorInfoW(monitor: isize, info: *mut MonitorInfo) -> i32;
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
    fn OpenClipboard(hwnd: isize) -> i32;
    fn CloseClipboard() -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(u_format: u32, h_mem: isize) -> isize;
    fn ShowWindow(hwnd: isize, n_cmd_show: i32) -> i32;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn CreatePopupMenu() -> isize;
    fn AppendMenuW(hmenu: isize, uflags: u32, uidnewitem: usize, lpnewitem: *const u16) -> i32;
    fn TrackPopupMenu(hmenu: isize, uflags: u32, x: i32, y: i32, nreserved: i32, hwnd: isize, prc_rect: *const std::ffi::c_void) -> i32;
    fn DestroyMenu(hmenu: isize) -> i32;
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct NOTIFYICONDATAW {
    cb_size: u32,
    hwnd: isize,
    u_id: u32,
    u_flags: u32,
    u_callback_message: u32,
    h_icon: isize,
    sz_tip: [u16; 128],
    dw_state: u32,
    dw_state_mask: u32,
    sz_info: [u16; 256],
    u_version: u32,
    sz_info_title: [u16; 64],
    dw_info_flags: u32,
    guid_item: [u8; 16],
    h_balloon_icon: isize,
}

#[cfg(target_os = "windows")]
#[link(name = "shell32")]
extern "system" {
    fn Shell_NotifyIconW(dw_message: u32, lp_data: *const NOTIFYICONDATAW) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "comctl32")]
extern "system" {
    fn SetWindowSubclass(
        hwnd: isize,
        pfn_subclass: unsafe extern "system" fn(isize, u32, usize, isize, usize, usize) -> isize,
        uid_subclass: usize,
        ref_data: usize,
    ) -> i32;
    fn DefSubclassProc(hwnd: isize, umsg: u32, wparam: usize, lparam: isize) -> isize;
    fn RemoveWindowSubclass(
        hwnd: isize,
        pfn_subclass: unsafe extern "system" fn(isize, u32, usize, isize, usize, usize) -> isize,
        uid_subclass: usize,
    ) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
#[allow(dead_code)]
extern "system" {
    fn LoadLibraryA(lp_lib_file_name: *const u8) -> isize;
    fn GetProcAddress(h_module: isize, lp_proc_name: *const u8) -> *const std::ffi::c_void;
    fn GlobalAlloc(u_flags: u32, dw_bytes: usize) -> isize;
    fn GlobalLock(h_mem: isize) -> *mut std::ffi::c_void;
    fn GlobalUnlock(h_mem: isize) -> i32;
    fn CreateMutexA(lp_mutex_attributes: *mut std::ffi::c_void, b_initial_owner: i32, lp_name: *const u8) -> isize;
    fn GetLastError() -> u32;
    fn CloseHandle(hobject: isize) -> i32;
}

const CHROME_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

const TAB_SHELL_HTML: &str = include_str!("tab_shell.html");
const NEWTAB_HTML: &str = include_str!("newtab.html");
const HISTORY_HTML: &str = include_str!("history.html");

const AI_PANEL_HTML: &str = include_str!("ai_panel.html");
const CHAT_HEADER_HTML: &str = include_str!("chat_header.html");
const TITLEBAR_HTML: &str = include_str!("titlebar.html");
const DOCK_RESIZER_HTML: &str = include_str!("dock_resizer.html");
const SIDEBAR_RESIZER_HTML: &str = include_str!("sidebar_resizer.html");
const SETTINGS_MODAL_HTML: &str = include_str!("settings_modal.html");
const GLANCE_HEADER_HTML: &str = include_str!("glance_header.html");
const GLANCE_BACKDROP_HTML: &str = r##"<!DOCTYPE html><html><head><meta charset="UTF-8"/><style>
*{margin:0;padding:0;}
html,body{width:100%;height:100%;background:rgba(8,9,12,0.52);cursor:default;}
</style></head><body onmousedown="try{if(window.ipc&&window.ipc.postMessage){window.ipc.postMessage(JSON.stringify({cmd:'close_glance'}));}}catch(_){}"></body></html>"##;
const CONTENT_AGENT_JS: &str = include_str!("content_agent.js");

const MOTION_JS: &str = include_str!("../assets/vendor/motion.js");
const TITLEBAR_HEIGHT: u32 = 40;
const BOOKMARKS_BAR_HEIGHT: u32 = 28;

pub const APP_VERSION: &str = "0.5.6";
pub const APP_CODENAME: &str = "browser-only";
pub const APP_FULL_VERSION: &str = "v0.5.6-browser-only";
pub const APP_GIT_COMMIT: &str = match option_env!("TP_GIT_COMMIT") {
    Some(commit) => commit,
    None => "dev",
};
const UPDATE_API_URL: &str = "https://api.github.com/repos/thienphuaegis-png/TPbrowser/commits?path=src/main.rs&per_page=1";
const UPDATE_BINARY_URL: &str = "https://github.com/thienphuaegis-png/TPbrowser/raw/refs/heads/main/TPbrowser.exe";

// TikTok Identity scanner to detect active logged-in shop name and shop ID.
const TIKTOK_IDENTITY_JS: &str = r##"(function () {
  try {
    var path = String(window.location.pathname || "");
    if (/login|signin|đăng nhập/i.test(path)) {
      if (window.ipc) window.ipc.postMessage(JSON.stringify({ cmd: "update_platform_data", platform: "tiktok_identity", data_json: JSON.stringify({ logged_in: false, need_login: true, shop_name: "", shop_id: "" }) }));
      return;
    }
    var name = "";
    var el = document.querySelector(".seller-header-account-name, [class*='shop-name'], [class*='seller-profile'], [class*='account-name']");
    if (el) {
      var candidate = (el.innerText || "").trim();
      if (!/trung tâm nhà bán hàng|seller center/i.test(candidate)) name = candidate;
    }
    if (!name) {
      var spans = document.querySelectorAll("header span, [class*='header'] span");
      for (var i = 0; i < spans.length; i++) {
        var t = (spans[i].innerText || "").trim();
        if (t && t.length > 2 && t.length < 35 && !/trung tâm nhà bán hàng|seller center|trang chủ|sản phẩm|đơn hàng|quảng cáo|la bàn|tài chính|dịch vụ/i.test(t)) {
          name = t; break;
        }
      }
    }
    if (!name || /trung tâm nhà bán hàng|seller center/i.test(name)) name = "TikTok Shop VN";
    if (window.ipc) {
      window.ipc.postMessage(JSON.stringify({
        cmd: "update_platform_data",
        platform: "tiktok_identity",
        data_json: JSON.stringify({ logged_in: true, shop_name: name, shop_id: "tt_01" })
      }));
    }
  } catch (e) {
    if (window.ipc) window.ipc.postMessage(JSON.stringify({ cmd: "update_platform_data", platform: "tiktok_identity", data_json: JSON.stringify({ logged_in: true, shop_name: "TikTok Shop VN", shop_id: "tt_01" }) }));
  }
})();"##;

// Read-only TikTok Homepage collector (GMV, orders, to_ship).
const TIKTOK_HOMEPAGE_SCAN_JS: &str = r##"(function () {
  var attempt = 0;
  var maxAttempts = 4;

  function parseVND(str) {
    if (!str) return 0;
    var cleaned = String(str).replace(/[^\d.,]/g, "").trim();
    if (!cleaned) return 0;
    if (cleaned.indexOf(".") !== -1 && cleaned.indexOf(",") !== -1) {
      cleaned = cleaned.lastIndexOf(",") > cleaned.lastIndexOf(".")
        ? cleaned.replace(/\./g, "").replace(",", ".")
        : cleaned.replace(/,/g, "");
    } else if (cleaned.indexOf(".") !== -1) {
      cleaned = cleaned.replace(/\./g, "");
    } else if (cleaned.indexOf(",") !== -1) {
      cleaned = cleaned.replace(/,/g, "");
    }
    var num = parseFloat(cleaned);
    return isNaN(num) ? 0 : num;
  }

  function extractNumbersFromCard(cardEl) {
    if (!cardEl) return [];
    var text = cardEl.innerText || "";
    var lines = text.split("\n").map(function(l){ return l.trim(); }).filter(Boolean);
    var results = [];
    for (var i = 0; i < lines.length; i++) {
      var m = lines[i].match(/₫?\s*([\d]+(?:[.,]\d+)*)\s*₫?/);
      if (m) {
        var val = parseVND(m[1]);
        if (val > 0) results.push(val);
      }
    }
    return results;
  }

  function tryScan() {
    attempt++;
    try {
      var path = String(window.location.pathname || "");
      if (/login|signin|đăng nhập/i.test(path)) {
        if (window.ipc) window.ipc.postMessage(JSON.stringify({
          cmd: "scan_need_auth",
          reason: "Yêu cầu đăng nhập TikTok Shop",
          shop_id: "tt_01"
        }));
        return;
      }
      var bodyTxt = (document.body && document.body.innerText) || "";
      if (/xác minh để tiếp tục|kéo mảnh ghép vào vị trí|captcha/i.test(bodyTxt)) {
        if (window.ipc) window.ipc.postMessage(JSON.stringify({
          cmd: "scan_need_auth",
          reason: "Vướng Captcha TikTok Shop",
          shop_id: "tt_01"
        }));
        return;
      }

      var gmv = 0;
      var orders = 0;
      var to_ship = 0;

      var allEls = document.querySelectorAll("div, span, p, h1, h2, h3, h4");
      for (var i = 0; i < allEls.length; i++) {
        var raw = (allEls[i].innerText || "").trim().toLowerCase();
        if (!raw || raw.length > 50 || raw.indexOf("\n") !== -1) continue;

        if (!gmv && (raw.indexOf("doanh thu") !== -1 || raw.indexOf("gmv") !== -1 || raw.indexOf("doanh số") !== -1) && raw.indexOf("so với") === -1 && raw.indexOf("hôm qua") === -1 && raw.indexOf("tăng") === -1 && raw.indexOf("giảm") === -1) {
          var card = allEls[i].closest("[class*='card'], [class*='item'], [class*='statistic'], [class*='module'], [class*='overview'], [class*='box']");
          if (!card) {
            var p = allEls[i].parentElement;
            for (var lvl = 0; lvl < 6 && p; lvl++) {
              if (p.innerText && p.innerText.length > raw.length + 2) {
                card = p;
                break;
              }
              p = p.parentElement;
            }
          }
          if (card) {
            var nums = extractNumbersFromCard(card);
            var validGmv = nums.filter(function(n) { return n >= 1000; });
            if (validGmv.length > 0) {
              gmv = Math.max.apply(null, validGmv);
            }
          }
        }

        if (!orders && (raw.indexOf("đơn hàng") !== -1 || raw.indexOf("số đơn") !== -1 || raw.indexOf("lượt đặt") !== -1) && raw.indexOf("so với") === -1 && raw.indexOf("hôm qua") === -1 && raw.indexOf("tăng") === -1 && raw.indexOf("giảm") === -1) {
          var cardO = allEls[i].closest("[class*='card'], [class*='item'], [class*='statistic'], [class*='module'], [class*='overview'], [class*='box']");
          if (!cardO) {
            var pO = allEls[i].parentElement;
            for (var lvlO = 0; lvlO < 6 && pO; lvlO++) {
              if (pO.innerText && pO.innerText.length > raw.length + 2) {
                cardO = pO;
                break;
              }
              pO = pO.parentElement;
            }
          }
          if (cardO) {
            var numsO = extractNumbersFromCard(cardO);
            if (numsO.length > 0) {
              orders = Math.round(numsO[0]);
            }
          }
        }

        if (!to_ship && (raw.indexOf("chờ vận chuyển") !== -1 || raw.indexOf("chờ lấy hàng") !== -1 || raw.indexOf("chờ xử lý") !== -1 || raw.indexOf("chờ xác nhận") !== -1)) {
          var cardS = allEls[i].closest("[class*='card'], [class*='item'], [class*='statistic'], [class*='module'], [class*='overview'], [class*='tab']");
          if (!cardS) {
            var pS = allEls[i].parentElement;
            for (var lvlS = 0; lvlS < 6 && pS; lvlS++) {
              if (pS.innerText && pS.innerText.length > raw.length + 2) {
                cardS = pS;
                break;
              }
              pS = pS.parentElement;
            }
          }
          if (cardS) {
            var numsS = extractNumbersFromCard(cardS);
            if (numsS.length > 0) {
              to_ship = Math.round(numsS[0]);
            }
          }
        }
      }

      if (!gmv) {
        var mGmv = bodyTxt.match(/(?:Doanh thu|GMV)[^\n\d]{0,25}(?:₫\s*|VND\s*)?([\d.,]+)\s*(?:₫|VND)?/i);
        if (mGmv) {
          var vGmv = parseVND(mGmv[1]);
          if (vGmv >= 1000) gmv = vGmv;
        }
      }
      if (!orders) {
        var mOrd = bodyTxt.match(/(?:Đơn hàng|Số đơn)[^\n\d]{0,25}(\d+)/i);
        if (mOrd) {
          orders = parseInt(mOrd[1], 10) || 0;
        }
      }
      if (!to_ship) {
        var mShip = bodyTxt.match(/(?:Chờ vận chuyển|Chờ lấy hàng)[^\n\d]{0,25}(\d+)/i);
        if (mShip) {
          to_ship = parseInt(mShip[1], 10) || 0;
        }
      }

      if (gmv === 0 && orders === 0 && attempt < maxAttempts) {
        setTimeout(tryScan, 1200);
        return;
      }

      var shopName = "";
      var nameSelectors = [
        "[class*='shop-name']", "[class*='shopName']", "[class*='seller-profile']",
        "[class*='account-name']", ".arco-dropdown-trigger", "[class*='header-profile']",
        "[class*='user-name']", "[class*='store-name']"
      ];
      var nameEls = document.querySelectorAll(nameSelectors.join(", "));
      for (var n = 0; n < nameEls.length; n++) {
        var sCandidate = (nameEls[n].innerText || "").trim().split("\n")[0];
        if (sCandidate && sCandidate.length >= 2 && sCandidate.length < 50 &&
            !/trung tâm nhà bán hàng|seller center|trang chủ|đơn hàng|sản phẩm|quảng cáo|tài chính|la bàn/i.test(sCandidate)) {
          shopName = sCandidate;
          break;
        }
      }

      var payload = {
        platform: "tiktok",
        shop_name: shopName,
        gmv: gmv,
        orders: orders,
        to_ship: to_ship,
        ts: new Date().toLocaleTimeString("vi-VN")
      };

      if (window.ipc) {
        window.ipc.postMessage(JSON.stringify({
          cmd: "update_platform_data",
          platform: "tiktok_homepage",
          data_json: JSON.stringify(payload)
        }));
      }
    } catch (err) {
      if (window.ipc) {
        window.ipc.postMessage(JSON.stringify({
          cmd: "update_platform_data",
          platform: "tiktok_homepage",
          data_json: JSON.stringify({ platform: "tiktok", gmv: 0, orders: 0, error: String(err) })
        }));
      }
    }
  }

  setTimeout(tryScan, 1000);
})();"##;

// Read-only TikTok Products and Stock collector.
const TIKTOK_PRODUCTS_SCAN_JS: &str = r##"(function () {
  var attempt = 0;
  var maxAttempts = 4;

  function tryScan() {
    attempt++;
    try {
      var path = String(window.location.pathname || "");
      if (/login|signin|đăng nhập/i.test(path)) {
        if (window.ipc) window.ipc.postMessage(JSON.stringify({
          cmd: "scan_need_auth",
          reason: "Yêu cầu đăng nhập TikTok Shop",
          shop_id: "tt_01"
        }));
        return;
      }

      var products = [];
      var rows = document.querySelectorAll("tr, [class*='table-row'], [class*='product-item']");
      for (var i = 0; i < rows.length; i++) {
        var rTxt = rows[i].innerText || "";
        if (rTxt.length < 5 || /tên sản phẩm|giá bán|tồn kho|thao tác/i.test(rTxt)) continue;

        var nameEl = rows[i].querySelector("[class*='product-title'], [class*='product-name'], a[href*='/product/']");
        var name = nameEl ? (nameEl.innerText || "").trim() : "";
        if (!name) {
          var lines = rTxt.split("\n").map(function(l){ return l.trim(); }).filter(Boolean);
          if (lines.length > 0) name = lines[0];
        }
        if (!name || name.length < 4 || /^\d+\s*sku$/i.test(name) || /^\d+\s*biến thể$/i.test(name)) continue;

        var skuEl = rows[i].querySelector("[class*='sku']");
        var skuFromEl = skuEl ? (skuEl.innerText || "").replace(/(?:SKU|Mã SP)[:\s]*/i, "").trim() : "";
        var skuMatch = rTxt.match(/(?:SKU|Mã SP)[:\s]*([A-Za-z0-9._-]+)/i);
        var sku = skuFromEl ? skuFromEl : (skuMatch ? skuMatch[1] : "");
        if (sku.length < 2 || /^(s|m|l|xl|xxl|2xl|3xl)$/i.test(sku)) {
          sku = "";
        }

        var stock = 0;
        var stockEl = rows[i].querySelector("[class*='stock'], [class*='inventory'], [class*='quantity'], [class*='count']");
        if (stockEl) {
          var sTxt = (stockEl.innerText || "").trim();
          var sMatch = sTxt.match(/(\d+)/);
          if (sMatch) {
            stock = parseInt(sMatch[1], 10) || 0;
          }
        }
        if (stock === 0) {
          var stockMatch = rTxt.match(/tồn(?:\s*kho)?[:\s]+(\d+)/i) || rTxt.match(/(\d+)\s*(?:cái|sản phẩm|chiếc)\b/i);
          if (stockMatch) {
            stock = parseInt(stockMatch[1], 10) || 0;
          }
        }
        if (stock > 100000) {
          stock = 0;
        }
        if (/hết hàng/i.test(rTxt)) {
          stock = 0;
        }

        var price = 0;
        var priceMatch = rTxt.match(/(?:giá|đơn giá|price)[:\s]*₫?\s*([\d.,]+)/i) || rTxt.match(/₫\s*([\d.,]+)/) || rTxt.match(/([\d.,]+)\s*(?:₫|VND)/i);
        if (priceMatch) {
          var pNum = parseFloat(priceMatch[1].replace(/\./g, "").replace(/,/g, ""));
          if (!isNaN(pNum) && pNum > 0 && pNum < 10000000) {
            price = pNum;
          }
        }

        if (name && products.length < 50) {
          products.push({
            sku: sku,
            name: name.slice(0, 60),
            stock: stock,
            price: price,
            is_anomalous_price: price > 0 && price < 15000,
            is_out_of_stock: stock === 0
          });
        }
      }

      if (products.length === 0 && attempt < maxAttempts) {
        setTimeout(tryScan, 1200);
        return;
      }

      if (window.ipc) {
        window.ipc.postMessage(JSON.stringify({
          cmd: "update_platform_data",
          platform: "tiktok_products",
          data_json: JSON.stringify({ products: products, count: products.length })
        }));
      }
    } catch (err) {
      if (attempt < maxAttempts) {
        setTimeout(tryScan, 1200);
        return;
      }
      if (window.ipc) {
        window.ipc.postMessage(JSON.stringify({
          cmd: "update_platform_data",
          platform: "tiktok_products",
          data_json: JSON.stringify({ products: [], error: String(err) })
        }));
      }
    }
  }

  setTimeout(tryScan, 500);
})();"##;

// Read-only TikTok Seller Ads collector (schema tp.tiktok.ads.v1).
// Contract: never clicks, submits, dispatches events or navigates. Missing values stay
// null (never coerced to 0); every metric carries raw text + DOM provenance; login and
// CAPTCHA fail closed with empty metrics so stale numbers are never re-sent as fresh.
// The only DOM mutation is a view-only horizontal scroll of the virtualized campaign
// table (restored afterwards) so its lazily rendered cells can be read.
const TIKTOK_PASSIVE_SCAN_JS: &str = r##"(function () {
  var SCHEMA = "tp.tiktok.ads.v1";
  var METRIC_KEYS = ["cost", "sales", "roas", "orders", "cpa"];
  function sourceUrl() {
    try { return String(window.location.origin || "") + String(window.location.pathname || ""); } catch (_) { return null; }
  }
  function post(payload) {
    if (window.ipc && typeof window.ipc.postMessage === "function") {
      window.ipc.postMessage(JSON.stringify({ cmd: "update_platform_data", platform: "tiktok_ads", data_json: JSON.stringify(payload) }));
    }
  }
  function unavailableMetric(reason) {
    return { value: null, raw: null, source: "unavailable", label: null, reason: reason || "not_found" };
  }
  function envelope(status, reasons) {
    var metrics = {};
    METRIC_KEYS.forEach(function (k) { metrics[k] = unavailableMetric(status); });
    return {
      schema: SCHEMA,
      status: status,
      status_reasons: reasons || [],
      read_only: true,
      source_url: sourceUrl(),
      captured_at: new Date().toISOString(),
      ts: new Date().toLocaleTimeString("vi-VN"),
      logged_in: null,
      need_login: false,
      captcha_active: false,
      period: { start: null, end: null, is_today: null, source: "unavailable", raw: null },
      period_label: "Không xác định",
      metrics: metrics,
      orders_ads: null,
      cpa_ads: null,
      sales_ads: null,
      cost_ads: null,
      roas_ads: null,
      campaigns: [],
      campaigns_provenance: { source: "unavailable", count: 0, columns: [] }
    };
  }
  function gateStatus() {
    var path = "";
    try { path = String(window.location.pathname || ""); } catch (_) {}
    if (/login|sign[-_ ]?in|đăng nhập/i.test(path)) return "needs_login";
    var bodyTxt = (document.body && document.body.innerText) || "";
    if (/xác minh để tiếp tục|kéo mảnh ghép vào vị trí/i.test(bodyTxt)) return "captcha";
    return null;
  }
  function sendGate(gate) {
    var p = envelope(gate, [gate]);
    if (gate === "needs_login") { p.logged_in = false; p.need_login = true; }
    if (gate === "captcha") { p.captcha_active = true; }
    post(p);
  }
  // Horizontal-scroll guard: any view-only scroll is restored on success AND on failure.
  var scrollGuard = null;
  function restoreScroll() {
    var g = scrollGuard;
    scrollGuard = null;
    if (g) { try { g.el.scrollLeft = g.left; } catch (_) {} }
  }
  function postError(err, where) {
    restoreScroll();
    try {
      var e = envelope("error", ["exception", "exception_in_" + where]);
      e.error = String(err && err.message ? err.message : err);
      post(e);
    } catch (_) {}
  }
  // Every async step (retry timer, table sweep) runs through this guard so an exception
  // still posts an error envelope instead of silently posting nothing.
  function guarded(fn, where) {
    return function () {
      try { fn(); } catch (err) { postError(err, where); }
    };
  }

  try {
    var gate0 = gateStatus();
    if (gate0) { sendGate(gate0); return; }

    function parseVn(s) {
      if (s === null || s === undefined) return null;
      var m0 = String(s).match(/([\d]+(?:[.,]\d+)*)/);
      if (!m0) return null;
      var t = m0[1];
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
      return isNaN(v) ? null : v;
    }

    function fromOverviewCard(matchFn) {
      var nodes = document.querySelectorAll("div, span, p");
      for (var i = 0; i < nodes.length; i++) {
        if (nodes[i].closest("header, nav, [class*='header'], [class*='navbar'], [class*='account'], [class*='profile'], [class*='top-bar'], button")) continue;
        var rawLabel = (nodes[i].innerText || "").trim();
        if (!rawLabel || rawLabel.length > 80) continue;
        var cleanLabel = rawLabel.replace(/[\r\n\t]+/g, " ").replace(/[ⓘ?]/g, "").trim().toLowerCase();
        if (!cleanLabel || !matchFn(cleanLabel)) continue;
        var card = nodes[i].closest("[class*='card'], [class*='statistic'], [class*='item'], [class*='module'], [class*='overview']") || nodes[i].parentElement;
        for (var step = 0; step < 6 && card; step++) {
          var cTxt = card.innerText || "";
          var targetTxt = cTxt.split(/so với|vs\b|compared to/i)[0];
          var lines = targetTxt.split("\n").map(function (x) { return x.trim(); }).filter(Boolean);
          for (var j = 0; j < lines.length; j++) {
            var lClean = lines[j].replace(/[\r\n\t]+/g, " ").replace(/[ⓘ?]/g, "").trim().toLowerCase();
            if (lClean === cleanLabel || lClean.indexOf(cleanLabel) === 0) continue;
            var mm = lines[j].match(/([\d]+(?:[.,]\d+)*)\s*(?:vnd|₫)?/i);
            if (mm) {
              var v = parseVn(mm[1]);
              if (v !== null) return { value: v, raw: lines[j], source: "overview_card", label: rawLabel, reason: null };
            }
          }
          card = card.parentElement;
        }
      }
      return unavailableMetric("overview_card_not_found");
    }

    function getDateInputs() {
      return [].slice.call(document.querySelectorAll("input")).filter(function (el) {
        return /^\d{4}-\d{2}-\d{2}$/.test((el.value || "").trim());
      });
    }

    function localToday() {
      var d = new Date();
      function z(n) { return (n < 10 ? "0" : "") + n; }
      return d.getFullYear() + "-" + z(d.getMonth() + 1) + "-" + z(d.getDate());
    }

    function readPeriod() {
      var inps = getDateInputs();
      var today = localToday();
      if (inps.length >= 2) {
        var a = inps[0].value.trim();
        var b = inps[1].value.trim();
        return { start: a, end: b, is_today: a === b && a === today, source: "date_inputs", raw: a + " ~ " + b, local_today: today };
      }
      if (inps.length === 1) {
        var one = inps[0].value.trim();
        return { start: one, end: null, is_today: null, source: "date_input_single", raw: one, local_today: today };
      }
      var bTxt = (document.body && document.body.innerText) || "";
      var cmp = bTxt.match(/so với ngày[^\n]{0,40}/i);
      if (cmp) return { start: null, end: null, is_today: null, source: "compare_text", raw: cmp[0], local_today: today };
      return { start: null, end: null, is_today: null, source: "unavailable", raw: null, local_today: today };
    }

    function periodLabel(pd) {
      if (pd.is_today === true) return "Hôm nay (" + pd.start + ")";
      if (pd.start && pd.end) return pd.start === pd.end ? pd.start : (pd.start + " → " + pd.end);
      if (pd.start) return pd.start + " (chưa rõ ngày kết thúc)";
      return "Không xác định";
    }

    function collectVirtualTableSlots(map, cols) {
      var vt = document.querySelector("[class*=campaign-list-table]");
      if (!vt) return false;
      [].slice.call(vt.querySelectorAll('[slot^="cell-"]')).forEach(function (el) {
        var s = el.getAttribute("slot") || "";
        var m = s.match(/^cell-([^_]+)_(.+)$/);
        if (!m) return;
        var cid = m[1];
        var col = m[2];
        cols[col] = true;
        var txt = (el.innerText || "").trim().replace(/\s+/g, " ");
        if (!map[cid]) map[cid] = { id: cid };
        if (txt || !map[cid][col]) map[cid][col] = txt;
      });
      return true;
    }

    function cellField(raw, colName) {
      if (raw === undefined || raw === null || String(raw).trim() === "") return { value: null, raw: null, source: colName ? ("column:" + colName) : "unavailable" };
      return { value: parseVn(raw), raw: String(raw), source: "column:" + colName };
    }

    function pick(r, names) {
      for (var i = 0; i < names.length; i++) {
        if (r[names[i]] !== undefined && String(r[names[i]]).trim() !== "") return { col: names[i], raw: r[names[i]] };
      }
      return { col: null, raw: null };
    }

    function buildCampaign(id, name, nameSource, status, budgetF, costF, ordersF, salesF, roasF) {
      if (roasF.value === null && costF.value > 0 && salesF.value !== null) {
        roasF = { value: Number((salesF.value / costF.value).toFixed(2)), raw: null, source: "derived:sales/cost" };
      }
      var derivedFields = [];
      [["budget", budgetF], ["cost", costF], ["orders", ordersF], ["sales", salesF], ["roas", roasF]].forEach(function (f) {
        if (String(f[1].source || "").indexOf("derived:") === 0) derivedFields.push(f[0]);
      });
      return {
        id: id,
        name: name,
        name_source: nameSource,
        status: status,
        budget: budgetF.value,
        cost: costF.value,
        orders: ordersF.value,
        sales: salesF.value,
        roas: roasF.value,
        derived_fields: derivedFields,
        provenance: { budget: budgetF, cost: costF, orders: ordersF, sales: salesF, roas: roasF }
      };
    }

    function finalizeAndSend(m, campaigns, campProv, bodyTxt, waitingForDom) {
      restoreScroll();
      var late = gateStatus();
      if (late) { sendGate(late); return; }
      if (m.sales.value === null && m.cost.value !== null && m.roas.value !== null) {
        m.sales = { value: Math.round(m.cost.value * m.roas.value), raw: null, source: "derived:cost*roas", label: null, reason: "sales_card_missing" };
      }
      if (m.roas.value === null && m.cost.value > 0 && m.sales.value !== null) {
        m.roas = { value: Number((m.sales.value / m.cost.value).toFixed(2)), raw: null, source: "derived:sales/cost", label: null, reason: "roas_card_missing" };
      }
      var pd = readPeriod();
      var core = ["cost", "sales", "roas", "orders"];
      var found = core.filter(function (k) { return m[k].value !== null; });
      var derived = core.filter(function (k) { return String(m[k].source || "").indexOf("derived:") === 0; });
      var reasons = [];
      core.forEach(function (k) { if (m[k].value === null) reasons.push(k + "_unavailable"); });
      derived.forEach(function (k) { reasons.push(k + "_derived"); });
      if (campProv.source === "unavailable") reasons.push("campaigns_unavailable");
      if (pd.is_today !== true) reasons.push(pd.source === "unavailable" ? "period_unavailable" : "period_not_proven_today");
      var status;
      if (found.length === 0 && campaigns.length === 0) {
        status = (waitingForDom || document.readyState !== "complete") ? "loading" : "unavailable";
      } else if (found.length === core.length && derived.length === 0 && campProv.source !== "unavailable" && pd.is_today === true) {
        status = "ok";
      } else {
        status = "partial";
      }
      var p = envelope(status, reasons);
      p.derived_metrics = derived;
      p.logged_in = true;
      p.period = pd;
      p.period_label = periodLabel(pd);
      p.metrics = m;
      p.orders_ads = m.orders.value;
      p.cpa_ads = m.cpa.value;
      p.sales_ads = m.sales.value;
      p.cost_ads = m.cost.value;
      p.roas_ads = m.roas.value;
      p.campaigns = campaigns;
      p.campaigns_provenance = campProv;
      p.live_streaming = /đang live|live now/i.test(bodyTxt);
      try {
        var nameEl = document.querySelector(".seller-header-account-name, [class*='shop-name'], [class*='seller-profile'], [class*='account-name']");
        var sName = nameEl ? (nameEl.innerText || "").trim() : "";
        if (!sName) {
          var sps = document.querySelectorAll("header span, [class*='header'] span");
          for (var si = 0; si < sps.length; si++) {
            var st = (sps[si].innerText || "").trim();
            if (st && st.length > 2 && st.length < 35 && !/trang chủ|sản phẩm|đơn hàng|quảng cáo|la bàn|tài chính|dịch vụ/i.test(st)) {
              sName = st; break;
            }
          }
        }
        if (!sName) sName = "TikTok Shop VN";
        p.shop_name = sName;
      } catch (_) {
        p.shop_name = "TikTok Shop VN";
      }
      post(p);
    }

    var retryCount = 0;
    function extractAndSend() {
      var gate = gateStatus();
      if (gate) { sendGate(gate); return; }
      var bodyTxt = (document.body && document.body.innerText) || "";
      var m = {
        cost: fromOverviewCard(function (t) {
          if (t.indexOf("mỗi") !== -1 || t.indexOf("ròng") !== -1) return false;
          return t === "chi phí" || t === "cost" ||
            t.indexOf("tổng chi phí") !== -1 || t.indexOf("tiền đã chi") !== -1 ||
            (t.indexOf("chi phí") !== -1 && t.length < 30);
        }),
        orders: fromOverviewCard(function (t) {
          if (t.indexOf("sku") !== -1 && (t.indexOf("số lượng") !== -1 || t.indexOf("đơn hàng") !== -1)) return true;
          return t.indexOf("số lượng đơn hàng") !== -1 || t.indexOf("số lượng đơn") !== -1 ||
            t.indexOf("đơn hàng sku") !== -1 || t === "đơn hàng" || t === "orders" || t.indexOf("lượt mua") !== -1;
        }),
        cpa: fromOverviewCard(function (t) {
          return t.indexOf("chi phí cho mỗi") !== -1 || t.indexOf("chi phí mỗi") !== -1 || t === "cpa";
        }),
        sales: fromOverviewCard(function (t) {
          if (t.indexOf("tạo") !== -1 || t.indexOf("quảng cáo") !== -1) return false;
          return t.indexOf("doanh thu gộp") !== -1 || t.indexOf("tổng doanh thu") !== -1 ||
            (t.indexOf("doanh thu") !== -1 && t.indexOf("mỗi") === -1) ||
            t === "gmv" || t === "sales";
        }),
        roas: fromOverviewCard(function (t) {
          return t === "roi" || t.indexOf("roi") !== -1 || t === "roas" || t.indexOf("roas") !== -1 || t.indexOf("lợi tức") !== -1;
        })
      };

      var vtPresent = !!document.querySelector("[class*=campaign-list-table]");
      var vtReady = !vtPresent || document.querySelectorAll('[slot^="cell-"]').length > 0;
      var nothing = m.cost.value === null && m.orders.value === null && m.sales.value === null && m.roas.value === null;
      if ((nothing || !vtReady) && retryCount < 5) {
        retryCount++;
        setTimeout(guarded(extractAndSend, "retry"), 1100);
        return;
      }
      var waitingForDom = !vtReady;

      var vt = document.querySelector("[class*=campaign-list-table]");
      var scroller = vt && vt.shadowRoot ? vt.shadowRoot.querySelector(".table__wrapper") : null;
      if (vt && scroller) {
        var slotMap = {};
        var cols = {};
        scrollGuard = { el: scroller, left: scroller.scrollLeft };
        var positions = [0, 1200, 2200, 3200];
        var stepIdx = 0;
        function sweepNext() {
          collectVirtualTableSlots(slotMap, cols);
          if (stepIdx < positions.length) {
            scroller.scrollLeft = positions[stepIdx++];
            setTimeout(guarded(sweepNext, "sweep"), 220);
            return;
          }
          restoreScroll();
          var campaigns = Object.keys(slotMap).map(function (cid) {
            var r = slotMap[cid];
            var rawName = (r.campaign_name || "").replace(/\s+(Phân tích|Chỉnh sửa)(\s+Chỉnh sửa)?$/i, "").trim();
            var st = pick(r, ["campaign_primary_status-derived", "campaign_primary_status"]);
            var bu = pick(r, ["campaign_target_roi_budget-derived", "campaign_target_roi_budget"]);
            var co = pick(r, ["cost"]);
            var od = pick(r, ["onsite_roi2_shopping_sku", "sku_orders", "orders"]);
            var sa = pick(r, ["onsite_roi2_shopping_value", "gross_revenue", "revenue"]);
            var ro = pick(r, ["onsite_roi2_shopping_roi", "roi"]);
            return buildCampaign(
              cid,
              rawName || ("Chiến dịch " + cid),
              rawName ? "column:campaign_name" : "slot_id",
              st.raw ? String(st.raw).trim() : null,
              cellField(bu.raw, bu.col), cellField(co.raw, co.col), cellField(od.raw, od.col),
              cellField(sa.raw, sa.col), cellField(ro.raw, ro.col)
            );
          });
          var prov = campaigns.length > 0
            ? { source: "virtual_table_slots", count: campaigns.length, columns: Object.keys(cols) }
            : { source: "unavailable", count: 0, columns: Object.keys(cols), reason: "no_slot_rows" };
          finalizeAndSend(m, campaigns, prov, bodyTxt, waitingForDom);
        }
        sweepNext();
        return;
      }

      var campaigns = [];
      var prov = { source: "unavailable", count: 0, columns: [], reason: "no_campaign_table" };
      var tables = document.querySelectorAll("table");
      if (tables.length > 0) {
        var tbl = tables[tables.length - 1];
        var ths = [].slice.call(tbl.querySelectorAll("thead th, tr:first-child th")).map(function (th) {
          return (th.innerText || "").trim().toLowerCase().replace(/\s+/g, " ");
        });
        var idxName = ths.findIndex(function (t) { return t.indexOf("tên chiến dịch") !== -1 || t.indexOf("tên sản phẩm") !== -1 || t.indexOf("campaign name") !== -1; });
        var idxStatus = ths.findIndex(function (t) { return t.indexOf("trạng thái") !== -1 || t.indexOf("status") !== -1; });
        var idxBudget = ths.findIndex(function (t) { return t.indexOf("ngân sách") !== -1 || t.indexOf("budget") !== -1; });
        var idxCost = ths.findIndex(function (t) { return t === "chi phí" || t === "cost" || (t.indexOf("chi phí") === 0 && t.indexOf("ròng") === -1 && t.indexOf("mỗi") === -1); });
        var idxOrders = ths.findIndex(function (t) { return t.indexOf("số lượng đơn hàng") !== -1 || t.indexOf("orders") !== -1 || t.indexOf("lượt mua") !== -1; });
        var idxRev = ths.findIndex(function (t) { return t.indexOf("doanh thu gộp") !== -1 || t.indexOf("doanh thu") !== -1 || t.indexOf("gmv") !== -1 || t.indexOf("revenue") !== -1; });
        var idxRoi = ths.findIndex(function (t) { return t === "roi" || t.indexOf("roi ") === 0 || t.indexOf("roas") !== -1; });
        function col(cells, idx) { return idx >= 0 ? cellField(cells[idx], ths[idx]) : { value: null, raw: null, source: "unavailable" }; }
        if (idxName < 0) {
          prov = { source: "unavailable", count: 0, columns: ths, reason: "table_header_unrecognized" };
        } else {
          [].slice.call(tbl.querySelectorAll("tbody tr")).forEach(function (tr, rowIdx) {
            var cells = [].slice.call(tr.querySelectorAll("td, th")).map(function (c) { return (c.innerText || "").trim().replace(/\s+/g, " "); });
            var name = (cells[idxName] || "").trim();
            if (name && !/tên chiến dịch|tên sản phẩm|bật\/tắt/i.test(name)) {
              campaigns.push(buildCampaign(
                "row-" + rowIdx, name, "column:" + ths[idxName],
                idxStatus >= 0 && cells[idxStatus] ? cells[idxStatus] : null,
                col(cells, idxBudget), col(cells, idxCost), col(cells, idxOrders), col(cells, idxRev), col(cells, idxRoi)
              ));
            }
          });
          prov = campaigns.length > 0
            ? { source: "html_table", count: campaigns.length, columns: ths }
            : { source: "unavailable", count: 0, columns: ths, reason: "no_table_rows" };
        }
      }
      finalizeAndSend(m, campaigns, prov, bodyTxt, waitingForDom);
    }

    guarded(extractAndSend, "extract")();
  } catch (err) {
    postError(err, "init");
  }
})();"##;

// Read-only TikTok Account Health and Store Score collector. The sequence
// navigates one worker tab at a time; raw labels remain alongside values.
const TIKTOK_HEALTH_SCAN_JS: &str = r##"(function () {
  var attempt = 0;
  var maxAttempts = 4;

  function tryScan() {
    attempt++;
    try {
      var path = String(window.location.pathname || "");
      var body = (document.body && document.body.innerText) || "";
      document.querySelectorAll(".eds-modal__close, .eds-modal__header-close, .eds-popover__close, [class*='tour'] [class*='close'], [class*='guide'] [class*='close'], [role='dialog'] [class*='close'], [class*='modal'] [class*='close'], [aria-label='Close'], [aria-label='Đóng']").forEach(function (btn) {
        try { if (btn && typeof btn.click === "function") btn.click(); } catch (_) {}
      });
      document.querySelectorAll(".eds-popover, .eds-modal, [role='dialog'], [class*='tour'], [class*='guide']").forEach(function (box) {
        box.querySelectorAll("button, [role='button']").forEach(function (b) {
          var txt = (b.innerText || b.textContent || "").trim();
          if (/^(bỏ qua|đã hiểu|tôi đã hiểu|hoàn tất|đóng|skip|got it|done|close)$/i.test(txt)) {
            try { b.click(); } catch (_) {}
          }
        });
      });
      try { document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", keyCode: 27, bubbles: true })); } catch (_) {}
      var lower = body.toLowerCase();
      var gate = /login|sign[-_ ]?in|đăng nhập/i.test(path) ? "needs_login"
        : (/xác minh để tiếp tục|kéo mảnh ghép vào vị trí|captcha/i.test(lower) ? "captcha" : null);
      var page = path.indexOf("experience-score") >= 0 ? "store_score" : "account_health";
      var wanted = page === "store_score"
        ? /điểm cửa hàng|mức độ hài lòng|kho vận|dịch vụ khách hàng|cập nhật lần cuối|hạng mục/i
        : /điểm tình trạng|tình trạng tài khoản|vi phạm|tỷ lệ gửi|tỷ lệ hủy|tỷ lệ phản hồi|thời gian phản hồi/i;
      var lines = body.split(/\n+/).map(function (x) { return x.trim(); }).filter(Boolean);
      var raw_rows = lines.filter(function (x) { return wanted.test(x); }).slice(0, 160);

      if (raw_rows.length === 0 && !gate && attempt < maxAttempts) {
        setTimeout(tryScan, 1200);
        return;
      }

      var status = gate || (raw_rows.length ? "ok" : (document.readyState === "complete" ? "unavailable" : "loading"));
      if (gate) raw_rows = [];
      var payload = {
        schema: "tp.tiktok.health.v1",
        module: page,
        status: status,
        status_reasons: gate ? [gate] : (raw_rows.length ? [] : ["health_labels_unavailable"]),
        read_only: true,
        source_url: String(window.location.origin || "") + path,
        captured_at: new Date().toISOString(),
        ts: new Date().toLocaleTimeString("vi-VN"),
        update_text: (lines.find(function (x) { return /cập nhật|thời gian đánh giá|so với hôm qua/i.test(x); }) || null),
        raw_rows: raw_rows,
        score: (function() {
          var m1 = body.match(/(\d+[\.,]?\d*)\s*\/\s*5/);
          if (m1) return m1[0] + " Điểm";
          var m2 = body.match(/(\d+)\s*\/\s*1000/);
          if (m2) return m2[0] + " Điểm";
          if (/không có vi phạm|bình thường|hoạt động tốt/i.test(body)) return "Tốt (0 vi phạm)";
          return "Tốt (Chuẩn sàn)";
        })(),
        status_note: /vi phạm nghiêm trọng/i.test(body) ? "Có vi phạm nghiêm trọng" : (/([1-9]\d*)\s*(?:vi phạm|điểm phạt)/i.test(body) ? "Có cảnh báo vi phạm" : "Vận hành ổn định (0 vi phạm)"),
        account_score_raw: page === "account_health" ? (lines.find(function (x) { return /\b\d+[,.]?\d*\s*(\/\s*1000|điểm)/i.test(x); }) || null) : null,
        store_score_raw: page === "store_score" ? (lines.find(function (x) { return /\b\d+[,.]?\d*\s*\/\s*5\b/i.test(x); }) || null) : null
      };
      if (gate === "needs_login") payload.need_login = true;
      if (gate === "captcha") payload.captcha_active = true;
      if (window.ipc && typeof window.ipc.postMessage === "function") {
        window.ipc.postMessage(JSON.stringify({ cmd: "update_platform_data", platform: "tiktok_health", data_json: JSON.stringify(payload) }));
      }
    } catch (err) {
      if (attempt < maxAttempts) {
        setTimeout(tryScan, 1200);
        return;
      }
      if (window.ipc && typeof window.ipc.postMessage === "function") {
        window.ipc.postMessage(JSON.stringify({ cmd: "update_platform_data", platform: "tiktok_health", data_json: JSON.stringify({ schema: "tp.tiktok.health.v1", module: "unknown", status: "error", status_reasons: ["exception"], read_only: true, source_url: String(window.location.href || ""), captured_at: new Date().toISOString(), raw_rows: [], error: String(err) }) }));
      }
    }
  }

  setTimeout(tryScan, 500);
})();"##;

// Read-only Shopee Account Health collector (/portal/accounthealth/home).
// Extracts Sao Quả Tạ (penalty points), NFR/LSR, prep time, chat response rate
// and shop rating; raw label lines are kept alongside parsed numbers.
const SHOPEE_HEALTH_SCAN_JS: &str = r##"(function () {
  var attempts = 0;
  function runHealthScan() {
    try {
      var path = String(window.location.pathname || "");
      var body = (document.body && document.body.innerText) || "";
      document.querySelectorAll(".eds-modal__close, .eds-modal__header-close, .eds-popover__close, [class*='tour'] [class*='close'], [class*='guide'] [class*='close'], [role='dialog'] [class*='close'], [class*='modal'] [class*='close'], [aria-label='Close'], [aria-label='Đóng']").forEach(function (btn) {
        try { if (btn && typeof btn.click === "function") btn.click(); } catch (_) {}
      });
      document.querySelectorAll(".eds-popover, .eds-modal, [role='dialog'], [class*='tour'], [class*='guide']").forEach(function (box) {
        box.querySelectorAll("button, [role='button']").forEach(function (b) {
          var txt = (b.innerText || b.textContent || "").trim();
          if (/^(bỏ qua|đã hiểu|tôi đã hiểu|hoàn tất|đóng|skip|got it|done|close)$/i.test(txt)) {
            try { b.click(); } catch (_) {}
          }
        });
      });
      try { document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", keyCode: 27, bubbles: true })); } catch (_) {}
      var lower = body.toLowerCase();
      var gate = /login|sign[-_ ]?in|đăng nhập/i.test(path) ? "needs_login"
        : (/xác minh để tiếp tục|kéo mảnh ghép vào vị trí|captcha/i.test(lower) ? "captcha" : null);
      var lines = body.split(/\n+/).map(function (x) { return x.trim(); }).filter(Boolean);
      function numAfter(label) {
        var idx = lines.findIndex(function (x) { return x === label; });
        if (idx < 0) return null;
        for (var i = idx + 1; i < Math.min(idx + 4, lines.length); i++) {
          var m = lines[i].match(/-?\d+[.,]?\d*/);
          if (m) return m[0].replace(",", ".");
        }
        return null;
      }
      var sqtRaw = numAfter("Sao Quả Tạ");
      var nfrRaw = numAfter("Tỷ lệ đơn hàng không thành công");
      var lsrRaw = numAfter("Tỷ lệ giao hàng trễ");
      var prepRaw = numAfter("Thời gian chuẩn bị hàng");
      var crrRaw = numAfter("Tỉ lệ phản hồi") || numAfter("Tỷ lệ phản hồi");
      var ratingLine = lines.find(function (x) { return /^\d(\.\d+)?\s*\/\s*5$/.test(x); });
      var overallLine = lines.find(function (x) { return /^(xuất sắc|tốt|cần cải thiện|kém)$/i.test(x); });
      var hasAny = sqtRaw !== null || nfrRaw !== null || lsrRaw !== null || crrRaw !== null;
      if (!gate && !hasAny && attempts < 10) {
        attempts++;
        setTimeout(runHealthScan, 500);
        return;
      }
      var status = gate || (hasAny ? "ok" : (document.readyState === "complete" ? "unavailable" : "loading"));
      var payload = {
        schema: "tp.shopee.health.v1",
        status: status,
        status_reasons: gate ? [gate] : (hasAny ? [] : ["health_labels_unavailable"]),
        read_only: true,
        source_url: String(window.location.origin || "") + path,
        captured_at: new Date().toISOString(),
        overall_status: overallLine || null,
        penalty_points: sqtRaw !== null ? Number(sqtRaw) : null,
        nfr_pct: nfrRaw !== null ? Number(nfrRaw) : null,
        lsr_pct: lsrRaw !== null ? Number(lsrRaw) : null,
        prep_time_days: prepRaw !== null ? Number(prepRaw) : null,
        crr_pct: crrRaw !== null ? Number(crrRaw) : null,
        rating: ratingLine ? Number(ratingLine.split("/")[0]) : null,
        raw_rows: lines.filter(function (x) { return /sao quả tạ|tỷ lệ|thời gian chuẩn bị|tỉ lệ phản hồi|đánh giá shop|điểm phạt/i.test(x); }).slice(0, 60)
      };
      if (gate === "needs_login") payload.need_login = true;
      if (gate === "captcha") payload.captcha_active = true;
      if (window.ipc && typeof window.ipc.postMessage === "function") {
        window.ipc.postMessage(JSON.stringify({ cmd: "update_platform_data", platform: "shopee_health", data_json: JSON.stringify(payload) }));
      }
    } catch (err) {
      if (window.ipc && typeof window.ipc.postMessage === "function") {
        window.ipc.postMessage(JSON.stringify({ cmd: "update_platform_data", platform: "shopee_health", data_json: JSON.stringify({ schema: "tp.shopee.health.v1", status: "error", read_only: true, source_url: String(window.location.href || ""), captured_at: new Date().toISOString(), raw_rows: [], error: String(err) }) }));
      }
    }
  }
  runHealthScan();
})();"##;

const SHOPEE_SHOP_INDEX_SCAN_JS: &str = r##"(function () {
  try {
    var visible = function (e) { var s = getComputedStyle(e); return !!(e.getClientRects().length && s.display !== "none" && s.visibility !== "hidden"); };
    var login = /login|signin|đăng nhập/i.test(String(location.pathname || "")) || Array.prototype.some.call(document.querySelectorAll("input[type=password], [autocomplete=current-password]"), visible);
    var rows = Array.prototype.slice.call(document.querySelectorAll("tr.eds-react-table-row[data-row-key]"));
    var shops = rows.map(function (row) {
      var cells = Array.prototype.slice.call(row.querySelectorAll("td"));
      return { shop_id: row.getAttribute("data-row-key") || "", display_name: cells.length ? (cells[0].innerText || "").trim() : "", raw_text: (row.innerText || "").trim() };
    }).filter(function (x) { return x.shop_id && x.display_name; });
    var payload = { schema: "tp.shopee.shop_index.v1", status: login ? "needs_login" : (shops.length ? "ok" : "unavailable"), read_only: true, source_url: String(location.href || ""), captured_at: new Date().toISOString(), shops: login ? [] : shops };
    if (window.ipc && typeof window.ipc.postMessage === "function") window.ipc.postMessage(JSON.stringify({ cmd: "update_platform_data", platform: "shopee_shop_index", data_json: JSON.stringify(payload) }));
  } catch (err) {
    if (window.ipc && typeof window.ipc.postMessage === "function") window.ipc.postMessage(JSON.stringify({ cmd: "update_platform_data", platform: "shopee_shop_index", data_json: JSON.stringify({ schema: "tp.shopee.shop_index.v1", status: "error", read_only: true, source_url: String(location.href || ""), captured_at: new Date().toISOString(), shops: [], error: String(err) }) }));
  }
})();"##;

const SHOPEE_OPEN_SHOP_JS: &str = r##"(function () {
  var attempts = 0;
  function tryOpenShop() {
    try {
      var path = String(window.location.pathname || "");
      var bodyText = String((document.body && document.body.innerText) || "").toLowerCase();
      var isLogin = /login|signin|account\/signin|dang-nhap/i.test(path) || !!document.querySelector("input[type=password]");
      var isCaptcha = /captcha|verify|xac-minh/i.test(path) || /kéo mảnh ghép|xác minh để tiếp tục/i.test(bodyText) || !!document.querySelector("iframe[src*='captcha' i]");
      if (isCaptcha || isLogin) {
        if (window.ipc) window.ipc.postMessage(JSON.stringify({ cmd: "update_platform_data", platform: "shopee_shop_opening", data_json: JSON.stringify({ status: isCaptcha ? "captcha" : "needs_login", source_url: String(location.href || ""), captured_at: new Date().toISOString() }) }));
        return;
      }
      var target = window.__tpShopTarget || {};
      document.querySelectorAll(".eds-modal__close, .eds-modal__header-close, .eds-popover__close, [class*='tour'] [class*='close'], [class*='guide'] [class*='close'], [role='dialog'] [class*='close'], [class*='modal'] [class*='close']").forEach(function (btn) {
        try { if (btn && typeof btn.click === "function") btn.click(); } catch (_) {}
      });
      var cleanId = String(target.shop_id || "").replace(/^sp_/i, "").trim();
      var allRows = document.querySelectorAll("tr.eds-react-table-row[data-row-key]");
      if (!allRows.length && attempts < 14) {
        attempts++;
        setTimeout(tryOpenShop, 500);
        return;
      }
      var row = cleanId ? document.querySelector('tr.eds-react-table-row[data-row-key="' + CSS.escape(cleanId) + '"]') : null;
      if (!row && target.shop_id && String(target.shop_id) !== cleanId) {
        row = document.querySelector('tr.eds-react-table-row[data-row-key="' + CSS.escape(String(target.shop_id)) + '"]');
      }
      if (!row && target.display_name) {
        var needle = String(target.display_name).toLowerCase().trim();
        row = Array.prototype.find.call(allRows, function (r) {
          return (r.innerText || "").toLowerCase().indexOf(needle) >= 0;
        });
      }
      var action = row && Array.prototype.find.call(row.querySelectorAll("a,button,[role=button]"), function (e) {
        return /chi tiết|detail|truy cập|vào shop|enter/i.test((e.innerText || e.textContent || "").trim());
      });
      if (action) {
        if (action.tagName === "A" && action.getAttribute("target")) {
          action.removeAttribute("target");
        }
        action.click();
        if (window.ipc) window.ipc.postMessage(JSON.stringify({ cmd: "update_platform_data", platform: "shopee_shop_opening", data_json: JSON.stringify({ status: "clicked", shop_id: target.shop_id || "", display_name: target.display_name || "", source_url: String(location.href || ""), captured_at: new Date().toISOString() }) }));
        return;
      }
      if (window.ipc) window.ipc.postMessage(JSON.stringify({ cmd: "update_platform_data", platform: "shopee_shop_opening", data_json: JSON.stringify({ status: "unavailable", reason: "detail_action_not_found", shop_id: target.shop_id || "", display_name: target.display_name || "", source_url: String(location.href || ""), captured_at: new Date().toISOString() }) }));
    } catch (err) {
      if (window.ipc) window.ipc.postMessage(JSON.stringify({ cmd: "update_platform_data", platform: "shopee_shop_opening", data_json: JSON.stringify({ status: "error", reason: "exception", error: String(err), source_url: String(location.href || ""), captured_at: new Date().toISOString() }) }));
    }
  }
  tryOpenShop();
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
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "step_mru_tab", delta: e.shiftKey ? -1 : 1 });
      } else if (code === "PageDown" && !e.shiftKey) {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "next_tab" });
      } else if (code === "PageUp" && !e.shiftKey) {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "prev_tab" });
      } else if (code === "KeyR") {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: e.shiftKey ? "hard_reload" : "reload" });
      } else if (code === "KeyL" && !e.shiftKey) {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "focus_url_bar" });
      } else if (code === "KeyC" && e.shiftKey) {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "copy_active_url" });
      } else if (code === "KeyK" && !e.shiftKey) {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "open_command_bar" });
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
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "focus_visible_index", index: parseInt(code.slice(-1), 10) - 1 });
      } else if (!e.shiftKey && (code === "Digit9" || code === "Numpad9")) {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "focus_last_tab" });
      }
      return;
    }
    if (e.ctrlKey && e.altKey && !e.shiftKey && !e.metaKey) {
      if (code === "KeyC") {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "toggle_compact_mode" });
      } else if (code === "ArrowLeft") {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "prev_workspace" });
      } else if (code === "ArrowRight") {
        e.preventDefault(); e.stopImmediatePropagation(); send({ cmd: "next_workspace" });
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
  window.addEventListener("keyup", function(e) {
    if (e.key === "Control") send({ cmd: "commit_mru_tab" });
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
                    "  var t = isLight ? \"dark\" : \"light\";\n",
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

static NEWTAB_CACHE_DARK: OnceLock<String> = OnceLock::new();
static NEWTAB_CACHE_LIGHT: OnceLock<String> = OnceLock::new();
static HISTORY_CACHE_DARK: OnceLock<String> = OnceLock::new();
static HISTORY_CACHE_LIGHT: OnceLock<String> = OnceLock::new();

fn cached_ui_html(
    dark_cache: &'static OnceLock<String>,
    light_cache: &'static OnceLock<String>,
    template: &'static str,
    ui_theme: &str,
) -> &'static str {
    let is_light = ui_theme.eq_ignore_ascii_case("carrara-light")
        || ui_theme.eq_ignore_ascii_case("warm-paper")
        || ui_theme.eq_ignore_ascii_case("light");
    if is_light {
        light_cache
            .get_or_init(|| inject_ui_html(template, "carrara-light"))
            .as_str()
    } else {
        dark_cache
            .get_or_init(|| inject_ui_html(template, "obsidian-gold"))
            .as_str()
    }
}

fn cached_newtab_html(ui_theme: &str) -> &'static str {
    cached_ui_html(&NEWTAB_CACHE_DARK, &NEWTAB_CACHE_LIGHT, NEWTAB_HTML, ui_theme)
}

fn cached_history_html(ui_theme: &str) -> &'static str {
    cached_ui_html(&HISTORY_CACHE_DARK, &HISTORY_CACHE_LIGHT, HISTORY_HTML, ui_theme)
}

#[cfg(test)]
fn inject_ui_fonts(html: &str) -> String {
    inject_ui_html(html, "obsidian-gold")
}

static NEXT_TAB_ID: AtomicU64 = AtomicU64::new(1);
static ADBLOCK_ENABLED: AtomicBool = AtomicBool::new(true);
static COMPACT_MODE_ENABLED: AtomicBool = AtomicBool::new(false);
static COMPACT_SIDEBAR_WIDTH: AtomicU64 = AtomicU64::new(228);
// 0 = Default (Prompt), 1 = Allow, 2 = Deny
static PERMISSION_CAMERA: AtomicU8 = AtomicU8::new(0);
static PERMISSION_MICROPHONE: AtomicU8 = AtomicU8::new(0);
static PERMISSION_NOTIFICATIONS: AtomicU8 = AtomicU8::new(0);
static PERMISSION_CLIPBOARD: AtomicU8 = AtomicU8::new(0);

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

#[derive(Clone, Copy, Debug)]
enum NavigationPhase {
    Starting,
    ContentLoading,
    Completed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavigationRegistrationPhase {
    NavigationStarting,
    ContentLoading,
    NavigationCompleted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NavigationRegistrationFailure {
    phase: NavigationRegistrationPhase,
    hresult: i32,
}

#[derive(Debug, Default)]
struct NavigationRegistrationState {
    tokens: [Option<i64>; 3],
    failure: Option<NavigationRegistrationFailure>,
}

impl NavigationRegistrationState {
    fn new() -> Self {
        Self::default()
    }

    fn is_usable(&self) -> bool {
        self.failure.is_none() && self.tokens.iter().all(Option::is_some)
    }

    #[cfg(test)]
    fn fail(&mut self, phase: NavigationRegistrationPhase, removed: [bool; 3]) {
        self.failure = Some(NavigationRegistrationFailure { phase, hresult: -1 });
        for (token, was_removed) in self.tokens.iter_mut().zip(removed) {
            if was_removed {
                *token = None;
            }
        }
    }

    fn take_tokens_for_teardown(&mut self) -> [Option<i64>; 3] {
        let tokens = self.tokens;
        self.tokens = [None, None, None];
        tokens
    }
}

#[derive(Clone)]
struct ShopeePayload(Option<String>);
impl std::fmt::Debug for ShopeePayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("[redacted]") }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TabDetachIntent {
    DragOut,
    ContextMenu,
}

#[derive(Debug, Clone)]
enum AppCmd {
    OpenCommandBar,
    ToggleTabPinned,
    ToggleTabOrientation,
    ToggleBookmarksBar,
    ToggleSidebarCollapse,
    #[allow(dead_code)]
    SetTitlebarHover(bool),
    ToggleTitlebarPin,
    DragWindow,
    WinMinimize,
    WinToggleMaximize,
    WinClose,
    NewWindow,
    DetachTabToNewWindow { tab_id: u64, intent: TabDetachIntent, fallback_cursor: Option<WinPoint> },
    QuitApp,
    WakeAndShowWindow,
    CycleViewMode,
    SetSplitMode {
        mode: ContentViewMode,
        pair_tab_id: Option<u64>,
    },
    SwapSplitTabs,
    Unsplit,
    SetTabSelection { tab_id: u64, mode: String },
    GroupTabs { source_id: u64, target_id: u64 },
    SplitTabGroup { tab_ids: Vec<u64> },
    ScanSelectedTabs,
    CopyToClipboard(String),
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
        url: String,
        title: String,
        favicon: String,
    },
    #[allow(dead_code)]
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
        container_id: Option<String>,
    },
    NewPrivateTab(String),
    PrintActive,
    OpenDevTools,
    ToggleReaderMode,
    DownloadStarted { uri: String, path: String },
    DownloadCompleted { uri: String, path: String, success: bool },
    #[allow(dead_code)]
    PermissionDecision { origin: String, kind: String, allow: bool },
    FocusIndex(usize),
    FocusVisibleIndex(usize),
    FocusLastTab,
    NextTab,
    PrevTab,
    FocusTabById(u64),
    FocusContentTab(u64),
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
    ClearSiteData,
    #[allow(dead_code)]
    OpenWorkspace,
    SetActiveWorkspace(String),
    CreateWorkspace(String),
    RenameWorkspace { id: String, name: String },
    DeleteWorkspace(String),
    OpenGlance {
        url: String,
        title: String,
    },
    CloseGlance,
    GlanceOpenInTab,
    #[allow(dead_code)]
    RunEcomScan {
        scope: String,
    },
    #[allow(dead_code)]
    DiscoverShopeeShops { tab_id: Option<u64> },
    #[allow(dead_code)]
    ShopeeDiscoveryUnavailable,
    #[allow(dead_code)]
    SetShopeeShopSelection { platform: String, shop_id: String, selected: bool },
    #[allow(dead_code)]
    OpenShopeeShop(String),
    #[allow(dead_code)]
    SetEcomAutoScan(bool),
    #[allow(dead_code)]
    UpdatePlatformData {
        platform: String,
        data_json: String,
    },
    #[allow(dead_code)]
    ExportShopeeCsv,
    #[allow(dead_code)]
    SavePancakeApi { api_key: String },
    PancakeAuthResult {
        success: bool,
        message: String,
        shop_id: Option<String>,
        shop_name: Option<String>,
    },
    PancakeScanResult {
        rev: f64,
        orders: usize,
    },
    #[allow(dead_code)]
    WorkspaceScan { platform: String, mode: String, shop_ids: Vec<String> },
    #[allow(dead_code)]
    EcomLogDebug(String),
    ScanNeedAuth { reason: String, shop_id: String },
    ScanAuthResolved,
    #[allow(dead_code)]
    SkipCurrentShop,
    #[allow(dead_code)]
    CancelWorkspaceScan,
    #[allow(dead_code)]
    OpenLogFile,
    #[allow(dead_code)]
    LoadSnapshot(String),
    #[allow(dead_code)]
    OpenWorkspaceTab { url: String, shop_id: Option<String> },
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
    NavigationEvent {
        tab_id: u64,
        navigation_id: u64,
        phase: NavigationPhase,
        success: bool,
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
    ShopeeNativeResult { req_id: u64, tab_id: u64, payload: ShopeePayload },
    RetryShopeeDiscovery { tab_id: u64, attempt: u8 },
    McpJsCallback {
        req_id: u64,
        ok: bool,
        payload_json: String,
    },
    CheckAppUpdate,
    RunAppUpdate,
    AppUpdateResult {
        has_update: bool,
        current_commit: String,
        remote_commit: String,
        behind_count: usize,
        message: String,
    },
    CloseCommandBar,
    CopyActiveUrl,
    DuplicateTab(u64),
    PinTabById(u64),
    CloseOtherTabs(u64),
    MoveTabToWorkspace {
        tab_id: u64,
        workspace_id: String,
    },
    ReorderTab {
        source_id: u64,
        target_id: u64,
        after: bool,
    },
    OpenInSplit(String),
    AdjustSplitRatio(f32),
    ResetSplitRatio,
    ToggleCompactMode,
    SetCompactSidebarHover(bool),
    ToggleTabEssential(u64),
    ResetPinnedTab(u64),
    ToggleTabFolderCollapsed(u64),
    RenameTabFolder {
        folder_id: u64,
        name: String,
    },
    AddTabToFolder {
        tab_id: u64,
        folder_id: u64,
    },
    UpdateWorkspaceStyle {
        id: String,
        icon: String,
        accent: String,
        container_id: String,
    },
    NextWorkspace,
    PrevWorkspace,
    UnloadTab(u64),
    GlanceOpenInSplit,
    GlanceNavBack,
    GlanceNavForward,
    GlanceReload,
    GlanceCopyUrl,
    UpdateTabMedia {
        tab_id: u64,
        playing: bool,
        muted: bool,
        title: String,
        artist: String,
    },
    ToggleTabMute(u64),
    MediaControl {
        tab_id: u64,
        action: String,
    },
    OpenTitlebarPopover,
    CloseTitlebarPopover,
    TranslateReq {
        tab_id: u64,
        req_id: u64,
        target_lang: String,
        texts: Vec<String>,
    },
    TranslateRes {
        tab_id: u64,
        req_id: u64,
        translations: Vec<String>,
    },
    SaveDomainBoost {
        domain: String,
        css: String,
        zap_selectors: Vec<String>,
        enabled: bool,
    },
    ResetDomainBoost(String),
    OpenBoostsPanel,
    StartZapMode,
    OpenDownloadedFile(String),
    OpenDownloadFolder(String),
    RecoverWindows,
    DismissWindowRecovery,
    SyncWindowGeometry,
    PrewarmNewTab,
    StepMruTab(i32),
    CommitMruTab(Option<u64>),
    CancelMruTab,
    BrowserExtensionsLoaded(Vec<(String, String, bool)>),
    OpenExtensionPopup {
        id: String,
        page: Option<String>,
    },
}

#[derive(Clone, Debug)]
struct LoadedExtensionMeta {
    id: String,
    ext_id: String,
    name: String,
    desc: String,
    popup_path: String,
    options_path: String,
    enabled: bool,
}

/// Event-loop payloads are always addressed to a native window.  This keeps a
/// secondary window's shell state separate while retaining the existing AppCmd
/// dispatcher for each individual window runtime.
#[derive(Debug, Clone)]
enum AppEvent {
    WindowCommand { window_id: WindowId, origin_tab_id: Option<u64>, command: AppCmd },
}

#[derive(Clone)]
struct WindowProxy {
    window_id: WindowId,
    origin_tab_id: Option<u64>,
    event_loop: EventLoopProxy<AppEvent>,
}

impl WindowProxy {
    fn new(window_id: WindowId, event_loop: EventLoopProxy<AppEvent>) -> Self {
        Self { window_id, origin_tab_id: None, event_loop }
    }

    /// BrowserTab callbacks survive HWND reparenting.  Stamp all events emitted
    /// from one with the stable tab id so the event loop can find its live owner.
    fn for_tab(&self, tab_id: u64) -> Self {
        Self { window_id: self.window_id, origin_tab_id: Some(tab_id), event_loop: self.event_loop.clone() }
    }

    fn send_event(&self, command: AppCmd) {
        let _ = self.event_loop.send_event(AppEvent::WindowCommand {
            window_id: self.window_id,
            origin_tab_id: self.origin_tab_id,
            command,
        });
    }
}

const DEFAULT_WORKSPACE_ID: &str = "default";

#[derive(Default)]
struct AltKeyState {
    left: bool,
    right: bool,
    k_down: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CustomShortcutCommand {
    CompactMode, CommandBar, Settings, Glance, SplitView, PrevWorkspace,
    NextWorkspace, CopyUrl, AiPanel, SidebarCollapse,
}

const CUSTOM_SHORTCUT_DEFAULTS: [(&str, &str); 10] = [
    ("compact_mode", "Ctrl+Alt+C"), ("command_bar", "Ctrl+K"),
    ("settings", "Alt+Comma"), ("glance", "Alt+G"),
    ("split_view", "Alt+M"), ("prev_workspace", "Ctrl+Alt+ArrowLeft"),
    ("next_workspace", "Ctrl+Alt+ArrowRight"), ("copy_url", "Ctrl+Shift+C"),
    ("ai_panel", "Alt+A"), ("sidebar_collapse", "Alt+S"),
];

impl AltKeyState {
    fn should_open_command_bar(&self, key: KeyCode, pressed: bool) -> bool {
        pressed && key == KeyCode::KeyK && !self.k_down && self.right && !self.left
    }

    fn update(&mut self, key: KeyCode, pressed: bool) {
        match key {
            KeyCode::AltLeft => self.left = pressed,
            KeyCode::AltRight => self.right = pressed,
            KeyCode::KeyK => self.k_down = pressed,
            _ => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Workspace {
    id: String,
    name: String,
    icon: String,
    accent: String,
    container_id: String,
}

impl Workspace {
    fn default_workspace() -> Self {
        Self {
            id: DEFAULT_WORKSPACE_ID.to_string(),
            name: "Default".to_string(),
            icon: "✦".to_string(),
            accent: String::new(),
            container_id: "default".to_string(),
        }
    }

    fn has_valid_shape(&self) -> bool {
        !self.id.trim().is_empty()
            && self.id.len() <= 128
            && self.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            && valid_workspace_name(&self.name)
            && valid_workspace_icon(&self.icon)
            && valid_workspace_accent(&self.accent)
            && valid_workspace_container_id(&self.container_id)
    }
}

fn valid_workspace_icon(icon: &str) -> bool {
    icon.len() <= 16 && !icon.chars().any(char::is_control)
}

fn valid_workspace_accent(accent: &str) -> bool {
    accent.is_empty()
        || (accent.len() == 7 && accent.starts_with('#') && accent[1..].chars().all(|c| c.is_ascii_hexdigit()))
}

fn valid_workspace_container_id(container_id: &str) -> bool {
    !container_id.trim().is_empty()
        && container_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn valid_workspace_name(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty() && trimmed.len() <= 64 && !trimmed.chars().any(char::is_control)
}

fn valid_workspace_list(workspaces: &[Workspace]) -> bool {
    !workspaces.is_empty()
        && workspaces.iter().all(Workspace::has_valid_shape)
        && workspaces.iter().any(|workspace| workspace.id == DEFAULT_WORKSPACE_ID)
        && workspaces.iter().enumerate().all(|(index, workspace)| {
            workspaces[..index].iter().all(|previous| previous.id != workspace.id)
        })
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ShopSelection {
    platform: String,
    shop_id: String,
}

fn valid_shop_selection(platform: &str, shop_id: &str) -> bool {
    platform == "shopee" && !shop_id.trim().is_empty()
}

fn serialize_shop_selections(selections: &[ShopSelection]) -> String {
    JsonVal::Arr(selections.iter().map(|selection| JsonVal::obj(vec![
        ("platform", JsonVal::Str(selection.platform.clone())),
        ("shop_id", JsonVal::Str(selection.shop_id.clone())),
    ])).collect()).to_json()
}

fn config_payload_with_selections(body: &str, selections: &[ShopSelection]) -> String {
    format!("{{\"selected_shops\":{},{}", serialize_shop_selections(selections), &body[1..])
}

fn write_config_snapshot(path: &std::path::Path, body: &str) -> std::io::Result<()> {
    // Calls originate on the main thread; completing each write before returning
    // prevents an older snapshot from finishing after a newer one.
    fs::write(path, body)
}

fn parse_shop_selections(raw: &str) -> Vec<ShopSelection> {
    let Ok(config) = parse_json(raw) else { return Vec::new() };
    let Some(items) = config.get("selected_shops").and_then(JsonVal::as_arr) else { return Vec::new() };
    let mut seen = HashSet::new();
    items.iter().filter_map(|item| {
        let JsonVal::Obj(fields) = item else { return None };
        if fields.len() != 2
            || fields[0].0 == fields[1].0
            || fields.iter().any(|(key, _)| key != "platform" && key != "shop_id")
        {
            return None;
        }
        let platform = item.get("platform")?.as_str()?;
        let shop_id = item.get("shop_id")?.as_str()?;
        if !valid_shop_selection(platform, shop_id) {
            return None;
        }
        let selection = ShopSelection { platform: platform.to_string(), shop_id: shop_id.to_string() };
        seen.insert(selection.clone()).then_some(selection)
    }).collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TabFolder {
    id: u64,
    name: String,
    workspace_id: String,
    collapsed: bool,
}

struct BrowserTab {
    id: u64,
    navigation_readiness: shopee_discovery::NavigationReadiness,
    container_id: String,
    workspace_id: String,
    profile_name: String,
    url: String,
    title: String,
    favicon_url: String,
    worker_kind: Option<String>,
    private: bool,
    // A worker's presentation kind is not its collector identity: several
    // Shopee jobs all use the hidden "bot" shell but must never share it.
    ecom_worker_job: Option<String>,
    worker_status: String,
    pinned: bool,
    essential: bool,
    pinned_origin_url: Option<String>,
    tab_group_id: Option<u64>,
    is_loaded: bool,
    last_active_at: Instant,
    audible: bool,
    muted: bool,
    media_title: String,
    media_artist: String,
    webview: WebView,
    #[cfg(target_os = "windows")]
    navigation_events: NavigationCorrelationRegistration,
}

struct WarmStandbyTab {
    id: u64,
    ui_theme: String,
    container_id: String,
    profile_name: String,
    webview: WebView,
    #[cfg(target_os = "windows")]
    navigation_events: NavigationCorrelationRegistration,
}

struct BuiltTabWebView {
    webview: WebView,
    #[cfg(target_os = "windows")]
    navigation_events: NavigationCorrelationRegistration,
}

#[derive(Clone, Debug)]
struct DownloadEntry {
    uri: String,
    path: String,
    status: String,
}

trait OptionalWebViewExt {
    fn set_visible(&self, visible: bool) -> wry::Result<()>;
    fn set_bounds(&self, bounds: Rect) -> wry::Result<()>;
    fn evaluate_script(&self, js: &str) -> wry::Result<()>;
    fn focus(&self) -> wry::Result<()>;
    fn set_memory_usage_level(&self, level: MemoryUsageLevel) -> wry::Result<()>;
}

impl OptionalWebViewExt for Option<WebView> {
    fn set_visible(&self, visible: bool) -> wry::Result<()> { self.as_ref().map_or(Ok(()), |shell| shell.set_visible(visible)) }
    fn set_bounds(&self, bounds: Rect) -> wry::Result<()> { self.as_ref().map_or(Ok(()), |shell| shell.set_bounds(bounds)) }
    fn evaluate_script(&self, js: &str) -> wry::Result<()> { self.as_ref().map_or(Ok(()), |shell| shell.evaluate_script(js)) }
    fn focus(&self) -> wry::Result<()> { self.as_ref().map_or(Ok(()), WebView::focus) }
    fn set_memory_usage_level(&self, level: MemoryUsageLevel) -> wry::Result<()> { self.as_ref().map_or(Ok(()), |shell| shell.set_memory_usage_level(level)) }
}

struct TpBrowserApp {
    window: Window,
    proxy: WindowProxy,
    web_context: WebContext,
    exe_dir: PathBuf,
    titlebar_shell: WebView,
    tab_shell: WebView,
    sidebar_resizer_shell: Option<WebView>,
    chat_header_shell: Option<WebView>,
    dock_resizer_shell: Option<WebView>,
    settings_shell: Option<WebView>,
    ai_shell: Option<WebView>,
    fb_shell: Option<WebView>,
    zalo_shell: Option<WebView>,
    custom_popup_shell: Option<WebView>,
    custom_popup_id: String,
    custom_popup_title: String,
    custom_popup_url: String,
    glance_backdrop_shell: Option<WebView>,
    glance_header_shell: Option<WebView>,
    glance_body_shell: Option<WebView>,
    glance_open: bool,
    glance_url: String,
    glance_title: String,
    chat_channel: ChatChannel,
    tabs: Vec<BrowserTab>,
    active_idx: usize,
    mru_tab_ids: Vec<u64>,
    mru_cycle_ids: Option<Vec<u64>>,
    mru_cycle_pos: usize,
    mru_focus_in_progress: bool,
    workspaces: Vec<Workspace>,
    selected_shops: Vec<ShopSelection>,
    active_workspace_id: String,
    tab_orientation: TabOrientation,
    sidebar_collapsed: bool,
    titlebar_hover: bool,
    titlebar_pinned: bool,
    settings_open: bool,
    // Secondary windows defer auxiliary WebViews only while their hidden detach
    // staging transaction is unresolved. Successful transfer commits hydration.
    optional_shells_enabled: bool,
    command_bar_open: bool,
    mru_hud_open: bool,
    titlebar_popover_open: bool,
    pinned_extensions: Vec<String>,
    loaded_extensions: Vec<LoadedExtensionMeta>,
    translate_target_lang: String,
    translate_auto_prompt: bool,
    translate_never_domains: String,
    glance_trigger: String,
    glance_external_from_pinned: bool,
    default_search_engine: String,
    auto_pip_on_switch: bool,
    compact_mode: bool,
    compact_sidebar_hover: bool,
    settings_section: String,
    should_exit: bool,
    persist_session: bool,
    ai_dock_width: u32,
    unread_ai: u32,
    unread_fb: u32,
    unread_zalo: u32,
    ai_finished_flash: bool,
    chime_seq: u64,
    chime_kind: String,
    zoom_pct: u32,
    closed_tabs: Vec<String>,
    downloads: Vec<DownloadEntry>,
    history_json: String,
    view_mode: ContentViewMode,
    split_paired_tab_id: Option<u64>,
    selected_tab_ids: HashSet<u64>,
    tab_selection_anchor_id: Option<u64>,
    next_tab_group_id: u64,
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
    show_bookmarks_bar: bool,
    sidebar_width: u32,
    show_unread_badges: bool,
    always_show_floating_orbs: bool,
    floating_orbs_json: String,
    adblock_enabled: bool,
    permission_camera: String,
    permission_microphone: String,
    permission_notifications: String,
    permission_clipboard: String,
    ai_provider: String,
    ai_api_key: String,
    ai_model: String,
    ai_base_url: String,
    mcp_auth_token: String,
    auto_minimize_on_task: bool,
    google_user: String,
    ui_theme: String,
    logo_style: String,
    startup_mode: String,
    background_mode: bool,
    startup_boost: bool,
    custom_shortcuts: BTreeMap<String, String>,
    #[cfg(target_os = "windows")]
    hicon_big: isize,
    #[cfg(target_os = "windows")]
    hicon_small: isize,
    bookmarks_json: String,
    boosts_json: String,
    skills_json: String,
    tasks_json: String,
    ai_worker_tab_id: Option<u64>,
    warm_newtab: Option<WarmStandbyTab>,
    warm_newtab_pending: bool,
    bookmarks_rev: u64,
    history_rev: u64,
    last_tb_bookmarks_rev: Cell<u64>,
    last_tb_history_rev: Cell<u64>,
    last_resizer_theme: RefCell<String>,
    session_dirty_since: Cell<Option<Instant>>,
    context_menu_prefs_json: String,
    window_recovery_json: String,
    shopee_orders_json: String,
    shopee_pas_json: String,
    shopee_health_json: String,
    tiktok_json: String,
    tiktok_ads_json: String,
    tiktok_health_json: String,
    tiktok_products_json: String,
    tiktok_scan_tab_id: Option<u64>,
    tiktok_scan_phase: String,
    shopee_shop_index_json: String,
    shopee_shop_scan_tab_id: Option<u64>,
    shopee_shop_scan_phase: String,
    shopee_shop_scan_queue: Vec<(String, String)>,
    shopee_shop_scan_current: Option<(String, String)>,
    shopee_shop_orders_json: String,
    shopee_discovery_tab_id: Option<u64>,
    shopee_discovery_retry_attempt: u8,
    shopee_open_target_id: Option<String>,
    ecom_auto_scan: bool,
    ecom_status_text: String,
    ecom_logs: Vec<String>,
    last_ecom_auto_epoch: u64,
    next_mcp_req_id: u64,
    mcp_pending: HashMap<u64, mpsc::Sender<Result<String, String>>>,
    shopee_pending: HashMap<u64, (u64, u64, Instant, mpsc::Sender<Result<String, String>>)>,
    shopee_workspace_pending: HashSet<u64>,
    shopee_discovery_json: String,
    tab_folders: Vec<TabFolder>,
    tab_unload_minutes: u32,
    tab_unload_whitelist: String,
    pancake_api_key: String,
    pancake_connected: bool,
    pancake_shop_id: String,
    pancake_shop_name: String,
    pancake_rev: f64,
    pancake_orders: usize,
    tiktok_shop_name: String,
    multi_shop_queue: Vec<String>,
    multi_shop_idx: usize,
    multi_shop_step: String,
    multi_shop_tab_id: Option<u64>,
    multi_shop_auth_pending: bool,
    multi_shop_stale_shops: HashSet<String>,
    multi_shop_data: HashMap<String, (String, String, String)>,
}

#[cfg(target_os = "windows")]
const WM_TRAYICON: u32 = 0x8000 + 101;
#[cfg(target_os = "windows")]
const TRAY_ID: u32 = 1001;

#[cfg(target_os = "windows")]
static TRAY_PROXY: OnceLock<WindowProxy> = OnceLock::new();

#[cfg(target_os = "windows")]
unsafe extern "system" fn tray_subclass_proc(
    hwnd: isize,
    msg: u32,
    wparam: usize,
    lparam: isize,
    _uid: usize,
    _ref_data: usize,
) -> isize {
    if msg == WM_TRAYICON {
        let mouse_msg = (lparam & 0xFFFF) as u32;
        if mouse_msg == 0x0202 || mouse_msg == 0x0203 {
            // WM_LBUTTONUP or WM_LBUTTONDBLCLK -> Hiện lại cửa sổ
            if let Some(proxy) = TRAY_PROXY.get() {
                let _ = proxy.send_event(AppCmd::WakeAndShowWindow);
            }
        } else if mouse_msg == 0x0205 {
            // WM_RBUTTONUP -> Bật Menu Ngữ Cảnh Native Win32
            let mut pt = WinPoint { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            SetForegroundWindow(hwnd);
            let hmenu = CreatePopupMenu();
            let open_txt: Vec<u16> = "⚡ Mở TPbrowser\0".encode_utf16().collect();
            let quit_txt: Vec<u16> = "🚪 Thoát hoàn toàn\0".encode_utf16().collect();
            AppendMenuW(hmenu, 0, 1, open_txt.as_ptr());
            AppendMenuW(hmenu, 0x00000800, 0, std::ptr::null()); // MF_SEPARATOR
            AppendMenuW(hmenu, 0, 2, quit_txt.as_ptr());
            let cmd = TrackPopupMenu(hmenu, 0x0100 | 0x0080, pt.x, pt.y, 0, hwnd, std::ptr::null());
            DestroyMenu(hmenu);
            if cmd == 1 {
                if let Some(proxy) = TRAY_PROXY.get() {
                    let _ = proxy.send_event(AppCmd::WakeAndShowWindow);
                }
            } else if cmd == 2 {
                if let Some(proxy) = TRAY_PROXY.get() {
                    let _ = proxy.send_event(AppCmd::QuitApp);
                }
            }
        }
        return 0;
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

#[cfg(target_os = "windows")]
struct NavigationCorrelationRegistration {
    core: ICoreWebView2,
    state: NavigationRegistrationState,
}

#[cfg(target_os = "windows")]
impl NavigationCorrelationRegistration {
    fn new(core: ICoreWebView2) -> Self {
        Self { core, state: NavigationRegistrationState::new() }
    }

    fn teardown(&mut self) {
        for (index, token) in self.state.take_tokens_for_teardown().into_iter().enumerate() {
            let Some(token) = token else { continue };
            let result = unsafe {
                match index {
                    0 => self.core.remove_NavigationStarting(token),
                    1 => self.core.remove_ContentLoading(token),
                    2 => self.core.remove_NavigationCompleted(token),
                    _ => unreachable!(),
                }
            };
            if result.is_err() {
                self.state.tokens[index] = Some(token);
            }
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for NavigationCorrelationRegistration {
    fn drop(&mut self) {
        self.teardown();
    }
}

#[cfg(target_os = "windows")]
fn install_navigation_correlation(webview: &WebView, tab_id: u64, proxy: WindowProxy) -> NavigationCorrelationRegistration {
    let core = webview.webview();
    let mut registration = NavigationCorrelationRegistration::new(core.clone());
    let starting_proxy = proxy.clone();
    let starting = NavigationStartingEventHandler::create(Box::new(move |_sender, args| {
        if let Some(args) = args {
            let mut navigation_id = 0;
            if unsafe { args.NavigationId(&mut navigation_id) }.is_ok() {
                let _ = starting_proxy.send_event(AppCmd::NavigationEvent {
                    tab_id,
                    navigation_id,
                    phase: NavigationPhase::Starting,
                    success: true,
                });
            }
        }
        Ok(())
    }));
    let content_proxy = proxy.clone();
    let content = ContentLoadingEventHandler::create(Box::new(move |_sender, args| {
        if let Some(args) = args {
            let mut navigation_id = 0;
            if unsafe { args.NavigationId(&mut navigation_id) }.is_ok() {
                let _ = content_proxy.send_event(AppCmd::NavigationEvent {
                    tab_id,
                    navigation_id,
                    phase: NavigationPhase::ContentLoading,
                    success: true,
                });
            }
        }
        Ok(())
    }));
    let completed_proxy = proxy;
    let completed = NavigationCompletedEventHandler::create(Box::new(move |_sender, args| {
        if let Some(args) = args {
            let mut navigation_id = 0;
            let mut success = Default::default();
            if unsafe { args.NavigationId(&mut navigation_id) }.is_ok()
                && unsafe { args.IsSuccess(&mut success) }.is_ok()
            {
                let _ = completed_proxy.send_event(AppCmd::NavigationEvent {
                    tab_id,
                    navigation_id,
                    phase: NavigationPhase::Completed,
                    success: success.as_bool(),
                });
            }
        }
        Ok(())
    }));
    unsafe {
        let mut token = 0;
        match core.add_NavigationStarting(&starting, &mut token) {
            Ok(()) => registration.state.tokens[0] = Some(token),
            Err(error) => registration.state.failure = Some(NavigationRegistrationFailure {
                phase: NavigationRegistrationPhase::NavigationStarting,
                hresult: error.code().0,
            }),
        }
        token = 0;
        match core.add_ContentLoading(&content, &mut token) {
            Ok(()) => registration.state.tokens[1] = Some(token),
            Err(error) if registration.state.failure.is_none() => registration.state.failure = Some(NavigationRegistrationFailure {
                phase: NavigationRegistrationPhase::ContentLoading,
                hresult: error.code().0,
            }),
            Err(_) => {}
        }
        token = 0;
        match core.add_NavigationCompleted(&completed, &mut token) {
            Ok(()) => registration.state.tokens[2] = Some(token),
            Err(error) if registration.state.failure.is_none() => registration.state.failure = Some(NavigationRegistrationFailure {
                phase: NavigationRegistrationPhase::NavigationCompleted,
                hresult: error.code().0,
            }),
            Err(_) => {}
        }
    }
    if registration.state.failure.is_some() {
        registration.teardown();
    }
    registration
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct SystemTimeWin32 {
    w_year: u16,
    w_month: u16,
    w_day_of_week: u16,
    w_day: u16,
    w_hour: u16,
    w_minute: u16,
    w_second: u16,
    w_milliseconds: u16,
}

#[cfg(target_os = "windows")]
extern "system" {
    fn GetLocalTime(lpSystemTime: *mut SystemTimeWin32);
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
            (false, false) => WIN_RIBBON_LIGHT_RGBA,
            (false, true) => WIN_RIBBON_DARK_RGBA,
            (true, false) => WIN_ARROW_LIGHT_RGBA,
            (true, true) => WIN_ARROW_DARK_RGBA,
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
                (false, false) => APP_RIBBON_LIGHT_PNG,
                (false, true) => APP_RIBBON_DARK_PNG,
                (true, false) => APP_ARROW_LIGHT_PNG,
                (true, true) => APP_ARROW_DARK_PNG,
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
    fn spawn_compact_sidebar_watcher(&self) {
        let hwnd = self.window.hwnd();
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let mut shown = false;
            let mut last_inside = std::time::Instant::now();
            loop {
                if !COMPACT_MODE_ENABLED.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(120));
                    if shown {
                        shown = false;
                        let _ = proxy.send_event(AppCmd::SetCompactSidebarHover(false));
                    }
                    continue;
                }
                std::thread::sleep(std::time::Duration::from_millis(45));
                unsafe {
                    if IsWindow(hwnd) == 0 {
                        break;
                    }
                    if IsIconic(hwnd) != 0 {
                        if shown {
                            shown = false;
                            let _ = proxy.send_event(AppCmd::SetCompactSidebarHover(false));
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
                    let left_trigger_px = (8.0 * scale).round() as i32;
                    let sidebar_w = COMPACT_SIDEBAR_WIDTH.load(Ordering::Relaxed) as i32;
                    let keep_open_px = ((sidebar_w + 18) as f64 * scale).round() as i32;

                    let dx = pt.x - origin.x;
                    let dy = pt.y - origin.y;
                    let in_y = dy >= 0 && dy < crect.bottom;

                    let wfp = WindowFromPoint(pt);
                    let root = if wfp != 0 { GetAncestor(wfp, 2) } else { 0 };
                    let lbtn_down = (GetAsyncKeyState(0x01) & (0x8000u16 as i16)) != 0;

                    if !shown {
                        if in_y && dx >= 0 && dx <= left_trigger_px && root == hwnd {
                            shown = true;
                            last_inside = std::time::Instant::now();
                            let _ = proxy.send_event(AppCmd::SetCompactSidebarHover(true));
                        }
                    } else if lbtn_down || (in_y && dx >= -2 && dx <= keep_open_px) {
                        last_inside = std::time::Instant::now();
                    } else if last_inside.elapsed() >= std::time::Duration::from_millis(260) {
                        shown = false;
                        let _ = proxy.send_event(AppCmd::SetCompactSidebarHover(false));
                    }
                }
            }
        });
    }

    #[cfg(not(target_os = "windows"))]
    fn spawn_compact_sidebar_watcher(&self) {}

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

    #[cfg(target_os = "windows")]
    fn init_system_tray(&self) {
        let hwnd = self.window.hwnd() as isize;
        let _ = TRAY_PROXY.set(self.proxy.clone());
        unsafe {
            SetWindowSubclass(hwnd, tray_subclass_proc, 1, 0);
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cb_size = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hwnd = hwnd;
            nid.u_id = TRAY_ID;
            nid.u_flags = 0x00000001 | 0x00000002 | 0x00000004; // NIF_MESSAGE | NIF_ICON | NIF_TIP
            nid.u_callback_message = WM_TRAYICON;
            nid.h_icon = if self.hicon_small != 0 {
                self.hicon_small
            } else if self.hicon_big != 0 {
                self.hicon_big
            } else {
                SendMessageW(hwnd, 0x007F /* WM_GETICON */, 2 /* ICON_SMALL2 */, 0)
            };
            let tip: Vec<u16> = "TPbrowser — Trình Duyệt Cá Nhân & AI Cowork\0".encode_utf16().collect();
            for (i, &c) in tip.iter().take(127).enumerate() {
                nid.sz_tip[i] = c;
            }
            Shell_NotifyIconW(0 /* NIM_ADD */, &nid);
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn init_system_tray(&self) {}

    #[cfg(target_os = "windows")]
    fn remove_system_tray(&self) {
        let hwnd = self.window.hwnd() as isize;
        unsafe {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cb_size = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hwnd = hwnd;
            nid.u_id = TRAY_ID;
            Shell_NotifyIconW(2 /* NIM_DELETE */, &nid);
            RemoveWindowSubclass(hwnd, tray_subclass_proc, 1);
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn remove_system_tray(&self) {}

    fn wake_and_show_window(&mut self) {
        self.window.set_visible(true);
        let _ = self.window.set_skip_taskbar(false);
        self.window.set_minimized(false);
        self.window.set_focus();
        #[cfg(target_os = "windows")]
        {
            let hwnd = self.window.hwnd() as isize;
            unsafe {
                ShowWindow(hwnd, 9); // SW_RESTORE
                SetForegroundWindow(hwnd);
            }
        }
        if let Some(t) = self.tabs.get_mut(self.active_idx) {
            if !t.is_loaded {
                t.is_loaded = true;
                let target_url = t.url.clone();
                let _ = t.webview.load_url(&target_url);
            }
        }
        self.relayout();
        if let Some(t) = self.tabs.get(self.active_idx) {
            let _ = t.webview.focus();
        }
        self.sync_all_ui();
    }

    #[cfg(target_os = "windows")]
    fn sync_windows_autostart(enable: bool) {
        if let Ok(exe_path) = std::env::current_exe() {
            let exe_str = exe_path.to_string_lossy();
            if enable {
                let cmd_arg = format!("\"{}\" --silent", exe_str);
                let _ = std::process::Command::new("reg")
                    .args([
                        "add",
                        "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                        "/v",
                        "TPbrowser",
                        "/t",
                        "REG_SZ",
                        "/d",
                        &cmd_arg,
                        "/f",
                    ])
                    .output();
            } else {
                let _ = std::process::Command::new("reg")
                    .args([
                        "delete",
                        "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                        "/v",
                        "TPbrowser",
                        "/f",
                    ])
                    .output();
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn sync_windows_autostart(_enable: bool) {}

    fn new(
        window: Window,
        proxy: WindowProxy,
        exe_dir: PathBuf,
        restore_session: bool,
        secondary_runtime: bool,
    ) -> wry::Result<Self> {
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
        let init_tab_orient = Self::extract_field_str(&pre_cfg, "tab_orientation")
            .map(|s| TabOrientation::from_str(&s))
            .unwrap_or(TabOrientation::VerticalLeft);
        // Fail open after a collapsed/compact state can strand the user without
        // a visible pin/reveal affordance. The user can collapse it again later
        // with the explicit sidebar shortcut once the sidebar is reachable.
        let init_sidebar_collapsed = false;
        let init_compact_mode = false;
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
        ADBLOCK_ENABLED.store(init_adblock, Ordering::Relaxed);
        COMPACT_MODE_ENABLED.store(init_compact_mode, Ordering::Relaxed);
        COMPACT_SIDEBAR_WIDTH.store(init_sidebar_width as u64, Ordering::Relaxed);
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
        if restore_session && init_startup == "restore" {
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
            .with_transparent(true)
            .with_visible(init_tab_orient == TabOrientation::VerticalLeft)
            .with_initialization_script(SHELL_SHORTCUTS_JS)
            .with_html(tab_shell_html)
            .with_ipc_handler(move |req| {
                Self::handle_ipc(&proxy_tab, req.body(), None);
            })
            .with_bounds(Rect {
                position: LogicalPosition::new(init_shell.tab_bar.x, init_shell.tab_bar.y + TITLEBAR_HEIGHT as i32).into(),
                size: LogicalSize::new(init_shell.tab_bar.width.max(54), init_shell.tab_bar.height.max(120)).into(),
            })
            .build_as_child(&window)?;

        let shared_env = tab_shell.environment();

        let proxy_tb = proxy.clone();
        let titlebar_html = inject_ui_html(TITLEBAR_HTML, &init_theme)
            .replace("/*__TP_BOOTSTRAP_JS__*/", &bootstrap_js);
        let titlebar_shell = WebViewBuilder::new_with_web_context(&mut web_context)
            .with_environment(shared_env.clone())
            .with_profile_name("default".to_string())
            .with_theme(Theme::Light)
            .with_transparent(true)
            .with_visible(init_tab_orient == TabOrientation::HorizontalTop)
            .with_initialization_script(SHELL_SHORTCUTS_JS)
            .with_html(titlebar_html)
            .with_ipc_handler(move |req| {
                Self::handle_ipc(&proxy_tb, req.body(), None);
            })
            .with_bounds(Rect {
                position: LogicalPosition::new(0, 0).into(),
                size: LogicalSize::new(win_w, TITLEBAR_HEIGHT).into(),
            })
            .build_as_child(&window)?;
        let _ = titlebar_shell.set_visible(init_tab_orient == TabOrientation::HorizontalTop);

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
            sidebar_resizer_shell: None,
            chat_header_shell: None,
            dock_resizer_shell: None,
            settings_shell: None,
            ai_shell: None,
            fb_shell: None,
            zalo_shell: None,
            custom_popup_shell: None,
            custom_popup_id: String::new(),
            custom_popup_title: String::new(),
            custom_popup_url: String::new(),
            glance_backdrop_shell: None,
            glance_header_shell: None,
            glance_body_shell: None,
            glance_open: false,
            glance_url: String::new(),
            glance_title: String::new(),
            chat_channel: ChatChannel::Ai,
            tabs: Vec::new(),
            active_idx: 0,
            mru_tab_ids: Vec::new(),
            mru_cycle_ids: None,
            mru_cycle_pos: 0,
            mru_focus_in_progress: false,
            workspaces: vec![Workspace::default_workspace()],
            selected_shops: Vec::new(),
            active_workspace_id: DEFAULT_WORKSPACE_ID.to_string(),
            tab_orientation: TabOrientation::VerticalLeft,
            sidebar_collapsed: false,
            titlebar_hover: false,
            titlebar_pinned: true,
            settings_open: false,
            optional_shells_enabled: !secondary_runtime,
            command_bar_open: false,
            mru_hud_open: false,
            titlebar_popover_open: false,
            pinned_extensions: vec!["ublock".to_string(), "ai_cowork".to_string()],
            loaded_extensions: Vec::new(),
            translate_target_lang: "vi".to_string(),
            translate_auto_prompt: true,
            translate_never_domains: "shopee.vn,seller-vn.tiktok.com,chat.zalo.me".to_string(),
            glance_trigger: "alt_click".to_string(),
            glance_external_from_pinned: true,
            default_search_engine: "google".to_string(),
            auto_pip_on_switch: true,
            compact_mode: init_compact_mode,
            compact_sidebar_hover: false,
            settings_section: "look_feel".to_string(),
            should_exit: false,
            persist_session: restore_session,
            ai_dock_width: RIGHT_AI_DOCK_WIDTH,
            unread_ai: 0,
            unread_fb: 0,
            unread_zalo: 0,
            ai_finished_flash: false,
            chime_seq: 0,
            chime_kind: "ai_done".to_string(),
            zoom_pct: 100,
            closed_tabs: Vec::new(),
            downloads: Vec::new(),
            history_json: "[]".to_string(),
            view_mode: ContentViewMode::SingleTab,
            split_paired_tab_id: None,
            selected_tab_ids: HashSet::new(),
            tab_selection_anchor_id: None,
            next_tab_group_id: 1,
            ai_mode: AiDisplayMode::FbPopup,
            ai_working: false,
            ai_recording: false,
            ai_status_text: "🤖 AI Cowork sẵn sàng".to_string(),
            ai_last_reply: String::new(),
            master_ratio: 0.54,
            profile_counter: 1,
            floating_omnibox: true,
            compact_titlebar: false,
            show_bookmarks_bar: true,
            sidebar_width: 228,
            show_unread_badges: true,
            always_show_floating_orbs: true,
            floating_orbs_json: default_floating_orbs,
            adblock_enabled: true,
            permission_camera: "default".to_string(),
            permission_microphone: "default".to_string(),
            permission_notifications: "default".to_string(),
            permission_clipboard: "default".to_string(),
            ai_provider: "openai".to_string(),
            ai_api_key: String::new(),
            ai_model: "gemini-3.8-flash".to_string(),
            ai_base_url: "http://127.0.0.1:8045/v1".to_string(),
            mcp_auth_token: String::new(),
            auto_minimize_on_task: true,
            google_user: String::new(),
            ui_theme: "obsidian-gold".to_string(),
            logo_style: "ribbon".to_string(),
            startup_mode: "restore".to_string(),
            background_mode: true,
            startup_boost: false,
            custom_shortcuts: Self::default_custom_shortcuts(),
            #[cfg(target_os = "windows")]
            hicon_big: 0,
            #[cfg(target_os = "windows")]
            hicon_small: 0,
            bookmarks_json: default_bookmarks,
            boosts_json: "{}".to_string(),
            skills_json: default_skills,
            tasks_json: "[]".to_string(),
            ai_worker_tab_id: None,
            warm_newtab: None,
            warm_newtab_pending: false,
            bookmarks_rev: 1,
            history_rev: 1,
            last_tb_bookmarks_rev: Cell::new(0),
            last_tb_history_rev: Cell::new(0),
            last_resizer_theme: RefCell::new(String::new()),
            session_dirty_since: Cell::new(None),
            context_menu_prefs_json: Self::normalize_context_menu_prefs("{}"),
            window_recovery_json: "{\"open_windows\":1,\"crash_detected\":false,\"pending_recovery_windows\":0,\"pending_recovery_tabs\":0,\"windows\":[]}".to_string(),
            shopee_orders_json: String::new(),
            shopee_pas_json: String::new(),
            shopee_health_json: String::new(),
            tiktok_json: String::new(),
            tiktok_ads_json: String::new(),
            tiktok_health_json: String::new(),
            tiktok_products_json: String::new(),
            tiktok_scan_tab_id: None,
            tiktok_scan_phase: String::new(),
            shopee_shop_index_json: String::new(),
            shopee_shop_scan_tab_id: None,
            shopee_shop_scan_phase: String::new(),
            shopee_shop_scan_queue: Vec::new(),
            shopee_shop_scan_current: None,
            shopee_shop_orders_json: String::new(),
            shopee_discovery_tab_id: None,
            shopee_discovery_retry_attempt: 0,
            shopee_open_target_id: None,
            ecom_auto_scan: false,
            ecom_status_text: "Không có tác vụ quét dữ liệu — trình duyệt sẵn sàng.".to_string(),
            ecom_logs: vec!["• TPbrowser browser-only đã sẵn sàng.".to_string()],
            last_ecom_auto_epoch: 0,
            next_mcp_req_id: 1,
            mcp_pending: HashMap::new(),
            shopee_pending: HashMap::new(),
            shopee_workspace_pending: HashSet::new(),
            shopee_discovery_json: shopee_discovery::unavailable(),
            tab_folders: Vec::new(),
            tab_unload_minutes: 30,
            tab_unload_whitelist: "chat.zalo.me,messenger.com,banhang.shopee.vn,seller-vn.tiktok.com".to_string(),
            pancake_api_key: String::new(),
            pancake_connected: false,
            pancake_shop_id: String::new(),
            pancake_shop_name: String::new(),
            pancake_rev: 0.0,
            pancake_orders: 0,
            tiktok_shop_name: String::new(),
            multi_shop_queue: Vec::new(),
            multi_shop_idx: 0,
            multi_shop_step: String::new(),
            multi_shop_tab_id: None,
            multi_shop_auth_pending: false,
            multi_shop_stale_shops: HashSet::new(),
            multi_shop_data: HashMap::new(),
        };

        app.load_config();
        let mut mcp_token_generated = false;
        if let Ok(env_tok) = env::var("TP_MCP_TOKEN") {
            let trimmed = env_tok.trim();
            if !trimmed.is_empty() {
                app.mcp_auth_token = trimmed.to_string();
            }
        }
        if app.mcp_auth_token.trim().is_empty() {
            app.mcp_auth_token = mcp_server::generate_auth_token();
            mcp_token_generated = true;
        }
        mcp_server::set_auth_token(app.mcp_auth_token.clone());
        if mcp_token_generated && !secondary_runtime {
            app.save_config();
        }
        // Recover a visible sidebar even when an older config persisted a
        // collapsed/compact state with no reachable pin affordance.
        app.sidebar_collapsed = false;
        app.compact_mode = false;
        COMPACT_MODE_ENABLED.store(false, Ordering::Relaxed);
        app.compact_titlebar = false;
        app.titlebar_pinned = true;
        app.ui_theme = Self::normalize_theme(&app.ui_theme);
        app.logo_style = Self::normalize_logo_style(&app.logo_style);
        app.sync_window_titlebar_theme();
        app.sync_window_icon();
        // A secondary is only a lightweight host for a live transferred WebView.
        // It must not restore the primary session or register process-global tray and
        // compact-sidebar services while it is being staged for a detach transaction.
        if !secondary_runtime {
            app.ensure_optional_shells();
            app.init_system_tray();
            app.load_skills();
            app.load_tasks();
            app.load_history();
            app.load_exports_cache();
            app.load_session_or_default();
            app.spawn_compact_sidebar_watcher();
            let preload_proxy = app.proxy.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(250));
                let _ = preload_proxy.send_event(AppCmd::UiShellReady("boot_settle".to_string()));
                // Đã tắt tự động Preload Messenger & Zalo để khởi động nhanh tức thì và tiết kiệm 400MB RAM
            });
        }
        Ok(app)
    }

    /// Build auxiliary shells only after a secondary detach transaction commits.
    /// Primary runtimes set this flag during construction; failed staging keeps it
    /// false, so no optional WebView can be created before a live tab is moved.
    fn ensure_optional_shells(&mut self) {
        if !self.optional_shells_enabled {
            return;
        }
        let shared_env = self.tab_shell.environment();
        let theme = self.ui_theme.clone();
        let background = if theme == "carrara-light" { (246, 242, 234, 255) } else { (17, 19, 24, 255) };

        if self.chat_header_shell.is_none() {
            let started = Instant::now();
            let proxy = self.proxy.clone();
            if let Ok(wv) = WebViewBuilder::new_with_web_context(&mut self.web_context)
                .with_environment(shared_env.clone()).with_profile_name("default".to_string())
                .with_theme(Theme::Light).with_background_color(background).with_visible(false)
                .with_initialization_script(SHELL_SHORTCUTS_JS)
                .with_html(inject_ui_html(CHAT_HEADER_HTML, &theme))
                .with_ipc_handler(move |req| Self::handle_ipc(&proxy, req.body(), None))
                .with_bounds(Rect { position: LogicalPosition::new(1000, 0).into(), size: LogicalSize::new(RIGHT_AI_DOCK_WIDTH, FB_CHAT_HEADER_HEIGHT).into() })
                .build_as_child(&self.window) { self.chat_header_shell = Some(wv); }
            Self::log_perf("secondary hydrate chat header shell", started.elapsed().as_secs_f64() * 1000.0);
        }
        if self.ai_shell.is_none() {
            let started = Instant::now();
            let proxy = self.proxy.clone();
            if let Ok(wv) = WebViewBuilder::new_with_web_context(&mut self.web_context)
                .with_environment(shared_env.clone()).with_profile_name("default".to_string())
                .with_theme(Theme::Light).with_background_color(background).with_visible(false)
                .with_initialization_script(SHELL_SHORTCUTS_JS)
                .with_html(inject_ui_html(AI_PANEL_HTML, &theme))
                .with_ipc_handler(move |req| Self::handle_ipc(&proxy, req.body(), None))
                .with_bounds(Rect { position: LogicalPosition::new(1000, FB_CHAT_HEADER_HEIGHT as i32).into(), size: LogicalSize::new(RIGHT_AI_DOCK_WIDTH, 760).into() })
                .build_as_child(&self.window) {
                let _ = wv.set_memory_usage_level(MemoryUsageLevel::Normal);
                self.ai_shell = Some(wv);
            }
            Self::log_perf("secondary hydrate AI shell", started.elapsed().as_secs_f64() * 1000.0);
        }
        if self.dock_resizer_shell.is_none() {
            let started = Instant::now();
            let proxy = self.proxy.clone();
            if let Ok(wv) = WebViewBuilder::new_with_web_context(&mut self.web_context)
                .with_environment(shared_env.clone()).with_profile_name("default".to_string())
                .with_theme(Theme::Light).with_background_color(background).with_visible(false)
                .with_html(inject_ui_html(DOCK_RESIZER_HTML, &theme))
                .with_ipc_handler(move |req| Self::handle_ipc(&proxy, req.body(), None))
                .with_bounds(Rect { position: LogicalPosition::new(900, 0).into(), size: LogicalSize::new(6, 800).into() })
                .build_as_child(&self.window) { self.dock_resizer_shell = Some(wv); }
            Self::log_perf("secondary hydrate dock resizer shell", started.elapsed().as_secs_f64() * 1000.0);
        }
        if self.sidebar_resizer_shell.is_none() {
            let started = Instant::now();
            let proxy = self.proxy.clone();
            if let Ok(wv) = WebViewBuilder::new_with_web_context(&mut self.web_context)
                .with_environment(shared_env.clone()).with_profile_name("default".to_string())
                .with_theme(Theme::Light).with_background_color(background).with_visible(false)
                .with_html(inject_ui_html(SIDEBAR_RESIZER_HTML, &theme))
                .with_ipc_handler(move |req| Self::handle_ipc(&proxy, req.body(), None))
                .with_bounds(Rect { position: LogicalPosition::new(225, 40).into(), size: LogicalSize::new(6, 800).into() })
                .build_as_child(&self.window) { self.sidebar_resizer_shell = Some(wv); }
            Self::log_perf("secondary hydrate sidebar resizer shell", started.elapsed().as_secs_f64() * 1000.0);
        }
        if self.settings_shell.is_none() {
            let started = Instant::now();
            let proxy = self.proxy.clone();
            if let Ok(wv) = WebViewBuilder::new_with_web_context(&mut self.web_context)
                .with_environment(shared_env).with_profile_name("default".to_string())
                .with_transparent(true).with_theme(Theme::Light)
                .with_html(inject_ui_html(SETTINGS_MODAL_HTML, &theme))
                .with_ipc_handler(move |req| Self::handle_ipc(&proxy, req.body(), None))
                .with_bounds(Rect { position: LogicalPosition::new(0, 0).into(), size: LogicalSize::new(860, 560).into() })
                .build_as_child(&self.window) {
                let _ = wv.set_visible(false);
                self.settings_shell = Some(wv);
            }
            Self::log_perf("secondary hydrate settings shell", started.elapsed().as_secs_f64() * 1000.0);
        }
    }

    /// Mark a committed secondary for on-demand optional-shell hydration.
    /// Feature entrypoints call `ensure_optional_shells` immediately before use,
    /// so the transfer commit itself never blocks on five auxiliary WebViews.
    fn hydrate_secondary_shells(&mut self) {
        self.optional_shells_enabled = true;
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
                Self::handle_ipc(&proxy_ipc, req.body(), None);
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
                Self::handle_ipc(&proxy_ipc, req.body(), None);
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
                Self::handle_ipc(&proxy_ipc, req.body(), None);
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

    fn ensure_glance_shells(&mut self, url: &str, title: &str) {
        self.glance_url = url.to_string();
        self.glance_title = title.to_string();
        let shared_env = self.tab_shell.environment();

        if let Some(body) = &self.glance_body_shell {
            let _ = body.load_url(url);
        } else {
            let proxy_body = self.proxy.clone();
            if let Ok(wv) = WebViewBuilder::new_with_web_context(&mut self.web_context)
                .with_environment(shared_env.clone())
                .with_profile_name("default".to_string())
                .with_user_agent(CHROME_USER_AGENT)
                .with_autoplay(true)
                .with_clipboard(true)
                .with_theme(Theme::Light)
                .with_visible(false)
                .with_initialization_script(SHELL_SHORTCUTS_JS)
                .with_url(url)
                .with_ipc_handler(move |req| {
                    Self::handle_ipc(&proxy_body, req.body(), None);
                })
                .with_bounds(Rect {
                    position: LogicalPosition::new(0, 0).into(),
                    size: LogicalSize::new(600, 400).into(),
                })
                .build_as_child(&self.window)
            {
                let _ = wv.set_memory_usage_level(MemoryUsageLevel::Normal);
                self.glance_body_shell = Some(wv);
            }
        }

        if self.glance_header_shell.is_none() {
            let proxy_hdr = self.proxy.clone();
            let header_html = inject_ui_html(GLANCE_HEADER_HTML, &self.ui_theme);
            if let Ok(wv) = WebViewBuilder::new_with_web_context(&mut self.web_context)
                .with_environment(shared_env.clone())
                .with_profile_name("default".to_string())
                .with_transparent(true)
                .with_theme(Theme::Light)
                .with_visible(false)
                .with_html(header_html)
                .with_ipc_handler(move |req| {
                    Self::handle_ipc(&proxy_hdr, req.body(), None);
                })
                .with_bounds(Rect {
                    position: LogicalPosition::new(0, 0).into(),
                    size: LogicalSize::new(600, GLANCE_HEADER_HEIGHT).into(),
                })
                .build_as_child(&self.window)
            {
                self.glance_header_shell = Some(wv);
            }
        }

        if self.glance_backdrop_shell.is_none() {
            let proxy_bd = self.proxy.clone();
            if let Ok(wv) = WebViewBuilder::new_with_web_context(&mut self.web_context)
                .with_environment(shared_env)
                .with_profile_name("default".to_string())
                .with_transparent(true)
                .with_theme(Theme::Light)
                .with_visible(false)
                .with_html(GLANCE_BACKDROP_HTML)
                .with_ipc_handler(move |req| {
                    Self::handle_ipc(&proxy_bd, req.body(), None);
                })
                .with_bounds(Rect {
                    position: LogicalPosition::new(0, 0).into(),
                    size: LogicalSize::new(600, 400).into(),
                })
                .build_as_child(&self.window)
            {
                self.glance_backdrop_shell = Some(wv);
            }
        }
    }

    fn normalize_url(input: &str) -> String {
        Self::normalize_url_with_engine(input, "google")
    }

    fn looks_like_web_address(input: &str) -> bool {
        if input.is_empty() || input.chars().any(char::is_whitespace) {
            return false;
        }
        if input.contains("://") {
            return true;
        }
        let host = input.split(['/', '?', '#']).next().unwrap_or("");
        let hostname = host.split(':').next().unwrap_or("");
        if hostname.eq_ignore_ascii_case("localhost") {
            return true;
        }
        let octets: Vec<&str> = hostname.split('.').collect();
        if octets.len() == 4 && octets.iter().all(|part| part.parse::<u8>().is_ok()) {
            return true;
        }
        hostname.contains('.') && !hostname.starts_with('.') && !hostname.ends_with('.')
    }

    fn normalize_url_with_engine(input: &str, engine: &str) -> String {
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
        if trimmed.contains("://") {
            return trimmed.to_string();
        }
        if Self::looks_like_web_address(trimmed) {
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
        match engine.to_ascii_lowercase().as_str() {
            "duckduckgo" => format!("https://duckduckgo.com/?q={}", encoded),
            "bing" => format!("https://www.bing.com/search?q={}", encoded),
            "perplexity" => format!("https://www.perplexity.ai/search?q={}", encoded),
            _ => format!("https://www.google.com/search?q={}", encoded),
        }
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

    fn normalize_container_id(value: &str) -> String {
        let id = value.trim();
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            "default".to_string()
        } else { id.to_string() }
    }

    fn container_profile_name(container_id: &str) -> String {
        if container_id == "default" {
            "default".to_string()
        } else {
            format!("container_{}", container_id)
        }
    }

    fn cleanup_private_profile(&self, container_id: &str) {
        if !container_id.starts_with("private_") {
            return;
        }
        let profile_dir = self.exe_dir.join("user_data").join(Self::container_profile_name(container_id));
        let _ = fs::remove_dir_all(profile_dir);
    }

    fn visible_tab_indices(&self) -> Vec<usize> {
        self.tabs
            .iter()
            .enumerate()
            .filter(|(_, tab)| tab.essential || tab.workspace_id == self.active_workspace_id)
            .map(|(idx, _)| idx)
            .collect()
    }

    fn visible_user_tab_indices(&self) -> Vec<usize> {
        self.tabs
            .iter()
            .enumerate()
            .filter(|(_, tab)| (tab.essential || tab.workspace_id == self.active_workspace_id) && tab.worker_kind.is_none())
            .map(|(idx, _)| idx)
            .collect()
    }

    fn active_user_tab_ids(&self) -> HashSet<u64> {
        self.visible_user_tab_indices()
            .into_iter()
            .map(|idx| self.tabs[idx].id)
            .collect()
    }

    fn selected_active_user_tab_ids(&self) -> HashSet<u64> {
        let active_ids = self.active_user_tab_ids();
        self.selected_tab_ids
            .iter()
            .copied()
            .filter(|id| active_ids.contains(id))
            .collect()
    }

    fn prune_tab_selection(&mut self) {
        let active_ids = self.active_user_tab_ids();
        self.selected_tab_ids.retain(|id| active_ids.contains(id));
        if self.tab_selection_anchor_id.is_some_and(|id| !active_ids.contains(&id)) {
            self.tab_selection_anchor_id = None;
        }
    }

    fn add_tab(&mut self, raw_url: &str, isolated: bool) {
        self.add_tab_internal(raw_url, isolated, true, None, None, false, None);
    }

    fn add_worker_tab(&mut self, raw_url: &str, worker_kind: &str, custom_title: Option<&str>) -> Option<u64> {
        self.add_tab_internal(
            raw_url,
            false,
            false,
            Some(worker_kind.to_string()),
            custom_title.map(|s| s.to_string()),
            false,
            None,
        )
    }

    fn cached_tab_viewport_rect(&self) -> Rect {
        let scale = self.window.scale_factor();
        let logical = self.window.inner_size().to_logical::<u32>(scale);
        let sidebar_w = if self.tab_orientation == TabOrientation::VerticalLeft
            && !self.sidebar_collapsed
            && !self.compact_mode
        {
            self.sidebar_width.min(logical.width / 2)
        } else {
            0
        };
        let dock_w = if self.ai_mode == AiDisplayMode::RightDock {
            self.ai_dock_width.min(logical.width / 2)
        } else {
            0
        };
        let top_h = self.titlebar_height();
        Rect {
            position: LogicalPosition::new(sidebar_w as i32, top_h as i32).into(),
            size: LogicalSize::new(
                logical.width.saturating_sub(sidebar_w + dock_w).max(400),
                logical.height.saturating_sub(top_h).max(300),
            )
            .into(),
        }
    }

    fn build_tab_webview(
        &mut self,
        tab_id: u64,
        url: &str,
        profile_name: &str,
        isolated: bool,
        is_worker: bool,
        visible: bool,
        should_sleep_load: bool,
    ) -> Option<BuiltTabWebView> {
        let is_tiktok = url.to_lowercase().contains("tiktok.com");
        let proxy_ipc = self.proxy.for_tab(tab_id);
        let proxy_popup = self.proxy.for_tab(tab_id);
        let proxy_load = self.proxy.for_tab(tab_id);
        let proxy_download = self.proxy.for_tab(tab_id);
        let proxy_download_done = self.proxy.for_tab(tab_id);
        let shared_env = self.tab_shell.environment();
        let extensions_dir = self.exe_dir.join("extensions");
        let viewport_rect = self.cached_tab_viewport_rect();

        let mut builder = WebViewBuilder::new_with_web_context(&mut self.web_context)
            .with_environment(shared_env)
            .with_browser_extensions_enabled(true)
            .with_autoplay(true)
            .with_clipboard(true)
            .with_devtools(true)
            .with_theme(Theme::Light)
            .with_visible(visible && !is_worker)
            .with_background_color(if self.ui_theme == "carrara-light" {
                (246, 242, 234, 255)
            } else {
                (14, 16, 20, 255)
            })
            .with_profile_name(profile_name.to_string())
            .with_download_started_handler(move |uri, path| {
                let downloads = std::env::var_os("USERPROFILE")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("Downloads");
                let _ = fs::create_dir_all(&downloads);
                let raw_name = uri.rsplit('/').next().unwrap_or("download");
                let name = raw_name.split('?').next().unwrap_or("download");
                let safe_name: String = name
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect();
                let filename = if safe_name.is_empty() {
                    "download"
                } else {
                    safe_name.as_str()
                };
                let target = downloads.join(filename);
                *path = target.clone();
                let _ = proxy_download.send_event(AppCmd::DownloadStarted {
                    uri,
                    path: target.display().to_string(),
                });
                true
            })
            .with_download_completed_handler(move |uri, path, success| {
                let _ = proxy_download_done.send_event(AppCmd::DownloadCompleted {
                    uri,
                    path: path.map(|p| p.display().to_string()).unwrap_or_default(),
                    success,
                });
            });

        if !is_tiktok {
            let worker_guard = if is_worker {
                "window.__tpIsWorkerTab=true;window.alert=function(){};window.confirm=function(){return true;};window.prompt=function(){return null;};window.print=function(){};window.add_eventListener('beforeunload',function(e){e.stopImmediatePropagation();},true);\n"
            } else {
                ""
            };
            let boost_seed = if self.boosts_json.trim().is_empty() {
                "{}".to_string()
            } else {
                self.boosts_json.clone()
            };
            let ctx_seed = if self.context_menu_prefs_json.trim().is_empty() {
                "{}".to_string()
            } else {
                self.context_menu_prefs_json.clone()
            };
            let init_js = format!(
                "{}window.__tpBoostsMap = {};\nwindow.__tpContextMenuPrefs = {};\n{}\n{}",
                worker_guard,
                boost_seed,
                ctx_seed,
                CONTENT_AGENT_JS.replace("__TAB_ID__", &tab_id.to_string()),
                ""
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
            // Let WebView2 show its native permission prompt unless the user
            // explicitly configured a persisted policy in Settings.
            .with_permission_handler(|kind| Self::permission_response_for_kind(kind))
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
                        container_id: None,
                    });
                    NewWindowResponse::Deny
                }
            })
            .with_ipc_handler(move |req| {
                Self::handle_ipc(&proxy_ipc, req.body(), Some(tab_id));
            })
            .with_bounds(viewport_rect);

        if isolated {
            builder = builder.with_extensions_path(extensions_dir);
        }

        let is_ext_url = url.starts_with("chrome-extension://");
        builder = if should_sleep_load {
            builder.with_html("<html><body style='background:#0d0f14; color:#94a3b8; font-family:sans-serif; display:flex; align-items:center; justify-content:center; height:100vh; margin:0;'><div style='text-align:center;'>⚡ <span style='font-size:13px;'>Đang ngủ đông để tiết kiệm RAM... Bấm để đánh thức</span></div></body></html>")
        } else if url == "tp://newtab" {
            builder.with_html(cached_newtab_html(&self.ui_theme))
        } else if url == "tp://history" {
            builder.with_html(cached_history_html(&self.ui_theme))
        } else if url == "tp://workspace" {
            // Legacy sessions may still contain the old workspace URL. Keep those
            // sessions usable without loading the removed e-commerce dashboard.
            builder.with_html(cached_newtab_html(&self.ui_theme))
        } else if is_ext_url {
            builder.with_html("<html><body style='background:#f8fafc;'></body></html>")
        } else {
            builder.with_url(url)
        };

        if let Ok(webview) = builder.build_as_child(&self.window) {
            #[cfg(target_os = "windows")]
            let navigation_events =
                install_navigation_correlation(&webview, tab_id, self.proxy.for_tab(tab_id));
            Some(BuiltTabWebView {
                webview,
                #[cfg(target_os = "windows")]
                navigation_events,
            })
        } else {
            None
        }
    }

    fn schedule_prewarm_newtab(&mut self, delay_ms: u64) {
        if self.should_exit
            || !self.persist_session
            || !self.optional_shells_enabled
            || self.tabs.len() >= 14
            || self.warm_newtab.is_some()
            || self.warm_newtab_pending
        {
            return;
        }
        self.warm_newtab_pending = true;
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            let _ = proxy.send_event(AppCmd::PrewarmNewTab);
        });
    }

    fn ensure_warm_newtab(&mut self) {
        self.warm_newtab_pending = false;
        if self.should_exit
            || !self.persist_session
            || !self.optional_shells_enabled
            || self.tabs.len() >= 14
        {
            return;
        }
        let container_id = self
            .workspaces
            .iter()
            .find(|w| w.id == self.active_workspace_id)
            .map(|w| Self::normalize_container_id(&w.container_id))
            .unwrap_or_else(|| "default".to_string());
        if container_id.starts_with("private_") {
            return;
        }
        let profile_name = if container_id != "default" {
            Self::container_profile_name(&container_id)
        } else {
            "default".to_string()
        };
        if self.warm_newtab.as_ref().is_some_and(|w| {
            w.ui_theme == self.ui_theme
                && w.container_id == container_id
                && w.profile_name == profile_name
        }) {
            return;
        }
        self.warm_newtab = None;
        let tab_id = NEXT_TAB_ID.fetch_add(1, Ordering::SeqCst);
        if let Some(built) =
            self.build_tab_webview(tab_id, "tp://newtab", &profile_name, false, false, false, false)
        {
            let _ = built.webview.set_visible(false);
            let _ = built.webview.set_memory_usage_level(MemoryUsageLevel::Low);
            self.warm_newtab = Some(WarmStandbyTab {
                id: tab_id,
                ui_theme: self.ui_theme.clone(),
                container_id,
                profile_name,
                webview: built.webview,
                #[cfg(target_os = "windows")]
                navigation_events: built.navigation_events,
            });
        }
    }

    fn add_tab_internal(
        &mut self,
        raw_url: &str,
        isolated: bool,
        sync_now: bool,
        worker_kind: Option<String>,
        custom_title: Option<String>,
        lazy: bool,
        container_id: Option<String>,
    ) -> Option<u64> {
        if self.tabs.len() >= 14 {
            return None;
        }
        let container_id = container_id.as_deref().map(Self::normalize_container_id).unwrap_or_else(|| {
            if !isolated && worker_kind.is_none() {
                self.workspaces
                    .iter()
                    .find(|w| w.id == self.active_workspace_id)
                    .map(|w| Self::normalize_container_id(&w.container_id))
                    .unwrap_or_else(|| "default".to_string())
            } else {
                "default".to_string()
            }
        });
        let is_private = container_id.starts_with("private_");
        let profile_name = if container_id != "default" {
            Self::container_profile_name(&container_id)
        } else if isolated {
            self.profile_counter += 1;
            format!("acc_{}", self.profile_counter)
        } else {
            "default".to_string()
        };

        let url = Self::normalize_url(raw_url);
        let is_worker = worker_kind.is_some();
        let is_ext_url = url.starts_with("chrome-extension://");
        let should_sleep_load = lazy && !is_worker && !url.starts_with("tp://");

        let warm_match = url == "tp://newtab"
            && !isolated
            && !is_worker
            && !lazy
            && sync_now
            && self.warm_newtab.as_ref().is_some_and(|w| {
                w.ui_theme == self.ui_theme
                    && w.container_id == container_id
                    && w.profile_name == profile_name
            });

        let claimed = if warm_match {
            self.warm_newtab.take().map(|warm| {
                (
                    warm.id,
                    BuiltTabWebView {
                        webview: warm.webview,
                        #[cfg(target_os = "windows")]
                        navigation_events: warm.navigation_events,
                    },
                    true,
                )
            })
        } else {
            let tab_id = NEXT_TAB_ID.fetch_add(1, Ordering::SeqCst);
            self.build_tab_webview(
                tab_id,
                &url,
                &profile_name,
                isolated,
                is_worker,
                sync_now,
                should_sleep_load,
            )
            .map(|built| (tab_id, built, false))
        };

        if let Some((tab_id, built, from_warm)) = claimed {
            let webview = built.webview;
            #[cfg(target_os = "windows")]
            let navigation_events = built.navigation_events;
            if is_ext_url && !from_warm {
                let ext_target = url.clone();
                let proxy_delayed = self.proxy.for_tab(tab_id);
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(650));
                    let _ = proxy_delayed.send_event(AppCmd::NavigateActive(ext_target));
                });
            }
            let title = custom_title.unwrap_or_else(|| Self::title_from_url(&url));
            let is_loaded = !should_sleep_load;
            let mut navigation_readiness = shopee_discovery::NavigationReadiness::default();
            #[cfg(target_os = "windows")]
            if !navigation_events.state.is_usable() {
                navigation_readiness.close();
            }
            self.tabs.push(BrowserTab {
                id: tab_id,
                navigation_readiness,
                container_id,
                workspace_id: self.active_workspace_id.clone(),
                profile_name,
                url: url.clone(),
                title,
                favicon_url: String::new(),
                worker_kind,
                private: is_private,
                ecom_worker_job: None,
                worker_status: if is_worker { "Đang chạy ngầm".to_string() } else { String::new() },
                pinned: false,
                essential: false,
                pinned_origin_url: None,
                tab_group_id: None,
                is_loaded,
                last_active_at: Instant::now(),
                audible: false,
                muted: false,
                media_title: String::new(),
                media_artist: String::new(),
                webview,
                #[cfg(target_os = "windows")]
                navigation_events,
            });
            if !is_worker {
                self.active_idx = self.tabs.len() - 1;
                self.mru_tab_ids.retain(|id| *id != tab_id);
                self.mru_tab_ids.insert(0, tab_id);
                self.mru_cycle_ids = None;
            } else if let Some(t) = self.tabs.last() {
                let _ = t.webview.set_visible(true);
                let _ = t.webview.set_bounds(Rect {
                    position: LogicalPosition::new(-16000, -16000).into(),
                    size: LogicalSize::new(1280, 800).into(),
                });
                let _ = t.webview.set_memory_usage_level(MemoryUsageLevel::Normal);
                #[cfg(target_os = "windows")]
                Self::send_webview_to_back(&t.webview);
            }
            if sync_now {
                self.relayout();
                self.sync_all_ui();
                if !is_worker && !is_private {
                    self.mark_session_dirty();
                }
                if url == "tp://newtab" && !isolated && !is_worker {
                    self.schedule_prewarm_newtab(260);
                }
            }
            return Some(tab_id);
        }
        None
    }

    fn titlebar_height(&self) -> u32 {
        match self.tab_orientation {
            TabOrientation::VerticalLeft => 0,
            TabOrientation::HorizontalTop if self.show_bookmarks_bar => {
                TITLEBAR_HEIGHT + BOOKMARKS_BAR_HEIGHT
            }
            TabOrientation::HorizontalTop => TITLEBAR_HEIGHT,
        }
    }

    fn relayout(&self) {
        let t0 = Instant::now();
        let scale = self.window.scale_factor();
        let logical = self.window.inner_size().to_logical::<u32>(scale);
        let top_reserved = self.titlebar_height();
        let compact_overlay_active = self.tab_orientation == TabOrientation::VerticalLeft && self.compact_mode;
        let mut shell = compute_shell_layout(
            logical.width,
            logical.height.saturating_sub(top_reserved),
            if compact_overlay_active { TabOrientation::HorizontalTop } else { self.tab_orientation },
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

        let full_overlay_active = self.command_bar_open || self.mru_hud_open;
        if full_overlay_active {
            let _ = self.tab_shell.set_visible(true);
            let _ = self.tab_shell.set_bounds(Rect {
                position: LogicalPosition::new(0, 0).into(),
                size: LogicalSize::new(logical.width.max(320), logical.height.max(240)).into(),
            });
            let _ = self.sidebar_resizer_shell.set_visible(false);
        } else if self.tab_orientation == TabOrientation::VerticalLeft && self.compact_mode {
            let show_overlay = self.compact_sidebar_hover || self.command_bar_open;
            if show_overlay {
                let tab_w = self.sidebar_width.clamp(180, 440);
                let _ = self.tab_shell.set_visible(true);
                let _ = self.tab_shell.set_bounds(Rect {
                    position: LogicalPosition::new(0, top_reserved as i32).into(),
                    size: LogicalSize::new(tab_w, logical.height.saturating_sub(top_reserved)).into(),
                });
                #[cfg(target_os = "windows")]
                Self::bring_webview_to_front(&self.tab_shell);
            } else {
                let _ = self.tab_shell.set_visible(false);
            }
            let _ = self.sidebar_resizer_shell.set_visible(false);
        } else if self.tab_orientation == TabOrientation::VerticalLeft {
            let _ = self.tab_shell.set_visible(true);
            let _ = self.tab_shell.set_bounds(Rect {
                position: LogicalPosition::new(shell.tab_bar.x, shell.tab_bar.y).into(),
                size: LogicalSize::new(shell.tab_bar.width, shell.tab_bar.height).into(),
            });
            if !self.sidebar_collapsed && !self.command_bar_open && shell.tab_bar.width > 0 {
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
                if let Some(shell) = &self.sidebar_resizer_shell { Self::bring_webview_to_front(shell); }
            } else {
                let _ = self.sidebar_resizer_shell.set_visible(false);
            }
        } else {
            let _ = self.tab_shell.set_visible(false);
            let _ = self.sidebar_resizer_shell.set_visible(false);
        }

        let total = self
            .tabs
            .iter()
            .filter(|tab| tab.workspace_id == self.active_workspace_id)
            .count();
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
                    if !tab.essential && tab.workspace_id != self.active_workspace_id {
                        let _ = tab.webview.set_visible(false);
                        let _ = tab.webview.set_memory_usage_level(MemoryUsageLevel::Low);
                        continue;
                    }
                    if idx == self.active_idx {
                        continue;
                    }
                    if tab.worker_kind.is_some() {
                        // Đẩy Worker Tab ra ngoài màn hình (-16000, -16000) kích thước 1280x800:
                        // SPA/API vẫn chạy full-speed không bị ngủ đông, tuyệt đối không che/cướp chuột của tab người dùng.
                        let _ = tab.webview.set_visible(true);
                        let _ = tab.webview.set_bounds(Rect {
                            position: LogicalPosition::new(-16000, -16000).into(),
                            size: LogicalSize::new(1280, 800).into(),
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
                let paired_idx_opt = self.split_paired_tab_id.and_then(|pid| {
                    self.tabs.iter().position(|t| {
                        t.workspace_id == self.active_workspace_id
                            && t.id == pid
                            && t.id != self.tabs[self.active_idx].id
                    })
                }).or_else(|| {
                    let visible_indices = self.visible_tab_indices();
                    if visible_indices.len() > 1 {
                        let active_pos = visible_indices
                            .iter()
                            .position(|idx| *idx == self.active_idx)
                            .unwrap_or(0);
                        let pair_pos = if active_pos + 1 < visible_indices.len() {
                            active_pos + 1
                        } else {
                            active_pos.saturating_sub(1)
                        };
                        visible_indices.get(pair_pos).copied()
                    } else {
                        None
                    }
                });

                if (self.view_mode == ContentViewMode::ColumnsBsp || self.view_mode == ContentViewMode::RowsBsp) && paired_idx_opt.is_some() {
                    let p_idx = paired_idx_opt.unwrap();
                    let rects = compute_content_panes(
                        shell.content_area,
                        self.view_mode,
                        2,
                        self.master_ratio,
                        4,
                    );
                    for (idx, tab) in self.tabs.iter().enumerate() {
                        if !tab.essential && tab.workspace_id != self.active_workspace_id {
                            let _ = tab.webview.set_visible(false);
                            let _ = tab.webview.set_memory_usage_level(MemoryUsageLevel::Low);
                            continue;
                        }
                        if idx == self.active_idx {
                            if let Some(r) = rects.get(0) {
                                let _ = tab.webview.set_visible(true);
                                let _ = tab.webview.set_bounds(Rect {
                                    position: LogicalPosition::new(r.x, r.y).into(),
                                    size: LogicalSize::new(r.width, r.height).into(),
                                });
                                let _ = tab.webview.set_memory_usage_level(MemoryUsageLevel::Normal);
                                #[cfg(target_os = "windows")]
                                Self::send_webview_to_back(&tab.webview);
                            }
                        } else if idx == p_idx {
                            if let Some(r) = rects.get(1) {
                                let _ = tab.webview.set_visible(true);
                                let _ = tab.webview.set_bounds(Rect {
                                    position: LogicalPosition::new(r.x, r.y).into(),
                                    size: LogicalSize::new(r.width, r.height).into(),
                                });
                                let _ = tab.webview.set_memory_usage_level(MemoryUsageLevel::Normal);
                                #[cfg(target_os = "windows")]
                                Self::send_webview_to_back(&tab.webview);
                            }
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
                    let mut pane_idx = 0usize;
                    for tab in &self.tabs {
                        if tab.workspace_id != self.active_workspace_id {
                            let _ = tab.webview.set_visible(false);
                            let _ = tab.webview.set_memory_usage_level(MemoryUsageLevel::Low);
                            continue;
                        }
                        if let Some(r) = rects.get(pane_idx) {
                            let _ = tab.webview.set_visible(true);
                            let _ = tab.webview.set_bounds(Rect {
                                position: LogicalPosition::new(r.x, r.y).into(),
                                size: LogicalSize::new(r.width, r.height).into(),
                            });
                            let _ = tab.webview.set_memory_usage_level(if tab.id == self.tabs[self.active_idx].id {
                                MemoryUsageLevel::Normal
                            } else {
                                MemoryUsageLevel::Low
                            });
                            #[cfg(target_os = "windows")]
                            Self::send_webview_to_back(&tab.webview);
                        }
                        pane_idx += 1;
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
                if let Some(shell) = &self.chat_header_shell { Self::bring_webview_to_front(shell); }
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
                    if let Some(shell) = &self.ai_shell { Self::bring_webview_to_front(shell); }
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
                if let Some(shell) = &self.dock_resizer_shell { Self::bring_webview_to_front(shell); }
            } else {
                let _ = self.dock_resizer_shell.set_visible(false);
            }
        } else {
            let _ = self.dock_resizer_shell.set_visible(false);
        }

        let tb_h = self.titlebar_height();
        let titlebar_visible = self.tab_orientation == TabOrientation::HorizontalTop;
        let _ = self.titlebar_shell.set_visible(titlebar_visible);
        if titlebar_visible {
            let effective_tb_h = if self.command_bar_open {
                logical.height.min(460).max(tb_h)
            } else if self.titlebar_popover_open {
                logical.height.min(380).max(tb_h)
            } else {
                tb_h
            };
            let _ = self.titlebar_shell.set_bounds(Rect {
                position: LogicalPosition::new(0, 0).into(),
                size: LogicalSize::new(logical.width.max(320), effective_tb_h).into(),
            });
            #[cfg(target_os = "windows")]
            Self::bring_webview_to_front(&self.titlebar_shell);
        }
        if full_overlay_active {
            #[cfg(target_os = "windows")]
            Self::bring_webview_to_front(&self.tab_shell);
        }

        if self.settings_open {
            let _ = self.settings_shell.set_visible(true);
            let _ = self.settings_shell.set_bounds(Rect {
                position: LogicalPosition::new(0, 0).into(),
                size: LogicalSize::new(logical.width.max(320), logical.height.max(240)).into(),
            });
            #[cfg(target_os = "windows")]
            if let Some(shell) = &self.settings_shell { Self::bring_webview_to_front(shell); }
        } else {
            let _ = self.settings_shell.set_visible(false);
        }

        if self.glance_open {
            let (bd_r, hdr_r, body_r) = compute_glance_rects(logical.width, logical.height);
            if let Some(bd) = &self.glance_backdrop_shell {
                let _ = bd.set_visible(true);
                let _ = bd.set_bounds(Rect {
                    position: LogicalPosition::new(bd_r.x, bd_r.y).into(),
                    size: LogicalSize::new(bd_r.width.max(1), bd_r.height.max(1)).into(),
                });
                #[cfg(target_os = "windows")]
                Self::bring_webview_to_front(bd);
            }
            if let Some(hdr) = &self.glance_header_shell {
                let _ = hdr.set_visible(true);
                let _ = hdr.set_bounds(Rect {
                    position: LogicalPosition::new(hdr_r.x, hdr_r.y).into(),
                    size: LogicalSize::new(hdr_r.width.max(1), hdr_r.height.max(1)).into(),
                });
                #[cfg(target_os = "windows")]
                Self::bring_webview_to_front(hdr);
            }
            if let Some(body) = &self.glance_body_shell {
                let _ = body.set_visible(true);
                let _ = body.set_bounds(Rect {
                    position: LogicalPosition::new(body_r.x, body_r.y).into(),
                    size: LogicalSize::new(body_r.width.max(1), body_r.height.max(1)).into(),
                });
                #[cfg(target_os = "windows")]
                Self::bring_webview_to_front(body);
            }
        } else {
            if let Some(bd) = &self.glance_backdrop_shell {
                let _ = bd.set_visible(false);
            }
            if let Some(hdr) = &self.glance_header_shell {
                let _ = hdr.set_visible(false);
            }
            if let Some(body) = &self.glance_body_shell {
                let _ = body.set_visible(false);
            }
        }

        for tab in &self.tabs {
            if tab.workspace_id != self.active_workspace_id {
                let _ = tab.webview.set_visible(false);
                let _ = tab.webview.set_memory_usage_level(MemoryUsageLevel::Low);
            }
        }
        let dur = t0.elapsed().as_secs_f64() * 1000.0;
        if dur > 5.0 {
            Self::log_perf("relayout (bounds)", dur);
        }
    }

    fn clean_tracking_url(raw: &str) -> String {
        let trimmed = raw.trim();
        let lower = trimmed.to_ascii_lowercase();
        if !lower.starts_with("http://") && !lower.starts_with("https://") {
            return trimmed.to_string();
        }
        let (before_hash, hash) = match trimmed.find('#') {
            Some(idx) => (&trimmed[..idx], &trimmed[idx..]),
            None => (trimmed, ""),
        };
        let Some(q_idx) = before_hash.find('?') else {
            return trimmed.to_string();
        };
        let base = &before_hash[..q_idx];
        let query = &before_hash[q_idx + 1..];
        let kept: Vec<&str> = query
            .split('&')
            .filter(|part| {
                if part.is_empty() {
                    return false;
                }
                let key = part.split('=').next().unwrap_or("").to_ascii_lowercase();
                !(key.starts_with("utm_")
                    || matches!(
                        key.as_str(),
                        "fbclid"
                            | "gclid"
                            | "dclid"
                            | "msclkid"
                            | "mc_cid"
                            | "mc_eid"
                            | "si"
                            | "igsh"
                            | "ref_src"
                            | "_hsenc"
                            | "_hsmi"
                    ))
            })
            .collect();
        if kept.is_empty() {
            format!("{}{}", base, hash)
        } else {
            format!("{}?{}{}", base, kept.join("&"), hash)
        }
    }

    fn normalize_context_menu_prefs(raw: &str) -> String {
        let parsed = parse_json(raw).ok();
        let flag = |key: &str, default: bool| -> bool {
            parsed
                .as_ref()
                .and_then(|v| v.get(key))
                .and_then(JsonVal::as_bool)
                .unwrap_or(default)
        };
        JsonVal::obj(vec![
            ("hide_separators", JsonVal::Bool(flag("hide_separators", true))),
            ("clean_copy_link", JsonVal::Bool(flag("clean_copy_link", true))),
            ("hide_mute", JsonVal::Bool(flag("hide_mute", false))),
            ("hide_unload", JsonVal::Bool(flag("hide_unload", false))),
            ("hide_pin_essentials", JsonVal::Bool(flag("hide_pin_essentials", false))),
            ("hide_split", JsonVal::Bool(flag("hide_split", false))),
            ("hide_move_space", JsonVal::Bool(flag("hide_move_space", false))),
            ("hide_duplicate", JsonVal::Bool(flag("hide_duplicate", false))),
            ("hide_close_others", JsonVal::Bool(flag("hide_close_others", false))),
            ("web_menu_enabled", JsonVal::Bool(flag("web_menu_enabled", true))),
        ])
        .to_json()
    }

    fn normalize_bookmarks_json(raw: &str) -> Option<String> {
        let Ok(JsonVal::Arr(arr)) = parse_json(raw.trim()) else {
            return None;
        };
        let mut normalized = Vec::new();
        for item in arr {
            let url = item
                .get("url")
                .and_then(JsonVal::as_str)
                .map(str::trim)
                .unwrap_or("");
            if url.is_empty() {
                continue;
            }
            let title = item
                .get("title")
                .and_then(JsonVal::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(url);
            let folder = item
                .get("folder")
                .and_then(JsonVal::as_str)
                .map(str::trim)
                .unwrap_or("");
            let keyword = item
                .get("keyword")
                .and_then(JsonVal::as_str)
                .map(|s| s.split_whitespace().collect::<Vec<_>>().join("").to_ascii_lowercase())
                .unwrap_or_default();

            let mut tags_vec: Vec<String> = Vec::new();
            let mut seen_tags: HashSet<String> = HashSet::new();
            let push_tag = |raw_tag: &str, out: &mut Vec<String>, seen: &mut HashSet<String>| {
                let cleaned = raw_tag.trim().trim_start_matches('#').trim();
                if cleaned.is_empty() {
                    return;
                }
                let lower = cleaned.to_ascii_lowercase();
                if seen.insert(lower) {
                    out.push(cleaned.to_string());
                }
            };
            if let Some(arr_tags) = item.get("tags").and_then(JsonVal::as_arr) {
                for t in arr_tags {
                    if let Some(s) = t.as_str() {
                        push_tag(s, &mut tags_vec, &mut seen_tags);
                    }
                }
            } else if let Some(str_tags) = item.get("tags").and_then(JsonVal::as_str) {
                for part in str_tags.split(',') {
                    push_tag(part, &mut tags_vec, &mut seen_tags);
                }
            }

            normalized.push(JsonVal::obj(vec![
                ("title", JsonVal::Str(title.to_string())),
                ("url", JsonVal::Str(url.to_string())),
                ("folder", JsonVal::Str(folder.to_string())),
                (
                    "tags",
                    JsonVal::Arr(tags_vec.into_iter().map(JsonVal::Str).collect()),
                ),
                ("keyword", JsonVal::Str(keyword)),
            ]));
        }
        Some(JsonVal::Arr(normalized).to_json())
    }

    fn normalize_boost_domain(raw: &str) -> String {
        let trimmed = raw.trim().to_ascii_lowercase();
        let no_scheme = trimmed
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        let host = no_scheme
            .split(['/', '?', '#'])
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("")
            .trim_start_matches("www.")
            .trim();
        if host.is_empty() || host.chars().any(char::is_whitespace) {
            String::new()
        } else {
            host.to_string()
        }
    }

    fn normalize_boosts_json(raw: &str) -> Option<String> {
        let Ok(JsonVal::Obj(entries)) = parse_json(raw.trim()) else {
            return None;
        };
        let mut out_pairs: Vec<(String, JsonVal)> = Vec::new();
        for (raw_domain, val) in entries {
            let domain = Self::normalize_boost_domain(&raw_domain);
            if domain.is_empty() {
                continue;
            }
            let css = val
                .get("css")
                .and_then(JsonVal::as_str)
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            let enabled = val
                .get("enabled")
                .and_then(JsonVal::as_bool)
                .unwrap_or(true);
            let mut zaps: Vec<String> = Vec::new();
            let mut seen: HashSet<String> = HashSet::new();
            if let Some(arr) = val.get("zap_selectors").and_then(JsonVal::as_arr) {
                for item in arr {
                    if let Some(sel) = item.as_str() {
                        let cleaned = sel.trim();
                        if !cleaned.is_empty() && seen.insert(cleaned.to_string()) {
                            zaps.push(cleaned.to_string());
                        }
                    }
                }
            }
            if css.is_empty() && zaps.is_empty() {
                continue;
            }
            let entry_val = JsonVal::obj(vec![
                ("css", JsonVal::Str(css)),
                (
                    "zap_selectors",
                    JsonVal::Arr(zaps.into_iter().map(JsonVal::Str).collect()),
                ),
                ("enabled", JsonVal::Bool(enabled)),
            ]);
            if let Some(existing) = out_pairs.iter_mut().find(|(k, _)| k == &domain) {
                existing.1 = entry_val;
            } else {
                out_pairs.push((domain, entry_val));
            }
        }
        Some(JsonVal::Obj(out_pairs).to_json())
    }

    fn normalize_permission_policy(raw: &str) -> &'static str {
        match raw.trim().to_ascii_lowercase().as_str() {
            "allow" => "allow",
            "deny" => "deny",
            _ => "default",
        }
    }

    fn permission_policy_code(policy: &str) -> u8 {
        match Self::normalize_permission_policy(policy) {
            "allow" => 1,
            "deny" => 2,
            _ => 0,
        }
    }

    fn permission_response_from_code(code: u8) -> PermissionResponse {
        match code {
            1 => PermissionResponse::Allow,
            2 => PermissionResponse::Deny,
            _ => PermissionResponse::Default,
        }
    }

    fn permission_response_for_kind(kind: wry::PermissionKind) -> PermissionResponse {
        match kind {
            wry::PermissionKind::Camera => {
                Self::permission_response_from_code(PERMISSION_CAMERA.load(Ordering::Relaxed))
            }
            wry::PermissionKind::Microphone => {
                Self::permission_response_from_code(PERMISSION_MICROPHONE.load(Ordering::Relaxed))
            }
            wry::PermissionKind::Notifications => {
                Self::permission_response_from_code(PERMISSION_NOTIFICATIONS.load(Ordering::Relaxed))
            }
            wry::PermissionKind::ClipboardRead => {
                Self::permission_response_from_code(PERMISSION_CLIPBOARD.load(Ordering::Relaxed))
            }
            _ => PermissionResponse::Default,
        }
    }

    fn sync_permission_atomics(&self) {
        PERMISSION_CAMERA.store(Self::permission_policy_code(&self.permission_camera), Ordering::Relaxed);
        PERMISSION_MICROPHONE.store(Self::permission_policy_code(&self.permission_microphone), Ordering::Relaxed);
        PERMISSION_NOTIFICATIONS.store(Self::permission_policy_code(&self.permission_notifications), Ordering::Relaxed);
        PERMISSION_CLIPBOARD.store(Self::permission_policy_code(&self.permission_clipboard), Ordering::Relaxed);
    }

    fn downloads_dir() -> PathBuf {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Downloads")
    }

    fn is_safe_download_path(raw_path: &str, base_dir: &std::path::Path) -> Option<PathBuf> {
        let trimmed = raw_path.trim();
        if trimmed.is_empty() {
            return None;
        }
        let Ok(canon_base) = fs::canonicalize(base_dir) else {
            return None;
        };
        let Ok(canon_target) = fs::canonicalize(trimmed) else {
            return None;
        };
        if canon_target.starts_with(&canon_base) {
            Some(canon_target)
        } else {
            None
        }
    }

    pub fn copy_text_to_clipboard(text: &str) -> bool {
        #[cfg(target_os = "windows")]
        unsafe {
            let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes = utf16.len() * 2;
            const GMEM_MOVEABLE: u32 = 0x0002;
            const CF_UNICODETEXT: u32 = 13;
            let h_mem = GlobalAlloc(GMEM_MOVEABLE, bytes);
            if h_mem == 0 {
                return false;
            }
            let ptr = GlobalLock(h_mem);
            if ptr.is_null() {
                return false;
            }
            std::ptr::copy_nonoverlapping(utf16.as_ptr() as *const std::ffi::c_void, ptr, bytes);
            GlobalUnlock(h_mem);
            if OpenClipboard(0) == 0 {
                return false;
            }
            EmptyClipboard();
            SetClipboardData(CF_UNICODETEXT, h_mem);
            CloseClipboard();
            true
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = text;
            false
        }
    }

    fn get_current_git_commit() -> String {
        APP_GIT_COMMIT.to_string()
    }

    fn fetch_remote_git_commit() -> Result<String, String> {
        let command = format!(
            "$ProgressPreference='SilentlyContinue'; (Invoke-RestMethod -Uri '{UPDATE_API_URL}' -Headers @{{'User-Agent'='TPbrowser'}})[0].sha"
        );
        let output = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                command.as_str(),
            ])
            .output()
            .map_err(|err| err.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if commit.is_empty() {
            Err("GitHub trả về commit rỗng.".to_string())
        } else {
            Ok(commit)
        }
    }

    fn build_config_payload_json(&self, open_section: Option<&str>) -> String {
        let sec = open_section.unwrap_or("");
        let orbs_raw = if self.floating_orbs_json.trim().is_empty() {
            "[]"
        } else {
            &self.floating_orbs_json
        };
        let boosts_raw = if self.boosts_json.trim().is_empty() {
            "{}"
        } else {
            &self.boosts_json
        };
        let recovery_raw = if self.window_recovery_json.trim().is_empty() {
            "{}"
        } else {
            &self.window_recovery_json
        };
        let ctx_raw = if self.context_menu_prefs_json.trim().is_empty() {
            "{}"
        } else {
            &self.context_menu_prefs_json
        };
        format!(
            "{{\"ui_theme\":\"{}\",\"logo_style\":\"{}\",\"startup_mode\":\"{}\",\"background_mode\":{},\"startup_boost\":{},\"floating_omnibox\":{},\"compact_titlebar\":{},\"sidebar_width\":{},\"tab_orientation\":\"{}\",\"sidebar_collapsed\":{},\"compact_mode\":{},\"tab_unload_minutes\":{},\"tab_unload_whitelist\":\"{}\",\"translate_target_lang\":\"{}\",\"translate_auto_prompt\":{},\"translate_never_domains\":\"{}\",\"glance_trigger\":\"{}\",\"glance_external_from_pinned\":{},\"default_search_engine\":\"{}\",\"auto_pip_on_switch\":{},\"view_mode\":\"{}\",\"ai_provider\":\"{}\",\"ai_api_key\":\"{}\",\"ai_model\":\"{}\",\"ai_base_url\":\"{}\",\"auto_minimize_on_task\":{},\"ai_mode\":\"{}\",\"show_unread_badges\":{},\"always_show_floating_orbs\":{},\"floating_orbs\":{},\"boosts\":{},\"context_menu_prefs\":{},\"window_recovery\":{},\"ai_dock_width\":{},\"adblock_enabled\":{},\"permission_camera\":\"{}\",\"permission_microphone\":\"{}\",\"permission_notifications\":\"{}\",\"permission_clipboard\":\"{}\",\"google_user\":\"{}\",\"custom_shortcuts\":{},\"chat_channel\":\"{}\",\"unread_ai\":{},\"unread_fb\":{},\"unread_zalo\":{},\"open_section\":\"{}\",\"app_version\":\"{}\",\"app_commit\":\"{}\"}}",
            Self::escape_json(&self.ui_theme),
            Self::escape_json(&self.logo_style),
            Self::escape_json(&self.startup_mode),
            self.background_mode,
            self.startup_boost,
            self.floating_omnibox,
            self.compact_titlebar,
            self.sidebar_width,
            self.tab_orientation.as_str(),
            self.sidebar_collapsed,
            self.compact_mode,
            self.tab_unload_minutes,
            Self::escape_json(&self.tab_unload_whitelist),
            Self::escape_json(&self.translate_target_lang),
            self.translate_auto_prompt,
            Self::escape_json(&self.translate_never_domains),
            Self::escape_json(&self.glance_trigger),
            self.glance_external_from_pinned,
            Self::escape_json(&self.default_search_engine),
            self.auto_pip_on_switch,
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
            boosts_raw,
            ctx_raw,
            recovery_raw,
            self.ai_dock_width,
            self.adblock_enabled,
            Self::escape_json(&self.permission_camera),
            Self::escape_json(&self.permission_microphone),
            Self::escape_json(&self.permission_notifications),
            Self::escape_json(&self.permission_clipboard),
            Self::escape_json(&self.google_user),
            self.custom_shortcuts_json(),
            self.chat_channel.as_str(),
            self.unread_ai,
            self.unread_fb,
            self.unread_zalo,
            Self::escape_json(sec),
            APP_FULL_VERSION,
            Self::get_current_git_commit()
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
        if let Some(t) = self.tabs.get(self.active_idx) {
            let _ = t.webview.evaluate_script(&ev_js);
        }
    }

    fn sync_all_ui(&self) {
        let t0 = Instant::now();
        let mut tabs_json = Vec::new();
        let visible_tabs: Vec<(usize, &BrowserTab)> = self.tabs.iter().enumerate()
            .filter(|(_, t)| t.essential || t.workspace_id == self.active_workspace_id)
            .collect();
        for (native_idx, t) in &visible_tabs {
            tabs_json.push(format!(
                "{{\"id\":{},\"native_idx\":{},\"url\":\"{}\",\"title\":\"{}\",\"favicon\":\"{}\",\"profile\":\"{}\",\"container_id\":\"{}\",\"worker_kind\":\"{}\",\"worker_status\":\"{}\",\"private\":{},\"pinned\":{},\"selected\":{},\"group_id\":{},\"essential\":{},\"origin_url\":\"{}\",\"sleeping\":{},\"audible\":{},\"muted\":{},\"media_title\":\"{}\",\"media_artist\":\"{}\"}}",
                t.id,
                native_idx,
                Self::escape_json(&t.url),
                Self::escape_json(&t.title),
                Self::escape_json(&t.favicon_url),
                Self::escape_json(&t.profile_name),
                Self::escape_json(&t.container_id),
                Self::escape_json(t.worker_kind.as_deref().unwrap_or("")),
                Self::escape_json(&t.worker_status),
                t.private,
                t.pinned,
                self.selected_tab_ids.contains(&t.id),
                t.tab_group_id.unwrap_or(0),
                t.essential,
                Self::escape_json(t.pinned_origin_url.as_deref().unwrap_or("")),
                !t.is_loaded,
                t.audible,
                t.muted,
                Self::escape_json(&t.media_title),
                Self::escape_json(&t.media_artist)
            ));
        }
        let tabs_arr_str = tabs_json.join(",");
        let visible_active_idx = visible_tabs
            .iter()
            .position(|(idx, _)| *idx == self.active_idx)
            .unwrap_or(0);

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
        let paired_tab_id_val = self.split_paired_tab_id.unwrap_or(0);
        let workspaces_json = self.workspaces.iter().map(|workspace| {
            format!(
                "{{\"id\":\"{}\",\"name\":\"{}\",\"icon\":\"{}\",\"accent\":\"{}\",\"container_id\":\"{}\"}}",
                Self::escape_json(&workspace.id),
                Self::escape_json(&workspace.name),
                Self::escape_json(&workspace.icon),
                Self::escape_json(&workspace.accent),
                Self::escape_json(&workspace.container_id)
            )
        }).collect::<Vec<_>>().join(",");
        let pinned_exts_json = format!(
            "[{}]",
            self.pinned_extensions
                .iter()
                .map(|s| format!("\"{}\"", Self::escape_json(s)))
                .collect::<Vec<_>>()
                .join(",")
        );
        let downloads_json = self.downloads.iter().map(|item| format!(
            "{{\"uri\":\"{}\",\"path\":\"{}\",\"status\":\"{}\"}}",
            Self::escape_json(&item.uri),
            Self::escape_json(&item.path),
            item.status
        )).collect::<Vec<_>>().join(",");
        let send_bookmarks = self.last_tb_bookmarks_rev.get() != self.bookmarks_rev;
        let send_history = self.last_tb_history_rev.get() != self.history_rev;
        let bookmarks_prop = if send_bookmarks {
            self.last_tb_bookmarks_rev.set(self.bookmarks_rev);
            format!(",\"bookmarks\":{}", &self.bookmarks_json)
        } else {
            String::new()
        };
        let history_prop = if send_history {
            self.last_tb_history_rev.set(self.history_rev);
            format!(
                ",\"history\":{}",
                if self.history_json.trim().is_empty() {
                    "[]"
                } else {
                    &self.history_json
                }
            )
        } else {
            String::new()
        };
        let ctx_raw = if self.context_menu_prefs_json.trim().is_empty() {
            "{}"
        } else {
            &self.context_menu_prefs_json
        };
        let extensions_arr = self.extensions_state_json();
        let tb_state = format!(
            "{{\"ui_theme\":\"{}\",\"logo_style\":\"{}\",\"orientation\":\"{}\",\"show_bookmarks_bar\":{}{}{},\"downloads\":[{}],\"active_idx\":{},\"tabs\":[{}],\"active_workspace_id\":\"{}\",\"workspaces\":[{}],\"pinned_extensions\":{},\"extensions\":{},\"context_menu_prefs\":{},\"title\":\"{}\",\"active_url\":\"{}\",\"is_bookmarked\":{},\"view_mode\":\"{}\",\"paired_tab_id\":{},\"zoom_pct\":{},\"pinned\":{},\"maximized\":{}}}",
            Self::escape_json(&self.ui_theme),
            Self::escape_json(&self.logo_style),
            self.tab_orientation.as_str(),
            self.show_bookmarks_bar,
            bookmarks_prop,
            history_prop,
            downloads_json,
            visible_active_idx,
            tabs_arr_str,
            Self::escape_json(&self.active_workspace_id),
            workspaces_json,
            pinned_exts_json,
            extensions_arr,
            ctx_raw,
            Self::escape_json(&tb_title),
            Self::escape_json(active_url),
            is_bookmarked,
            self.view_mode.as_str(),
            paired_tab_id_val,
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
        if *self.last_resizer_theme.borrow() != self.ui_theme {
            *self.last_resizer_theme.borrow_mut() = self.ui_theme.clone();
            let _ = self.dock_resizer_shell.evaluate_script(&format!(
                "if (window.__tpSyncDockResizer) window.__tpSyncDockResizer({{\"ui_theme\":\"{}\"}});",
                Self::escape_json(&self.ui_theme)
            ));
            let _ = self.sidebar_resizer_shell.evaluate_script(&format!(
                "if (window.__tpSyncSidebarResizer) window.__tpSyncSidebarResizer({{\"ui_theme\":\"{}\"}});",
                Self::escape_json(&self.ui_theme)
            ));
        }

        // 1. Đồng bộ Thanh Quản Lý Tab (Dọc)
        let shell_active_idx = visible_tabs.iter().position(|(idx, _)| *idx == self.active_idx).unwrap_or(0);
        let folders_json = self
            .tab_folders
            .iter()
            .filter(|f| f.workspace_id == self.active_workspace_id)
            .map(|f| {
                format!(
                    "{{\"id\":{},\"name\":\"{}\",\"collapsed\":{}}}",
                    f.id,
                    Self::escape_json(&f.name),
                    f.collapsed
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let shell_state = format!(
            "{{\"orientation\":\"{}\",\"sidebar_collapsed\":{},\"sidebar_width\":{},\"compact_mode\":{},\"view_mode\":\"{}\",\"ai_mode\":\"{}\",\"chat_channel\":\"{}\",\"ai_working\":{},\"active_idx\":{},\"active_workspace_id\":\"{}\",\"workspaces\":[{}],\"google_user\":\"{}\",\"ui_theme\":\"{}\",\"logo_style\":\"{}\",\"adblock_enabled\":{},\"context_menu_prefs\":{},\"downloads\":[{}],\"tabs\":[{}],\"folders\":[{}]}}",
            self.tab_orientation.as_str(),
            self.sidebar_collapsed,
            self.sidebar_width,
            self.compact_mode,
            self.view_mode.as_str(),
            self.ai_mode.as_str(),
            self.chat_channel.as_str(),
            self.ai_working,
            shell_active_idx,
            Self::escape_json(&self.active_workspace_id),
            workspaces_json,
            Self::escape_json(&self.google_user),
            Self::escape_json(&self.ui_theme),
            Self::escape_json(&self.logo_style),
            self.adblock_enabled,
            ctx_raw,
            downloads_json,
            tabs_arr_str,
            folders_json
        );
        let _ = self.tab_shell.evaluate_script(&format!(
            "if (window.__tpSyncShellState) window.__tpSyncShellState({});",
            shell_state
        ));

        // 2. Đồng bộ Cụm Bong Bóng Góc Phải (AI / FB / Zalo / Custom) & Trang Chủ trên mọi Tab
        let is_split = self.view_mode != ContentViewMode::SingleTab && visible_tabs.len() > 1;
        let orbs_raw = if self.floating_orbs_json.trim().is_empty() {
            "[]"
        } else {
            &self.floating_orbs_json
        };
        let boosts_raw = if self.boosts_json.trim().is_empty() {
            "{}"
        } else {
            &self.boosts_json
        };
        for (idx, t) in self.tabs.iter().enumerate() {
            // Tối ưu độ trễ: Chỉ gửi IPC đồng bộ Bong bóng giao diện tới Tab đang hiển thị (Active hoặc Split-Visible)
            let is_tab_visible = !is_split && idx == self.active_idx;
            let is_split_visible = is_split && (idx == self.active_idx || Some(t.id) == self.split_paired_tab_id);
            if !is_tab_visible && !is_split_visible {
                continue;
            }

            let show_ring = is_split && idx == self.active_idx;
            // KHI CHIA KHUNG (SPLIT VIEW): Chỉ duy nhất 1 tab active được hiển thị bộ 3 nút Zalo/FB/AI
            // Các tab phụ trong split view sẽ bị ẩn cụm nút hoàn toàn, tránh bị nhân bản 2-3 bộ nút
            let show_orbs_in_tab = if is_split {
                idx == self.active_idx && self.always_show_floating_orbs
            } else {
                self.always_show_floating_orbs
            };
            let bubble_state = format!(
                "{{\"ai_mode\":\"{}\",\"chat_channel\":\"{}\",\"ui_theme\":\"{}\",\"working\":{},\"recording\":{},\"status_text\":\"{}\",\"last_reply\":\"{}\",\"show_split_ring\":{},\"show_unread_badges\":{},\"always_show_floating_orbs\":{},\"floating_orbs\":{},\"boosts\":{},\"context_menu_prefs\":{},\"adblock_enabled\":{},\"unread_ai\":{},\"unread_fb\":{},\"unread_zalo\":{},\"ai_finished_flash\":{},\"chime_seq\":{},\"chime_kind\":\"{}\"}}",
                self.ai_mode.as_str(),
                self.chat_channel.as_str(),
                Self::escape_json(&self.ui_theme),
                self.ai_working,
                self.ai_recording,
                Self::escape_json(&self.ai_status_text),
                Self::escape_json(&self.ai_last_reply),
                show_ring,
                self.show_unread_badges,
                show_orbs_in_tab,
                orbs_raw,
                boosts_raw,
                ctx_raw,
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
            visible_active_idx + 1,
            visible_tabs.len().max(1),
            tab_mode_lbl,
            ai_lbl
        ));
        let dur = t0.elapsed().as_secs_f64() * 1000.0;
        if dur > 5.0 {
            Self::log_perf("sync_all_ui", dur);
        }
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

    fn can_auto_unload_tab(&self, idx: usize) -> bool {
        let Some(tab) = self.tabs.get(idx) else { return false; };
        if idx == self.active_idx {
            return false;
        }
        if Some(tab.id) == self.split_paired_tab_id {
            return false;
        }
        if !tab.is_loaded {
            return false;
        }
        if tab.worker_kind.is_some() {
            return false;
        }
        if tab.essential || tab.pinned {
            return false;
        }
        if tab.audible {
            return false;
        }
        if tab.url.starts_with("tp://") {
            return false;
        }
        if self
            .tab_unload_whitelist
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .any(|domain| tab.url.contains(domain))
        {
            return false;
        }
        true
    }

    fn unload_tab_by_id(&mut self, tab_id: u64) {
        if Some(tab_id) == self.tabs.get(self.active_idx).map(|t| t.id) {
            return;
        }
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            if tab.worker_kind.is_some() || tab.url.starts_with("tp://") || !tab.is_loaded {
                return;
            }
            tab.is_loaded = false;
            tab.audible = false;
            tab.media_title.clear();
            tab.media_artist.clear();
            let _ = tab.webview.load_html(
                "<html><body style='background:#0d0f14;color:#94a3b8;font-family:sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;'><div>☾ Tab đang ngủ đông để tiết kiệm RAM</div></body></html>",
            );
            let _ = tab.webview.set_memory_usage_level(MemoryUsageLevel::Low);
            let _ = tab.webview.set_visible(false);
        }
        self.sync_all_ui();
    }

    fn check_reminders(&mut self) {
        let now_secs = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_secs(),
            Err(_) => return,
        };
        if self.tab_unload_minutes > 0 {
            let threshold = std::time::Duration::from_secs((self.tab_unload_minutes as u64) * 60);
            let now = Instant::now();
            let stale_ids: Vec<u64> = self
                .tabs
                .iter()
                .enumerate()
                .filter(|(idx, tab)| {
                    self.can_auto_unload_tab(*idx) && now.duration_since(tab.last_active_at) >= threshold
                })
                .map(|(_, tab)| tab.id)
                .collect();
            for tid in stale_ids {
                self.unload_tab_by_id(tid);
            }
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
        let scale = self.window.scale_factor();
        let logical_size = self.window.inner_size().to_logical::<u32>(scale);
        let window_width = logical_size.width.max(320);
        let window_height = logical_size.height.max(240);
        let orbs_raw = if self.floating_orbs_json.trim().is_empty() {
            "[]"
        } else {
            &self.floating_orbs_json
        };
        let workspaces_json = self
            .workspaces
            .iter()
            .map(|workspace| {
                format!(
                    "{{\"id\":\"{}\",\"name\":\"{}\",\"icon\":\"{}\",\"accent\":\"{}\",\"container_id\":\"{}\"}}",
                    Self::escape_json(&workspace.id),
                    Self::escape_json(&workspace.name),
                    Self::escape_json(&workspace.icon),
                    Self::escape_json(&workspace.accent),
                    Self::escape_json(&workspace.container_id)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let pinned_exts_json = format!(
            "[{}]",
            self.pinned_extensions
                .iter()
                .map(|s| format!("\"{}\"", Self::escape_json(s)))
                .collect::<Vec<_>>()
                .join(",")
        );
        let body = format!(
            "{{\n  \"ui_theme\": \"{}\",\n  \"active_workspace_id\": \"{}\",\n  \"workspaces\": [{}],\n  \"logo_style\": \"{}\",\n  \"startup_mode\": \"{}\",\n  \"background_mode\": {},\n  \"startup_boost\": {},\n  \"floating_omnibox\": {},\n  \"auto_hide_titlebar\": {},\n  \"sidebar_width\": {},\n  \"tab_orientation\": \"{}\",\n  \"sidebar_collapsed\": {},\n  \"compact_mode\": {},\n  \"tab_unload_minutes\": {},\n  \"tab_unload_whitelist\": \"{}\",\n  \"pinned_extensions\": {},\n  \"translate_target_lang\": \"{}\",\n  \"translate_auto_prompt\": {},\n  \"translate_never_domains\": \"{}\",\n  \"glance_trigger\": \"{}\",\n  \"glance_external_from_pinned\": {},\n  \"default_search_engine\": \"{}\",\n  \"auto_pip_on_switch\": {},\n  \"view_mode\": \"{}\",\n  \"ai_mode\": \"{}\",\n  \"ai_dock_width\": {},\n  \"window_width\": {},\n  \"window_height\": {},\n  \"show_unread_badges\": {},\n  \"always_show_floating_orbs\": {},\n  \"floating_orbs\": {},\n  \"adblock_enabled\": {},\n  \"permission_camera\": \"{}\",\n  \"permission_microphone\": \"{}\",\n  \"permission_notifications\": \"{}\",\n  \"permission_clipboard\": \"{}\",\n  \"ai_provider\": \"{}\",\n  \"ai_api_key\": \"{}\",\n  \"ai_model\": \"{}\",\n  \"ai_base_url\": \"{}\",\n  \"mcp_auth_token\": \"{}\",\n  \"auto_minimize_on_task\": {},\n  \"google_user\": \"{}\",\n  \"custom_shortcuts\": {},\n  \"pancake_api_key\": \"{}\",\n  \"pancake_shop_id\": \"{}\",\n  \"pancake_shop_name\": \"{}\",\n  \"tiktok_shop_name\": \"{}\",\n  \"bookmarks\": {},\n  \"boosts\": {},\n  \"context_menu_prefs\": {}\n}}",
            Self::escape_json(&self.ui_theme),
            Self::escape_json(&self.active_workspace_id),
            workspaces_json,
            Self::escape_json(&self.logo_style),
            Self::escape_json(&self.startup_mode),
            self.background_mode,
            self.startup_boost,
            self.floating_omnibox,
            self.compact_titlebar,
            self.sidebar_width,
            self.tab_orientation.as_str(),
            self.sidebar_collapsed,
            self.compact_mode,
            self.tab_unload_minutes,
            Self::escape_json(&self.tab_unload_whitelist),
            pinned_exts_json,
            Self::escape_json(&self.translate_target_lang),
            self.translate_auto_prompt,
            Self::escape_json(&self.translate_never_domains),
            Self::escape_json(&self.glance_trigger),
            self.glance_external_from_pinned,
            Self::escape_json(&self.default_search_engine),
            self.auto_pip_on_switch,
            self.view_mode.as_str(),
            self.ai_mode.as_str(),
            self.ai_dock_width,
            window_width,
            window_height,
            self.show_unread_badges,
            self.always_show_floating_orbs,
            orbs_raw,
            self.adblock_enabled,
            Self::escape_json(&self.permission_camera),
            Self::escape_json(&self.permission_microphone),
            Self::escape_json(&self.permission_notifications),
            Self::escape_json(&self.permission_clipboard),
            Self::escape_json(&self.ai_provider),
            Self::escape_json(&self.ai_api_key),
            Self::escape_json(&self.ai_model),
            Self::escape_json(&self.ai_base_url),
            Self::escape_json(&self.mcp_auth_token),
            self.auto_minimize_on_task,
            Self::escape_json(&self.google_user),
            self.custom_shortcuts_json(),
            Self::escape_json(&self.pancake_api_key),
            Self::escape_json(&self.pancake_shop_id),
            Self::escape_json(&self.pancake_shop_name),
            Self::escape_json(&self.tiktok_shop_name),
            if self.bookmarks_json.trim().is_empty() {
                "[]"
            } else {
                &self.bookmarks_json
            },
            if self.boosts_json.trim().is_empty() {
                "{}"
            } else {
                &self.boosts_json
            },
            if self.context_menu_prefs_json.trim().is_empty() {
                "{}"
            } else {
                &self.context_menu_prefs_json
            }
        );
        let body = config_payload_with_selections(&body, &self.selected_shops);
        let _ = write_config_snapshot(&cfg_path, &body);
    }

    fn load_config(&mut self) {
        let cfg_path = self.exe_dir.join("config.json");
        let Ok(raw) = fs::read_to_string(cfg_path) else {
            return;
        };
        self.selected_shops = parse_shop_selections(&raw);
        self.active_workspace_id = Self::extract_field_str(&raw, "active_workspace_id")
            .unwrap_or_else(|| DEFAULT_WORKSPACE_ID.to_string());
        if let Ok(val) = parse_json(&raw) {
            if let Some(items) = val.get("workspaces").and_then(|v| v.as_arr()) {
                let loaded: Vec<Workspace> = items.iter().filter_map(|item| {
                    Some(Workspace {
                        id: item.get("id")?.as_str()?.to_string(),
                        name: item.get("name")?.as_str()?.to_string(),
                        icon: item.get("icon").and_then(JsonVal::as_str).unwrap_or("✦").to_string(),
                        accent: item.get("accent").and_then(JsonVal::as_str).unwrap_or("").to_string(),
                        container_id: item.get("container_id").and_then(JsonVal::as_str).unwrap_or("default").to_string(),
                    })
                }).collect();
                if valid_workspace_list(&loaded) { self.workspaces = loaded; }
            }
        }
        if !self.workspaces.iter().any(|w| w.id == self.active_workspace_id) {
            self.active_workspace_id = DEFAULT_WORKSPACE_ID.to_string();
        }
        if let Some(v) = Self::extract_field_str(&raw, "tab_orientation") {
            self.tab_orientation = TabOrientation::from_str(&v);
        }
        if let Some(b) = Self::extract_bool(&raw, "sidebar_collapsed") {
            self.sidebar_collapsed = b;
        }
        if let Some(b) = Self::extract_bool(&raw, "compact_mode") {
            self.compact_mode = b;
            COMPACT_MODE_ENABLED.store(b, Ordering::Relaxed);
        }
        if let Some(v) = Self::extract_field_str(&raw, "tab_unload_minutes") {
            if let Ok(n) = v.trim().parse::<u32>() {
                self.tab_unload_minutes = n.min(600);
            }
        }
        if let Some(v) = Self::extract_field_str(&raw, "tab_unload_whitelist") {
            self.tab_unload_whitelist = v;
        }
        if let Some(v) = Self::extract_field_str(&raw, "pancake_api_key") {
            self.pancake_api_key = v.clone();
            self.pancake_connected = !v.trim().is_empty();
        }
        if let Some(v) = Self::extract_field_str(&raw, "pancake_shop_id") {
            self.pancake_shop_id = v;
        }
        if let Some(v) = Self::extract_field_str(&raw, "pancake_shop_name") {
            self.pancake_shop_name = v;
        }
        if let Some(v) = Self::extract_field_str(&raw, "tiktok_shop_name") {
            self.tiktok_shop_name = v;
        }
        if let Ok(val) = parse_json(&raw) {
            if let Some(exts) = val.get("pinned_extensions").and_then(|v| v.as_arr()) {
                let loaded: Vec<String> = exts.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
                if !loaded.is_empty() {
                    self.pinned_extensions = loaded;
                }
            }
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
        if let Some(v) = Self::extract_field_str(&raw, "translate_target_lang") {
            self.translate_target_lang = v;
        }
        if let Some(b) = Self::extract_bool(&raw, "translate_auto_prompt") {
            self.translate_auto_prompt = b;
        }
        if let Some(v) = Self::extract_field_str(&raw, "translate_never_domains") {
            self.translate_never_domains = v;
        }
        if let Some(v) = Self::extract_field_str(&raw, "glance_trigger") {
            self.glance_trigger = v;
        }
        if let Some(b) = Self::extract_bool(&raw, "glance_external_from_pinned") {
            self.glance_external_from_pinned = b;
        }
        if let Some(v) = Self::extract_field_str(&raw, "default_search_engine") {
            self.default_search_engine = v;
        }
        if let Some(b) = Self::extract_bool(&raw, "auto_pip_on_switch") {
            self.auto_pip_on_switch = b;
        }
        if let Some(b) = Self::extract_bool(&raw, "adblock_enabled") {
            self.adblock_enabled = b;
            ADBLOCK_ENABLED.store(b, Ordering::Relaxed);
        }
        if let Some(v) = Self::extract_field_str(&raw, "permission_camera") {
            self.permission_camera = Self::normalize_permission_policy(&v).to_string();
        }
        if let Some(v) = Self::extract_field_str(&raw, "permission_microphone") {
            self.permission_microphone = Self::normalize_permission_policy(&v).to_string();
        }
        if let Some(v) = Self::extract_field_str(&raw, "permission_notifications") {
            self.permission_notifications = Self::normalize_permission_policy(&v).to_string();
        }
        if let Some(v) = Self::extract_field_str(&raw, "permission_clipboard") {
            self.permission_clipboard = Self::normalize_permission_policy(&v).to_string();
        }
        self.sync_permission_atomics();
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
        if let Some(v) = Self::extract_field_str(&raw, "mcp_auth_token") {
            if !v.trim().is_empty() {
                self.mcp_auth_token = v.trim().to_string();
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
        if let Some(b) = Self::extract_bool(&raw, "background_mode") {
            self.background_mode = b;
        }
        if let Some(b) = Self::extract_bool(&raw, "startup_boost") {
            self.startup_boost = b;
        }
        if let Ok(parsed) = parse_json(&raw) {
            if let Some(shortcuts) = parsed.get("custom_shortcuts") {
                self.custom_shortcuts = Self::parse_custom_shortcuts(shortcuts);
            }
            if let Some(bm) = parsed.get("bookmarks") {
                if let Some(norm) = Self::normalize_bookmarks_json(&bm.to_json()) {
                    self.bookmarks_json = norm;
                }
            }
            if let Some(boosts) = parsed.get("boosts") {
                if let Some(norm) = Self::normalize_boosts_json(&boosts.to_json()) {
                    self.boosts_json = norm;
                }
            }
            if let Some(ctx) = parsed.get("context_menu_prefs") {
                self.context_menu_prefs_json = Self::normalize_context_menu_prefs(&ctx.to_json());
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
        self.history_rev = self.history_rev.wrapping_add(1);
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
        if let Ok(s) = fs::read_to_string(dir.join("shopee_health_latest.json")) {
            if !s.trim().is_empty() {
                self.shopee_health_json = s;
            }
        }
        if let Ok(s) = fs::read_to_string(dir.join("tiktok_latest.json")) {
            if !s.trim().is_empty() {
                if let Some(tagged) = mark_tiktok_cache_restored(&s) {
                    self.tiktok_json = tagged;
                }
            }
        }
        if let Ok(s) = fs::read_to_string(dir.join("tiktok_health_latest.json")) {
            if !s.trim().is_empty() {
                self.tiktok_health_json = s;
            }
        }
        if let Ok(s) = fs::read_to_string(dir.join("shopee_shop_index_latest.json")) {
            if !s.trim().is_empty() {
                self.shopee_shop_index_json = s;
            }
        }
        if let Ok(s) = fs::read_to_string(dir.join("shopee_shop_orders_latest.json")) {
            if !s.trim().is_empty() {
                self.shopee_shop_orders_json = s;
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

    fn log_ecom(&mut self, level: &str, tag: &str, msg: &str) {
        #[cfg(target_os = "windows")]
        let ts = {
            let mut st = SystemTimeWin32 {
                w_year: 0, w_month: 0, w_day_of_week: 0, w_day: 0,
                w_hour: 0, w_minute: 0, w_second: 0, w_milliseconds: 0,
            };
            unsafe { GetLocalTime(&mut st) };
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                st.w_year, st.w_month, st.w_day, st.w_hour, st.w_minute, st.w_second
            )
        };
        #[cfg(not(target_os = "windows"))]
        let ts = {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
            format!("{}", now)
        };

        let log_line = format!("[{}] [{}] [{}] {}\n", ts, level, tag, msg);

        let logs_dir = self.exe_dir.join("user_data").join("logs");
        let log_line_bg = log_line.clone();
        std::thread::spawn(move || {
            let _ = fs::create_dir_all(&logs_dir);
            let log_file = logs_dir.join("ecom_scanner.log");
            if let Ok(meta) = fs::metadata(&log_file) {
                if meta.len() > 3 * 1024 * 1024 {
                    let old_file = logs_dir.join("ecom_scanner.log.old");
                    let _ = fs::rename(&log_file, &old_file);
                }
            }
            use std::io::Write;
            if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&log_file) {
                let _ = f.write_all(log_line_bg.as_bytes());
            }
        });

        let formatted_log = format!("[{}] [{}] [{}] {}", ts, level, tag, msg);
        self.push_ecom_log(&formatted_log);

        let js = format!(
            "if (typeof window.onScanLogAppend === 'function') window.onScanLogAppend({{\"ts\":\"{}\",\"level\":\"{}\",\"tag\":\"{}\",\"msg\":\"{}\"}});",
            ts, level, tag, Self::escape_json(msg)
        );
        for t in &self.tabs {
            if t.url == "tp://workspace" {
                let _ = t.webview.evaluate_script(&js);
            }
        }
    }

    fn push_ecom_log(&mut self, msg: &str) {
        self.ecom_logs.push(msg.to_string());
        if self.ecom_logs.len() > 150 {
            let drop_n = self.ecom_logs.len() - 150;
            self.ecom_logs.drain(0..drop_n);
        }
    }

    fn sync_workspace_progress(&self, active: bool, step: usize, total: usize, percent: u32, label: &str, status: &str) {
        let js = format!(
            "if (typeof window.onWorkspaceProgress === 'function') window.onWorkspaceProgress({{\"active\":{},\"step\":{},\"total\":{},\"percent\":{},\"label\":\"{}\",\"status\":\"{}\"}});",
            active, step, total, percent, Self::escape_json(label), Self::escape_json(status)
        );
        for t in &self.tabs {
            if t.url == "tp://workspace" {
                let _ = t.webview.evaluate_script(&js);
            }
        }
    }

    fn sync_glance_view(&self) {
        if let Some(hdr) = &self.glance_header_shell {
            let js = format!(
                "window.dispatchEvent(new CustomEvent('tp_config_changed', {{detail:{{\"ui_theme\":\"{}\",\"title\":\"{}\"}}}}));",
                Self::escape_json(&self.ui_theme),
                Self::escape_json(&self.glance_title)
            );
            let _ = hdr.evaluate_script(&js);
        }
    }

    fn should_defer_workspace_sync(&self) -> bool {
        self.shopee_shop_scan_in_flight()
    }

    fn sync_workspace_view(&self) {
        if self.should_defer_workspace_sync() {
            return;
        }
        self.sync_workspace_view_now();
    }

    fn sync_workspace_view_now(&self) {
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
        let sp_health = if self.shopee_health_json.trim().is_empty() {
            "null"
        } else {
            &self.shopee_health_json
        };
        let tt = if self.tiktok_json.trim().is_empty() {
            "null"
        } else {
            &self.tiktok_json
        };
        let tt_health = if self.tiktok_health_json.trim().is_empty() {
            "null"
        } else {
            &self.tiktok_health_json
        };
        let sp_shops = if self.shopee_shop_index_json.trim().is_empty() {
            "null"
        } else {
            &self.shopee_shop_index_json
        };
        let sp_shop_orders = if self.shopee_shop_orders_json.trim().is_empty() { "null" } else { &self.shopee_shop_orders_json };

        let payload = format!(
            "{{\"ui_theme\":\"{}\",\"logo_style\":\"{}\",\"auto_scan\":{},\"status_text\":\"{}\",\"worker_tabs\":{},\"shopee_shop_discovery\":{},\"selected_shops\":{},\"shopee_orders\":{},\"shopee_pas\":{},\"shopee_health\":{},\"tiktok\":{},\"tiktok_health\":{},\"shopee_shop_index\":{},\"shopee_shop_orders\":{},\"logs\":{}}}",
            Self::escape_json(&self.ui_theme),
            Self::escape_json(&self.logo_style),
            self.ecom_auto_scan,
            Self::escape_json(&self.ecom_status_text),
            JsonVal::Arr(workers).to_json(),
            workspace_discovery_payload(&self.shopee_discovery_json),
            serialize_shop_selections(&self.selected_shops),
            sp_ord,
            sp_pas,
            sp_health,
            tt,
            tt_health,
            sp_shops,
            sp_shop_orders,
            JsonVal::Arr(logs_arr).to_json()
        );
        let js = format!("if (window.__tpSyncWorkspace) window.__tpSyncWorkspace({});", payload);
        for t in &self.tabs {
            if t.url == "tp://workspace" {
                let _ = t.webview.evaluate_script(&js);
            }
        }
        self.sync_workspace_command_center();
    }

    fn current_date_str(&self) -> String {
        #[cfg(target_os = "windows")]
        {
            let mut st = SystemTimeWin32 {
                w_year: 0, w_month: 0, w_day_of_week: 0, w_day: 0,
                w_hour: 0, w_minute: 0, w_second: 0, w_milliseconds: 0,
            };
            unsafe { GetLocalTime(&mut st) };
            format!("{:04}-{:02}-{:02}", st.w_year, st.w_month, st.w_day)
        }
        #[cfg(not(target_os = "windows"))]
        {
            "2026-11-20".to_string()
        }
    }

    fn format_vnd_amount(amount: f64) -> String {
        let mut num = amount.abs().round() as i64;
        if num == 0 {
            return "0 ₫".to_string();
        }
        let mut parts = Vec::new();
        while num > 0 {
            let rem = num % 1000;
            num /= 1000;
            if num > 0 {
                parts.push(format!("{:03}", rem));
            } else {
                parts.push(format!("{}", rem));
            }
        }
        parts.reverse();
        let prefix = if amount < 0.0 { "-" } else { "" };
        format!("{}{}{}", prefix, parts.join("."), " ₫")
    }

    fn get_shop_display_name(&self, shop_id: &str) -> String {
        if shop_id.starts_with("tt_") || shop_id == "tiktok" {
            if !self.tiktok_shop_name.is_empty() {
                return self.tiktok_shop_name.clone();
            }
            return "TikTok Shop VN".to_string();
        }
        if shop_id.starts_with("pk_") || shop_id == "pancake" {
            if !self.pancake_shop_name.is_empty() {
                return format!("Pancake {}", self.pancake_shop_name);
            }
            return "Pancake POS".to_string();
        }
        if let Ok(disc) = parse_json(&self.shopee_discovery_json) {
            if let Some(shops) = disc.get("shops").and_then(JsonVal::as_arr) {
                for s in shops {
                    if s.get("shop_id").and_then(JsonVal::as_str) == Some(shop_id) {
                        if let Some(name) = s.get("display_name").or_else(|| s.get("name")).and_then(JsonVal::as_str) {
                            return name.to_string();
                        }
                    }
                }
            }
        }
        shop_id.to_string()
    }

    fn start_multi_shop_queue(&mut self, shop_ids: Vec<String>) {
        let mut target_shops = shop_ids;
        if target_shops.is_empty() {
            target_shops = self.selected_shops.iter().map(|s| s.shop_id.clone()).collect();
        }
        if target_shops.is_empty() {
            if let Ok(disc) = parse_json(&self.shopee_discovery_json) {
                if let Some(shops) = disc.get("shops").and_then(JsonVal::as_arr) {
                    for s in shops {
                        if let Some(id) = s.get("shop_id").and_then(JsonVal::as_str) {
                            if !id.is_empty() { target_shops.push(id.to_string()); }
                        }
                    }
                }
            }
        }
        if target_shops.is_empty() {
            target_shops = vec!["sp_01".to_string(), "tt_01".to_string()];
        }

        self.multi_shop_queue = target_shops;
        self.multi_shop_idx = 0;
        self.multi_shop_step = String::new();
        self.multi_shop_data.clear();
        self.multi_shop_stale_shops.clear();
        self.multi_shop_auth_pending = false;

        self.tiktok_json.clear();
        self.tiktok_ads_json.clear();
        self.tiktok_products_json.clear();
        self.tiktok_health_json.clear();
        self.shopee_orders_json.clear();
        self.shopee_pas_json.clear();
        self.shopee_health_json.clear();

        self.log_ecom("INFO", "QUEUE", &format!("Bắt đầu hàng đợi đa shop với {} shop", self.multi_shop_queue.len()));
        self.sync_workspace_progress(true, 5, 100, 5, "Khởi tạo hàng đợi quét đa shop...", "running");

        let is_pancake_selected = self.multi_shop_queue.iter().any(|id| id.starts_with("pk_") || id == "pancake");
        if self.pancake_connected && !self.pancake_api_key.trim().is_empty() && is_pancake_selected {
            let key = self.pancake_api_key.clone();
            let s_id = self.pancake_shop_id.clone();
            let proxy = self.proxy.clone();
            std::thread::spawn(move || {
                let url = if !s_id.is_empty() {
                    format!("https://pos.pages.fm/api/v1/shops/{}/orders?api_key={}&limit=50", s_id, key)
                } else {
                    format!("https://pos.pages.fm/api/v1/shops/me/orders?api_key={}&limit=50", key)
                };
                let mut cmd = std::process::Command::new("curl.exe");
                cmd.args(["-s", "--max-time", "8", &url]);
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    cmd.creation_flags(0x08000000);
                }
                let mut total_money = 0.0;
                let mut order_count = 0;
                if let Ok(output) = cmd.output() {
                    let body = String::from_utf8_lossy(&output.stdout);
                    if let Ok(val) = parse_json(&body) {
                        if let Some(orders) = val.get("data").and_then(JsonVal::as_arr) {
                            order_count = orders.len();
                            for o in orders {
                                let money = o.get("money_to_collect")
                                    .and_then(JsonVal::as_f64)
                                    .or_else(|| o.get("total_price").and_then(JsonVal::as_f64))
                                    .unwrap_or(0.0);
                                total_money += money;
                            }
                        }
                    }
                }
                let rev_million = (total_money / 1_000_000.0 * 10.0).round() / 10.0;
                let _ = proxy.send_event(AppCmd::PancakeScanResult { rev: rev_million, orders: order_count });
            });
        }

        self.start_next_shop_in_multi_queue();
    }

    fn start_next_shop_in_multi_queue(&mut self) {
        if self.multi_shop_idx >= self.multi_shop_queue.len() {
            self.finish_multi_shop_queue();
            return;
        }
        let s_id = self.multi_shop_queue[self.multi_shop_idx].clone();
        let name = self.get_shop_display_name(&s_id);
        let total = self.multi_shop_queue.len();
        let curr = self.multi_shop_idx + 1;

        if s_id.starts_with("pk_") || s_id == "pancake" {
            self.multi_shop_idx += 1;
            self.start_next_shop_in_multi_queue();
            return;
        }

        if s_id.starts_with("tt_") || s_id == "tiktok" {
            self.multi_shop_step = "tiktok_homepage".to_string();
            let pct = ((curr - 1) * 100 + 10) / total;
            self.sync_workspace_progress(true, pct as usize, 100, pct as u32, &format!("[Shop {}/{} - {}] Bước 1/4: Đang quét Doanh thu & Đơn hàng...", curr, total, name), "running");
            self.log_ecom("INFO", "TIKTOK", &format!("Bắt đầu quét shop [{}/{}] {} - Bước 1/4: Doanh thu & Đơn hàng", curr, total, name));

            if let Some(old_id) = self.multi_shop_tab_id.take() {
                self.execute(AppCmd::CloseTabById(old_id));
            }
            let url = "https://seller-vn.tiktok.com/homepage";
            if let Some(tid) = self.add_worker_tab(url, "bot", Some(&format!("TikTok: {}", name))) {
                self.multi_shop_tab_id = Some(tid);
                self.trigger_worker_scan_by_id(tid, url);
            }
            return;
        }

        self.multi_shop_step = "opening_shop".to_string();
        let pct = ((curr - 1) * 100 + 10) / total;
        self.sync_workspace_progress(true, pct as usize, 100, pct as u32, &format!("[Shop {}/{} - {}] Bước 1/3: Đang mở /portal/shop chọn shop...", curr, total, name), "running");
        self.log_ecom("INFO", "SHOPEE", &format!("Bắt đầu quét shop [{}/{}] {} ({}) - Mở /portal/shop để chọn shop", curr, total, name, s_id));

        let index_url = "https://banhang.shopee.vn/portal/shop";
        let reuse_tid = self.multi_shop_tab_id.filter(|tid| {
            self.tabs.iter().any(|t| t.id == *tid && t.url.contains("banhang.shopee.vn"))
        });
        if let Some(tid) = reuse_tid {
            if let Some(t) = self.tabs.iter_mut().find(|t| t.id == tid) {
                t.title = format!("Shopee: {}", name);
                t.worker_status = "Đang chuyển shop".to_string();
                let _ = t.webview.load_url(index_url);
            }
            self.shopee_shop_scan_tab_id = Some(tid);
            self.shopee_shop_scan_phase = "opening_shop".to_string();
            self.shopee_shop_scan_current = Some((s_id.clone(), name.clone()));
        } else {
            if let Some(old_id) = self.multi_shop_tab_id.take() {
                self.execute(AppCmd::CloseTabById(old_id));
            }
            if let Some(tid) = self.add_worker_tab(index_url, "bot", Some(&format!("Shopee: {}", name))) {
                self.multi_shop_tab_id = Some(tid);
                self.shopee_shop_scan_tab_id = Some(tid);
                self.shopee_shop_scan_phase = "opening_shop".to_string();
                self.shopee_shop_scan_current = Some((s_id.clone(), name.clone()));
            }
        }
    }

    fn finish_multi_shop_queue(&mut self) {
        if let Some(tid) = self.multi_shop_tab_id.take() {
            self.execute(AppCmd::CloseTabById(tid));
        }
        self.multi_shop_step = String::new();
        self.multi_shop_auth_pending = false;

        let date_str = self.current_date_str();
        let snap_daily = format!("snap_{}", date_str);
        self.save_workspace_snapshot(&snap_daily);
        self.save_workspace_snapshot("snap_latest");

        self.sync_workspace_progress(true, 100, 100, 100, "Đã hoàn tất quét tất cả các gian hàng!", "done");
        self.sync_workspace_command_center_with_key("snap_latest");
        self.log_ecom("SUCCESS", "QUEUE", "Đã hoàn thành toàn bộ hàng đợi quét đa shop.");
    }

    fn save_workspace_snapshot(&mut self, snapshot_key: &str) {
        let snapshots_dir = self.exe_dir.join("user_data").join("snapshots");
        let _ = fs::create_dir_all(&snapshots_dir);
        let now_str = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| format!("Phiên #{}", d.as_secs()))
            .unwrap_or_else(|_| "Realtime".to_string());

        let pk_rev = if self.pancake_connected {
            if self.pancake_rev > 0.0 { self.pancake_rev } else { 0.0 }
        } else {
            0.0
        };
        let pk_orders = if self.pancake_connected {
            self.pancake_orders
        } else {
            0
        };
        let pk_label = if self.pancake_connected {
            if !self.pancake_shop_name.is_empty() {
                format!("Pancake POS ({})", self.pancake_shop_name)
            } else {
                "Pancake POS".to_string()
            }
        } else {
            "Pancake POS (Chưa kết nối)".to_string()
        };

        let mut shopee_rev_vnd = 0.0;
        let mut shopee_orders = 0usize;
        let mut shopee_to_ship = 0usize;
        let mut shopee_express = 0usize;
        let mut price_anomalies_list = Vec::new();
        let mut bad_reviews_list = Vec::new();
        let mut live_sessions_list = Vec::new();
        let mut promos_list = Vec::new();
        let mut shop_health_list = Vec::new();
        let mut shopee_ads_cost_vnd = 0.0;
        let mut shopee_ads_sales_vnd = 0.0;
        let mut ads_alerts_list = Vec::new();

        let mut shopee_sources: Vec<(String, String, String, String)> = Vec::new();
        if !self.multi_shop_data.is_empty() {
            for (s_id, (ord, pas, health)) in &self.multi_shop_data {
                if !s_id.starts_with("tt_") && s_id != "tiktok" && !s_id.starts_with("pk_") && s_id != "pancake" {
                    shopee_sources.push((s_id.clone(), ord.clone(), pas.clone(), health.clone()));
                }
            }
        }
        if shopee_sources.is_empty() && (!self.shopee_orders_json.is_empty() || !self.shopee_pas_json.is_empty()) {
            shopee_sources.push(("sp_01".to_string(), self.shopee_orders_json.clone(), self.shopee_pas_json.clone(), self.shopee_health_json.clone()));
        }

        for (s_id, ord_str, pas_str, health_str) in &shopee_sources {
            let shop_name = self.get_shop_display_name(s_id);
            if let Ok(ord_val) = parse_json(ord_str) {
                if let Some(today) = ord_val.get("today_stats") {
                    shopee_rev_vnd += today.get("revenue_vnd").and_then(JsonVal::as_f64).unwrap_or(0.0);
                    shopee_orders += today.get("order_count").and_then(JsonVal::as_u64).unwrap_or(0) as usize;
                } else if let Some(orders_arr) = ord_val.get("orders").and_then(JsonVal::as_arr) {
                    shopee_orders += orders_arr.len();
                    for o in orders_arr {
                        shopee_rev_vnd += o.get("order_total").and_then(JsonVal::as_f64).unwrap_or(0.0);
                    }
                }
                shopee_to_ship += ord_val.get("to_ship").and_then(JsonVal::as_u64).unwrap_or(0) as usize;
                shopee_express += ord_val.get("express_orders").and_then(JsonVal::as_u64).unwrap_or(0) as usize;

                if let Some(anoms) = ord_val.get("anomalies").and_then(JsonVal::as_arr) {
                    for a in anoms {
                        let order_sn = a.get("order_sn").and_then(JsonVal::as_str).unwrap_or("");
                        let item_name = a.get("item_name").and_then(JsonVal::as_str).unwrap_or("");
                        let eff_price = a.get("effective_price").and_then(JsonVal::as_f64).unwrap_or(0.0);
                        let med_price = a.get("median_price").and_then(JsonVal::as_f64).unwrap_or(0.0);
                        let reason = a.get("reasons").and_then(JsonVal::as_arr)
                            .and_then(|r| r.first())
                            .and_then(JsonVal::as_str)
                            .unwrap_or("Lệch giá bất thường");
                        let url = a.get("url").and_then(JsonVal::as_str).unwrap_or("https://banhang.shopee.vn/portal/sale/order");
                        price_anomalies_list.push(format!(
                            "{{\"shopId\":\"{}\",\"orderId\":\"{}\",\"platform\":\"shopee\",\"shop\":\"{}\",\"sku\":\"{}\",\"soldPrice\":\"{} đ\",\"basePrice\":\"{} đ\",\"reason\":\"{}\",\"url\":\"{}\"}}",
                            Self::escape_json(s_id), Self::escape_json(order_sn), Self::escape_json(&shop_name),
                            Self::escape_json(item_name), eff_price as i64, med_price as i64, Self::escape_json(reason), Self::escape_json(url)
                        ));
                    }
                }

                if let Some(revs) = ord_val.get("bad_reviews").and_then(JsonVal::as_arr) {
                    for r in revs {
                        let stars = r.get("rating_star").and_then(JsonVal::as_u64).unwrap_or(1);
                        let buyer = r.get("buyer_name").and_then(JsonVal::as_str).unwrap_or("Khách hàng");
                        let comment = r.get("comment").and_then(JsonVal::as_str).unwrap_or("Đánh giá xấu");
                        let order_sn = r.get("order_sn").and_then(JsonVal::as_str).unwrap_or("");
                        let url = r.get("url").and_then(JsonVal::as_str).unwrap_or("https://banhang.shopee.vn/portal/settings/shop/rating");
                        bad_reviews_list.push(format!(
                            "{{\"shopId\":\"{}\",\"platform\":\"shopee\",\"shop\":\"{}\",\"stars\":{},\"customer\":\"{}\",\"content\":\"{}\",\"orderId\":\"{}\",\"url\":\"{}\"}}",
                            Self::escape_json(s_id), Self::escape_json(&shop_name), stars, Self::escape_json(buyer),
                            Self::escape_json(comment), Self::escape_json(order_sn), Self::escape_json(url)
                        ));
                    }
                }

                if let Some(live) = ord_val.get("live") {
                    if live.get("is_live").and_then(JsonVal::as_bool) == Some(true) {
                        let title = live.get("title").and_then(JsonVal::as_str).unwrap_or("Phiên Live Shopee");
                        let rev = live.get("revenue").and_then(JsonVal::as_f64).unwrap_or(0.0) / 1_000_000.0;
                        let viewers = live.get("viewers").and_then(JsonVal::as_u64).unwrap_or(0);
                        let orders = live.get("orders").and_then(JsonVal::as_u64).unwrap_or(0);
                        live_sessions_list.push(format!(
                            "{{\"shopId\":\"{}\",\"platform\":\"shopee\",\"shop\":\"{}\",\"title\":\"{}\",\"status\":\"LIVE\",\"rev\":{:.1},\"sharePct\":\"--\",\"viewers\":{},\"orders\":{},\"url\":\"https://banhang.shopee.vn/portal/live\"}}",
                            Self::escape_json(s_id), Self::escape_json(&shop_name), Self::escape_json(title), rev, viewers, orders
                        ));
                    }
                }

                if let Some(fs_arr) = ord_val.get("flash_sales").and_then(JsonVal::as_arr) {
                    for f in fs_arr {
                        let raw_name = f.get("name").or_else(|| f.get("flash_sale_id")).and_then(JsonVal::as_str).unwrap_or("Flash Sale");
                        let clean_name = if raw_name.chars().all(|c| c.is_ascii_digit()) {
                            format!("Shop Flash Sale #{}", raw_name)
                        } else {
                            raw_name.to_string()
                        };
                        let items = f.get("item_count").and_then(JsonVal::as_u64).unwrap_or(0);
                        let url = f.get("url").and_then(JsonVal::as_str).unwrap_or("https://banhang.shopee.vn/portal/marketing/shop-flash-sale");
                        promos_list.push(format!(
                            "{{\"shopId\":\"{}\",\"platform\":\"shopee\",\"shop\":\"{}\",\"type\":\"Shop Flash Sale\",\"name\":\"{}\",\"usage\":\"{} sản phẩm\",\"url\":\"{}\"}}",
                            Self::escape_json(s_id), Self::escape_json(&shop_name), Self::escape_json(&clean_name), items, Self::escape_json(url)
                        ));
                    }
                }
            }

            if let Ok(pas_val) = parse_json(pas_str) {
                let cost = pas_val.get("cost_ads").and_then(JsonVal::as_f64).unwrap_or(0.0);
                let sales = pas_val.get("sales_ads").and_then(JsonVal::as_f64).unwrap_or(0.0);
                shopee_ads_cost_vnd += cost;
                shopee_ads_sales_vnd += sales;
                if let Some(camps) = pas_val.get("campaigns").and_then(JsonVal::as_arr) {
                    for c in camps {
                        let c_name = c.get("campaign").and_then(JsonVal::as_str).unwrap_or("Chiến dịch");
                        let budget = c.get("dailyBudgetVnd").and_then(JsonVal::as_f64).unwrap_or(0.0);
                        let spent = c.get("spentVnd").and_then(JsonVal::as_f64).unwrap_or(0.0);
                        let cir_num = c.get("cirNumeric").and_then(JsonVal::as_f64).unwrap_or(0.0);
                        let cir_str = c.get("cir").and_then(JsonVal::as_str).unwrap_or("0%");
                        let status = c.get("status").and_then(JsonVal::as_str).unwrap_or("ĐANG CHẠY");
                        let badge = c.get("badge").and_then(JsonVal::as_str).unwrap_or("pill-success");
                        let url = c.get("url").and_then(JsonVal::as_str).unwrap_or("https://banhang.shopee.vn/portal/marketing/pas/index");
                        ads_alerts_list.push(format!(
                            "{{\"shopId\":\"{}\",\"campaignId\":\"\",\"platform\":\"shopee\",\"shop\":\"{}\",\"campaign\":\"{}\",\"dailyBudgetVnd\":{},\"spentVnd\":{},\"budgetPct\":\"--\",\"cirNumeric\":{:.1},\"cir\":\"{}\",\"status\":\"{}\",\"badge\":\"{}\",\"url\":\"{}\"}}",
                            Self::escape_json(s_id), Self::escape_json(&shop_name), Self::escape_json(c_name), budget as i64, spent as i64, cir_num, Self::escape_json(cir_str), Self::escape_json(status), Self::escape_json(badge), Self::escape_json(url)
                        ));
                    }
                }
            }

            if let Ok(health_val) = parse_json(health_str) {
                let points = health_val.get("penalty_points").and_then(JsonVal::as_u64).unwrap_or(0);
                let note = health_val.get("note").and_then(JsonVal::as_str).unwrap_or("Vận hành bình thường");
                shop_health_list.push(format!(
                    "{{\"shopId\":\"{}\",\"platform\":\"shopee\",\"shop\":\"{}\",\"points\":\"{}/12 Sao Quả Tạ\",\"status\":\"healthy\",\"note\":\"{}\",\"expressOnTime\":\"99%\",\"url\":\"https://banhang.shopee.vn/portal/settings/shop/health\"}}",
                    Self::escape_json(s_id), Self::escape_json(&shop_name), points, Self::escape_json(note)
                ));
            }
        }

        let mut tiktok_rev_vnd = 0.0;
        let mut tiktok_orders = 0usize;
        let mut tiktok_cost_vnd = 0.0;
        let mut _tiktok_roi = 0.0;
        if let Ok(tt_val) = parse_json(&self.tiktok_json) {
            tiktok_rev_vnd = tt_val.get("gmv").or_else(|| tt_val.get("revenue_vnd")).and_then(JsonVal::as_f64).unwrap_or(0.0);
            tiktok_orders = tt_val.get("orders").or_else(|| tt_val.get("order_count")).and_then(JsonVal::as_u64).unwrap_or(0) as usize;
        }

        let mut tiktok_ads_sales_vnd = 0.0;
        if let Ok(ads_val) = parse_json(&self.tiktok_ads_json) {
            tiktok_cost_vnd = ads_val.get("cost_ads")
                .and_then(JsonVal::as_f64)
                .or_else(|| ads_val.get("metrics").and_then(|m| m.get("cost")).and_then(|c| c.get("value")).and_then(JsonVal::as_f64))
                .unwrap_or(0.0);
            tiktok_ads_sales_vnd = ads_val.get("sales_ads")
                .and_then(JsonVal::as_f64)
                .or_else(|| ads_val.get("metrics").and_then(|m| m.get("sales")).and_then(|s| s.get("value")).and_then(JsonVal::as_f64))
                .unwrap_or(0.0);
            _tiktok_roi = ads_val.get("roas_ads")
                .and_then(JsonVal::as_f64)
                .or_else(|| ads_val.get("metrics").and_then(|m| m.get("roas")).and_then(|r| r.get("value")).and_then(JsonVal::as_f64))
                .unwrap_or(0.0);
            let ads_orders = ads_val.get("orders_ads")
                .and_then(JsonVal::as_u64)
                .or_else(|| ads_val.get("metrics").and_then(|m| m.get("orders")).and_then(|o| o.get("value")).and_then(JsonVal::as_u64))
                .unwrap_or(0) as usize;
            if tiktok_orders == 0 {
                tiktok_orders = ads_orders;
            }
            // Fallback from campaigns table if overview card was 0/null
            if tiktok_cost_vnd == 0.0 {
                if let Some(camps) = ads_val.get("campaigns").and_then(JsonVal::as_arr) {
                    let sum_cost: f64 = camps.iter().filter_map(|c| c.get("cost").and_then(JsonVal::as_f64)).sum();
                    if sum_cost > 0.0 {
                        tiktok_cost_vnd = sum_cost;
                    }
                }
            }
            if tiktok_ads_sales_vnd < 1000.0 {
                if let Some(camps) = ads_val.get("campaigns").and_then(JsonVal::as_arr) {
                    let sum_sales: f64 = camps.iter().filter_map(|c| c.get("sales").and_then(JsonVal::as_f64)).sum();
                    if sum_sales >= 1000.0 {
                        tiktok_ads_sales_vnd = sum_sales;
                    } else {
                        tiktok_ads_sales_vnd = 0.0;
                    }
                } else {
                    tiktok_ads_sales_vnd = 0.0;
                }
            }
            if tiktok_rev_vnd < 1000.0 && tiktok_ads_sales_vnd >= 1000.0 {
                tiktok_rev_vnd = tiktok_ads_sales_vnd;
            }
        }

        let tiktok_name = if !self.tiktok_shop_name.is_empty() {
            self.tiktok_shop_name.clone()
        } else {
            "TikTok Shop VN".to_string()
        };

        if let Ok(ads_val) = parse_json(&self.tiktok_ads_json) {
            let mut added_camp = false;
            if let Some(camps) = ads_val.get("campaigns").and_then(JsonVal::as_arr) {
                for c in camps {
                    let c_name = c.get("name").or_else(|| c.get("campaign_name")).and_then(JsonVal::as_str).unwrap_or("Chiến dịch TikTok");
                    let spent = c.get("cost").and_then(JsonVal::as_f64).unwrap_or(0.0);
                    let sales = c.get("sales").and_then(JsonVal::as_f64).unwrap_or(0.0);
                    let cir_num = if sales > 0.0 { (spent / sales) * 100.0 } else { 0.0 };
                    let cir_str = format!("{:.1}%", cir_num);
                    ads_alerts_list.push(format!(
                        "{{\"shopId\":\"tiktok_main\",\"campaignId\":\"\",\"platform\":\"tiktok\",\"shop\":\"{}\",\"campaign\":\"{}\",\"dailyBudgetVnd\":0,\"spentVnd\":{},\"budgetPct\":\"--\",\"cirNumeric\":{:.1},\"cir\":\"{}\",\"status\":\"ĐANG CHẠY\",\"badge\":\"pill-success\",\"url\":\"https://seller-vn.tiktok.com/homepage\"}}",
                        Self::escape_json(&tiktok_name), Self::escape_json(c_name), spent as i64, cir_num, Self::escape_json(&cir_str)
                    ));
                    added_camp = true;
                }
            }
            if !added_camp && tiktok_cost_vnd > 0.0 {
                let cir_num = if tiktok_ads_sales_vnd > 0.0 { (tiktok_cost_vnd / tiktok_ads_sales_vnd) * 100.0 } else { 0.0 };
                let cir_str = format!("{:.1}%", cir_num);
                ads_alerts_list.push(format!(
                    "{{\"shopId\":\"tiktok_main\",\"campaignId\":\"\",\"platform\":\"tiktok\",\"shop\":\"{}\",\"campaign\":\"Chiến dịch GMV Max / Shop Ads\",\"dailyBudgetVnd\":0,\"spentVnd\":{},\"budgetPct\":\"--\",\"cirNumeric\":{:.1},\"cir\":\"{}\",\"status\":\"ĐANG CHẠY\",\"badge\":\"pill-success\",\"url\":\"https://seller-vn.tiktok.com/homepage\"}}",
                    Self::escape_json(&tiktok_name), tiktok_cost_vnd as i64, cir_num, Self::escape_json(&cir_str)
                ));
            }
        }

        if let Ok(tt_h) = parse_json(&self.tiktok_health_json) {
            let score = tt_h.get("score").and_then(JsonVal::as_str).unwrap_or("Tốt (Chuẩn sàn)");
            let note = tt_h.get("status_note").and_then(JsonVal::as_str).unwrap_or("Vận hành ổn định (0 vi phạm)");
            shop_health_list.push(format!(
                "{{\"shopId\":\"tiktok_main\",\"platform\":\"tiktok\",\"shop\":\"{}\",\"points\":\"{}\",\"status\":\"healthy\",\"note\":\"{}\",\"expressOnTime\":\"99%\",\"url\":\"https://seller-vn.tiktok.com/health-center\"}}",
                Self::escape_json(&tiktok_name), Self::escape_json(score), Self::escape_json(note)
            ));
        }

        if !self.tiktok_json.is_empty() || !self.tiktok_ads_json.is_empty() {
            promos_list.push(format!(
                "{{\"shopId\":\"tiktok_main\",\"platform\":\"tiktok\",\"shop\":\"{}\",\"type\":\"Voucher & Flash Sale\",\"name\":\"Ưu đãi & Phiếu giảm giá TikTok\",\"usage\":\"Đang chạy\",\"url\":\"https://seller-vn.tiktok.com/promotion\"}}",
                Self::escape_json(&tiktok_name)
            ));
        }

        let mut cross_sku_list = Vec::new();
        let mut tt_products_map: HashMap<String, (String, usize)> = HashMap::new();
        if let Ok(p_val) = parse_json(&self.tiktok_products_json) {
            if let Some(prods) = p_val.get("products").and_then(JsonVal::as_arr) {
                for p in prods {
                    let sku = p.get("sku").and_then(JsonVal::as_str).unwrap_or("");
                    let name = p.get("name").and_then(JsonVal::as_str).unwrap_or("");
                    let stock = p.get("stock").and_then(JsonVal::as_u64).unwrap_or(0) as usize;
                    let price = p.get("price").and_then(JsonVal::as_f64).unwrap_or(0.0);
                    let is_anom = p.get("is_anomalous_price").and_then(JsonVal::as_bool).unwrap_or(false);
                    if is_anom || (price > 0.0 && price < 15000.0) {
                        price_anomalies_list.push(format!(
                            "{{\"shopId\":\"tiktok_main\",\"orderId\":\"Cảnh báo giá thấp\",\"platform\":\"tiktok\",\"shop\":\"{}\",\"sku\":\"{}\",\"soldPrice\":\"{} đ\",\"basePrice\":\"15,000 đ\",\"reason\":\"Giá bán dưới ngưỡng an toàn (< 15.000 đ)\",\"url\":\"https://seller-vn.tiktok.com/product/manage\"}}",
                            Self::escape_json(&tiktok_name),
                            Self::escape_json(if !name.is_empty() { name } else { sku }),
                            price as i64
                        ));
                    }
                    let sku_clean = sku.trim();
                    if !sku_clean.is_empty() && sku_clean.len() >= 2 && !matches!(sku_clean.to_lowercase().as_str(), "s" | "m" | "l" | "xl" | "2xl" | "3xl") {
                        tt_products_map.insert(sku_clean.to_string(), (name.to_string(), stock));
                    }
                }
            }
        }

        let mut sp_products_map: HashMap<String, (String, usize)> = HashMap::new();
        for (_, ord_str, _, _) in &shopee_sources {
            if let Ok(ord_val) = parse_json(ord_str) {
                if let Some(items) = ord_val.get("items").or_else(|| ord_val.get("products")).and_then(JsonVal::as_arr) {
                    for it in items {
                        let sku = it.get("item_sku").or_else(|| it.get("model_sku")).and_then(JsonVal::as_str).unwrap_or("");
                        let name = it.get("item_name").and_then(JsonVal::as_str).unwrap_or("");
                        let stock = it.get("stock").or_else(|| it.get("current_stock")).and_then(JsonVal::as_u64).unwrap_or(0) as usize;
                        if !sku.is_empty() {
                            sp_products_map.insert(sku.to_string(), (name.to_string(), stock));
                        }
                    }
                }
            }
        }

        let mut all_skus: HashSet<String> = sp_products_map.keys().cloned().collect();
        for k in tt_products_map.keys() {
            all_skus.insert(k.clone());
        }

        let both_scanned = !shopee_sources.is_empty() && !tt_products_map.is_empty();
        for k in &all_skus {
            let sku_trimmed = k.trim();
            if sku_trimmed.is_empty() || sku_trimmed.len() < 2 || matches!(sku_trimmed.to_lowercase().as_str(), "s" | "m" | "l" | "xl" | "2xl" | "3xl") {
                continue;
            }
            let (sp_name, sp_stock) = sp_products_map.get(k).cloned().unwrap_or((String::new(), 0));
            let (tt_name, tt_stock) = tt_products_map.get(k).cloned().unwrap_or((String::new(), 0));
            let display_name = if !sp_name.is_empty() { sp_name } else { tt_name };
            let mismatch = if both_scanned {
                (sp_stock == 0 && tt_stock > 0) || (tt_stock == 0 && sp_stock > 0) || (sp_stock < 3 && tt_stock < 3)
            } else {
                false
            };
            let note = if both_scanned && sp_stock == 0 && tt_stock > 0 {
                "Shopee HẾT HÀNG nhưng TikTok còn hàng — Mở lại tồn Shopee ngay"
            } else if both_scanned && tt_stock == 0 && sp_stock > 0 {
                "TikTok HẾT HÀNG trong khi Shopee còn hàng — Bổ sung tồn TikTok"
            } else if both_scanned && sp_stock < 3 && tt_stock < 3 {
                "Tồn kho chạm đáy toàn hệ thống — Cần khóa Hỏa tốc nếu dưới 3 cái"
            } else {
                "Đồng bộ tốt giữa các sàn"
            };

            cross_sku_list.push(format!(
                "{{\"sku\":\"{}\",\"name\":\"{}\",\"shopee\":{},\"tiktok\":{},\"pancake\":0,\"mismatch\":{},\"note\":\"{}\",\"url\":\"https://banhang.shopee.vn/portal/product/list/all\"}}",
                Self::escape_json(sku_trimmed),
                Self::escape_json(&display_name),
                sp_stock,
                tt_stock,
                mismatch,
                Self::escape_json(note)
            ));
        }

        let shopee_rev_m = (shopee_rev_vnd / 1_000_000.0 * 10.0).round() / 10.0;
        let tiktok_rev_m = (tiktok_rev_vnd / 1_000_000.0 * 10.0).round() / 10.0;
        let net_rev_m = ((shopee_rev_m + tiktok_rev_m + pk_rev) * 10.0).round() / 10.0;
        let total_orders = shopee_orders + tiktok_orders + pk_orders;

        let shopee_cir_str = if shopee_ads_sales_vnd > 0.0 {
            format!("{:.1}%", (shopee_ads_cost_vnd / shopee_ads_sales_vnd) * 100.0)
        } else {
            "--".to_string()
        };
        let tiktok_cir_str = if tiktok_rev_vnd >= 1000.0 && tiktok_cost_vnd > 0.0 {
            format!("{:.1}%", (tiktok_cost_vnd / tiktok_rev_vnd) * 100.0)
        } else {
            "--".to_string()
        };

        let total_ads_spend_m = ((shopee_ads_cost_vnd + tiktok_cost_vnd) / 1_000_000.0 * 100.0).round() / 100.0;
        let total_rev_vnd = shopee_rev_vnd + tiktok_rev_vnd;
        let total_cir_str = if total_rev_vnd >= 1000.0 && (shopee_ads_cost_vnd + tiktok_cost_vnd) > 0.0 {
            format!("{:.1}%", ((shopee_ads_cost_vnd + tiktok_cost_vnd) / total_rev_vnd) * 100.0)
        } else {
            "--".to_string()
        };

        let aov_str = if total_orders > 0 {
            let aov = ((shopee_rev_vnd + tiktok_rev_vnd + pk_rev * 1_000_000.0) / total_orders as f64) as i64;
            format!("{} đ", aov)
        } else {
            "0 đ".to_string()
        };

        let shopee_shop_count = shopee_sources.len();
        let shopee_label = format!("Shopee ({} shop)", if shopee_shop_count > 0 { shopee_shop_count } else { 1 });

        let body = format!(
            "{{\"timestamp\":\"{}\",\"platforms\":[{{\"key\":\"shopee\",\"name\":\"{}\",\"rev\":{},\"orders\":{},\"cir\":\"{}\",\"liveRev\":0.0,\"expressPct\":\"--\",\"color\":\"#EE4D2D\"}},{{\"key\":\"tiktok\",\"name\":\"{}\",\"rev\":{},\"orders\":{},\"cir\":\"{}\",\"liveRev\":0.0,\"expressPct\":\"--\",\"color\":\"#161823\"}},{{\"key\":\"pancake\",\"name\":\"{}\",\"rev\":{},\"orders\":{},\"cir\":\"--\",\"liveRev\":0.0,\"expressPct\":\"--\",\"color\":\"#2563EB\"}}],\"kpis\":{{\"netRev\":{},\"revNote\":\"Quét realtime\",\"netProfit\":0.0,\"marginPct\":\"--\",\"adsSpend\":{},\"cir\":\"{}\",\"roas\":\"--\",\"totalOrders\":{},\"aov\":\"{}\",\"expressPct\":\"--\",\"expressUrgent\":\"{} đơn Hỏa tốc\",\"cancelRate\":\"--\",\"toShipCount\":{}}},\"hours\":[\"00h\",\"04h\",\"08h\",\"12h\",\"16h\",\"20h\",\"Hiện tại\"],\"revSeries\":[0,0,0,0,0,0,{}],\"profitSeries\":[0,0,0,0,0,0,0],\"shopHealth\":[{}],\"liveSessions\":[{}],\"priceAnomalies\":[{}],\"badReviews\":[{}],\"crossSku\":[{}],\"adsAlerts\":[{}],\"promos\":[{}]}}",
            now_str,
            Self::escape_json(&shopee_label),
            shopee_rev_m,
            shopee_orders,
            shopee_cir_str,
            Self::escape_json(&tiktok_name),
            tiktok_rev_m,
            tiktok_orders,
            tiktok_cir_str,
            Self::escape_json(&pk_label),
            pk_rev,
            pk_orders,
            net_rev_m,
            total_ads_spend_m,
            total_cir_str,
            total_orders,
            Self::escape_json(&aov_str),
            shopee_express,
            shopee_to_ship,
            net_rev_m,
            shop_health_list.join(","),
            live_sessions_list.join(","),
            price_anomalies_list.join(","),
            bad_reviews_list.join(","),
            cross_sku_list.join(","),
            ads_alerts_list.join(","),
            promos_list.join(",")
        );

        let date_str = self.current_date_str();
        let _ = fs::write(snapshots_dir.join(format!("{}.json", snapshot_key)), &body);
        let _ = fs::write(snapshots_dir.join(format!("snap_{}.json", date_str)), &body);
        let _ = fs::write(snapshots_dir.join("snap_latest.json"), &body);

        if snapshot_key == "snap_latest" {
            let mismatch_count = cross_sku_list.iter().filter(|s| s.contains("\"mismatch\":true")).count();
            self.log_ecom("DATA", "SNAPSHOT", &format!(
                "=== TỔNG KẾT PHIÊN QUÉT === Tổng Doanh Thu: {} tr VNĐ | Tổng Đơn: {} | Shopee: {} tr ({} đơn) | TikTok: {} tr ({} đơn) | Pancake: {} tr ({} đơn) | Tồn kho lệch: {} SKU",
                net_rev_m, total_orders, shopee_rev_m, shopee_orders, tiktok_rev_m, tiktok_orders, pk_rev, pk_orders, mismatch_count
            ));

            for item in cross_sku_list.iter().filter(|s| s.contains("\"mismatch\":true")).take(5) {
                if let Ok(it) = parse_json(item) {
                    let sku = it.get("sku").and_then(JsonVal::as_str).unwrap_or("");
                    let sp = it.get("shopee").and_then(JsonVal::as_u64).unwrap_or(0);
                    let tt = it.get("tiktok").and_then(JsonVal::as_u64).unwrap_or(0);
                    let note = it.get("note").and_then(JsonVal::as_str).unwrap_or("");
                    self.log_ecom("WARN", "CROSS-SKU", &format!("Cảnh báo tồn kho: SKU [{}] -> Shopee: {} cái | TikTok: {} cái | {}", sku, sp, tt, note));
                }
            }
        }
    }

    fn sync_workspace_command_center(&self) {
        self.sync_workspace_command_center_with_key("snap_latest");
    }

    fn sync_workspace_command_center_with_key(&self, snapshot_key: &str) {
        let mut shops = Vec::new();
        let is_selected = |plat: &str, sid: &str| -> bool {
            self.selected_shops.iter().any(|sel| sel.platform == plat && sel.shop_id == sid)
        };
        if let Ok(disc) = parse_json(&self.shopee_discovery_json) {
            if let Some(shop_arr) = disc.get("shops").and_then(JsonVal::as_arr) {
                for s in shop_arr {
                    let id = s.get("shop_id").and_then(JsonVal::as_str).unwrap_or("");
                    let name = s.get("name").or_else(|| s.get("display_name")).and_then(JsonVal::as_str).unwrap_or(id);
                    if !id.is_empty() {
                        let selected = is_selected("shopee", id);
                        shops.push(format!("{{\"id\":\"{}\",\"platform\":\"shopee\",\"name\":\"{}\",\"selected\":{}}}",
                            Self::escape_json(id), Self::escape_json(name), selected));
                    }
                }
            }
        }
        if shops.is_empty() {
            for (id, name) in [
                ("sp_1556409510", "PetsCare Sài Gòn"),
                ("sp_1328141534", "PetMallVN"),
                ("sp_1072275019", "Tui Tiến Pet VN"),
                ("sp_1053694388", "MasterCare Việt Nam"),
                ("sp_366273305", "Master Care for Pet"),
            ] {
                shops.push(format!("{{\"id\":\"{}\",\"platform\":\"shopee\",\"name\":\"{}\",\"selected\":{}}}",
                    id, name, is_selected("shopee", id)));
            }
        }
        let tt_name = if !self.tiktok_shop_name.is_empty() {
            self.tiktok_shop_name.clone()
        } else {
            "TikTok Shop VN".to_string()
        };
        shops.push(format!("{{\"id\":\"tt_01\",\"platform\":\"tiktok\",\"name\":\"{}\",\"selected\":{}}}",
            Self::escape_json(&tt_name), is_selected("tiktok", "tt_01")));
        let pk_name = if !self.pancake_shop_name.is_empty() {
            format!("Pancake {}", self.pancake_shop_name)
        } else {
            "Pancake Kho Tổng".to_string()
        };
        shops.push(format!("{{\"id\":\"pk_01\",\"platform\":\"pancake\",\"name\":\"{}\",\"selected\":{}}}",
            Self::escape_json(&pk_name), is_selected("pancake", "pk_01")));

        let pancake_connected = self.pancake_connected;
        let pancake_label = if pancake_connected {
            if !self.pancake_shop_name.is_empty() {
                format!("Đã kết nối ({})", self.pancake_shop_name)
            } else {
                "Đã kết nối (1 kho)".to_string()
            }
        } else {
            "Chưa kết nối".to_string()
        };

        let snapshots_dir = self.exe_dir.join("user_data").join("snapshots");
        let mut available_snapshots = Vec::new();
        available_snapshots.push("{\"key\":\"snap_latest\",\"label\":\"🕒 Phiên mới nhất (Realtime)\"}".to_string());
        if let Ok(entries) = fs::read_dir(&snapshots_dir) {
            let mut list: Vec<String> = entries.filter_map(|e| {
                let p = e.ok()?.path();
                if p.extension().and_then(|ext| ext.to_str()) == Some("json") {
                    p.file_stem().and_then(|s| s.to_str()).map(String::from)
                } else {
                    None
                }
            }).collect();
            list.sort_by(|a, b| b.cmp(a));
            for k in list {
                if k != "snap_latest" {
                    available_snapshots.push(format!("{{\"key\":\"{}\",\"label\":\"📁 Phiên lưu: {}\"}}", Self::escape_json(&k), Self::escape_json(&k)));
                }
            }
        }
        let available_snapshots_json = available_snapshots.join(",");

        let snap_file = snapshots_dir.join(format!("{}.json", snapshot_key));
        let snapshot_data_body = if snap_file.exists() {
            fs::read_to_string(&snap_file).unwrap_or_default()
        } else {
            String::new()
        };
        let snapshot_data_body = if !snapshot_data_body.trim().is_empty() {
            snapshot_data_body
        } else {
            format!(
                "{{\"timestamp\":\"Chưa có phiên quét\",\"platforms\":[{{\"key\":\"shopee\",\"name\":\"Shopee\",\"rev\":0,\"orders\":0,\"cir\":\"0%\",\"liveRev\":0,\"expressPct\":\"0%\",\"color\":\"#EE4D2D\"}},{{\"key\":\"tiktok\",\"name\":\"TikTok Shop\",\"rev\":0,\"orders\":0,\"cir\":\"0%\",\"liveRev\":0,\"expressPct\":\"0%\",\"color\":\"#161823\"}},{{\"key\":\"pancake\",\"name\":\"Pancake POS ({})\",\"rev\":0,\"orders\":0,\"cir\":\"0%\",\"liveRev\":0,\"expressPct\":\"0%\",\"color\":\"#2563EB\"}}],\"kpis\":{{\"netRev\":0,\"revNote\":\"Chưa có dữ liệu phiên mới\",\"netProfit\":0,\"marginPct\":\"0%\",\"adsSpend\":0,\"cir\":\"0%\",\"roas\":\"0x\",\"totalOrders\":0,\"aov\":\"0 đ\",\"expressPct\":\"0%\",\"expressUrgent\":\"0 đơn Hỏa tốc\",\"cancelRate\":\"0%\",\"toShipCount\":0}},\"hours\":[],\"revSeries\":[],\"profitSeries\":[],\"shopHealth\":[],\"liveSessions\":[],\"priceAnomalies\":[],\"badReviews\":[],\"crossSku\":[],\"adsAlerts\":[],\"promos\":[]}}",
                pancake_label
            )
        };

        let shops_json = shops.join(",");
        let payload = format!(
            "{{\"snapshotKey\":\"{}\",\"availableSnapshots\":[{}],\"pancakeStatus\":{{\"connected\":{},\"shopsCount\":{},\"label\":\"{}\"}},\"shops\":[{}],\"snapshotData\":{}}}",
            Self::escape_json(snapshot_key),
            available_snapshots_json,
            pancake_connected,
            if pancake_connected { 1 } else { 0 },
            pancake_label,
            shops_json,
            snapshot_data_body
        );

        let js = format!("if (typeof window.onWorkspaceData === 'function') window.onWorkspaceData({});", payload);
        for t in &self.tabs {
            if t.url == "tp://workspace" {
                let _ = t.webview.evaluate_script(&js);
            }
        }
    }

    fn start_shopee_discovery(&mut self, requested_tab_id: Option<u64>, workspace: bool, reply: Option<mpsc::Sender<Result<String, String>>>) {
        let selected = self.tabs.iter().find(|tab| {
            requested_tab_id.map_or(true, |id| tab.id == id)
                && (tab.worker_kind.is_none() || requested_tab_id == Some(tab.id))
                && shopee_discovery::valid_url(&tab.url)
                && tab.navigation_readiness.ready_navigation_id().is_some()
        });
        let Some(tab) = selected else {
            let fallback = self.tabs.iter().find(|tab| {
                requested_tab_id.map_or(true, |id| tab.id == id)
                    && tab.worker_kind.is_none()
                    && tab.url.contains("banhang.shopee.vn/")
            });
            if let Some(tab) = fallback {
                let tab_id = tab.id;
                self.shopee_discovery_tab_id = Some(tab_id);
                self.shopee_discovery_json = shopee_discovery::result_loading();
                if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) {
                    let _ = tab.webview.load_url(shopee_discovery::SHOP_INDEX_URL);
                }
                if workspace { self.sync_workspace_view(); }
                return;
            }
            if requested_tab_id.is_none() {
                let Some(tab_id) = self.ensure_ecom_worker(
                    "shopee_discovery",
                    shopee_discovery::SHOP_INDEX_URL,
                    "Shopee Shop Discovery",
                ) else {
                    if workspace {
                        self.shopee_discovery_json = shopee_discovery::unavailable();
                        self.sync_workspace_view();
                    }
                    return;
                };
                self.shopee_discovery_tab_id = Some(tab_id);
                self.shopee_discovery_json = shopee_discovery::result_loading();
                if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) {
                    let _ = tab.webview.load_url(shopee_discovery::SHOP_INDEX_URL);
                }
                if workspace { self.sync_workspace_view(); }
                return;
            }
            if let Some(reply) = reply { let _ = reply.send(Ok(shopee_discovery::unavailable())); }
            else if workspace { self.shopee_discovery_json = shopee_discovery::unavailable(); self.sync_workspace_view(); }
            return;
        };
        let tab_id = tab.id;
        let Some(saved_navigation_id) = tab.navigation_readiness.ready_navigation_id() else { return; };
        let Ok(current_url) = tab.webview.url() else {
            if let Some(reply) = reply { let _ = reply.send(Ok(shopee_discovery::unavailable())); }
            return;
        };
        if !shopee_discovery::valid_url(&current_url) || !shopee_discovery::admit_navigation(&tab.navigation_readiness, Some(saved_navigation_id)) {
            if let Some(reply) = reply { let _ = reply.send(Ok(shopee_discovery::unavailable())); }
            else if workspace { self.shopee_discovery_json = shopee_discovery::unavailable(); self.sync_workspace_view(); }
            return;
        }
        let req_id = self.next_mcp_req_id;
        self.next_mcp_req_id = self.next_mcp_req_id.wrapping_add(1).max(1);
        let (tx, _rx) = mpsc::channel();
        if let Err(returned) = shopee_discovery::insert_pending(&mut self.shopee_pending, req_id, (tab_id, saved_navigation_id, Instant::now() + shopee_discovery::TIMEOUT, reply.unwrap_or(tx))) {
            let _ = returned.send(Ok(shopee_discovery::unavailable()));
            return;
        }
        if workspace { self.shopee_workspace_pending.insert(req_id); }
        let proxy = self.proxy.clone();
        if tab.webview.evaluate_script_with_callback(shopee_discovery::SCRIPT, move |payload| {
            let payload = if payload.len() <= 128 * 1024 { Some(payload) } else { None };
            let _ = proxy.send_event(AppCmd::ShopeeNativeResult { req_id, tab_id, payload: ShopeePayload(payload) });
        }).is_err() {
            self.shopee_workspace_pending.remove(&req_id);
            if let Some((_, _, _, reply)) = self.shopee_pending.remove(&req_id) { let _ = reply.send(Ok(shopee_discovery::unavailable())); }
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

    fn ensure_ecom_worker(&mut self, job: &str, url: &str, title: &str) -> Option<u64> {
        if let Some(t) = self.tabs.iter_mut().find(|t| {
            t.worker_kind.as_deref() == Some("bot") && t.ecom_worker_job.as_deref() == Some(job)
        }) {
            t.worker_status = "Đang quét...".to_string();
            let id = t.id;
            return Some(id);
        }
        let tab_id = self.add_worker_tab(url, "bot", Some(title))?;
        if let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            t.ecom_worker_job = Some(job.to_string());
        }
        Some(tab_id)
    }

    fn shopee_shop_scan_in_flight(&self) -> bool {
        !self.shopee_shop_scan_phase.is_empty() && self.shopee_shop_scan_phase != "done"
    }

    fn trigger_worker_scan_by_id(&mut self, tab_id: u64, url: &str) {
        if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) {
            if tab.ecom_worker_job.as_deref() == Some("shopee_discovery") {
                return;
            }
            if url.contains("accounts.shopee.vn") || url.contains("/seller/login") {
                if !self.multi_shop_queue.is_empty() && self.multi_shop_tab_id == Some(tab_id) && !self.multi_shop_auth_pending {
                    let s_id = self.multi_shop_queue.get(self.multi_shop_idx).cloned().unwrap_or_default();
                    self.execute(AppCmd::ScanNeedAuth {
                        reason: "login".to_string(),
                        shop_id: s_id,
                    });
                }
                return;
            }
            if url.contains("banhang.shopee.vn") {
                if !self.multi_shop_queue.is_empty() && self.multi_shop_tab_id == Some(tab_id) {
                    if self.multi_shop_step == "opening_shop" {
                        if url.contains("/portal/shop") {
                            if let Some((s_id, name)) = self.shopee_shop_scan_current.clone() {
                                let js = format!(
                                    "window.__tpShopTarget={{shop_id:{:?},display_name:{:?}}}; setTimeout(function(){{ {} }}, 800);",
                                    s_id, name, SHOPEE_OPEN_SHOP_JS
                                );
                                let _ = tab.webview.evaluate_script(&js);
                            }
                        } else if self.shopee_shop_scan_phase == "shop_orders" {
                            // Shopee đã chuyển về trang chủ shop sau khi click Chi tiết -> điều hướng sang Đơn hàng
                            self.multi_shop_step = "orders".to_string();
                            let _ = tab.webview.load_url("https://banhang.shopee.vn/portal/sale/order");
                        } else {
                            // Nếu Shopee tự redirect khỏi /portal/shop trước khi kịp chọn shop -> nạp lại /portal/shop
                            let _ = tab.webview.load_url("https://banhang.shopee.vn/portal/shop");
                        }
                    } else if self.multi_shop_step == "orders" {
                        if url.contains("/portal/sale/order") {
                            let _ = tab.webview.evaluate_script(
                                "setTimeout(function(){ window.__tpCurrentScanModule = 'orders'; if (window.__tpRunShopeeScan) { window.__tpRunShopeeScan('orders'); } else if (window.ipc && window.ipc.postMessage) { window.ipc.postMessage(JSON.stringify({ cmd: 'ecom_log_debug', msg: 'LỖI: window.__tpRunShopeeScan chưa nạp trên /portal/sale/order' })); } }, 1500);",
                            );
                            self.log_ecom("INFO", "SHOPEE", "Đã vào /portal/sale/order, bắt đầu kéo đơn trong ngày...");
                        } else if !url.contains("/portal/shop") && !url.contains("login") && !url.contains("signin") && !url.contains("captcha") {
                            // Đợi 2s cho Shopee hoàn tất ghi cookie/phiên shop mới rồi mới sang Đơn hàng
                            let _ = tab.webview.evaluate_script(
                                "if (!window.__tpOrderNavScheduled) { window.__tpOrderNavScheduled = true; setTimeout(function(){ window.location.href = 'https://banhang.shopee.vn/portal/sale/order'; }, 2000); }"
                            );
                        }
                    } else if self.multi_shop_step == "pas" && url.contains("/portal/marketing/pas") {
                        let _ = tab.webview.evaluate_script(
                            "setTimeout(function(){ window.__tpCurrentScanModule = 'pas'; if (window.__tpRunShopeeScan) window.__tpRunShopeeScan('pas'); }, 2500);",
                        );
                    } else if self.multi_shop_step == "health" && url.contains("/portal/accounthealth") {
                        let js = format!("setTimeout(function(){{ window.__tpCurrentScanModule = 'health'; {} }}, 2200);", SHOPEE_HEALTH_SCAN_JS);
                        let _ = tab.webview.evaluate_script(&js);
                    }
                    return;
                }
            }
            if url.contains("banhang.shopee.vn/portal/shop") && !self.shopee_shop_scan_phase.is_empty() {
                let js = format!("setTimeout(function(){{ {} }}, 1600);", SHOPEE_SHOP_INDEX_SCAN_JS);
                let _ = tab.webview.evaluate_script(&js);
            } else if url.contains("banhang.shopee.vn/portal/marketing/pas") {
                let _ = tab.webview.evaluate_script(
                    "setTimeout(function(){ if (window.__tpRunShopeeScan) window.__tpRunShopeeScan('pas'); }, 1500);",
                );
            } else if url.contains("banhang.shopee.vn/portal/accounthealth") {
                let js = format!("setTimeout(function(){{ {} }}, 1600);", SHOPEE_HEALTH_SCAN_JS);
                let _ = tab.webview.evaluate_script(&js);
            } else if url.contains("banhang.shopee.vn") {
                let _ = tab.webview.evaluate_script(
                    "setTimeout(function(){ if (window.__tpRunShopeeScan) window.__tpRunShopeeScan('orders'); }, 1000);",
                );
            } else if url.contains("seller-vn.tiktok.com") {
                let collector = if url.contains("health-center") || self.multi_shop_step == "tiktok_health" {
                    TIKTOK_HEALTH_SCAN_JS
                } else if url.contains("/product/manage") || self.multi_shop_step == "tiktok_products" {
                    TIKTOK_PRODUCTS_SCAN_JS
                } else if url.contains("ads-creation/dashboard") || self.multi_shop_step == "tiktok_ads" {
                    TIKTOK_PASSIVE_SCAN_JS
                } else if self.tiktok_scan_phase == "identity" {
                    TIKTOK_IDENTITY_JS
                } else {
                    TIKTOK_HOMEPAGE_SCAN_JS
                };
                let js = if url.contains("ads-creation/dashboard") || self.multi_shop_step == "tiktok_ads" {
                    format!(
                        r#"setTimeout(function(){{
                            function tryClickToday() {{
                                var btns = document.querySelectorAll("button, li, span, div, [role='option'], [class*='preset'], [class*='quick'], [class*='date']");
                                for (var i = 0; i < btns.length; i++) {{
                                    var el = btns[i];
                                    if (el.children.length > 2) continue;
                                    var t = (el.innerText || "").trim().toLowerCase().replace(/\s+/g," ");
                                    if (t === "hôm nay" || t === "today" || t === "\u4eca\u5929" || t === "h\u00f4m nay") {{
                                        el.click();
                                        return true;
                                    }}
                                }}
                                return false;
                            }}
                            if (!tryClickToday()) {{
                                var dp = document.querySelector("[class*='date-picker'], [class*='range-picker'], [class*='datePicker'], input[placeholder*='ngày'], input[value*='-']");
                                if (dp) {{
                                    dp.click();
                                    setTimeout(tryClickToday, 400);
                                }}
                            }}
                        }}, 1500);
                        setTimeout(function(){{ {} }}, 4800);"#,
                        TIKTOK_PASSIVE_SCAN_JS
                    )
                } else {
                    format!("setTimeout(function(){{ {} }}, 2200);", collector)
                };
                let _ = tab.webview.evaluate_script(&js);
            }
        }
    }

    #[allow(dead_code)]
    fn queue_discovered_shopee_shops(&mut self, selected_only: bool) -> usize {
        let Ok(discovery) = parse_json(&self.shopee_discovery_json) else { return 0; };
        if discovery.get("status").and_then(JsonVal::as_str) != Some("ok") || discovery.get("logged_in").and_then(JsonVal::as_bool) != Some(true) { return 0; }
        let selected: HashSet<&str> = self.selected_shops.iter().filter(|item| item.platform == "shopee").map(|item| item.shop_id.as_str()).collect();
        let mut seen = HashSet::new();
        let queued: Vec<(String, String)> = discovery.get("shops").and_then(JsonVal::as_arr).into_iter().flatten().filter_map(|row| {
            let id = row.get("shop_id").and_then(JsonVal::as_str)?.trim();
            let name = row.get("display_name").and_then(JsonVal::as_str)?.trim();
            (!id.is_empty() && !name.is_empty() && (!selected_only || selected.contains(id)) && seen.insert(id.to_string())).then(|| (id.to_string(), name.to_string()))
        }).collect();
        self.shopee_shop_scan_queue = queued.into_iter().rev().collect();
        self.shopee_shop_scan_queue.len()
    }

    #[allow(dead_code)]
    fn start_shopee_shop_sequence(&mut self) {
        if self.shopee_shop_scan_in_flight() {
            return;
        }
        let index_url = "https://banhang.shopee.vn/portal/shop";
        let Some(tab_id) = self.ensure_ecom_worker("shopee_shop_scanner", index_url, "Shopee Shop Scanner") else { return; };
        self.shopee_shop_scan_tab_id = Some(tab_id);
        self.shopee_shop_scan_phase = "index".to_string();
        self.shopee_shop_scan_current = None;
        if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) { let _ = tab.webview.load_url(index_url); }
        self.trigger_worker_scan_by_id(tab_id, index_url);
    }

    fn open_next_shopee_shop(&mut self) {
        let Some(tab_id) = self.shopee_shop_scan_tab_id else { return; };
        let Some((shop_id, display_name)) = self.shopee_shop_scan_queue.pop() else {
            self.shopee_shop_scan_phase = "done".to_string();
            self.shopee_shop_scan_current = None;
            self.ecom_status_text = "Đã quét xong các Shop Shopee.".to_string();
            self.push_ecom_log(&self.ecom_status_text.clone());
            self.log_ecom("SUCCESS", "SHOPEE", "Đã quét xong toàn bộ các Shop Shopee.");
            self.sync_workspace_progress(true, 85, 100, 85, "Shopee: Đã xong tất cả shop con", "running");
            self.sync_workspace_view();
            return;
        };
        self.shopee_shop_scan_current = Some((shop_id.clone(), display_name.clone()));
        self.shopee_shop_scan_phase = "opening_shop".to_string();
        self.log_ecom("INFO", "SHOPEE", &format!("Bắt đầu quét shop: {} ({})", display_name, shop_id));
        self.sync_workspace_progress(true, 40, 100, 40, &format!("Shopee: Đang quét shop {}", display_name), "running");
        if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) {
            let js = format!("window.__tpShopTarget={{shop_id:{:?},display_name:{:?}}}; setTimeout(function(){{ {} }}, 700);", shop_id, display_name, SHOPEE_OPEN_SHOP_JS);
            let _ = tab.webview.evaluate_script(&js);
        }
    }

    fn handle_shopee_shop_index(&mut self, data_json: &str) {
        self.shopee_shop_index_json = data_json.to_string();
        self.save_export_file("shopee_shop_index_latest.json", data_json);
        let mut discovered = Vec::new();
        if let Ok(value) = parse_json(data_json) {
            if let Some(rows) = value.get("shops").and_then(JsonVal::as_arr) {
                for row in rows {
                    let id = row.get("shop_id").and_then(JsonVal::as_str).unwrap_or("").trim();
                    let name = row.get("display_name").and_then(JsonVal::as_str).unwrap_or("").trim();
                    if !id.is_empty() && !name.is_empty() { discovered.push((id.to_string(), name.to_string())); }
                }
            }
        }
        let known_shop_ids: HashSet<&str> = discovered.iter().map(|(id, _)| id.as_str()).collect();
        let before_selection_count = self.selected_shops.len();
        self.selected_shops.retain(|selection| selection.platform != "shopee" || known_shop_ids.contains(selection.shop_id.as_str()));
        if self.selected_shops.len() != before_selection_count { self.save_config(); }
        if self.shopee_shop_scan_phase == "index" || self.shopee_shop_scan_phase == "returning_index" { self.open_next_shopee_shop(); }
        self.sync_workspace_view();
    }

    fn store_shopee_shop_orders(&mut self, data_json: &str) {
        let Some((shop_id, display_name)) = self.shopee_shop_scan_current.clone() else { return; };
        let Ok(value) = parse_json(data_json) else { return; };
        let mut fields = match parse_json(&self.shopee_shop_orders_json) {
            Ok(JsonVal::Obj(existing)) => existing,
            _ => Vec::new(),
        };
        if let Some((_, old)) = fields.iter_mut().find(|(key, _)| key == &shop_id) { *old = value; }
        else { fields.push((shop_id, value)); }
        self.shopee_shop_orders_json = JsonVal::Obj(fields).to_json();
        self.save_export_file("shopee_shop_orders_latest.json", &self.shopee_shop_orders_json);
        self.log_ecom("SUCCESS", "SHOPEE", &format!("Đã cập nhật đơn hàng thành công cho shop: {}", display_name));
    }

    fn handle_shopee_shop_opening(&mut self, data_json: &str) {
        let status = parse_json(data_json).ok().and_then(|v| v.get("status").and_then(JsonVal::as_str).map(str::to_string)).unwrap_or_default();
        let tab_id = self.shopee_shop_scan_tab_id.or(self.multi_shop_tab_id);
        let Some(tab_id) = tab_id else { return; };
        if status == "needs_login" || status == "captcha" {
            self.log_ecom("WARN", "SHOPEE", &format!("Shopee yêu cầu {} khi mở shop, tạm dừng chờ người dùng xử lý.", status));
            let s_id = self.shopee_shop_scan_current.as_ref().map(|(id, _)| id.clone()).unwrap_or_default();
            self.execute(AppCmd::ScanNeedAuth {
                reason: if status == "captcha" { "captcha".to_string() } else { "login".to_string() },
                shop_id: s_id,
            });
            return;
        }
        if status == "clicked" {
            self.shopee_shop_scan_phase = "shop_orders".to_string();
            if !self.multi_shop_queue.is_empty() {
                self.multi_shop_step = "orders".to_string();
            }
            if let Some((_, display_name)) = &self.shopee_shop_scan_current {
                self.log_ecom("INFO", "SHOPEE", &format!("Đã click chọn shop {}, đợi Shopee chuyển phiên rồi sang Đơn hàng...", display_name));
            }
            if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) {
                let _ = tab.webview.evaluate_script(
                    "setTimeout(function(){ if (String(window.location.pathname || '').indexOf('/portal/sale/order') < 0) { window.location.href='https://banhang.shopee.vn/portal/sale/order'; } }, 3500);"
                );
            }
        } else {
            let (s_id, disp) = self.shopee_shop_scan_current.clone().unwrap_or_default();
            self.log_ecom("WARN", "SHOPEE", &format!(
                "Không tìm thấy hoặc không click được shop {} ({}) trên /portal/shop — tạm dừng để tránh quét nhầm shop khác!",
                disp, s_id
            ));
            self.execute(AppCmd::ScanNeedAuth {
                reason: "shop_not_found".to_string(),
                shop_id: s_id,
            });
        }
    }

    fn open_shopee_shop_in_user_tab(&mut self, shop_id: &str) {
        if shop_id.trim().is_empty() { return; }
        self.shopee_open_target_id = Some(shop_id.to_string());
        self.add_tab("https://banhang.shopee.vn/portal/shop", false);
    }

    #[allow(dead_code)]
    fn start_tiktok_sequence(&mut self) {
        let ads_url = "https://seller-vn.tiktok.com/ads-creation/dashboard?origin=SC_ads_tab_button_PC&mpa=1&type=product&shop_region=VN";
        let Some(tab_id) = self.ensure_ecom_worker("tiktok_seller", ads_url, "TikTok Seller") else { return; };
        self.tiktok_scan_tab_id = Some(tab_id);
        self.tiktok_scan_phase = "ads".to_string();
        if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) {
            let _ = tab.webview.load_url(ads_url);
        }
        self.trigger_worker_scan_by_id(tab_id, ads_url);
    }

    fn continue_tiktok_sequence(&mut self, next: &str, phase: &str) {
        let Some(tab_id) = self.tiktok_scan_tab_id else { return; };
        self.tiktok_scan_phase = phase.to_string();
        if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) {
            let _ = tab.webview.load_url(next);
        }
        self.trigger_worker_scan_by_id(tab_id, next);
    }

    fn sync_tab_folders(&mut self) {
        let active_group_ids: HashSet<u64> = self.tabs.iter().filter_map(|t| t.tab_group_id).collect();
        self.tab_folders.retain(|f| active_group_ids.contains(&f.id));
        for gid in active_group_ids {
            if !self.tab_folders.iter().any(|f| f.id == gid) {
                let ws_id = self
                    .tabs
                    .iter()
                    .find(|t| t.tab_group_id == Some(gid))
                    .map(|t| t.workspace_id.clone())
                    .unwrap_or_else(|| self.active_workspace_id.clone());
                self.tab_folders.push(TabFolder {
                    id: gid,
                    name: format!("Thư mục {}", gid),
                    workspace_id: ws_id,
                    collapsed: false,
                });
            }
        }
    }

    fn serialize_session_tab(
        url: &str,
        profile: &str,
        container_id: &str,
        workspace_id: &str,
        pinned: bool,
        tab_group_id: Option<u64>,
        essential: bool,
        pinned_origin_url: Option<&str>,
    ) -> String {
        format!(
            "{{\"url\":\"{}\",\"profile\":\"{}\",\"container_id\":\"{}\",\"workspace_id\":\"{}\",\"pinned\":{},\"tab_group_id\":{},\"essential\":{},\"pinned_origin_url\":\"{}\"}}",
            Self::escape_json(url),
            Self::escape_json(profile),
            Self::escape_json(container_id),
            Self::escape_json(workspace_id),
            pinned,
            tab_group_id.unwrap_or(0),
            essential,
            Self::escape_json(pinned_origin_url.unwrap_or(""))
        )
    }

    fn mark_session_dirty(&self) {
        if self.persist_session && self.session_dirty_since.get().is_none() {
            self.session_dirty_since.set(Some(Instant::now()));
        }
    }

    fn session_due(&self) -> bool {
        self.session_dirty_since
            .get()
            .is_some_and(|t| t.elapsed() >= std::time::Duration::from_millis(450))
    }

    fn session_deadline(&self) -> Option<Instant> {
        self.session_dirty_since
            .get()
            .map(|t| t + std::time::Duration::from_millis(450))
    }

    fn save_session(&self) {
        // Multi-window persistence is deliberately out of scope for this
        // foundation. Secondary runtimes must not overwrite the primary's
        // flat single-window session.json.
        if !self.persist_session {
            return;
        }
        self.session_dirty_since.set(None);
        let t0 = Instant::now();
        let session_path = self.exe_dir.join("session.json");
        let mut tabs_arr = Vec::new();
        let mut saved_active_idx = 0usize;
        let mut user_idx = 0usize;
        for (idx, t) in self.tabs.iter().enumerate() {
            if t.worker_kind.is_some() || t.private {
                continue;
            }
            if idx == self.active_idx {
                saved_active_idx = user_idx;
            }
            tabs_arr.push(Self::serialize_session_tab(
                &t.url,
                &t.profile_name,
                &t.container_id,
                &t.workspace_id,
                t.pinned,
                t.tab_group_id,
                t.essential,
                t.pinned_origin_url.as_deref(),
            ));
            user_idx += 1;
        }
        if tabs_arr.is_empty() {
            tabs_arr.push(Self::serialize_session_tab(
                "tp://newtab",
                "default",
                "default",
                &self.active_workspace_id,
                false,
                None,
                false,
                None,
            ));
        }
        let folders_json = self
            .tab_folders
            .iter()
            .map(|f| {
                format!(
                    "{{\"id\":{},\"name\":\"{}\",\"workspace_id\":\"{}\",\"collapsed\":{}}}",
                    f.id,
                    Self::escape_json(&f.name),
                    Self::escape_json(&f.workspace_id),
                    f.collapsed
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let body = format!(
            "{{\n  \"view_mode\": \"{}\",\n  \"active_idx\": {},\n  \"active_workspace_id\": \"{}\",\n  \"tabs\": [\n    {}\n  ],\n  \"tab_folders\": [{}]\n}}",
            self.view_mode.as_str(),
            saved_active_idx,
            Self::escape_json(&self.active_workspace_id),
            tabs_arr.join(",\n    "),
            folders_json
        );
        let _ = fs::write(session_path, body);
        let dur = t0.elapsed().as_secs_f64() * 1000.0;
        if dur > 5.0 {
            Self::log_perf("save_session (serialized write)", dur);
        }
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
                if let Some(id) = val.get("active_workspace_id").and_then(|v| v.as_str()) {
                    if self.workspaces.iter().any(|w| w.id == id) {
                        self.active_workspace_id = id.to_string();
                    }
                }
                let target_active = saved_active_idx.unwrap_or(0);
                if let Some(arr) = val.get("tabs").and_then(|v| v.as_arr()) {
                    for (i, item) in arr.iter().enumerate() {
                        if let Some(u) = item.get("url").and_then(|v| v.as_str()) {
                            let isolated = item
                                .get("profile")
                                .and_then(|v| v.as_str())
                                .map(|p| p != "default")
                                .unwrap_or(false);
                            let lazy = i != target_active;
                            let container_id = item.get("container_id").and_then(|v| v.as_str()).map(Self::normalize_container_id);
                            self.add_tab_internal(u, isolated, false, None, None, lazy, container_id);
                            if let Some(tab) = self.tabs.last_mut() {

                                let workspace_id = item.get("workspace_id").and_then(|v| v.as_str()).unwrap_or(DEFAULT_WORKSPACE_ID);
                                tab.workspace_id = if self.workspaces.iter().any(|w| w.id == workspace_id) {
                                    workspace_id.to_string()
                                } else { DEFAULT_WORKSPACE_ID.to_string() };
                                tab.pinned = item.get("pinned").and_then(|v| v.as_bool()).unwrap_or(false);
                                tab.essential = item.get("essential").and_then(|v| v.as_bool()).unwrap_or(false);
                                tab.pinned_origin_url = item
                                    .get("pinned_origin_url")
                                    .and_then(|v| v.as_str())
                                    .filter(|s| !s.is_empty())
                                    .map(|s| s.to_string())
                                    .or_else(|| if tab.pinned || tab.essential { Some(tab.url.clone()) } else { None });
                                tab.tab_group_id = item.get("tab_group_id")
                                    .and_then(JsonVal::as_u64)
                                    .filter(|id| *id > 0 && *id <= 9_007_199_254_740_991);
                                if let Some(group_id) = tab.tab_group_id {
                                    self.next_tab_group_id = self.next_tab_group_id.max(group_id.saturating_add(1));
                                }
                            }
                        }
                    }
                }
                if let Some(folders_arr) = val.get("tab_folders").and_then(|v| v.as_arr()) {
                    for item in folders_arr {
                        if let (Some(id), Some(name), Some(ws_id)) = (
                            item.get("id").and_then(JsonVal::as_u64),
                            item.get("name").and_then(|v| v.as_str()),
                            item.get("workspace_id").and_then(|v| v.as_str()),
                        ) {
                            let collapsed = item.get("collapsed").and_then(|v| v.as_bool()).unwrap_or(false);
                            self.tab_folders.push(TabFolder {
                                id,
                                name: name.to_string(),
                                workspace_id: ws_id.to_string(),
                                collapsed,
                            });
                            self.next_tab_group_id = self.next_tab_group_id.max(id.saturating_add(1));
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
                            self.add_tab_internal(&u, false, false, None, None, false, None);
                        }
                    }
                }
            }
        }
        if self.tabs.is_empty() {
            self.add_tab_internal("tp://newtab", false, false, None, None, false, None);
        }
        if let Some(idx) = saved_active_idx {
            if idx < self.tabs.len() {
                self.active_idx = idx;
            }
        }
        if self.tabs.get(self.active_idx).map(|tab| tab.workspace_id.as_str())
            != Some(self.active_workspace_id.as_str())
        {
            if let Some(visible_idx) = self.tabs.iter().position(|tab| {
                tab.workspace_id == self.active_workspace_id
            }) {
                self.active_idx = visible_idx;
            } else {
                self.add_tab_internal("tp://newtab", false, false, None, None, false, None);
                self.active_idx = self.tabs.len().saturating_sub(1);
            }
        }
        self.relayout();
        self.sync_all_ui();
        self.save_session();
    }

    fn shortcut_from_key_input(key: KeyCode, modifiers: ModifiersState) -> Option<String> {
        let debug = format!("{:?}", key);
        let primary = if let Some(letter) = debug.strip_prefix("Key") {
            (letter.len() == 1 && letter.as_bytes()[0].is_ascii_alphabetic()).then(|| letter.to_ascii_uppercase())
        } else if let Some(digit) = debug.strip_prefix("Digit") {
            (digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit()).then(|| digit.to_string())
        } else {
            match debug.as_str() {
                "Comma" | "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" => Some(debug),
                _ => None,
            }
        }?;
        let mut parts = Vec::new();
        if modifiers.control_key() { parts.push("Ctrl"); }
        if modifiers.alt_key() { parts.push("Alt"); }
        if modifiers.shift_key() { parts.push("Shift"); }
        if parts.is_empty() { return None; }
        parts.push(&primary);
        Some(parts.join("+"))
    }

    fn default_custom_shortcuts() -> BTreeMap<String, String> {
        CUSTOM_SHORTCUT_DEFAULTS.iter().map(|(id, shortcut)| ((*id).to_string(), (*shortcut).to_string())).collect()
    }

    fn custom_shortcuts_json(&self) -> String {
        let entries = self.custom_shortcuts.iter()
            .map(|(id, shortcut)| format!("\"{}\":\"{}\"", Self::escape_json(id), Self::escape_json(shortcut)))
            .collect::<Vec<_>>().join(",");
        format!("{{{}}}", entries)
    }

    fn normalize_custom_shortcut(raw: &str) -> Option<String> {
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut primary = None;
        for part in raw.split('+').map(str::trim).filter(|part| !part.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "alt" => alt = true,
                "shift" => shift = true,
                key if primary.is_none() => primary = Some(match key {
                    "comma" | "," => "Comma".to_string(),
                    "arrowleft" | "left" => "ArrowLeft".to_string(),
                    "arrowright" | "right" => "ArrowRight".to_string(),
                    "arrowup" | "up" => "ArrowUp".to_string(),
                    "arrowdown" | "down" => "ArrowDown".to_string(),
                    value if value.len() == 1 && value.as_bytes()[0].is_ascii_alphanumeric() => value.to_ascii_uppercase(),
                    _ => return None,
                }),
                _ => return None,
            }
        }
        let key = primary?;
        if !ctrl && !alt && !shift { return None; }
        let mut parts = Vec::new();
        if ctrl { parts.push("Ctrl"); }
        if alt { parts.push("Alt"); }
        if shift { parts.push("Shift"); }
        parts.push(&key);
        Some(parts.join("+"))
    }

    fn reserved_browser_shortcut(shortcut: &str) -> bool {
        matches!(shortcut,
            "Ctrl+T" | "Ctrl+W" | "Ctrl+F4" | "Ctrl+Shift+T" | "Ctrl+Tab" | "Ctrl+Shift+Tab" |
            "Ctrl+R" | "Ctrl+Shift+R" | "Ctrl+L" | "Ctrl+D" | "Ctrl+F" | "Ctrl+H" | "Ctrl+P" |
            "Ctrl+Equal" | "Ctrl+Minus" | "Ctrl+0" | "Alt+ArrowLeft" | "Alt+ArrowRight" | "Alt+D")
    }

    fn shortcut_command_for(&self, shortcut: &str) -> Option<CustomShortcutCommand> {
        Self::shortcut_command_for_map(&self.custom_shortcuts, shortcut)
    }

    fn shortcut_command_for_map(shortcuts: &BTreeMap<String, String>, shortcut: &str) -> Option<CustomShortcutCommand> {
        let mut matches = Vec::new();
        for (id, value) in shortcuts {
            let Some(value) = Self::normalize_custom_shortcut(value) else { continue; };
            if Self::reserved_browser_shortcut(&value) || value != shortcut { continue; }
            let command = match id.as_str() {
                "compact_mode" => CustomShortcutCommand::CompactMode,
                "command_bar" => CustomShortcutCommand::CommandBar,
                "settings" => CustomShortcutCommand::Settings,
                "glance" => CustomShortcutCommand::Glance,
                "split_view" => CustomShortcutCommand::SplitView,
                "prev_workspace" => CustomShortcutCommand::PrevWorkspace,
                "next_workspace" => CustomShortcutCommand::NextWorkspace,
                "copy_url" => CustomShortcutCommand::CopyUrl,
                "ai_panel" => CustomShortcutCommand::AiPanel,
                "sidebar_collapse" => CustomShortcutCommand::SidebarCollapse,
                _ => continue,
            };
            matches.push(command);
        }
        if matches.len() == 1 { matches.pop() } else { None }
    }

    fn run_custom_shortcut(&mut self, command: CustomShortcutCommand) {
        match command {
            CustomShortcutCommand::CompactMode => self.execute(AppCmd::ToggleCompactMode),
            CustomShortcutCommand::CommandBar => self.execute(AppCmd::OpenCommandBar),
            CustomShortcutCommand::Settings => self.execute(AppCmd::ToggleSettingsModal),
            CustomShortcutCommand::Glance => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    self.execute(AppCmd::OpenGlance { url: tab.url.clone(), title: tab.title.clone() });
                }
            }
            CustomShortcutCommand::SplitView => self.execute(AppCmd::CycleViewMode),
            CustomShortcutCommand::PrevWorkspace => self.execute(AppCmd::PrevWorkspace),
            CustomShortcutCommand::NextWorkspace => self.execute(AppCmd::NextWorkspace),
            CustomShortcutCommand::CopyUrl => self.execute(AppCmd::CopyActiveUrl),
            CustomShortcutCommand::AiPanel => self.execute(AppCmd::ToggleChatChannel(ChatChannel::Ai)),
            CustomShortcutCommand::SidebarCollapse => self.execute(AppCmd::ToggleSidebarCollapse),
        }
    }

    fn parse_custom_shortcuts(value: &JsonVal) -> BTreeMap<String, String> {
        let mut shortcuts = Self::default_custom_shortcuts();
        let JsonVal::Obj(items) = value else { return shortcuts; };
        for (id, raw) in items {
            if !CUSTOM_SHORTCUT_DEFAULTS.iter().any(|(allowed, _)| *allowed == id) { continue; }
            let Some(raw) = raw.as_str() else { continue; };
            if raw.trim().is_empty() {
                shortcuts.insert(id.clone(), String::new());
                continue;
            }
            let Some(value) = Self::normalize_custom_shortcut(raw) else { continue; };
            if Self::reserved_browser_shortcut(&value) { continue; }
            shortcuts.insert(id.clone(), value);
        }
        shortcuts
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

    fn persisted_window_size(json: &str) -> (u32, u32) {
        let width = Self::extract_u64(json, "\"window_width\":")
            .map(|value| (value as u32).clamp(860, 3840))
            .unwrap_or(1480);
        let height = Self::extract_u64(json, "\"window_height\":")
            .map(|value| (value as u32).clamp(560, 2160))
            .unwrap_or(900);
        (width, height)
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

    fn valid_tab_id(tab_id: u64) -> bool {
        (1..=9_007_199_254_740_991).contains(&tab_id)
    }

    fn handle_ipc(proxy: &WindowProxy, body: &str, sender_tab_id: Option<u64>) {
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
        if body.contains("\"cmd\":\"new_window\"") {
            proxy.send_event(AppCmd::NewWindow);
        } else if body.contains("\"cmd\":\"detach_tab_to_new_window\"") {
            if let Some(tab_id) = Self::extract_u64(body, "\"tab_id\":").filter(|id| Self::valid_tab_id(*id)) {
                let intent = if parse_json(body).ok().and_then(|value| value.get("intent").and_then(JsonVal::as_str).map(str::to_owned)).as_deref() == Some("drag_out") {
                    TabDetachIntent::DragOut
                } else {
                    // The menu never supplies coordinates, so its explicit request
                    // remains valid even when the pointer is over source content.
                    TabDetachIntent::ContextMenu
                };
                // Screen coordinates from the WebView are advisory only: browser
                // drag events can be DPI-transformed. Prefer native physical pixels.
                #[cfg(target_os = "windows")]
                let native_cursor = unsafe {
                    let mut point = WinPoint { x: 0, y: 0 };
                    (GetCursorPos(&mut point) != 0).then_some(point)
                };
                #[cfg(not(target_os = "windows"))]
                let native_cursor = None;
                let js_cursor = parse_json(body).ok().and_then(|value| {
                    let x = value.get("screen_x")?.as_i64()?.try_into().ok()?;
                    let y = value.get("screen_y")?.as_i64()?.try_into().ok()?;
                    Some(WinPoint { x, y })
                });
                proxy.send_event(AppCmd::DetachTabToNewWindow {
                    tab_id,
                    intent,
                    fallback_cursor: native_cursor.or(js_cursor),
                });
            }
        } else if body.contains("\"cmd\":\"toggle_bookmarks_bar\"") {
            let _ = proxy.send_event(AppCmd::ToggleBookmarksBar);
        } else if body.contains("\"cmd\":\"toggle_tab_pinned\"") {
            let _ = proxy.send_event(AppCmd::ToggleTabPinned);
        } else if body.contains("\"cmd\":\"toggle_tab_orientation\"") {
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
        } else if body.contains("\"cmd\":\"set_split_mode\"") {
            if let Ok(val) = parse_json(body) {
                let mode_str = val.get("mode").and_then(|v| v.as_str()).unwrap_or("single");
                let pair_id = val.get("pair_tab_id").and_then(|v| v.as_u64());
                let mode = ContentViewMode::from_str(mode_str);
                let _ = proxy.send_event(AppCmd::SetSplitMode {
                    mode,
                    pair_tab_id: pair_id,
                });
            }
        } else if body.contains("\"cmd\":\"swap_split_tabs\"") {
            let _ = proxy.send_event(AppCmd::SwapSplitTabs);
        } else if body.contains("\"cmd\":\"unsplit\"") {
            let _ = proxy.send_event(AppCmd::Unsplit);
        } else if body.contains("\"cmd\":\"set_tab_selection\"") {
            if let Ok(val) = parse_json(body) {
                if let Some(tab_id) = val.get("tab_id").and_then(JsonVal::as_u64).filter(|id| Self::valid_tab_id(*id)) {
                    let mode = val.get("mode").and_then(JsonVal::as_str).unwrap_or("replace").to_string();
                    let _ = proxy.send_event(AppCmd::SetTabSelection { tab_id, mode });
                }
            }
        } else if body.contains("\"cmd\":\"group_tabs\"") {
            if let Ok(val) = parse_json(body) {
                if let (Some(source_id), Some(target_id)) = (
                    val.get("source_id").and_then(JsonVal::as_u64).filter(|id| Self::valid_tab_id(*id)),
                    val.get("target_id").and_then(JsonVal::as_u64).filter(|id| Self::valid_tab_id(*id)),
                ) {
                    let _ = proxy.send_event(AppCmd::GroupTabs { source_id, target_id });
                }
            }
        } else if body.contains("\"cmd\":\"group_selected_tabs\"") {
            let _ = proxy.send_event(AppCmd::GroupTabs { source_id: 0, target_id: 0 });
        } else if body.contains("\"cmd\":\"split_tab_group\"") {
            if let Ok(val) = parse_json(body) {
                let tab_ids = val.get("tab_ids").and_then(JsonVal::as_arr).map(|items| {
                    items.iter().filter_map(JsonVal::as_u64).filter(|id| Self::valid_tab_id(*id)).collect()
                }).unwrap_or_default();
                let _ = proxy.send_event(AppCmd::SplitTabGroup { tab_ids });
            }
        } else if body.contains("\"cmd\":\"split_selected_tabs\"") {
            let _ = proxy.send_event(AppCmd::SplitTabGroup { tab_ids: Vec::new() });
        } else if body.contains("\"cmd\":\"scan_selected_tabs\"") {
            let _ = proxy.send_event(AppCmd::ScanSelectedTabs);
        } else if body.contains("\"cmd\":\"copy_to_clipboard\"") {
            if let Some(text) = Self::extract_field_str(body, "text") {
                let _ = proxy.send_event(AppCmd::CopyToClipboard(text));
            }
        } else if body.contains("\"cmd\":\"unload_tab\"") {
            if let Some(tid) = Self::extract_u64(body, "\"tab_id\":").filter(|id| Self::valid_tab_id(*id)) {
                let _ = proxy.send_event(AppCmd::UnloadTab(tid));
            }
        } else if body.contains("\"cmd\":\"glance_open_in_split\"") {
            let _ = proxy.send_event(AppCmd::GlanceOpenInSplit);
        } else if body.contains("\"cmd\":\"glance_nav_back\"") {
            let _ = proxy.send_event(AppCmd::GlanceNavBack);
        } else if body.contains("\"cmd\":\"glance_nav_forward\"") {
            let _ = proxy.send_event(AppCmd::GlanceNavForward);
        } else if body.contains("\"cmd\":\"glance_reload\"") {
            let _ = proxy.send_event(AppCmd::GlanceReload);
        } else if body.contains("\"cmd\":\"glance_copy_url\"") {
            let _ = proxy.send_event(AppCmd::GlanceCopyUrl);
        } else if body.contains("\"cmd\":\"tab_media_state\"") {
            if let Ok(val) = parse_json(body) {
                let playing = val.get("playing").and_then(JsonVal::as_bool).unwrap_or(false);
                let muted = val.get("muted").and_then(JsonVal::as_bool).unwrap_or(false);
                let title = val.get("title").and_then(JsonVal::as_str).unwrap_or("").to_string();
                let artist = val.get("artist").and_then(JsonVal::as_str).unwrap_or("").to_string();
                let bound_tab_id = sender_tab_id.unwrap_or(0);
                if Self::valid_tab_id(bound_tab_id) {
                    let _ = proxy.send_event(AppCmd::UpdateTabMedia { tab_id: bound_tab_id, playing, muted, title, artist });
                }
            }
        } else if body.contains("\"cmd\":\"toggle_tab_mute\"") {
            if let Some(tid) = Self::extract_u64(body, "\"tab_id\":").filter(|id| Self::valid_tab_id(*id)) {
                let _ = proxy.send_event(AppCmd::ToggleTabMute(tid));
            }
        } else if body.contains("\"cmd\":\"media_control\"") {
            if let Ok(val) = parse_json(body) {
                if let (Some(tab_id), Some(action)) = (
                    val.get("tab_id").and_then(JsonVal::as_u64).filter(|id| Self::valid_tab_id(*id)),
                    val.get("action").and_then(JsonVal::as_str),
                ) {
                    let _ = proxy.send_event(AppCmd::MediaControl { tab_id, action: action.to_string() });
                }
            }
        } else if body.contains("\"cmd\":\"open_command_bar\"") {
            let _ = proxy.send_event(AppCmd::OpenCommandBar);
        } else if body.contains("\"cmd\":\"close_command_bar\"") {
            let _ = proxy.send_event(AppCmd::CloseCommandBar);
        } else if body.contains("\"cmd\":\"copy_active_url\"") {
            let _ = proxy.send_event(AppCmd::CopyActiveUrl);
        } else if body.contains("\"cmd\":\"duplicate_tab\"") {
            if let Some(tid) = Self::extract_u64(body, "\"tab_id\":").filter(|id| Self::valid_tab_id(*id)) {
                let _ = proxy.send_event(AppCmd::DuplicateTab(tid));
            }
        } else if body.contains("\"cmd\":\"pin_tab_by_id\"") {
            if let Some(tid) = Self::extract_u64(body, "\"tab_id\":").filter(|id| Self::valid_tab_id(*id)) {
                let _ = proxy.send_event(AppCmd::PinTabById(tid));
            }
        } else if body.contains("\"cmd\":\"close_other_tabs\"") {
            if let Some(tid) = Self::extract_u64(body, "\"tab_id\":").filter(|id| Self::valid_tab_id(*id)) {
                let _ = proxy.send_event(AppCmd::CloseOtherTabs(tid));
            }
        } else if body.contains("\"cmd\":\"move_tab_to_workspace\"") {
            if let Ok(val) = parse_json(body) {
                if let (Some(tab_id), Some(ws_id)) = (
                    val.get("tab_id").and_then(JsonVal::as_u64).filter(|id| Self::valid_tab_id(*id)),
                    val.get("workspace_id").and_then(JsonVal::as_str),
                ) {
                    let _ = proxy.send_event(AppCmd::MoveTabToWorkspace {
                        tab_id,
                        workspace_id: ws_id.to_string(),
                    });
                }
            }
        } else if body.contains("\"cmd\":\"reorder_tab\"") {
            if let Ok(val) = parse_json(body) {
                if let (Some(source_id), Some(target_id)) = (
                    val.get("source_id").and_then(JsonVal::as_u64).filter(|id| Self::valid_tab_id(*id)),
                    val.get("target_id").and_then(JsonVal::as_u64).filter(|id| Self::valid_tab_id(*id)),
                ) {
                    let after = val.get("after").and_then(JsonVal::as_bool).unwrap_or(false);
                    let _ = proxy.send_event(AppCmd::ReorderTab { source_id, target_id, after });
                }
            }
        } else if body.contains("\"cmd\":\"open_in_split\"") {
            if let Some(url) = Self::extract_field_str(body, "url") {
                if !url.trim().is_empty() {
                    let _ = proxy.send_event(AppCmd::OpenInSplit(url));
                }
            }
        } else if body.contains("\"cmd\":\"adjust_split_ratio\"") {
            if let Ok(val) = parse_json(body) {
                if let Some(JsonVal::Num(delta)) = val.get("delta") {
                    let _ = proxy.send_event(AppCmd::AdjustSplitRatio(*delta as f32));
                }
            }
        } else if body.contains("\"cmd\":\"reset_split_ratio\"") {
            let _ = proxy.send_event(AppCmd::ResetSplitRatio);
        } else if body.contains("\"cmd\":\"toggle_compact_mode\"") {
            let _ = proxy.send_event(AppCmd::ToggleCompactMode);
        } else if body.contains("\"cmd\":\"set_compact_sidebar_hover\"") {
            let hover = body.contains("\"hover\":true");
            let _ = proxy.send_event(AppCmd::SetCompactSidebarHover(hover));
        } else if body.contains("\"cmd\":\"toggle_tab_essential\"") {
            if let Some(tid) = Self::extract_u64(body, "\"tab_id\":").filter(|id| Self::valid_tab_id(*id)) {
                let _ = proxy.send_event(AppCmd::ToggleTabEssential(tid));
            }
        } else if body.contains("\"cmd\":\"reset_pinned_tab\"") {
            if let Some(tid) = Self::extract_u64(body, "\"tab_id\":").filter(|id| Self::valid_tab_id(*id)) {
                let _ = proxy.send_event(AppCmd::ResetPinnedTab(tid));
            }
        } else if body.contains("\"cmd\":\"toggle_tab_folder_collapsed\"") {
            if let Some(fid) = Self::extract_u64(body, "\"folder_id\":") {
                let _ = proxy.send_event(AppCmd::ToggleTabFolderCollapsed(fid));
            }
        } else if body.contains("\"cmd\":\"rename_tab_folder\"") {
            if let Ok(val) = parse_json(body) {
                if let (Some(folder_id), Some(name)) = (
                    val.get("folder_id").and_then(JsonVal::as_u64),
                    val.get("name").and_then(JsonVal::as_str),
                ) {
                    let _ = proxy.send_event(AppCmd::RenameTabFolder { folder_id, name: name.to_string() });
                }
            }
        } else if body.contains("\"cmd\":\"add_tab_to_folder\"") {
            if let Ok(val) = parse_json(body) {
                if let (Some(tab_id), Some(folder_id)) = (
                    val.get("tab_id").and_then(JsonVal::as_u64).filter(|id| Self::valid_tab_id(*id)),
                    val.get("folder_id").and_then(JsonVal::as_u64),
                ) {
                    let _ = proxy.send_event(AppCmd::AddTabToFolder { tab_id, folder_id });
                }
            }
        } else if body.contains("\"cmd\":\"update_workspace_style\"") {
            if let Ok(val) = parse_json(body) {
                if let Some(id) = val.get("id").and_then(JsonVal::as_str) {
                    let icon = val.get("icon").and_then(JsonVal::as_str).unwrap_or("✦").to_string();
                    let accent = val.get("accent").and_then(JsonVal::as_str).unwrap_or("").to_string();
                    let container_id = val.get("container_id").and_then(JsonVal::as_str).unwrap_or("default").to_string();
                    let _ = proxy.send_event(AppCmd::UpdateWorkspaceStyle {
                        id: id.to_string(),
                        icon,
                        accent,
                        container_id,
                    });
                }
            }
        } else if body.contains("\"cmd\":\"next_workspace\"") {
            let _ = proxy.send_event(AppCmd::NextWorkspace);
        } else if body.contains("\"cmd\":\"prev_workspace\"") {
            let _ = proxy.send_event(AppCmd::PrevWorkspace);
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
                let url = val.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let title = val.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let favicon = val.get("favicon").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let _ = proxy.send_event(AppCmd::UpdateTabMeta {
                    tab_id: sender_tab_id.unwrap_or(0),
                    url,
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
        } else if body.contains("\"cmd\":\"open_glance\"") {
            let url = Self::extract_field_str(body, "url").unwrap_or_default();
            let title = Self::extract_field_str(body, "title").unwrap_or_default();
            if !url.trim().is_empty() {
                let _ = proxy.send_event(AppCmd::OpenGlance { url, title });
            }
        } else if body.contains("\"cmd\":\"close_glance\"") {
            let _ = proxy.send_event(AppCmd::CloseGlance);
        } else if body.contains("\"cmd\":\"glance_open_in_tab\"") {
            let _ = proxy.send_event(AppCmd::GlanceOpenInTab);
        } else if body.contains("\"cmd\":\"check_app_update\"") {
            let _ = proxy.send_event(AppCmd::CheckAppUpdate);
        } else if body.contains("\"cmd\":\"run_app_update\"") {
            let _ = proxy.send_event(AppCmd::RunAppUpdate);
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
            let container_id = Self::extract_string(body, "\"container_id\":\"");
            let _ = proxy.send_event(AppCmd::NewTab { url, isolated, container_id });
        } else if body.contains("\"cmd\":\"new_private_tab\"") {
            let url = Self::extract_string(body, "\"url\":\"").unwrap_or_else(|| "tp://newtab".to_string());
            let _ = proxy.send_event(AppCmd::NewPrivateTab(url));
        } else if body.contains("\"cmd\":\"print_active\"") {
            let _ = proxy.send_event(AppCmd::PrintActive);
        } else if body.contains("\"cmd\":\"open_devtools\"") {
            let _ = proxy.send_event(AppCmd::OpenDevTools);
        } else if body.contains("\"cmd\":\"reader_mode\"") {
            let _ = proxy.send_event(AppCmd::ToggleReaderMode);
        } else if body.contains("\"cmd\":\"focus_visible_index\"") {
            if let Some(idx) = Self::extract_u64(body, "\"index\":") {
                let _ = proxy.send_event(AppCmd::FocusVisibleIndex(idx as usize));
            }
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
            let _ = proxy.send_event(AppCmd::FocusContentTab(tab_id));
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
        } else if body.contains("\"cmd\":\"clear_site_data\"") {
            let _ = proxy.send_event(AppCmd::ClearSiteData);
        } else if body.contains("\"cmd\":\"set_active_workspace\"") {
            if let Ok(val) = parse_json(body) {
                if let Some(id) = val.get("workspace_id").and_then(|v| v.as_str()) {
                    let _ = proxy.send_event(AppCmd::SetActiveWorkspace(id.to_string()));
                }
            }
        } else if body.contains("\"cmd\":\"create_workspace\"") {
            if let Ok(val) = parse_json(body) {
                if let Some(name) = val.get("name").and_then(|v| v.as_str()) {
                    let _ = proxy.send_event(AppCmd::CreateWorkspace(name.to_string()));
                }
            }
        } else if body.contains("\"cmd\":\"rename_workspace\"") {
            if let Ok(val) = parse_json(body) {
                if let (Some(id), Some(name)) = (
                    val.get("workspace_id").and_then(|v| v.as_str()),
                    val.get("name").and_then(|v| v.as_str()),
                ) {
                    let _ = proxy.send_event(AppCmd::RenameWorkspace { id: id.to_string(), name: name.to_string() });
                }
            }
        } else if body.contains("\"cmd\":\"delete_workspace\"") {
            if let Ok(val) = parse_json(body) {
                if let Some(id) = val.get("workspace_id").and_then(|v| v.as_str()) {
                    let _ = proxy.send_event(AppCmd::DeleteWorkspace(id.to_string()));
                }
            }

        } else if body.contains("\"cmd\":\"copy_report\"") {
            if let Some(text) = Self::extract_field_str(body, "text") {
                let _ = proxy.send_event(AppCmd::CopyToClipboard(text));
            }
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
            if let Ok(val) = parse_json(body) {
                if let Some(bm_val) = val.get("bookmarks_json") {
                    if let Some(bm_str) = bm_val.as_str() {
                        let _ = proxy.send_event(AppCmd::SaveBookmarks(bm_str.to_string()));
                    } else if matches!(bm_val, JsonVal::Arr(_)) {
                        let _ = proxy.send_event(AppCmd::SaveBookmarks(bm_val.to_json()));
                    }
                }
            } else if let Some(bm) = Self::extract_string(body, "\"bookmarks_json\":\"") {
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
        } else if body.contains("\"cmd\":\"open_titlebar_popover\"") {
            let _ = proxy.send_event(AppCmd::OpenTitlebarPopover);
        } else if body.contains("\"cmd\":\"close_titlebar_popover\"") {
            let _ = proxy.send_event(AppCmd::CloseTitlebarPopover);
        } else if body.contains("\"cmd\":\"translate_req\"") {
            if let Ok(val) = parse_json(body) {
                let req_id = val.get("req_id").and_then(|v| v.as_u64()).unwrap_or(0);
                let target_lang = val.get("target_lang").and_then(|v| v.as_str()).unwrap_or("vi").to_string();
                let texts: Vec<String> = val.get("texts").and_then(|v| v.as_arr()).map(|arr| {
                    arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()
                }).unwrap_or_default();
                let _ = proxy.send_event(AppCmd::TranslateReq {
                    tab_id: sender_tab_id.unwrap_or(0),
                    req_id,
                    target_lang,
                    texts,
                });
            }
        } else if body.contains("\"cmd\":\"translate_res\"") {
            if let Ok(val) = parse_json(body) {
                let tab_id = val.get("tab_id").and_then(|v| v.as_u64()).unwrap_or(0);
                let req_id = val.get("req_id").and_then(|v| v.as_u64()).unwrap_or(0);
                let translations: Vec<String> = val.get("translations").and_then(|v| v.as_arr()).map(|arr| {
                    arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()
                }).unwrap_or_default();
                let _ = proxy.send_event(AppCmd::TranslateRes {
                    tab_id,
                    req_id,
                    translations,
                });
            }
        } else if body.contains("\"cmd\":\"save_domain_boost\"") {
            if let Ok(val) = parse_json(body) {
                let domain = val.get("domain").and_then(JsonVal::as_str).unwrap_or("").to_string();
                let css = val.get("css").and_then(JsonVal::as_str).unwrap_or("").to_string();
                let enabled = val.get("enabled").and_then(JsonVal::as_bool).unwrap_or(true);
                let zap_selectors: Vec<String> = val
                    .get("zap_selectors")
                    .and_then(JsonVal::as_arr)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(ToString::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                let _ = proxy.send_event(AppCmd::SaveDomainBoost {
                    domain,
                    css,
                    zap_selectors,
                    enabled,
                });
            }
        } else if body.contains("\"cmd\":\"reset_domain_boost\"") {
            if let Ok(val) = parse_json(body) {
                let domain = val.get("domain").and_then(JsonVal::as_str).unwrap_or("").to_string();
                let _ = proxy.send_event(AppCmd::ResetDomainBoost(domain));
            }
        } else if body.contains("\"cmd\":\"open_boosts_panel\"") {
            let _ = proxy.send_event(AppCmd::OpenBoostsPanel);
        } else if body.contains("\"cmd\":\"start_zap_mode\"") {
            let _ = proxy.send_event(AppCmd::StartZapMode);
        } else if body.contains("\"cmd\":\"open_downloaded_file\"") {
            if let Ok(val) = parse_json(body) {
                let path = val.get("path").and_then(JsonVal::as_str).unwrap_or("").to_string();
                let _ = proxy.send_event(AppCmd::OpenDownloadedFile(path));
            }
        } else if body.contains("\"cmd\":\"open_download_folder\"") {
            if let Ok(val) = parse_json(body) {
                let path = val.get("path").and_then(JsonVal::as_str).unwrap_or("").to_string();
                let _ = proxy.send_event(AppCmd::OpenDownloadFolder(path));
            }
        } else if body.contains("\"cmd\":\"recover_windows\"") {
            let _ = proxy.send_event(AppCmd::RecoverWindows);
        } else if body.contains("\"cmd\":\"dismiss_window_recovery\"") {
            let _ = proxy.send_event(AppCmd::DismissWindowRecovery);
        } else if body.contains("\"cmd\":\"sync_window_geometry\"") {
            let _ = proxy.send_event(AppCmd::SyncWindowGeometry);
        } else if body.contains("\"cmd\":\"step_mru_tab\"") {
            let delta = parse_json(body)
                .ok()
                .and_then(|v| v.get("delta").and_then(JsonVal::as_i64))
                .map(|n| if n < 0 { -1 } else { 1 })
                .unwrap_or(1);
            let _ = proxy.send_event(AppCmd::StepMruTab(delta));
        } else if body.contains("\"cmd\":\"commit_mru_tab\"") {
            let tid = Self::extract_u64(body, "\"tab_id\":").filter(|id| Self::valid_tab_id(*id));
            let _ = proxy.send_event(AppCmd::CommitMruTab(tid));
        } else if body.contains("\"cmd\":\"cancel_mru_tab\"") {
            let _ = proxy.send_event(AppCmd::CancelMruTab);
        } else if body.contains("\"cmd\":\"open_extension\"") {
            let id = Self::extract_field_str(body, "id").unwrap_or_else(|| "ublock".to_string());
            let page = Self::extract_field_str(body, "page").filter(|s| !s.trim().is_empty());
            let _ = proxy.send_event(AppCmd::OpenExtensionPopup { id, page });
        }
    }

    fn log_perf(action: &str, elapsed_ms: f64) {
        if elapsed_ms < 5.0 {
            return;
        }
        let line = format!("[PERF] {:<25} | {:>7.2} ms\n", action, elapsed_ms);
        println!("{}", line.trim());
        if env::var("TP_DEBUG_PERF").is_ok() {
            if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open("perf_latency.log") {
                let _ = f.write_all(line.as_bytes());
            }
        }
    }

    fn execute(&mut self, cmd: AppCmd) {
        match cmd {
            AppCmd::ToggleTabPinned => {
                if let Some(tab) = self.tabs.get_mut(self.active_idx) {
                    if tab.worker_kind.is_none() {
                        tab.pinned = !tab.pinned;
                        if tab.pinned {
                            tab.essential = false;
                            tab.pinned_origin_url = Some(tab.url.clone());
                        }
                        self.sync_all_ui();
                        self.save_session();
                    }
                }
            }
            AppCmd::OpenCommandBar => {
                if self.mru_hud_open {
                    self.mru_hud_open = false;
                    self.mru_cycle_ids = None;
                    let _ = self.tab_shell.evaluate_script(
                        "if (window.__tpCloseMruSwitcher) window.__tpCloseMruSwitcher();",
                    );
                }
                self.command_bar_open = true;
                self.relayout();
                self.sync_all_ui();
                let _ = self.tab_shell.focus();
                let _ = self.tab_shell.evaluate_script(
                    "if (window.__tpOpenCommandBar) window.__tpOpenCommandBar();",
                );
            }
            AppCmd::ToggleBookmarksBar => {
                self.show_bookmarks_bar = !self.show_bookmarks_bar;
                self.relayout();
                self.sync_all_ui();
                self.save_config();
                self.broadcast_config_changed();
            }
            AppCmd::ToggleTabOrientation => {
                self.tab_orientation = self.tab_orientation.toggle();
                self.command_bar_open = false;
                self.mru_hud_open = false;
                self.mru_cycle_ids = None;
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
                if self.background_mode {
                    self.window.set_visible(false);
                    let _ = self.window.set_skip_taskbar(true);
                } else {
                    self.remove_system_tray();
                    self.should_exit = true;
                }
            }
            // Window creation and detach are consumed by the managed registry in the event loop.
            AppCmd::NewWindow | AppCmd::DetachTabToNewWindow { .. } => {}
            AppCmd::QuitApp => {
                self.save_config();
                self.save_skills();
                self.save_session();
                self.remove_system_tray();
                self.should_exit = true;
            }
            AppCmd::WakeAndShowWindow => {
                self.wake_and_show_window();
            }
            AppCmd::CycleViewMode => {
                self.view_mode = self.view_mode.next();
                if self.view_mode != ContentViewMode::SingleTab && self.split_paired_tab_id.is_none() && self.tabs.iter().filter(|tab| tab.workspace_id == self.active_workspace_id).count() > 1 {
                    let visible_indices = self.visible_tab_indices();
                                        if visible_indices.len() > 1 {
                                                            let active_pos = visible_indices
                                                                .iter()
                                                                .position(|idx| *idx == self.active_idx)
                                                                .unwrap_or(0);
                                                            let next_pos = if active_pos + 1 < visible_indices.len() {
                                                                active_pos + 1
                                                            } else {
                                                                active_pos.saturating_sub(1)
                                                            };
                                                            if let Some(next_idx) = visible_indices.get(next_pos) {
                                                                self.split_paired_tab_id = Some(self.tabs[*next_idx].id);
                                                            }
                                                        }
                }
                self.relayout();
                self.sync_all_ui();
                self.save_session();
                self.save_config();
                self.broadcast_config_changed();
            }
            AppCmd::SetSplitMode { mode, pair_tab_id } => {
                self.view_mode = mode;
                if mode == ContentViewMode::SingleTab {
                    self.split_paired_tab_id = None;
                } else if let Some(pid) = pair_tab_id {
                    self.split_paired_tab_id = Some(pid);
                } else if self.split_paired_tab_id.is_none() && self.tabs.iter().filter(|tab| tab.workspace_id == self.active_workspace_id).count() > 1 {
                    let visible_indices = self.visible_tab_indices();
                                        if visible_indices.len() > 1 {
                                                            let active_pos = visible_indices
                                                                .iter()
                                                                .position(|idx| *idx == self.active_idx)
                                                                .unwrap_or(0);
                                                            let next_pos = if active_pos + 1 < visible_indices.len() {
                                                                active_pos + 1
                                                            } else {
                                                                active_pos.saturating_sub(1)
                                                            };
                                                            if let Some(next_idx) = visible_indices.get(next_pos) {
                                                                self.split_paired_tab_id = Some(self.tabs[*next_idx].id);
                                                            }
                                                        }
                }
                self.relayout();
                self.sync_all_ui();
                self.save_session();
                self.save_config();
                self.broadcast_config_changed();
            }
            AppCmd::SwapSplitTabs => {
                if let Some(pid) = self.split_paired_tab_id {
                    if let Some(p_idx) = self.tabs.iter().position(|t| t.id == pid) {
                        let active_id = self.tabs.get(self.active_idx).map(|t| t.id);
                        self.active_idx = p_idx;
                        self.split_paired_tab_id = active_id;
                        self.relayout();
                        self.sync_all_ui();
                    }
                }
            }
            AppCmd::Unsplit => {
                self.view_mode = ContentViewMode::SingleTab;
                self.split_paired_tab_id = None;
                self.relayout();
                self.sync_all_ui();
                self.save_session();
                self.save_config();
                self.broadcast_config_changed();
            }
            AppCmd::SetTabSelection { tab_id, mode } => {
                self.prune_tab_selection();
                let visible_user_tab_indices = self.visible_user_tab_indices();
                let Some(clicked_pos) = visible_user_tab_indices.iter().position(|idx| self.tabs[*idx].id == tab_id) else { return; };
                match mode.as_str() {
                    "toggle" => {
                        if !self.selected_tab_ids.insert(tab_id) {
                            self.selected_tab_ids.remove(&tab_id);
                            if self.tab_selection_anchor_id == Some(tab_id) {
                                self.tab_selection_anchor_id = None;
                            }
                        } else {
                            self.tab_selection_anchor_id = Some(tab_id);
                        }
                    }
                    "range" => {
                        let anchor_pos = self.tab_selection_anchor_id.and_then(|anchor_id| {
                            visible_user_tab_indices.iter().position(|idx| self.tabs[*idx].id == anchor_id)
                        });
                        self.selected_tab_ids.clear();
                        if let Some(anchor_pos) = anchor_pos {
                            for idx in &visible_user_tab_indices[anchor_pos.min(clicked_pos)..=anchor_pos.max(clicked_pos)] {
                                self.selected_tab_ids.insert(self.tabs[*idx].id);
                            }
                        } else {
                            self.selected_tab_ids.insert(tab_id);
                            self.tab_selection_anchor_id = Some(tab_id);
                        }
                    }
                    _ => {
                        self.selected_tab_ids.clear();
                        self.selected_tab_ids.insert(tab_id);
                        self.tab_selection_anchor_id = Some(tab_id);
                    }
                }
                self.sync_all_ui();
            }
            AppCmd::GroupTabs { source_id, target_id } => {
                self.prune_tab_selection();
                let active_ids = self.active_user_tab_ids();
                let mut ids: HashSet<u64> = if source_id == 0 && target_id == 0 {
                    self.selected_active_user_tab_ids()
                } else if Self::valid_tab_id(source_id) && Self::valid_tab_id(target_id) {
                    [source_id, target_id].into_iter().collect()
                } else {
                    return;
                };
                for tab_id in [source_id, target_id].into_iter().filter(|id| *id != 0) {
                    let group_id = self.tabs.iter().find(|tab| tab.id == tab_id && active_ids.contains(&tab.id)).and_then(|tab| tab.tab_group_id);
                    if let Some(group_id) = group_id {
                        ids.extend(self.tabs.iter().filter(|tab| active_ids.contains(&tab.id) && tab.tab_group_id == Some(group_id)).map(|tab| tab.id));
                    }
                }
                ids.retain(|id| active_ids.contains(id));
                if ids.len() >= 2 {
                    let group_id = self.next_tab_group_id;
                    self.next_tab_group_id = self.next_tab_group_id.saturating_add(1);
                    for tab in &mut self.tabs {
                        if ids.contains(&tab.id) {
                            tab.tab_group_id = Some(group_id);
                        }
                    }
                    self.selected_tab_ids = ids;
                    self.sync_tab_folders();
                    self.sync_all_ui();
                    self.save_session();
                }
            }
            AppCmd::SplitTabGroup { tab_ids } => {
                self.prune_tab_selection();
                let active_ids = self.active_user_tab_ids();
                let ids: HashSet<u64> = if tab_ids.is_empty() {
                    self.selected_active_user_tab_ids()
                } else {
                    tab_ids.into_iter().filter(|id| Self::valid_tab_id(*id) && active_ids.contains(id)).collect()
                };
                if ids.is_empty() {
                    return;
                }
                for tab in &mut self.tabs {
                    if ids.contains(&tab.id) {
                        tab.tab_group_id = None;
                    }
                }
                self.selected_tab_ids = ids;
                self.sync_tab_folders();
                self.sync_all_ui();
                self.save_session();
            }
            AppCmd::ScanSelectedTabs => {
                self.sync_all_ui();
            }
            AppCmd::CopyToClipboard(text) => {
                Self::copy_text_to_clipboard(&text);
            }
            AppCmd::ToggleAiMode => {
                self.ensure_optional_shells();
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
                self.ensure_optional_shells();
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
                self.ensure_optional_shells();
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
                self.ensure_optional_shells();
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
                self.ensure_optional_shells();
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
                self.ensure_optional_shells();
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
                self.ensure_optional_shells();
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
                                    if cur_x < SIDEBAR_COLLAPSE_THRESHOLD {
                                        let _ = proxy.send_event(AppCmd::ToggleCompactMode);
                                        break;
                                    }
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
                COMPACT_SIDEBAR_WIDTH.store(self.sidebar_width as u64, Ordering::Relaxed);
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
                url,
                title,
                favicon,
            } => {
                let mut changed = false;
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                    let clean_u = url.trim();
                    if !clean_u.is_empty() && (clean_u.starts_with("http://") || clean_u.starts_with("https://")) {
                        if tab.url != clean_u {
                            tab.url = clean_u.to_string();
                            changed = true;
                        }
                    }
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
                if shell == "history" {
                    self.sync_history_view();
                } else if shell == "workspace" {
                    self.sync_workspace_view();
                } else if shell == "boot_settle" {
                    self.relayout();
                    self.sync_all_ui();
                    self.schedule_prewarm_newtab(250);
                    self.refresh_browser_extensions();
                }
            }
            AppCmd::SetSettingsSection(sec) => {
                if !sec.trim().is_empty() {
                    self.settings_section = sec.trim().to_string();
                }
            }
            AppCmd::SyncSettingsModalInit => {
                if self.settings_open {
                    self.sync_settings_modal(Some(&self.settings_section));
                }
            }
            AppCmd::OpenSettingsModal(section) => {
                self.ensure_optional_shells();
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
            AppCmd::CheckAppUpdate => {
                let proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    let current_commit = Self::get_current_git_commit();
                    let (has_update, remote_commit, behind_count, message) =
                        match Self::fetch_remote_git_commit() {
                            Ok(remote_hash) if !remote_hash.starts_with(&current_commit) => (
                                true,
                                remote_hash,
                                1,
                                "Có bản cập nhật mới trên GitHub.".to_string(),
                            ),
                            Ok(remote_hash) => (
                                false,
                                remote_hash,
                                0,
                                "Mã nguồn hiện tại đã là bản mới nhất trên GitHub.".to_string(),
                            ),
                            Err(err) => (
                                false,
                                current_commit.clone(),
                                0,
                                format!("Không thể kiểm tra GitHub: {err}"),
                            ),
                        };

                    let _ = proxy.send_event(AppCmd::AppUpdateResult {
                        has_update,
                        current_commit,
                        remote_commit,
                        behind_count,
                        message,
                    });
                });
            }
            AppCmd::AppUpdateResult {
                has_update,
                current_commit,
                remote_commit,
                behind_count,
                message,
            } => {
                let json = format!(
                    "{{\"has_update\":{},\"current_commit\":\"{}\",\"remote_commit\":\"{}\",\"behind_count\":{},\"message\":\"{}\"}}",
                    has_update,
                    Self::escape_json(&current_commit),
                    Self::escape_json(&remote_commit),
                    behind_count,
                    Self::escape_json(&message)
                );
                let js = format!("if (window.__tpOnUpdateStatus) window.__tpOnUpdateStatus({});", json);
                let _ = self.settings_shell.evaluate_script(&js);
            }
            AppCmd::RunAppUpdate => {
                if let Ok(current_exe) = env::current_exe() {
                    let target = current_exe.to_string_lossy().replace('\'', "''");
                    let script_path = env::temp_dir().join(format!("tpbrowser-update-{}.ps1", std::process::id()));
                    let script = format!(
                        "$ErrorActionPreference='Stop'\n$target='{target}'\n$temp=$target+'.download'\n$ProgressPreference='SilentlyContinue'\nInvoke-WebRequest -UseBasicParsing -Uri '{UPDATE_BINARY_URL}' -OutFile $temp\nfor ($i=0; $i -lt 30; $i++) {{ try {{ Move-Item -Force $temp $target; break }} catch {{ Start-Sleep -Seconds 1 }} }}\nStart-Process -FilePath $target\nRemove-Item -LiteralPath $PSCommandPath -Force\n"
                    );
                    if fs::write(&script_path, script).is_ok() {
                        let _ = std::process::Command::new("powershell.exe")
                            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
                            .arg(&script_path)
                            .spawn();
                        self.remove_system_tray();
                        self.should_exit = true;
                    }
                }
            }
            AppCmd::SaveSetting { key, value } => {
                match key.as_str() {
                    "ui_theme" => {
                        self.ui_theme = Self::normalize_theme(&value);
                        self.sync_window_titlebar_theme();
                        self.sync_window_icon();
                        self.warm_newtab = None;
                        self.schedule_prewarm_newtab(220);
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
                    "compact_mode" => {
                        self.compact_mode = Self::parse_bool_str(&value, false);
                        COMPACT_MODE_ENABLED.store(self.compact_mode, Ordering::Relaxed);
                        self.compact_sidebar_hover = false;
                        self.relayout();
                    }
                    "tab_unload_minutes" => {
                        if let Ok(m) = value.trim().parse::<u32>() {
                            self.tab_unload_minutes = m.min(600);
                        }
                    }
                    "tab_unload_whitelist" => {
                        self.tab_unload_whitelist = value.trim().to_string();
                    }
                    "pinned_extensions" => {
                        if let Ok(JsonVal::Arr(arr)) = parse_json(&value) {
                            let loaded: Vec<String> = arr.into_iter().filter_map(|v| match v {
                                JsonVal::Str(s) => Some(s),
                                _ => None,
                            }).collect();
                            if !loaded.is_empty() {
                                self.pinned_extensions = loaded;
                            }
                        } else {
                            let loaded: Vec<String> = value.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                            if !loaded.is_empty() {
                                self.pinned_extensions = loaded;
                            }
                        }
                        self.sync_all_ui();
                        self.save_config();
                    }
                    "translate_target_lang" => {
                        self.translate_target_lang = value.trim().to_string();
                        self.save_config();
                    }
                    "translate_auto_prompt" => {
                        self.translate_auto_prompt = Self::parse_bool_str(&value, true);
                        self.save_config();
                    }
                    "translate_never_domains" => {
                        self.translate_never_domains = value.trim().to_string();
                        self.save_config();
                    }
                    "glance_trigger" => {
                        self.glance_trigger = if value.contains("shift") { "shift_click".to_string() } else { "alt_click".to_string() };
                        self.save_config();
                    }
                    "glance_external_from_pinned" => {
                        self.glance_external_from_pinned = Self::parse_bool_str(&value, true);
                        self.save_config();
                    }
                    "default_search_engine" => {
                        self.default_search_engine = value.trim().to_string();
                        self.save_config();
                    }
                    "auto_pip_on_switch" => {
                        self.auto_pip_on_switch = Self::parse_bool_str(&value, true);
                        self.save_config();
                    }
                    "show_bookmarks_bar" => {
                        self.show_bookmarks_bar = Self::parse_bool_str(&value, true);
                    }
                    "background_mode" => {
                        self.background_mode = Self::parse_bool_str(&value, true);
                    }
                    "startup_boost" => {
                        self.startup_boost = Self::parse_bool_str(&value, false);
                        Self::sync_windows_autostart(self.startup_boost);
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
                    "context_menu_prefs" => {
                        self.context_menu_prefs_json = Self::normalize_context_menu_prefs(&value);
                        self.save_config();
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
                    "permission_camera" => {
                        self.permission_camera = Self::normalize_permission_policy(&value).to_string();
                        self.sync_permission_atomics();
                    }
                    "permission_microphone" => {
                        self.permission_microphone = Self::normalize_permission_policy(&value).to_string();
                        self.sync_permission_atomics();
                    }
                    "permission_notifications" => {
                        self.permission_notifications = Self::normalize_permission_policy(&value).to_string();
                        self.sync_permission_atomics();
                    }
                    "permission_clipboard" => {
                        self.permission_clipboard = Self::normalize_permission_policy(&value).to_string();
                        self.sync_permission_atomics();
                    }
                    "custom_shortcuts" => {
                        if let Ok(parsed) = parse_json(&value) {
                            self.custom_shortcuts = Self::parse_custom_shortcuts(&parsed);
                        }
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
                self.ensure_optional_shells();
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
            AppCmd::NewTab { url, isolated, container_id } => {
                let id = container_id.as_deref().map(Self::normalize_container_id);
                self.add_tab_internal(&url, isolated, true, None, None, false, id);
            }
            AppCmd::NewPrivateTab(url) => {
                self.profile_counter = self.profile_counter.saturating_add(1);
                let container = format!("private_{}", self.profile_counter);
                self.add_tab_internal(&url, true, true, None, None, false, Some(container));
            }
            AppCmd::PrintActive => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    let _ = tab.webview.evaluate_script("try { window.print(); } catch (_) {}");
                }
            }
            AppCmd::OpenDevTools => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    tab.webview.open_devtools();
                }
            }
            AppCmd::ToggleReaderMode => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    let _ = tab.webview.evaluate_script(r#"(() => {
                        const id = 'tp-reader-mode-style';
                        const old = document.getElementById(id);
                        if (old) { old.remove(); document.documentElement.removeAttribute('data-tp-reader'); return; }
                        const style = document.createElement('style');
                        style.id = id;
                        style.textContent = `html[data-tp-reader="on"] body { background:#f7f3ea !important; color:#292524 !important; }
                            html[data-tp-reader="on"] body { max-width:760px !important; margin:0 auto !important; padding:40px 28px !important; font-family:Georgia, serif !important; font-size:19px !important; line-height:1.75 !important; }
                            html[data-tp-reader="on"] nav, html[data-tp-reader="on"] header, html[data-tp-reader="on"] aside, html[data-tp-reader="on"] footer, html[data-tp-reader="on"] video, html[data-tp-reader="on"] iframe { display:none !important; }
                            html[data-tp-reader="on"] img { max-width:100% !important; height:auto !important; }`;
                        document.head.appendChild(style);
                        document.documentElement.setAttribute('data-tp-reader', 'on');
                    })()"#);
                }
            }
            AppCmd::DownloadStarted { uri, path } => {
                self.downloads.push(DownloadEntry { uri: uri.clone(), path: path.clone(), status: "downloading".to_string() });
                if self.downloads.len() > 20 { self.downloads.remove(0); }
                self.ai_status_text = format!("Đang tải xuống: {}", path);
                self.sync_all_ui();
            }
            AppCmd::DownloadCompleted { uri, path, success } => {
                if let Some(item) = self.downloads.iter_mut().rev().find(|item| item.uri == uri) {
                    item.path = if path.is_empty() { item.path.clone() } else { path.clone() };
                    item.status = if success { "complete".to_string() } else { "failed".to_string() };
                }
                self.ai_status_text = if success {
                    format!("Đã tải xong: {}", if path.is_empty() { uri } else { path })
                } else {
                    format!("Tải xuống thất bại: {}", uri)
                };
                self.sync_all_ui();
            }
            AppCmd::PermissionDecision { origin, kind, allow } => {
                self.ai_status_text = format!("Quyền {} cho {}: {}", kind, origin, if allow { "cho phép" } else { "từ chối" });
                self.sync_all_ui();
            }
            AppCmd::FocusVisibleIndex(visible_idx) => {
                if let Some(native_idx) = self.visible_tab_indices().get(visible_idx).copied() {
                    self.execute(AppCmd::FocusIndex(native_idx));
                }
            }
            AppCmd::FocusIndex(idx) => {
                if idx < self.tabs.len() && self.tabs[idx].workspace_id == self.active_workspace_id {
                    let tab_id = self.tabs[idx].id;
                    if self.tabs[idx].worker_kind.is_none() && !self.mru_focus_in_progress {
                        self.mru_tab_ids.retain(|id| *id != tab_id);
                        self.mru_tab_ids.insert(0, tab_id);
                    }
                    if !self.mru_focus_in_progress {
                        self.mru_cycle_ids = None;
                    }
                    if let Some(prev) = self.tabs.get_mut(self.active_idx) {
                        prev.last_active_at = Instant::now();
                    }
                    if self.auto_pip_on_switch && self.active_idx != idx {
                        if let Some(old_t) = self.tabs.get(self.active_idx) {
                            let _ = old_t.webview.evaluate_script("if (window.__tpAutoPip) window.__tpAutoPip(true);");
                        }
                    }
                    self.active_idx = idx;
                    if self.auto_pip_on_switch {
                        if let Some(new_t) = self.tabs.get(self.active_idx) {
                            let _ = new_t.webview.evaluate_script("if (window.__tpAutoPip) window.__tpAutoPip(false);");
                        }
                    }
                    // Đánh thức tab nếu đang ở trạng thái ngủ đông (Lazy Loading)
                    if let Some(t) = self.tabs.get_mut(self.active_idx) {
                        t.last_active_at = Instant::now();
                        if !t.is_loaded {
                            t.is_loaded = true;
                            let target_url = t.url.clone();
                            let _ = t.webview.load_url(&target_url);
                        }
                    }
                    self.relayout();
                    if let Some(t) = self.tabs.get(self.active_idx) {
                        let _ = t.webview.focus();
                    }
                    self.sync_all_ui();
                    self.save_session();
                }
            }
            AppCmd::FocusLastTab => {
                if let Some(last) = self.visible_tab_indices().last().copied() {
                    self.execute(AppCmd::FocusIndex(last));
                }
            }
            AppCmd::NextTab => {
                let visible_indices = self.visible_tab_indices();
                if visible_indices.len() > 1 {
                    let active_pos = visible_indices
                        .iter()
                        .position(|idx| *idx == self.active_idx)
                        .unwrap_or(0);
                    let next_pos = (active_pos + 1) % visible_indices.len();
                    self.execute(AppCmd::FocusIndex(visible_indices[next_pos]));
                }
            }
            AppCmd::PrevTab => {
                let visible_indices = self.visible_tab_indices();
                if visible_indices.len() > 1 {
                    let active_pos = visible_indices
                        .iter()
                        .position(|idx| *idx == self.active_idx)
                        .unwrap_or(0);
                    let prev_pos = if active_pos == 0 {
                        visible_indices.len() - 1
                    } else {
                        active_pos - 1
                    };
                    self.execute(AppCmd::FocusIndex(visible_indices[prev_pos]));
                }
            }
            AppCmd::FocusTabById(tab_id) => {
                if let Some(pos) = self.tabs.iter().position(|t| t.id == tab_id) {
                    if pos != self.active_idx {
                        self.execute(AppCmd::FocusIndex(pos));
                    } else if let Some(t) = self.tabs.get(pos) {
                        let _ = t.webview.focus();
                    }
                }
            }
            AppCmd::FocusContentTab(tab_id) => {
                let mut needs_relayout = false;
                if self.titlebar_popover_open {
                    self.titlebar_popover_open = false;
                    needs_relayout = true;
                }
                if self.command_bar_open {
                    self.execute(AppCmd::CloseCommandBar);
                    needs_relayout = false;
                }
                if let Some(pos) = self.tabs.iter().position(|t| t.id == tab_id) {
                    if pos != self.active_idx {
                        self.execute(AppCmd::FocusIndex(pos));
                        needs_relayout = false;
                    }
                }
                if needs_relayout {
                    self.relayout();
                }
            }
            AppCmd::CloseTabById(tab_id) => {
                if let Some(pos) = self.tabs.iter().position(|t| t.id == tab_id) {
                    self.execute(AppCmd::CloseIndex(pos));
                    self.sync_workspace_view();
                }
            }
            AppCmd::CloseIndex(idx) => {
                if idx < self.tabs.len() && self.tabs[idx].worker_kind.is_some() {
                    let removed_id = self.tabs[idx].id;
                    self.tabs[idx].navigation_readiness.close();
                    #[cfg(target_os = "windows")]
                    self.tabs[idx].navigation_events.teardown();
                    let requests: Vec<u64> = self.shopee_pending.iter().filter_map(|(id, (tab, _, _, _))| (*tab == removed_id).then_some(*id)).collect();
                    for id in &requests {
                        self.shopee_workspace_pending.remove(id);
                        if let Some((_, _, _, reply)) = self.shopee_pending.remove(id) { let _ = reply.send(Ok(shopee_discovery::unavailable())); }
                    }
                    if !requests.is_empty() {
                        self.shopee_discovery_json = shopee_discovery::unavailable();
                        self.sync_workspace_view();
                    }
                    if self.ai_worker_tab_id == Some(removed_id) {
                        self.ai_worker_tab_id = None;
                    }
                    self.selected_tab_ids.remove(&removed_id);
                    if self.tab_selection_anchor_id == Some(removed_id) {
                        self.tab_selection_anchor_id = None;
                    }
                    self.tabs.remove(idx);
                    if idx < self.active_idx {
                        self.active_idx = self.active_idx.saturating_sub(1);
                    } else if self.active_idx >= self.tabs.len() {
                        self.active_idx = self.tabs.len().saturating_sub(1);
                    }
                    self.sync_tab_folders();
                    self.relayout();
                    self.sync_all_ui();
                    return;
                }
                let ws_tabs_count = self.tabs.iter().filter(|tab| tab.worker_kind.is_none() && tab.workspace_id == self.active_workspace_id).count();
                if ws_tabs_count > 1
                    && idx < self.tabs.len()
                    && self.tabs[idx].workspace_id == self.active_workspace_id
                {
                    let removed_id = self.tabs[idx].id;
                    self.tabs[idx].navigation_readiness.close();
                    #[cfg(target_os = "windows")]
                    self.tabs[idx].navigation_events.teardown();
                    let requests: Vec<u64> = self.shopee_pending.iter().filter_map(|(id, (tab, _, _, _))| (*tab == removed_id).then_some(*id)).collect();
                    for id in &requests {
                        self.shopee_workspace_pending.remove(id);
                        if let Some((_, _, _, reply)) = self.shopee_pending.remove(id) { let _ = reply.send(Ok(shopee_discovery::unavailable())); }
                    }
                    if !requests.is_empty() {
                        self.shopee_discovery_json = shopee_discovery::unavailable();
                        self.sync_workspace_view();
                    }
                    if self.ai_worker_tab_id == Some(removed_id) {
                        self.ai_worker_tab_id = None;
                    }
                    self.selected_tab_ids.remove(&removed_id);
                    if self.tab_selection_anchor_id == Some(removed_id) {
                        self.tab_selection_anchor_id = None;
                    }
                    let closed_url = self.tabs[idx].url.clone();
                    let private_container_id = self.tabs[idx]
                        .private
                        .then(|| self.tabs[idx].container_id.clone());
                    if !self.tabs[idx].private && (closed_url.starts_with("http://") || closed_url.starts_with("https://")) {
                        self.closed_tabs.push(closed_url);
                        if self.closed_tabs.len() > 25 {
                            self.closed_tabs.remove(0);
                        }
                    }
                    self.tabs.remove(idx);
                    if let Some(container_id) = private_container_id {
                        self.cleanup_private_profile(&container_id);
                    }
                    if idx < self.active_idx {
                        self.active_idx = self.active_idx.saturating_sub(1);
                    } else if self.active_idx >= self.tabs.len() {
                        self.active_idx = self.tabs.len() - 1;
                    }
                    self.sync_tab_folders();
                    self.relayout();
                    self.sync_all_ui();
                    self.save_session();
                } else if ws_tabs_count == 1
                    && idx < self.tabs.len()
                    && self.tabs[idx].workspace_id == self.active_workspace_id
                {
                    if self.tabs[idx].url != "tp://newtab" {
                        self.execute(AppCmd::NavigateActive("tp://newtab".to_string()));
                    }
                }
            }
            AppCmd::CloseActiveTab => {
                let idx = self.active_idx;
                if let Some(tab) = self.tabs.get(idx) {
                    if (tab.pinned || tab.essential) && tab.worker_kind.is_none() {
                        let origin = tab.pinned_origin_url.clone();
                        let tab_id = tab.id;
                        let cur_url = tab.url.clone();
                        if let Some(origin_url) = origin {
                            if origin_url != cur_url {
                                self.execute(AppCmd::ResetPinnedTab(tab_id));
                                if let Some(t) = self.tabs.iter().find(|t| t.id == tab_id) {
                                    let _ = t.webview.evaluate_script(
                                        "if (window.__tpShowToast) window.__tpShowToast(\"↺ Đã quay về trang gốc của Tab Ghim\");",
                                    );
                                }
                                return;
                            }
                        }
                        let ws_id = self.active_workspace_id.clone();
                        if let Some(next_idx) = self.tabs.iter().position(|t| {
                            t.id != tab_id
                                && t.worker_kind.is_none()
                                && !t.pinned
                                && !t.essential
                                && (t.workspace_id == ws_id || t.essential)
                        }) {
                            self.active_idx = next_idx;
                            self.relayout();
                            self.sync_all_ui();
                        }
                        return;
                    }
                }
                self.execute(AppCmd::CloseIndex(idx));
            }
            AppCmd::NavigateActive(raw_url) => {
                let normalized = Self::normalize_url_with_engine(&raw_url, &self.default_search_engine);
                if let Some(tab) = self.tabs.get_mut(self.active_idx) {
                    tab.url = normalized.clone();
                    tab.title = Self::title_from_url(&normalized);
                    if normalized == "tp://newtab" {
                        let _ = tab.webview.load_html(cached_newtab_html(&self.ui_theme));
                    } else if normalized == "tp://history" {
                        let _ = tab.webview.load_html(cached_history_html(&self.ui_theme));
                    } else if normalized == "tp://workspace" {
                        let _ = tab.webview.load_html(cached_newtab_html(&self.ui_theme));
                    } else {
                        let _ = tab.webview.load_url(&normalized);
                    }
                }
                let is_private = self.tabs.get(self.active_idx).is_some_and(|tab| tab.private);
                if !is_private && (normalized.starts_with("http://") || normalized.starts_with("https://")) {
                    let t = Self::title_from_url(&normalized);
                    self.record_history(&t, &normalized);
                } else if normalized == "tp://history" {
                    self.sync_history_view();
                }
                self.sync_all_ui();
                if !is_private {
                    self.save_session();
                }
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
                        let _ = tab.webview.load_html(cached_newtab_html(&self.ui_theme));
                    } else if tab.url == "tp://history" {
                        let _ = tab.webview.load_html(cached_history_html(&self.ui_theme));
                    } else if tab.url == "tp://workspace" {
                        let _ = tab.webview.load_html(cached_newtab_html(&self.ui_theme));
                    } else {
                        let _ = tab.webview.reload();
                    }
                }
            }
            AppCmd::HardReloadActive => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    if tab.url == "tp://newtab" {
                        let _ = tab.webview.load_html(cached_newtab_html(&self.ui_theme));
                    } else if tab.url == "tp://history" {
                        let _ = tab.webview.load_html(cached_history_html(&self.ui_theme));
                    } else if tab.url == "tp://workspace" {
                        let _ = tab.webview.load_html(cached_newtab_html(&self.ui_theme));
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
                if self.tab_orientation == TabOrientation::VerticalLeft {
                    if self.sidebar_collapsed {
                        self.execute(AppCmd::OpenCommandBar);
                    } else {
                        let _ = self.tab_shell.focus();
                        let _ = self.tab_shell.evaluate_script(
                            "if (window.__tpFocusVerticalUrl) window.__tpFocusVerticalUrl();",
                        );
                    }
                } else {
                    if !self.titlebar_pinned && !self.titlebar_hover {
                        self.titlebar_hover = true;
                        self.relayout();
                    }
                    let _ = self.titlebar_shell.focus();
                    let _ = self.titlebar_shell.evaluate_script(
                        "if (window.__tpFocusTitlebarUrl) window.__tpFocusTitlebarUrl();",
                    );
                }
            }
            AppCmd::ToggleFindBar => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    let _ = tab.webview.focus();
                    let _ = tab.webview.evaluate_script(&format!(
                        "if (window.__tpToggleFindBar) window.__tpToggleFindBar(undefined, \"{}\");",
                        Self::escape_json(&self.ui_theme)
                    ));
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
                self.history_rev = self.history_rev.wrapping_add(1);
                self.save_history();
                self.sync_history_view();
            }
            AppCmd::ClearSiteData => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    let _ = tab.webview.clear_all_browsing_data();
                    let _ = tab.webview.evaluate_script(
                        "if (window.__tpShowToast) window.__tpShowToast('Đã xóa dữ liệu website hiện tại');",
                    );
                }
            }
            AppCmd::CreateWorkspace(name) => {
                let name = name.trim();
                if valid_workspace_name(name) {
                    let id = format!("workspace-{}", std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
                    if !self.workspaces.iter().any(|workspace| workspace.id == id) {
                        self.workspaces.push(Workspace {
                            id: id.clone(),
                            name: name.to_string(),
                            icon: "✦".to_string(),
                            accent: String::new(),
                            container_id: Self::normalize_container_id(&id),
                        });
                        self.active_workspace_id = id;
                        self.selected_tab_ids.clear();
                        self.tab_selection_anchor_id = None;
                        self.view_mode = ContentViewMode::SingleTab;
                        self.split_paired_tab_id = None;
                        self.add_tab("tp://newtab", false);
                        self.relayout();
                        self.sync_all_ui();
                        self.save_config();
                        self.save_session();
                    }
                }
            }
            AppCmd::SetActiveWorkspace(workspace_id) => {
                if self.workspaces.iter().any(|workspace| workspace.id == workspace_id) {
                    self.active_workspace_id = workspace_id;
                    self.selected_tab_ids.clear();
                    self.tab_selection_anchor_id = None;
                    if let Some(idx) = self.tabs.iter().position(|tab| tab.workspace_id == self.active_workspace_id) {
                        self.active_idx = idx;
                    } else {
                        self.view_mode = ContentViewMode::SingleTab;
                        self.split_paired_tab_id = None;
                        self.add_tab("tp://newtab", false);
                    }
                    self.relayout();
                    self.sync_all_ui();
                    self.save_config();
                    self.save_session();
                }
            }
            AppCmd::RenameWorkspace { id, name } => {
                let name = name.trim();
                if valid_workspace_name(name) {
                    if let Some(workspace) = self.workspaces.iter_mut().find(|workspace| workspace.id == id) {
                        workspace.name = name.to_string();
                        self.sync_all_ui();
                        self.save_config();
                        self.save_session();
                    }
                }
            }
            AppCmd::DeleteWorkspace(id) => {
                if id != DEFAULT_WORKSPACE_ID && self.workspaces.iter().any(|workspace| workspace.id == id) {
                    for tab in self.tabs.iter_mut().filter(|tab| tab.workspace_id == id) {
                        tab.workspace_id = DEFAULT_WORKSPACE_ID.to_string();
                    }
                    self.workspaces.retain(|workspace| workspace.id != id);
                    self.selected_tab_ids.clear();
                    self.tab_selection_anchor_id = None;
                    if self.active_workspace_id == id {
                        self.active_workspace_id = DEFAULT_WORKSPACE_ID.to_string();
                    }
                    if let Some(idx) = self.tabs.iter().position(|tab| tab.workspace_id == self.active_workspace_id) {
                        self.active_idx = idx;
                    }
                    self.view_mode = ContentViewMode::SingleTab;
                    self.split_paired_tab_id = None;
                    self.relayout();
                    self.sync_all_ui();
                    self.save_config();
                    self.save_session();
                }
            }
            AppCmd::OpenWorkspace => {
                if let Some(pos) = self.tabs.iter().position(|t| t.url == "tp://newtab") {
                    self.execute(AppCmd::FocusIndex(pos));
                } else {
                    self.add_tab("tp://newtab", false);
                }
            }
            AppCmd::OpenGlance { url, title } => {
                let norm_url = Self::normalize_url(&url);
                let display_title = if title.trim().is_empty() {
                    norm_url.clone()
                } else {
                    title
                };
                self.ensure_glance_shells(&norm_url, &display_title);
                self.glance_open = true;
                self.relayout();
                self.sync_glance_view();
            }
            AppCmd::CloseGlance => {
                if self.glance_open {
                    self.glance_open = false;
                    self.relayout();
                    if let Some(t) = self.tabs.get(self.active_idx) {
                        let _ = t.webview.focus();
                    }
                }
            }
            AppCmd::GlanceOpenInTab => {
                if self.glance_open && !self.glance_url.is_empty() {
                    let url = self.glance_url.clone();
                    self.execute(AppCmd::CloseGlance);
                    self.add_tab(&url, false);
                }
            }
            AppCmd::OpenShopeeShop(shop_id) => {
                self.open_shopee_shop_in_user_tab(&shop_id);
            }
            AppCmd::RunEcomScan { scope } => {
                let _ = scope;
                self.ecom_status_text = "Tính năng quét dữ liệu đã được tắt trong chế độ trình duyệt.".to_string();
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
                        let is_passive = parse_json(&data_json)
                            .ok()
                            .and_then(|v| v.get("scan_source").and_then(JsonVal::as_str).map(|s| s == "passive"))
                            .unwrap_or(false);
                        if is_passive && !self.multi_shop_queue.is_empty() && self.multi_shop_step == "orders" {
                            // Bỏ qua cập nhật thụ động giữa chừng để chờ fetchShopeeOrdersViaApi() quét đủ tất cả tab đơn hàng
                            return;
                        }
                        self.shopee_orders_json = data_json.clone();
                        self.save_export_file("shopee_orders_latest.json", &data_json);
                        if self.shopee_shop_scan_phase == "shop_orders" { self.store_shopee_shop_orders(&data_json); }
                        self.ecom_status_text = "✅ Đã cập nhật Đơn hàng Shopee.".to_string();

                        let s_id = if self.multi_shop_idx < self.multi_shop_queue.len() {
                            self.multi_shop_queue[self.multi_shop_idx].clone()
                        } else {
                            "sp_01".to_string()
                        };
                        let name = self.get_shop_display_name(&s_id);

                        if let Ok(val) = parse_json(&data_json) {
                            let mut rev = 0.0;
                            let mut orders_today = 0u64;
                            let mut cancelled = 0u64;
                            if let Some(today) = val.get("today_stats") {
                                rev = today.get("revenue_vnd").and_then(JsonVal::as_f64).unwrap_or(0.0);
                                orders_today = today.get("order_count").and_then(JsonVal::as_u64).unwrap_or(0);
                                cancelled = today.get("cancelled_count").and_then(JsonVal::as_u64).unwrap_or(0);
                            }
                            let to_ship = val.get("to_ship").and_then(JsonVal::as_u64).unwrap_or(0);
                            let express = val.get("express_orders").and_then(JsonVal::as_u64).unwrap_or(0);
                            let anom_count = val.get("anomalies").and_then(JsonVal::as_arr).map(|a| a.len()).unwrap_or(0);
                            let bad_rev_count = val.get("bad_reviews").and_then(JsonVal::as_arr).map(|a| a.len()).unwrap_or(0);

                            self.log_ecom("DATA", "SHOPEE", &format!(
                                "[{}] Doanh thu hôm nay: {} | {} đơn hôm nay (hủy {}) | Chờ giao: {} | Hỏa tốc: {} | Đánh giá xấu 1-3★: {} | Đơn lệch giá: {}",
                                name, Self::format_vnd_amount(rev), orders_today, cancelled, to_ship, express, bad_rev_count, anom_count
                            ));
                        }
                        for t in &mut self.tabs {
                            if t.worker_kind.as_deref() == Some("bot") && t.url.contains("banhang.shopee.vn/portal/sale") {
                                t.worker_status = if data_json.contains("\"need_login\":true") {
                                    "Cần đăng nhập".to_string()
                                } else {
                                    "Đã lấy xong".to_string()
                                };
                            }
                        }
                        if self.multi_shop_queue.is_empty() && self.shopee_shop_scan_phase == "shop_orders" {
                            self.shopee_shop_scan_phase = "returning_index".to_string();
                            if let Some(tab_id) = self.shopee_shop_scan_tab_id {
                                if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) { let _ = tab.webview.load_url("https://banhang.shopee.vn/portal/shop"); }
                            }
                        }
                        if self.multi_shop_step == "orders" && self.multi_shop_idx < self.multi_shop_queue.len() {
                            let s_id = self.multi_shop_queue[self.multi_shop_idx].clone();
                            let name = self.get_shop_display_name(&s_id);
                            let entry = self.multi_shop_data.entry(s_id.clone()).or_insert_with(|| (String::new(), String::new(), String::new()));
                            entry.0 = data_json.clone();

                            let total = self.multi_shop_queue.len();
                            let curr = self.multi_shop_idx + 1;
                            self.multi_shop_step = "pas".to_string();
                            let pct = ((curr - 1) * 100 + 40) / total;
                            self.sync_workspace_progress(true, pct as usize, 100, pct as u32, &format!("[Shop {}/{} - {}] Bước 2/3: Đang quét Shopee Ads...", curr, total, name), "running");
                            self.log_ecom("INFO", "SHOPEE", &format!("[Shop {}/{}] {} - Chuyển sang Bước 2/3: Shopee Ads", curr, total, name));

                            if let Some(tid) = self.multi_shop_tab_id {
                                if let Some(t) = self.tabs.iter().find(|t| t.id == tid) {
                                    let pas_url = "https://banhang.shopee.vn/portal/marketing/pas/index?source_page_id=1&type=new_cpc_homepage&group=today";
                                    let _ = t.webview.load_url(pas_url);
                                }
                            }
                        }
                    }
                    "shopee_pas" => {
                        self.shopee_pas_json = data_json.clone();
                        self.save_export_file("shopee_pas_latest.json", &data_json);
                        self.ecom_status_text = "✅ Đã cập nhật dữ liệu Quảng cáo Shopee PAS.".to_string();

                        let s_id = if self.multi_shop_idx < self.multi_shop_queue.len() {
                            self.multi_shop_queue[self.multi_shop_idx].clone()
                        } else {
                            "sp_01".to_string()
                        };
                        let name = self.get_shop_display_name(&s_id);

                        if let Ok(val) = parse_json(&data_json) {
                            let cost = val.get("cost_ads").and_then(JsonVal::as_f64).unwrap_or(0.0);
                            let sales = val.get("sales_ads").and_then(JsonVal::as_f64).unwrap_or(0.0);
                            let roas = val.get("roas_ads").and_then(JsonVal::as_f64).unwrap_or(0.0);
                            let camps = val.get("campaigns").and_then(JsonVal::as_arr).map(|a| a.len()).unwrap_or(0);
                            let cir = if sales > 0.0 { (cost / sales) * 100.0 } else { 0.0 };
                            self.log_ecom("DATA", "SHOPEE", &format!(
                                "[{}] Shopee Ads: Chi phí: {} | Doanh thu Ads: {} | ROAS: {:.2}x | CIR: {:.1}% | {} chiến dịch",
                                name, Self::format_vnd_amount(cost), Self::format_vnd_amount(sales), roas, cir, camps
                            ));
                        }
                        for t in &mut self.tabs {
                            if t.worker_kind.as_deref() == Some("bot") && t.url.contains("/portal/marketing/pas") {
                                t.worker_status = if data_json.contains("\"need_login\":true") {
                                    "Cần đăng nhập".to_string()
                                } else {
                                    "Đã lấy xong".to_string()
                                };
                            }
                        }
                        if self.multi_shop_step == "pas" && self.multi_shop_idx < self.multi_shop_queue.len() {
                            let s_id = self.multi_shop_queue[self.multi_shop_idx].clone();
                            let name = self.get_shop_display_name(&s_id);
                            let entry = self.multi_shop_data.entry(s_id.clone()).or_insert_with(|| (String::new(), String::new(), String::new()));
                            entry.1 = data_json.clone();

                            let total = self.multi_shop_queue.len();
                            let curr = self.multi_shop_idx + 1;
                            self.multi_shop_step = "health".to_string();
                            let pct = ((curr - 1) * 100 + 75) / total;
                            self.sync_workspace_progress(true, pct as usize, 100, pct as u32, &format!("[Shop {}/{} - {}] Bước 3/3: Đang quét Hiệu Quả Hoạt Động...", curr, total, name), "running");
                            self.log_ecom("INFO", "SHOPEE", &format!("[Shop {}/{}] {} - Chuyển sang Bước 3/3: Hiệu Quả Hoạt Động", curr, total, name));

                            if let Some(tid) = self.multi_shop_tab_id {
                                if let Some(t) = self.tabs.iter().find(|t| t.id == tid) {
                                    let health_url = "https://banhang.shopee.vn/portal/accounthealth/home";
                                    let _ = t.webview.load_url(health_url);
                                }
                            }
                        }
                    }
                    "shopee_health" => {
                        self.shopee_health_json = data_json.clone();
                        self.save_export_file("shopee_health_latest.json", &data_json);
                        self.ecom_status_text = "✅ Đã cập nhật Hiệu Quả Hoạt Động Shopee.".to_string();

                        let s_id = if self.multi_shop_idx < self.multi_shop_queue.len() {
                            self.multi_shop_queue[self.multi_shop_idx].clone()
                        } else {
                            "sp_01".to_string()
                        };
                        let name = self.get_shop_display_name(&s_id);

                        if let Ok(val) = parse_json(&data_json) {
                            let points = val.get("penalty_points").and_then(JsonVal::as_u64).unwrap_or(0);
                            let note = val.get("penalty_note").and_then(JsonVal::as_str).unwrap_or("Bình thường");
                            let on_time = val.get("on_time_ship_rate").and_then(JsonVal::as_str).unwrap_or("--");
                            self.log_ecom("DATA", "SHOPEE", &format!(
                                "[{}] Sức khỏe Shop: Điểm phạt: {}/12 | Giao đúng hạn: {} | Ghi chú: {}",
                                name, points, on_time, note
                            ));
                        }
                        for t in &mut self.tabs {
                            if t.worker_kind.as_deref() == Some("bot") && t.url.contains("/portal/accounthealth") {
                                t.worker_status = if data_json.contains("\"need_login\":true") {
                                    "Cần đăng nhập".to_string()
                                } else {
                                    "Đã lấy xong".to_string()
                                };
                            }
                        }
                        if self.multi_shop_step == "health" && self.multi_shop_idx < self.multi_shop_queue.len() {
                            let s_id = self.multi_shop_queue[self.multi_shop_idx].clone();
                            let name = self.get_shop_display_name(&s_id);
                            let entry = self.multi_shop_data.entry(s_id.clone()).or_insert_with(|| (String::new(), String::new(), String::new()));
                            entry.2 = data_json.clone();

                            self.multi_shop_idx += 1;
                            let next_is_shopee = self.multi_shop_queue.get(self.multi_shop_idx)
                                .map(|id| !id.starts_with("tt_") && id != "tiktok" && !id.starts_with("pk_") && id != "pancake")
                                .unwrap_or(false);
                            if next_is_shopee {
                                self.log_ecom("SUCCESS", "SHOPEE", &format!("Hoàn tất 3 bước shop Shopee: {} -> Quay lại /portal/shop đổi sang shop tiếp theo", name));
                            } else {
                                self.log_ecom("SUCCESS", "SHOPEE", &format!("Hoàn tất 3 bước shop Shopee: {} -> Đóng tab giải phóng RAM", name));
                                if let Some(tid) = self.multi_shop_tab_id.take() {
                                    self.execute(AppCmd::CloseTabById(tid));
                                }
                            }
                            self.start_next_shop_in_multi_queue();
                        }
                    }
                    "shopee_shop_index" => {
                        self.handle_shopee_shop_index(&data_json);
                    }
                    "shopee_shop_opening" => {
                        self.handle_shopee_shop_opening(&data_json);
                    }
                    "tiktok_homepage" => {
                        if let Ok(val) = parse_json(&data_json) {
                            if let Some(name) = val.get("shop_name").and_then(JsonVal::as_str) {
                                if !name.is_empty() && !name.contains("Trung tâm nhà bán hàng") && !name.contains("Seller Center") {
                                    self.tiktok_shop_name = name.to_string();
                                    self.save_config();
                                    self.log_ecom("DATA", "TIKTOK", &format!("Đã nhận diện gian hàng: {}", name));
                                }
                            }
                            let gmv = val.get("gmv").and_then(JsonVal::as_f64).unwrap_or(0.0);
                            let orders = val.get("orders").and_then(JsonVal::as_f64).unwrap_or(0.0) as u64;
                            let to_ship = val.get("to_ship").and_then(JsonVal::as_f64).unwrap_or(0.0) as u64;
                            if let Some(err) = val.get("error").and_then(JsonVal::as_str) {
                                self.log_ecom("ERROR", "TIKTOK", &format!("Lỗi cào trang chủ TikTok: {}", err));
                            } else if gmv == 0.0 && orders == 0 {
                                self.log_ecom("WARN", "TIKTOK", "Trang chủ TikTok: Chưa tìm thấy thẻ Doanh thu/Đơn hàng (GMV: 0 ₫, 0 đơn). Cần kiểm tra xem tab có bị yêu cầu đăng nhập/Captcha không.");
                            } else {
                                self.log_ecom("DATA", "TIKTOK", &format!(
                                    "Trang chủ TikTok: Doanh thu hôm nay: {} | Số đơn hôm nay: {} | Chờ vận chuyển: {}",
                                    Self::format_vnd_amount(gmv), orders, to_ship
                                ));
                            }
                        }
                        self.tiktok_json = data_json.clone();
                        self.save_export_file("tiktok_homepage_latest.json", &data_json);

                        if self.multi_shop_step == "tiktok_homepage" {
                            if let Some(tid) = self.multi_shop_tab_id {
                                self.multi_shop_step = "tiktok_ads".to_string();
                                let total = self.multi_shop_queue.len();
                                let curr = self.multi_shop_idx + 1;
                                let name = if !self.tiktok_shop_name.is_empty() { self.tiktok_shop_name.clone() } else { "TikTok Shop VN".to_string() };
                                let pct = ((curr * 100) / (total + 1)) + 5;
                                self.sync_workspace_progress(true, pct as usize, 100, pct as u32, &format!("[Shop {}/{} - {}] Bước 2/4: Đang quét TikTok Ads & Chi phí...", curr, total, name), "running");
                                self.log_ecom("INFO", "TIKTOK", &format!("[Shop {}/{}] {} - Chuyển sang Bước 2/4: TikTok Ads", curr, total, name));

                                let ads_url = "https://seller-vn.tiktok.com/ads-creation/dashboard?origin=SC_ads_tab_button_PC&mpa=1&type=product&shop_region=VN";
                                if let Some(tab) = self.tabs.iter().find(|t| t.id == tid) {
                                    let _ = tab.webview.load_url(ads_url);
                                }
                                self.trigger_worker_scan_by_id(tid, ads_url);
                            }
                        }
                    }
                    "tiktok_ads" | "tiktok" => {
                        self.tiktok_ads_json = data_json.clone();
                        self.save_export_file("tiktok_ads_latest.json", &data_json);
                        if let Ok(val) = parse_json(&data_json) {
                            if let Some(metrics) = val.get("metrics") {
                                // metrics.cost is a nested object {value, raw, source} — read .value
                                let mut cost = metrics.get("cost").and_then(|m| m.get("value")).and_then(JsonVal::as_f64)
                                    .unwrap_or_else(|| metrics.get("cost").and_then(JsonVal::as_f64).unwrap_or(0.0));
                                let mut sales = metrics.get("sales").and_then(|m| m.get("value")).and_then(JsonVal::as_f64)
                                    .or_else(|| metrics.get("sales").and_then(JsonVal::as_f64))
                                    .or_else(|| metrics.get("gmv").and_then(|m| m.get("value")).and_then(JsonVal::as_f64))
                                    .unwrap_or(0.0);
                                let roas = metrics.get("roas").and_then(|m| m.get("value")).and_then(JsonVal::as_f64)
                                    .or_else(|| metrics.get("roas").and_then(JsonVal::as_f64))
                                    .or_else(|| metrics.get("roi").and_then(|m| m.get("value")).and_then(JsonVal::as_f64))
                                    .unwrap_or(0.0);
                                let orders = metrics.get("orders").and_then(|m| m.get("value")).and_then(JsonVal::as_u64)
                                    .or_else(|| metrics.get("orders").and_then(JsonVal::as_u64))
                                    .unwrap_or(0);
                                if cost == 0.0 {
                                    cost = val.get("cost_ads").and_then(JsonVal::as_f64).unwrap_or(0.0);
                                }
                                if sales < 1000.0 {
                                    sales = val.get("sales_ads").and_then(JsonVal::as_f64).unwrap_or(0.0);
                                    if sales < 1000.0 {
                                        sales = 0.0;
                                    }
                                }
                                let status = val.get("status").and_then(JsonVal::as_str).unwrap_or("unknown");
                                self.log_ecom("DATA", "TIKTOK", &format!(
                                    "TikTok Ads: Chi phí: {} | Doanh thu Ads: {} | ROAS: {:.2}x | Đơn Ads: {} | Trạng thái: {}",
                                    Self::format_vnd_amount(cost), Self::format_vnd_amount(sales), roas, orders, status
                                ));
                            } else {
                                let (status_text, _worker_status) = tiktok_status_messages(tiktok_ads_status(&data_json));
                                self.ecom_status_text = status_text.to_string();
                                self.push_ecom_log(&self.ecom_status_text.clone());
                                self.log_ecom("INFO", "TIKTOK", &format!("Dữ liệu TikTok Ads: {}", status_text));
                            }
                        }
                        for t in &mut self.tabs {
                            if t.worker_kind.as_deref() == Some("bot") && t.url.contains("seller-vn.tiktok.com") {
                                t.worker_status = "Đã lấy xong".to_string();
                            }
                        }
                        if self.multi_shop_step == "tiktok_ads" {
                            if let Some(tid) = self.multi_shop_tab_id {
                                self.multi_shop_step = "tiktok_products".to_string();
                                let total = self.multi_shop_queue.len();
                                let curr = self.multi_shop_idx + 1;
                                let name = if !self.tiktok_shop_name.is_empty() { self.tiktok_shop_name.clone() } else { "TikTok Shop VN".to_string() };
                                let pct = ((curr * 100) / (total + 1)) + 10;
                                self.sync_workspace_progress(true, pct as usize, 100, pct as u32, &format!("[Shop {}/{} - {}] Bước 3/4: Đang kiểm tra Tồn kho sản phẩm...", curr, total, name), "running");
                                self.log_ecom("INFO", "TIKTOK", &format!("[Shop {}/{}] {} - Chuyển sang Bước 3/4: Tồn kho sản phẩm", curr, total, name));

                                let prod_url = "https://seller-vn.tiktok.com/product/manage";
                                if let Some(tab) = self.tabs.iter().find(|t| t.id == tid) {
                                    let _ = tab.webview.load_url(prod_url);
                                }
                                self.trigger_worker_scan_by_id(tid, prod_url);
                            }
                        } else if self.tiktok_scan_phase == "ads" {
                            self.sync_workspace_progress(true, 75, 100, 75, "TikTok: Đang quét Trung tâm sức khỏe...", "running");
                            self.continue_tiktok_sequence(
                                "https://seller-vn.tiktok.com/health-center?shop_region=VN",
                                "health_center",
                            );
                        }
                    }
                    "tiktok_products" => {
                        self.tiktok_products_json = data_json.clone();
                        self.save_export_file("tiktok_products_latest.json", &data_json);
                        if let Ok(val) = parse_json(&data_json) {
                            if let Some(prods) = val.get("products").and_then(JsonVal::as_arr) {
                                let total = prods.len();
                                let mut out_of_stock = Vec::new();
                                let mut low_stock = Vec::new();
                                for p in prods {
                                    let sku = p.get("sku").and_then(JsonVal::as_str).unwrap_or("");
                                    let stock = p.get("stock").and_then(JsonVal::as_u64).unwrap_or(0);
                                    if stock == 0 {
                                        if out_of_stock.len() < 5 { out_of_stock.push(sku); }
                                    } else if stock <= 3 {
                                        if low_stock.len() < 5 { low_stock.push(format!("{}({})", sku, stock)); }
                                    }
                                }
                                let out_str = if out_of_stock.is_empty() { "Không có".to_string() } else { out_of_stock.join(", ") };
                                let low_str = if low_stock.is_empty() { "Không có".to_string() } else { low_stock.join(", ") };
                                self.log_ecom("DATA", "TIKTOK", &format!(
                                    "Kho TikTok: Đã quét {} sản phẩm/SKU | Hết hàng (0 cái): {} | Sắp hết (≤3 cái): {}",
                                    total, out_str, low_str
                                ));
                            } else if let Some(err) = val.get("error").and_then(JsonVal::as_str) {
                                self.log_ecom("ERROR", "TIKTOK", &format!("Lỗi cào sản phẩm TikTok: {}", err));
                            }
                        }
                        self.log_ecom("SUCCESS", "TIKTOK", "Đã quét xong thông tin sản phẩm & tồn kho TikTok Shop -> Đóng tab giải phóng RAM.");

                        if self.multi_shop_step == "tiktok_products" {
                            if let Some(tid) = self.multi_shop_tab_id {
                                self.multi_shop_step = "tiktok_health".to_string();
                                let total = self.multi_shop_queue.len();
                                let curr = self.multi_shop_idx + 1;
                                let name = if !self.tiktok_shop_name.is_empty() { self.tiktok_shop_name.clone() } else { "TikTok Shop VN".to_string() };
                                let pct = ((curr * 100) / (total + 1)) + 15;
                                self.sync_workspace_progress(true, pct as usize, 100, pct as u32, &format!("[Shop {}/{} - {}] Bước 4/4: Đang kiểm tra Điểm tình trạng tài khoản...", curr, total, name), "running");
                                self.log_ecom("INFO", "TIKTOK", &format!("[Shop {}/{}] {} - Chuyển sang Bước 4/4: Trung tâm sức khỏe", curr, total, name));

                                let health_url = "https://seller-vn.tiktok.com/health-center?shop_region=VN";
                                if let Some(tab) = self.tabs.iter().find(|t| t.id == tid) {
                                    let _ = tab.webview.load_url(health_url);
                                }
                                self.trigger_worker_scan_by_id(tid, health_url);
                            }
                        } else {
                            if let Some(tid) = self.multi_shop_tab_id.take() {
                                self.execute(AppCmd::CloseTabById(tid));
                            }
                            self.multi_shop_step = String::new();
                            self.multi_shop_idx += 1;
                            self.start_next_shop_in_multi_queue();
                        }
                    }
                    "tiktok_identity" => {
                        if let Ok(val) = parse_json(&data_json) {
                            if let Some(name) = val.get("shop_name").and_then(JsonVal::as_str) {
                                if !name.is_empty() {
                                    self.tiktok_shop_name = name.to_string();
                                    self.save_config();
                                    self.log_ecom("SUCCESS", "TIKTOK", &format!("Đã nhận diện gian hàng: {}", name));
                                }
                            }
                        }
                    }
                    "tiktok_health" => {
                        self.tiktok_health_json = data_json.clone();
                        self.save_export_file("tiktok_health_latest.json", &data_json);
                        if let Ok(val) = parse_json(&data_json) {
                            let score = val.get("score").and_then(JsonVal::as_str).unwrap_or("Tốt");
                            let note = val.get("status_note").and_then(JsonVal::as_str).unwrap_or("Vận hành ổn định");
                            self.log_ecom("DATA", "TIKTOK", &format!("Sức khỏe TikTok: Điểm cửa hàng: {} | Ghi chú: {}", score, note));
                        }
                        if self.multi_shop_step == "tiktok_health" {
                            self.log_ecom("SUCCESS", "TIKTOK", "Đã quét xong sức khỏe TikTok Shop -> Đóng tab giải phóng RAM.");
                            if let Some(tid) = self.multi_shop_tab_id.take() {
                                self.execute(AppCmd::CloseTabById(tid));
                            }
                            self.multi_shop_step = String::new();
                            self.multi_shop_idx += 1;
                            self.start_next_shop_in_multi_queue();
                        } else if self.tiktok_scan_phase == "health_center" {
                            self.sync_workspace_progress(true, 90, 100, 90, "TikTok: Đang quét Điểm trải nghiệm...", "running");
                            self.continue_tiktok_sequence(
                                "https://seller-vn.tiktok.com/health-center/experience-score?shop_region=VN",
                                "store_score",
                            );
                        } else {
                            self.tiktok_scan_phase = "done".to_string();
                            self.log_ecom("SUCCESS", "TIKTOK", "Đã hoàn thành quét TikTok Shop (Ads & Health Center).");
                            self.sync_workspace_progress(false, 100, 100, 100, "Đã quét xong toàn bộ gian hàng!", "done");
                            self.save_workspace_snapshot("snap_latest");
                        }
                    }
                    _ => {}
                }
                self.sync_workspace_view();
            }
            AppCmd::ExportShopeeCsv => {
                self.export_shopee_csv();
            }
            AppCmd::SavePancakeApi { api_key } => {
                let trimmed = api_key.trim().to_string();
                self.pancake_api_key = trimmed.clone();
                if trimmed.is_empty() {
                    self.pancake_connected = false;
                    self.pancake_shop_id.clear();
                    self.pancake_shop_name.clear();
                    self.pancake_rev = 0.0;
                    self.pancake_orders = 0;
                    self.save_config();
                    self.push_ecom_log("Pancake API: Đã xóa khóa API.");
                    self.save_workspace_snapshot("snap_latest");
                    self.sync_workspace_command_center();
                } else {
                    let key = trimmed.clone();
                    let proxy = self.proxy.clone();
                    self.push_ecom_log("Pancake API: Đang xác thực khóa API...");
                    std::thread::spawn(move || {
                        let mut cmd = std::process::Command::new("curl.exe");
                        cmd.args(["-s", "--max-time", "6", &format!("https://pos.pages.fm/api/v1/shops/check?api_key={}", key)]);
                        #[cfg(windows)]
                        {
                            use std::os::windows::process::CommandExt;
                            cmd.creation_flags(0x08000000);
                        }
                        let mut success = false;
                        let mut message = "Không thể kết nối Pancake API".to_string();
                        let mut shop_id = None;
                        let mut shop_name = None;
                        if let Ok(output) = cmd.output() {
                            let body = String::from_utf8_lossy(&output.stdout);
                            if let Ok(val) = parse_json(&body) {
                                if val.get("success").and_then(JsonVal::as_bool) == Some(true) {
                                    success = true;
                                    message = "Đã kết nối Pancake POS thành công".to_string();
                                    if let Some(data) = val.get("data") {
                                        match data {
                                            JsonVal::Obj(_) => {
                                                if let Some(id_val) = data.get("id") {
                                                    let id_str = match id_val {
                                                        JsonVal::Num(n) => n.to_string(),
                                                        JsonVal::Str(s) => s.clone(),
                                                        _ => String::new(),
                                                    };
                                                    if !id_str.is_empty() { shop_id = Some(id_str); }
                                                }
                                                if let Some(name) = data.get("name").and_then(JsonVal::as_str) {
                                                    shop_name = Some(name.to_string());
                                                }
                                            }
                                            JsonVal::Arr(arr) => {
                                                if let Some(first) = arr.first() {
                                                    if let Some(id_val) = first.get("id") {
                                                        let id_str = match id_val {
                                                            JsonVal::Num(n) => n.to_string(),
                                                            JsonVal::Str(s) => s.clone(),
                                                            _ => String::new(),
                                                        };
                                                        if !id_str.is_empty() { shop_id = Some(id_str); }
                                                    }
                                                    if let Some(name) = first.get("name").and_then(JsonVal::as_str) {
                                                        shop_name = Some(name.to_string());
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                } else if let Some(msg) = val.get("message").and_then(JsonVal::as_str) {
                                    message = msg.to_string();
                                }
                            }
                        }
                        let _ = proxy.send_event(AppCmd::PancakeAuthResult { success, message, shop_id, shop_name });
                    });
                }
            }
            AppCmd::PancakeAuthResult { success, message, shop_id, shop_name } => {
                self.pancake_connected = success;
                if success {
                    if let Some(id) = shop_id { self.pancake_shop_id = id; }
                    if let Some(name) = shop_name { self.pancake_shop_name = name; }
                    let disp_name = if self.pancake_shop_name.is_empty() { "1 kho" } else { &self.pancake_shop_name };
                    self.push_ecom_log(&format!("Pancake POS: {} ({})", message, disp_name));

                    // Trigger immediate scan to fetch initial orders
                    let key = self.pancake_api_key.clone();
                    let s_id = self.pancake_shop_id.clone();
                    let proxy = self.proxy.clone();
                    std::thread::spawn(move || {
                        let url = if !s_id.is_empty() {
                            format!("https://pos.pages.fm/api/v1/shops/{}/orders?api_key={}&limit=50", s_id, key)
                        } else {
                            format!("https://pos.pages.fm/api/v1/shops/me/orders?api_key={}&limit=50", key)
                        };
                        let mut cmd = std::process::Command::new("curl.exe");
                        cmd.args(["-s", "--max-time", "8", &url]);
                        #[cfg(windows)]
                        {
                            use std::os::windows::process::CommandExt;
                            cmd.creation_flags(0x08000000);
                        }
                        let mut total_money = 0.0;
                        let mut order_count = 0;
                        if let Ok(output) = cmd.output() {
                            let body = String::from_utf8_lossy(&output.stdout);
                            if let Ok(val) = parse_json(&body) {
                                if let Some(orders) = val.get("data").and_then(JsonVal::as_arr) {
                                    order_count = orders.len();
                                    for o in orders {
                                        let money = o.get("money_to_collect")
                                            .and_then(JsonVal::as_f64)
                                            .or_else(|| o.get("total_price").and_then(JsonVal::as_f64))
                                            .unwrap_or(0.0);
                                        total_money += money;
                                    }
                                }
                            }
                        }
                        let rev_million = (total_money / 1_000_000.0 * 10.0).round() / 10.0;
                        let _ = proxy.send_event(AppCmd::PancakeScanResult { rev: rev_million, orders: order_count });
                    });
                } else {
                    self.push_ecom_log(&format!("Pancake POS: {}", message));
                }
                self.save_config();
                self.save_workspace_snapshot("snap_latest");
                self.sync_workspace_command_center();
            }
            AppCmd::PancakeScanResult { rev, orders } => {
                self.pancake_rev = rev;
                self.pancake_orders = orders;
                self.log_ecom("DATA", "PANCAKE", &format!("Pancake POS: Thu thập {} đơn hàng | Doanh thu thu hộ ước tính: {} tr VNĐ", orders, rev));
                self.save_workspace_snapshot("snap_latest");
                self.sync_workspace_command_center();
            }
            AppCmd::EcomLogDebug(msg) => {
                self.log_ecom("INFO", "SHOPEE", &msg);
                if !self.multi_shop_queue.is_empty() {
                    let total = self.multi_shop_queue.len().max(1);
                    let pct = ((self.multi_shop_idx * 100) / total).min(95);
                    let shop_label = self
                        .shopee_shop_scan_current
                        .as_ref()
                        .map(|(_, n)| n.clone())
                        .unwrap_or_else(|| "Shopee".to_string());
                    self.sync_workspace_progress(
                        true,
                        self.multi_shop_idx + 1,
                        total,
                        pct as u32,
                        &format!("[{}] {}", shop_label, msg),
                        "running",
                    );
                }
            }
            AppCmd::WorkspaceScan { platform, mode, shop_ids } => {
                self.push_ecom_log(&format!("Quét Command Center: platform={}, mode={}", platform, mode));
                if !shop_ids.is_empty() {
                    for s_id in &shop_ids {
                        if !self.selected_shops.iter().any(|s| s.shop_id == *s_id) {
                            self.selected_shops.push(ShopSelection {
                                platform: if s_id.starts_with("tt_") { "tiktok".to_string() } else if s_id.starts_with("pk_") { "pancake".to_string() } else { "shopee".to_string() },
                                shop_id: s_id.clone(),
                            });
                        }
                    }
                }
                self.start_multi_shop_queue(shop_ids);
            }
            AppCmd::ScanNeedAuth { reason, shop_id } => {
                self.multi_shop_auth_pending = true;
                let s_id = if !shop_id.is_empty() { shop_id } else {
                    self.multi_shop_queue.get(self.multi_shop_idx).cloned().unwrap_or_default()
                };
                let name = self.get_shop_display_name(&s_id);
                self.log_ecom("WARN", "AUTH", &format!("Shop {} ({}) yêu cầu xác thực [{}]", name, s_id, reason));

                if let Some(tab_id) = self.multi_shop_tab_id {
                    if let Some(pos) = self.tabs.iter().position(|t| t.id == tab_id) {
                        self.tabs[pos].worker_kind = None;
                        self.tabs[pos].workspace_id = self.active_workspace_id.clone();
                        let _ = self.tabs[pos].webview.set_visible(true);
                        self.execute(AppCmd::FocusIndex(pos));
                        let _ = self.tabs[pos].webview.focus();
                        self.relayout();
                        self.sync_all_ui();
                    }
                }
                let msg = format!("⚠️ [{}] Cần xác thực / giải Captcha để tiếp tục...", name);
                self.sync_workspace_progress(true, 50, 100, 50, &msg, "running");
                let js = format!("if (window.onWorkspaceAuthRequired) {{ window.onWorkspaceAuthRequired({{ shopName: '{}', reason: '{}' }}); }}", Self::escape_json(&name), Self::escape_json(&reason));
                for t in &self.tabs {
                    if t.url == "tp://workspace" {
                        let _ = t.webview.evaluate_script(&js);
                    }
                }
            }
            AppCmd::ScanAuthResolved => {
                self.multi_shop_auth_pending = false;
                self.log_ecom("INFO", "AUTH", "Đã xác thực xong, tiếp tục quét ngầm...");
                if let Some(tab_id) = self.multi_shop_tab_id {
                    if let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                        t.worker_kind = Some("bot".to_string());
                    }
                    self.relayout();
                }
                let s_id = self.multi_shop_queue.get(self.multi_shop_idx).cloned().unwrap_or_default();
                let name = self.get_shop_display_name(&s_id);
                let js = format!("if (window.onWorkspaceAuthResolved) {{ window.onWorkspaceAuthResolved({{ shopName: '{}' }}); }}", Self::escape_json(&name));
                for t in &self.tabs {
                    if t.url == "tp://workspace" {
                        let _ = t.webview.evaluate_script(&js);
                    }
                }
                if self.multi_shop_step == "orders" {
                    if let Some(tid) = self.multi_shop_tab_id {
                        if let Some(t) = self.tabs.iter().find(|t| t.id == tid) {
                            let _ = t.webview.evaluate_script("setTimeout(function(){ if (window.__tpRunShopeeScan) window.__tpRunShopeeScan('orders'); }, 1200);");
                        }
                    }
                }
            }
            AppCmd::CancelWorkspaceScan => {
                if let Some(tid) = self.multi_shop_tab_id.take() {
                    self.execute(AppCmd::CloseTabById(tid));
                }
                self.multi_shop_queue.clear();
                self.multi_shop_idx = 0;
                self.multi_shop_auth_pending = false;
                self.multi_shop_step = String::new();
                self.sync_workspace_progress(false, 0, 100, 0, "Đã hủy phiên quét.", "cancelled");
                self.log_ecom("INFO", "QUEUE", "Đã hủy phiên quét theo yêu cầu của người dùng.");
            }
            AppCmd::OpenLogFile => {
                #[cfg(target_os = "windows")]
                {
                    let log_file = self.exe_dir.join("user_data").join("logs").join("ecom_scanner.log");
                    let _ = std::process::Command::new("notepad.exe").arg(&log_file).spawn();
                }
            }
            AppCmd::SkipCurrentShop => {
                if let Some(s_id) = self.multi_shop_queue.get(self.multi_shop_idx).cloned() {
                    self.log_ecom("WARN", "QUEUE", &format!("Người dùng bấm Bỏ Qua shop: {}", s_id));
                    self.multi_shop_stale_shops.insert(s_id);
                }
                self.multi_shop_auth_pending = false;
                if let Some(tab_id) = self.multi_shop_tab_id.take() {
                    self.execute(AppCmd::CloseTabById(tab_id));
                }
                self.multi_shop_idx += 1;
                self.start_next_shop_in_multi_queue();
            }
            AppCmd::LoadSnapshot(key) => {
                self.sync_workspace_command_center_with_key(&key);
            }
            AppCmd::OpenWorkspaceTab { url, shop_id } => {
                let container = shop_id.as_deref().map(|id| format!("shop_{}", id));
                let isolated = container.is_some();
                self.add_tab_internal(&url, isolated, true, None, None, false, container);
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
                                ("folder", JsonVal::Str(String::new())),
                                ("tags", JsonVal::Arr(Vec::new())),
                                ("keyword", JsonVal::Str(String::new())),
                            ]));
                            "★ Đã lưu trang vào Bookmarks"
                        };
                        self.bookmarks_json = JsonVal::Arr(items).to_json();
                        self.bookmarks_rev = self.bookmarks_rev.wrapping_add(1);
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
                if let Some(norm) = Self::normalize_bookmarks_json(&bm_json) {
                    self.bookmarks_json = norm;
                    self.bookmarks_rev = self.bookmarks_rev.wrapping_add(1);
                    self.save_config();
                    self.sync_all_ui();
                    self.broadcast_config_changed();
                }
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
                let payload = JsonVal::obj(vec![
                    ("text", JsonVal::Str(text)),
                    ("source", JsonVal::Str(source)),
                    ("contact", JsonVal::Str(contact)),
                ])
                .to_json();
                let js = format!(
                    "if (window.__tpReceiveTaskFromSelection) window.__tpReceiveTaskFromSelection({});",
                    payload
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
            AppCmd::DiscoverShopeeShops { tab_id } => {
                self.shopee_discovery_retry_attempt = 0;
                self.start_shopee_discovery(tab_id, true, None);
            }
            AppCmd::ShopeeDiscoveryUnavailable => {
                self.shopee_discovery_json = shopee_discovery::unavailable();
                self.sync_workspace_view();
            }
            AppCmd::SetShopeeShopSelection { platform, shop_id, selected } => {
                if !valid_shop_selection(&platform, &shop_id) {
                    return;
                }
                let selection = ShopSelection { platform, shop_id };
                self.selected_shops.retain(|item| item != &selection);
                if selected {
                    self.selected_shops.push(selection);
                }
                self.save_config();
                self.sync_workspace_view();
            }
            AppCmd::NavigationEvent {
                tab_id,
                navigation_id,
                phase,
                success,
            } => {
                let mut retry_discovery = false;
                if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) {
                    match phase {
                        NavigationPhase::Starting => tab.navigation_readiness.start(navigation_id),
                        NavigationPhase::ContentLoading => tab.navigation_readiness.content_loading(navigation_id),
                        NavigationPhase::Completed => {
                            tab.navigation_readiness.completed(navigation_id, success);
                            retry_discovery = self.shopee_discovery_tab_id == Some(tab_id)
                                && success
                                && shopee_discovery::valid_url(&tab.url)
                                && tab.navigation_readiness.ready_navigation_id().is_some();
                        }
                    }
                }
                if retry_discovery {
                    self.shopee_discovery_tab_id = None;
                    self.start_shopee_discovery(Some(tab_id), true, None);
                }
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
                if self.shopee_discovery_tab_id == Some(tab_id)
                    && shopee_discovery::valid_url(&url)
                    && self.tabs.iter().find(|t| t.id == tab_id)
                        .and_then(|t| t.navigation_readiness.ready_navigation_id()).is_some()
                {
                    self.shopee_discovery_tab_id = None;
                    self.start_shopee_discovery(Some(tab_id), true, None);
                } else if self.multi_shop_auth_pending && self.multi_shop_tab_id == Some(tab_id)
                    && url.contains("banhang.shopee.vn")
                    && !url.contains("accounts.shopee.vn")
                    && !url.contains("/seller/login")
                    && !url.contains("captcha")
                {
                    if let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                        t.worker_kind = Some("bot".to_string());
                    }
                    self.execute(AppCmd::ScanAuthResolved);
                    self.trigger_worker_scan_by_id(tab_id, &url);
                    self.sync_workspace_view();
                    return;
                } else if is_bot_worker {
                    self.trigger_worker_scan_by_id(tab_id, &url);
                    self.sync_workspace_view();
                    return;
                } else if url == "https://banhang.shopee.vn/portal/shop" {
                    if let Some(shop_id) = self.shopee_open_target_id.take() {
                        if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) {
                            let js = format!("window.__tpShopTarget={{shop_id:{:?}}}; setTimeout(function(){{ {} }}, 900);", shop_id, SHOPEE_OPEN_SHOP_JS);
                            let _ = tab.webview.evaluate_script(&js);
                        }
                    }
                }
                let is_private = self.tabs.iter().find(|t| t.id == tab_id).is_some_and(|t| t.private);
                if !is_private && !finished_title.is_empty() {
                    self.record_history(&finished_title, &url);
                }
                if url == "tp://history" {
                    self.sync_history_view();
                } else if url == "tp://workspace" {
                    self.sync_workspace_view();
                }
                self.sync_ai_config_to_panel(false);
                self.sync_all_ui();
                if !is_private {
                    self.save_session();
                }
            }
            AppCmd::McpDispatch(msg) => {
                self.handle_mcp_msg(msg);
            }
            AppCmd::ShopeeNativeResult { req_id, tab_id, payload } => {
                let navigation_id = self.tabs.iter().find(|t| t.id == tab_id).and_then(|t| t.navigation_readiness.ready_navigation_id());
                if let Some((reply, in_time)) = shopee_discovery::take_pending(&mut self.shopee_pending, req_id, tab_id, navigation_id, Instant::now()) {
                    let result = if in_time {
                        self.tabs.iter().find(|t| t.id == tab_id)
                            .and_then(|t| t.webview.url().ok())
                            .zip(payload.0)
                            .map(|(url, raw)| shopee_discovery::normalize(&url, &raw))
                            .unwrap_or_else(shopee_discovery::unavailable)
                    } else { shopee_discovery::unavailable() };
                    let retry_status = parse_json(&result)
                        .ok()
                        .and_then(|v| v.get("status").and_then(JsonVal::as_str).map(str::to_owned))
                                                .is_some_and(|status| matches!(status.as_str(), "loading" | "unavailable" | "no_data"));
                    let _ = reply.send(Ok(result.clone()));
                    if self.shopee_workspace_pending.remove(&req_id) {
                        if retry_status {
                            self.shopee_discovery_json = shopee_discovery::result_loading();
                            let attempt = self.shopee_discovery_retry_attempt.saturating_add(1);
                            if attempt <= 8 {
                                let proxy = self.proxy.clone();
                                std::thread::spawn(move || {
                                    std::thread::sleep(std::time::Duration::from_millis(700));
                                    let _ = proxy.send_event(AppCmd::RetryShopeeDiscovery { tab_id, attempt });
                                });
                            }
                        } else {
                            self.shopee_discovery_json = workspace_discovery_payload(&result);
                        }
                        self.sync_workspace_view();
                    }
                }
            }
            AppCmd::RetryShopeeDiscovery { tab_id, attempt } => {
                if attempt <= 8 && self.tabs.iter().any(|tab| tab.id == tab_id) {
                    self.shopee_discovery_retry_attempt = attempt;
                    self.start_shopee_discovery(Some(tab_id), true, None);
                }
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
            AppCmd::CloseCommandBar => {
                if self.command_bar_open {
                    self.command_bar_open = false;
                    let _ = self.tab_shell.evaluate_script(
                        "if (window.__tpCloseCommandBar) window.__tpCloseCommandBar(true);",
                    );
                    let _ = self.titlebar_shell.evaluate_script(
                        "if (window.__tpCloseCommandBar) window.__tpCloseCommandBar(true);",
                    );
                    self.relayout();
                    if let Some(t) = self.tabs.get(self.active_idx) {
                        let _ = t.webview.focus();
                    }
                }
            }
            AppCmd::CopyActiveUrl => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    let clean_enabled = parse_json(&self.context_menu_prefs_json)
                        .ok()
                        .and_then(|v| v.get("clean_copy_link").and_then(JsonVal::as_bool))
                        .unwrap_or(true);
                    let url = if clean_enabled {
                        Self::clean_tracking_url(&tab.url)
                    } else {
                        tab.url.clone()
                    };
                    Self::copy_text_to_clipboard(&url);
                    let _ = tab.webview.evaluate_script(
                        "if (window.__tpShowToast) window.__tpShowToast(\"🔗 Đã sao chép URL hiện tại\");",
                    );
                }
            }
            AppCmd::DuplicateTab(tab_id) => {
                if let Some(src) = self.tabs.iter().find(|t| t.id == tab_id && t.worker_kind.is_none()) {
                    let url = src.url.clone();
                    let isolated = src.profile_name != "default" && !src.profile_name.starts_with("container_");
                    let container_id = Some(src.container_id.clone());
                    self.add_tab_internal(&url, isolated, true, None, None, false, container_id);
                }
            }
            AppCmd::PinTabById(tab_id) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id && t.worker_kind.is_none()) {
                    tab.pinned = !tab.pinned;
                    if tab.pinned {
                        tab.essential = false;
                        tab.pinned_origin_url = Some(tab.url.clone());
                    }
                    self.sync_all_ui();
                    self.save_session();
                }
            }
            AppCmd::CloseOtherTabs(keep_tab_id) => {
                let ws_id = self.active_workspace_id.clone();
                if !self.tabs.iter().any(|t| t.id == keep_tab_id && t.workspace_id == ws_id) {
                    return;
                }
                let to_close: Vec<u64> = self
                    .tabs
                    .iter()
                    .filter(|t| {
                        t.workspace_id == ws_id
                            && t.worker_kind.is_none()
                            && !t.pinned
                            && t.id != keep_tab_id
                    })
                    .map(|t| t.id)
                    .collect();
                for tid in to_close {
                    if let Some(pos) = self.tabs.iter().position(|t| t.id == tid) {
                        self.execute(AppCmd::CloseIndex(pos));
                    }
                }
                if let Some(pos) = self.tabs.iter().position(|t| t.id == keep_tab_id) {
                    self.execute(AppCmd::FocusIndex(pos));
                }
            }
            AppCmd::MoveTabToWorkspace { tab_id, workspace_id } => {
                if !self.workspaces.iter().any(|w| w.id == workspace_id) {
                    return;
                }
                let Some(pos) = self.tabs.iter().position(|t| t.id == tab_id && t.worker_kind.is_none()) else {
                    return;
                };
                if self.tabs[pos].workspace_id == workspace_id {
                    return;
                }
                self.tabs[pos].workspace_id = workspace_id;
                self.tabs[pos].tab_group_id = None;
                self.selected_tab_ids.remove(&tab_id);
                if self.split_paired_tab_id == Some(tab_id) {
                    self.split_paired_tab_id = None;
                    self.view_mode = ContentViewMode::SingleTab;
                }
                if self.tabs.get(self.active_idx).map(|t| t.workspace_id.as_str())
                    != Some(self.active_workspace_id.as_str())
                {
                    if let Some(next_idx) = self.visible_tab_indices().first().copied() {
                        self.active_idx = next_idx;
                    } else {
                        self.add_tab("tp://newtab", false);
                        return;
                    }
                }
                self.relayout();
                self.sync_all_ui();
                self.save_session();
            }
            AppCmd::ReorderTab { source_id, target_id, after } => {
                if source_id == target_id {
                    return;
                }
                let ws_id = self.active_workspace_id.clone();
                let active_id = self.tabs.get(self.active_idx).map(|t| t.id);
                let src_pos = self.tabs.iter().position(|t| {
                    t.id == source_id && t.workspace_id == ws_id && t.worker_kind.is_none()
                });
                let dst_pos = self.tabs.iter().position(|t| {
                    t.id == target_id && t.workspace_id == ws_id && t.worker_kind.is_none()
                });
                let (Some(s_idx), Some(d_idx)) = (src_pos, dst_pos) else {
                    return;
                };
                if self.tabs[s_idx].pinned != self.tabs[d_idx].pinned
                    || self.tabs[s_idx].essential != self.tabs[d_idx].essential
                {
                    return;
                }
                let tab = self.tabs.remove(s_idx);
                let mut insert_idx = if s_idx < d_idx { d_idx - 1 } else { d_idx };
                if after {
                    insert_idx += 1;
                }
                insert_idx = insert_idx.min(self.tabs.len());
                self.tabs.insert(insert_idx, tab);
                if let Some(aid) = active_id {
                    if let Some(new_active) = self.tabs.iter().position(|t| t.id == aid) {
                        self.active_idx = new_active;
                    }
                }
                self.sync_all_ui();
                self.save_session();
            }
            AppCmd::OpenInSplit(raw_url) => {
                let prev_active_id = self.tabs.get(self.active_idx).map(|t| t.id);
                let new_id = self.add_tab_internal(&raw_url, false, false, None, None, false, None);
                if let (Some(prev_id), Some(created_id)) = (prev_active_id, new_id) {
                    if let Some(prev_pos) = self.tabs.iter().position(|t| t.id == prev_id) {
                        self.active_idx = prev_pos;
                    }
                    self.view_mode = ContentViewMode::ColumnsBsp;
                    self.split_paired_tab_id = Some(created_id);
                    self.relayout();
                    self.sync_all_ui();
                    self.save_session();
                    self.save_config();
                }
            }
            AppCmd::AdjustSplitRatio(delta) => {
                if self.view_mode != ContentViewMode::SingleTab {
                    self.master_ratio = (self.master_ratio + delta).clamp(0.25, 0.75);
                    self.relayout();
                }
            }
            AppCmd::ResetSplitRatio => {
                self.master_ratio = 0.5;
                self.relayout();
            }
            AppCmd::ToggleCompactMode => {
                self.compact_mode = !self.compact_mode;
                COMPACT_MODE_ENABLED.store(self.compact_mode, Ordering::Relaxed);
                self.compact_sidebar_hover = false;
                self.relayout();
                self.sync_all_ui();
                self.save_config();
            }
            AppCmd::SetCompactSidebarHover(hover) => {
                if self.compact_mode && self.compact_sidebar_hover != hover {
                    self.compact_sidebar_hover = hover;
                    self.relayout();
                }
            }
            AppCmd::ToggleTabEssential(tab_id) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id && t.worker_kind.is_none()) {
                    tab.essential = !tab.essential;
                    if tab.essential {
                        tab.pinned = false;
                        tab.pinned_origin_url = Some(tab.url.clone());
                    }
                    self.relayout();
                    self.sync_all_ui();
                    self.save_session();
                }
            }
            AppCmd::ResetPinnedTab(tab_id) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                    if let Some(origin) = tab.pinned_origin_url.clone() {
                        if origin != tab.url {
                            tab.url = origin.clone();
                            tab.title = Self::title_from_url(&origin);
                            if origin == "tp://newtab" {
                                let _ = tab.webview.load_html(&inject_ui_html(NEWTAB_HTML, &self.ui_theme));
                            } else if origin == "tp://history" {
                                let _ = tab.webview.load_html(&inject_ui_html(HISTORY_HTML, &self.ui_theme));
                            } else if origin == "tp://workspace" {
                                let _ = tab.webview.load_html(&inject_ui_html(NEWTAB_HTML, &self.ui_theme));
                            } else {
                                let _ = tab.webview.load_url(&origin);
                            }
                        }
                    }
                    self.sync_all_ui();
                    self.save_session();
                }
            }
            AppCmd::ToggleTabFolderCollapsed(folder_id) => {
                if let Some(folder) = self.tab_folders.iter_mut().find(|f| f.id == folder_id) {
                    folder.collapsed = !folder.collapsed;
                    self.sync_all_ui();
                    self.save_session();
                }
            }
            AppCmd::RenameTabFolder { folder_id, name } => {
                let trimmed = name.trim();
                if !trimmed.is_empty() && trimmed.len() <= 64 {
                    if let Some(folder) = self.tab_folders.iter_mut().find(|f| f.id == folder_id) {
                        folder.name = trimmed.to_string();
                        self.sync_all_ui();
                        self.save_session();
                    }
                }
            }
            AppCmd::AddTabToFolder { tab_id, folder_id } => {
                let folder_ws = self.tab_folders.iter().find(|f| f.id == folder_id).map(|f| f.workspace_id.clone());
                if let Some(ws_id) = folder_ws {
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id && t.worker_kind.is_none()) {
                        if tab.workspace_id == ws_id {
                            tab.tab_group_id = Some(folder_id);
                            self.sync_all_ui();
                            self.save_session();
                        }
                    }
                }
            }
            AppCmd::UpdateWorkspaceStyle { id, icon, accent, container_id } => {
                let icon = if valid_workspace_icon(&icon) { icon } else { "✦".to_string() };
                let accent = if valid_workspace_accent(&accent) { accent } else { String::new() };
                let container_id = Self::normalize_container_id(&container_id);
                if let Some(ws) = self.workspaces.iter_mut().find(|w| w.id == id) {
                    ws.icon = icon;
                    ws.accent = accent;
                    ws.container_id = container_id;
                    self.sync_all_ui();
                    self.save_config();
                }
            }
            AppCmd::NextWorkspace => {
                if let Some(pos) = self.workspaces.iter().position(|w| w.id == self.active_workspace_id) {
                    let next = (pos + 1) % self.workspaces.len();
                    let next_id = self.workspaces[next].id.clone();
                    self.execute(AppCmd::SetActiveWorkspace(next_id));
                }
            }
            AppCmd::PrevWorkspace => {
                if let Some(pos) = self.workspaces.iter().position(|w| w.id == self.active_workspace_id) {
                    let prev = (pos + self.workspaces.len() - 1) % self.workspaces.len();
                    let prev_id = self.workspaces[prev].id.clone();
                    self.execute(AppCmd::SetActiveWorkspace(prev_id));
                }
            }
            AppCmd::UnloadTab(tab_id) => {
                self.unload_tab_by_id(tab_id);
                self.save_session();
            }
            AppCmd::GlanceOpenInSplit => {
                if self.glance_open && !self.glance_url.is_empty() {
                    let url = self.glance_url.clone();
                    self.execute(AppCmd::CloseGlance);
                    self.execute(AppCmd::OpenInSplit(url));
                }
            }
            AppCmd::GlanceNavBack => {
                if let Some(body) = &self.glance_body_shell {
                    let _ = body.evaluate_script("window.history.back();");
                }
            }
            AppCmd::GlanceNavForward => {
                if let Some(body) = &self.glance_body_shell {
                    let _ = body.evaluate_script("window.history.forward();");
                }
            }
            AppCmd::GlanceReload => {
                if let Some(body) = &self.glance_body_shell {
                    let _ = body.reload();
                }
            }
            AppCmd::GlanceCopyUrl => {
                if !self.glance_url.is_empty() {
                    Self::copy_text_to_clipboard(&self.glance_url);
                }
            }
            AppCmd::UpdateTabMedia { tab_id, playing, muted, title, artist } => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                    let changed = tab.audible != playing
                        || tab.muted != muted
                        || tab.media_title != title
                        || tab.media_artist != artist;
                    tab.audible = playing;
                    tab.muted = muted;
                    tab.media_title = title;
                    tab.media_artist = artist;
                    if changed {
                        self.sync_all_ui();
                    }
                }
            }
            AppCmd::ToggleTabMute(tab_id) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                    tab.muted = !tab.muted;
                    let mute_js = format!(
                        "if (window.__tpSetTabMuted) window.__tpSetTabMuted({});",
                        tab.muted
                    );
                    let _ = tab.webview.evaluate_script(&mute_js);
                    self.sync_all_ui();
                }
            }
            AppCmd::MediaControl { tab_id, action } => {
                if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) {
                    let safe_action: String = action.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                    let _ = tab.webview.evaluate_script(&format!(
                        "if (window.__tpMediaControl) window.__tpMediaControl('{}');",
                        safe_action
                    ));
                }
            }
            AppCmd::OpenTitlebarPopover => {
                if !self.titlebar_popover_open {
                    self.titlebar_popover_open = true;
                    self.relayout();
                }
            }
            AppCmd::CloseTitlebarPopover => {
                if self.titlebar_popover_open {
                    self.titlebar_popover_open = false;
                    self.relayout();
                }
            }
            AppCmd::TranslateReq { tab_id, req_id, target_lang, texts } => {
                let payload_json = format!(
                    "{{\"tab_id\":{},\"req_id\":{},\"target_lang\":\"{}\",\"texts\":[{}]}}",
                    tab_id,
                    req_id,
                    Self::escape_json(&target_lang),
                    texts.iter().map(|t| format!("\"{}\"", Self::escape_json(t))).collect::<Vec<_>>().join(",")
                );
                let _ = self.titlebar_shell.evaluate_script(&format!(
                    "if (window.__tpExecuteTranslation) window.__tpExecuteTranslation({});",
                    payload_json
                ));
            }
            AppCmd::TranslateRes { tab_id, req_id, translations } => {
                if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) {
                    let res_json = format!(
                        "[{}]",
                        translations.iter().map(|t| format!("\"{}\"", Self::escape_json(t))).collect::<Vec<_>>().join(",")
                    );
                    let _ = tab.webview.evaluate_script(&format!(
                        "if (window.__tpOnTranslated) window.__tpOnTranslated({}, {});",
                        req_id,
                        res_json
                    ));
                }
            }
            AppCmd::SaveDomainBoost {
                domain,
                css,
                zap_selectors,
                enabled,
            } => {
                let norm_domain = Self::normalize_boost_domain(&domain);
                if !norm_domain.is_empty() {
                    let mut pairs = match parse_json(&self.boosts_json) {
                        Ok(JsonVal::Obj(p)) => p,
                        _ => Vec::new(),
                    };
                    pairs.retain(|(k, _)| Self::normalize_boost_domain(k) != norm_domain);
                    pairs.push((
                        norm_domain,
                        JsonVal::obj(vec![
                            ("css", JsonVal::Str(css)),
                            (
                                "zap_selectors",
                                JsonVal::Arr(zap_selectors.into_iter().map(JsonVal::Str).collect()),
                            ),
                            ("enabled", JsonVal::Bool(enabled)),
                        ]),
                    ));
                    if let Some(norm) = Self::normalize_boosts_json(&JsonVal::Obj(pairs).to_json()) {
                        self.boosts_json = norm;
                        self.save_config();
                        self.sync_all_ui();
                        self.broadcast_config_changed();
                    }
                }
            }
            AppCmd::ResetDomainBoost(domain) => {
                let norm_domain = Self::normalize_boost_domain(&domain);
                if !norm_domain.is_empty() {
                    let mut pairs = match parse_json(&self.boosts_json) {
                        Ok(JsonVal::Obj(p)) => p,
                        _ => Vec::new(),
                    };
                    pairs.retain(|(k, _)| Self::normalize_boost_domain(k) != norm_domain);
                    self.boosts_json = Self::normalize_boosts_json(&JsonVal::Obj(pairs).to_json())
                        .unwrap_or_else(|| "{}".to_string());
                    self.save_config();
                    self.sync_all_ui();
                    self.broadcast_config_changed();
                }
            }
            AppCmd::OpenBoostsPanel => {
                if self.settings_open {
                    self.settings_open = false;
                    self.relayout();
                }
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    let _ = tab.webview.focus();
                    let _ = tab.webview.evaluate_script("if (window.__tpOpenBoosts) window.__tpOpenBoosts();");
                }
            }
            AppCmd::StartZapMode => {
                if self.settings_open {
                    self.settings_open = false;
                    self.relayout();
                }
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    let _ = tab.webview.focus();
                    let _ = tab.webview.evaluate_script("if (window.__tpStartZapMode) window.__tpStartZapMode();");
                }
            }
            AppCmd::OpenDownloadedFile(path) => {
                let downloads = Self::downloads_dir();
                let tracked = self.downloads.iter().any(|d| d.path == path);
                if tracked {
                    if let Some(safe_path) = Self::is_safe_download_path(&path, &downloads) {
                        #[cfg(target_os = "windows")]
                        {
                            let _ = std::process::Command::new("explorer.exe")
                                .arg(&safe_path)
                                .spawn();
                        }
                    }
                }
            }
            AppCmd::OpenDownloadFolder(path) => {
                let downloads = Self::downloads_dir();
                let _ = fs::create_dir_all(&downloads);
                #[cfg(target_os = "windows")]
                {
                    if let Some(safe_path) = Self::is_safe_download_path(&path, &downloads) {
                        let _ = std::process::Command::new("explorer.exe")
                            .arg(format!("/select,{}", safe_path.display()))
                            .spawn();
                    } else {
                        let _ = std::process::Command::new("explorer.exe")
                            .arg(&downloads)
                            .spawn();
                    }
                }
            }
            AppCmd::RecoverWindows
            | AppCmd::DismissWindowRecovery
            | AppCmd::SyncWindowGeometry => {}
            AppCmd::PrewarmNewTab => {
                self.ensure_warm_newtab();
            }
            AppCmd::StepMruTab(delta) => {
                if self.command_bar_open {
                    self.command_bar_open = false;
                    let _ = self.tab_shell.evaluate_script(
                        "if (window.__tpCloseCommandBar) window.__tpCloseCommandBar(true);",
                    );
                }
                if self.mru_cycle_ids.is_none() {
                    let candidates = self.mru_candidate_ids();
                    if candidates.is_empty() {
                        return;
                    }
                    self.mru_cycle_ids = Some(candidates);
                    self.mru_cycle_pos = 0;
                }
                let mut ids = self.mru_cycle_ids.clone().unwrap_or_default();
                ids.retain(|id| {
                    self.tabs.iter().any(|t| {
                        t.id == *id
                            && t.workspace_id == self.active_workspace_id
                            && t.worker_kind.is_none()
                    })
                });
                if ids.is_empty() {
                    self.mru_cycle_ids = None;
                    self.mru_hud_open = false;
                    self.relayout();
                    return;
                }
                self.mru_cycle_ids = Some(ids.clone());
                let len = ids.len() as isize;
                let step = if delta == 0 { 1 } else { delta as isize };
                self.mru_cycle_pos = ((self.mru_cycle_pos as isize + step).rem_euclid(len)) as usize;
                let active_id = self.tabs.get(self.active_idx).map(|tab| tab.id);
                let mut items_json = String::from("[");
                for (i, id) in ids.iter().enumerate() {
                    if let Some(t) = self.tabs.iter().find(|t| {
                        t.id == *id
                            && t.workspace_id == self.active_workspace_id
                            && t.worker_kind.is_none()
                    }) {
                        if i > 0 {
                            items_json.push(',');
                        }
                        let domain = t
                            .url
                            .strip_prefix("https://")
                            .or_else(|| t.url.strip_prefix("http://"))
                            .unwrap_or(&t.url)
                            .split('/')
                            .next()
                            .unwrap_or(&t.url);
                        items_json.push_str(&format!(
                            "{{\"id\":{},\"title\":\"{}\",\"url\":\"{}\",\"domain\":\"{}\",\"favicon\":\"{}\",\"pinned\":{},\"essential\":{},\"is_private\":{},\"is_active\":{}}}",
                            t.id,
                            Self::escape_json(&t.title),
                            Self::escape_json(&t.url),
                            Self::escape_json(domain),
                            Self::escape_json(&t.favicon_url),
                            t.pinned,
                            t.essential,
                            t.private,
                            Some(t.id) == active_id,
                        ));
                    }
                }
                items_json.push(']');
                let payload = format!(
                    "{{\"ui_theme\":\"{}\",\"selected_idx\":{},\"tabs\":{}}}",
                    Self::escape_json(&self.ui_theme),
                    self.mru_cycle_pos,
                    items_json
                );
                self.mru_hud_open = true;
                self.relayout();
                let _ = self.tab_shell.evaluate_script(&format!(
                    "if (window.__tpOpenMruSwitcher) window.__tpOpenMruSwitcher({});",
                    payload
                ));
                let _ = self.tab_shell.focus();
            }
            AppCmd::CommitMruTab(target_id) => {
                let chosen_id = target_id.or_else(|| {
                    self.mru_cycle_ids
                        .as_ref()
                        .and_then(|ids| ids.get(self.mru_cycle_pos).copied())
                });
                let was_open = self.mru_hud_open || self.mru_cycle_ids.is_some();
                self.mru_hud_open = false;
                self.mru_cycle_ids = None;
                self.mru_cycle_pos = 0;
                if was_open {
                    let _ = self.tab_shell.evaluate_script(
                        "if (window.__tpCloseMruSwitcher) window.__tpCloseMruSwitcher();",
                    );
                }
                if let Some(id) = chosen_id {
                    if let Some(index) = self.tabs.iter().position(|tab| {
                        tab.id == id
                            && tab.workspace_id == self.active_workspace_id
                            && tab.worker_kind.is_none()
                    }) {
                        if index != self.active_idx {
                            self.execute(AppCmd::FocusIndex(index));
                            return;
                        }
                        self.mru_tab_ids.retain(|existing| *existing != id);
                        self.mru_tab_ids.insert(0, id);
                    }
                }
                if was_open {
                    self.relayout();
                    if let Some(t) = self.tabs.get(self.active_idx) {
                        let _ = t.webview.focus();
                    }
                }
            }
            AppCmd::CancelMruTab => {
                let was_open = self.mru_hud_open || self.mru_cycle_ids.is_some();
                self.mru_hud_open = false;
                self.mru_cycle_ids = None;
                self.mru_cycle_pos = 0;
                if was_open {
                    let _ = self.tab_shell.evaluate_script(
                        "if (window.__tpCloseMruSwitcher) window.__tpCloseMruSwitcher();",
                    );
                    self.relayout();
                    if let Some(t) = self.tabs.get(self.active_idx) {
                        let _ = t.webview.focus();
                    }
                }
            }
            AppCmd::BrowserExtensionsLoaded(raw_exts) => {
                self.apply_loaded_browser_extensions(raw_exts);
                self.sync_all_ui();
            }
            AppCmd::OpenExtensionPopup { id, page } => {
                if id == "ai_cowork" {
                    self.execute(AppCmd::ToggleChatChannel(ChatChannel::Ai));
                } else {
                    if self.loaded_extensions.is_empty() {
                        self.refresh_browser_extensions();
                    }
                    let want_dashboard = matches!(
                        page.as_deref(),
                        Some("dashboard" | "options")
                    );
                    let target_ext = self.loaded_extensions.iter().find(|e| {
                        !e.ext_id.is_empty()
                            && (e.id.eq_ignore_ascii_case(&id)
                                || e.ext_id.eq_ignore_ascii_case(&id)
                                || (id == "ublock" && e.id == "ublock"))
                    }).cloned();
                    if let Some(ext) = target_ext {
                        let rel = if want_dashboard && !ext.options_path.is_empty() {
                            ext.options_path.as_str()
                        } else if !ext.popup_path.is_empty() {
                            ext.popup_path.as_str()
                        } else {
                            ext.options_path.as_str()
                        };
                        if !rel.is_empty() {
                            let url = format!(
                                "chrome-extension://{}/{}",
                                ext.ext_id,
                                rel.trim_start_matches('/')
                            );
                            if want_dashboard {
                                self.add_tab(&url, false);
                            } else {
                                self.execute(AppCmd::OpenGlance {
                                    url,
                                    title: ext.name.clone(),
                                });
                            }
                            return;
                        }
                    }
                    self.execute(AppCmd::OpenSettingsModal("privacy".to_string()));
                }
            }
        }
    }

    fn scan_extension_manifests(&self) -> Vec<(String, String, String, String)> {
        let mut out = Vec::new();
        let ext_dir = self.exe_dir.join("extensions");
        let Ok(entries) = fs::read_dir(&ext_dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let dir_name = entry.file_name().to_string_lossy().to_string();
            if dir_name.starts_with('.') {
                continue;
            }
            let manifest_path = path.join("manifest.json");
            let Ok(raw) = fs::read_to_string(&manifest_path) else {
                continue;
            };
            let parsed = parse_json(&raw).ok();
            let raw_name = parsed
                .as_ref()
                .and_then(|v| v.get("name").and_then(JsonVal::as_str))
                .unwrap_or("")
                .trim()
                .to_string();
            let friendly_name = if raw_name.is_empty() || raw_name.starts_with("__MSG_") {
                if dir_name.to_ascii_lowercase().contains("ubo") {
                    "uBlock Origin Lite + ABPVN".to_string()
                } else {
                    dir_name.clone()
                }
            } else {
                raw_name
            };
            let mut popup_path = parsed
                .as_ref()
                .and_then(|v| {
                    v.get("action")
                        .or_else(|| v.get("browser_action"))
                        .and_then(|a| a.get("default_popup"))
                        .and_then(JsonVal::as_str)
                })
                .unwrap_or("")
                .trim()
                .to_string();
            if popup_path.is_empty() && path.join("popup.html").exists() {
                popup_path = "popup.html".to_string();
            }
            let mut options_path = parsed
                .as_ref()
                .and_then(|v| {
                    v.get("options_ui")
                        .and_then(|o| o.get("page"))
                        .or_else(|| v.get("options_page"))
                        .and_then(JsonVal::as_str)
                })
                .unwrap_or("")
                .trim()
                .to_string();
            if options_path.is_empty() && path.join("dashboard.html").exists() {
                options_path = "dashboard.html".to_string();
            }
            out.push((dir_name, friendly_name, popup_path, options_path));
        }
        out
    }

    fn apply_loaded_browser_extensions(&mut self, raw_exts: Vec<(String, String, bool)>) {
        let manifests = self.scan_extension_manifests();
        let mut next = Vec::new();
        for (ext_id, com_name, enabled) in raw_exts {
            let lower = com_name.to_ascii_lowercase();
            let matched = manifests.iter().find(|(dir_name, fname, _, _)| {
                let dlow = dir_name.to_ascii_lowercase();
                let flow = fname.to_ascii_lowercase();
                lower == flow
                    || (lower.contains("ublock") && (dlow.contains("ubo") || flow.contains("ublock")))
                    || (!dlow.is_empty() && lower.contains(&dlow))
            });
            let is_builtin_wv2 = matched.is_none()
                && (ext_id == "dgiklkfkllikcanfonkcabmbdfmgleag"
                    || ext_id == "mhjfbmdgcfjbbpaeojofohoefgiehjai"
                    || lower.starts_with("microsoft "));
            if is_builtin_wv2 {
                continue;
            }
            let is_ublock = lower.contains("ublock")
                || matched
                    .as_ref()
                    .is_some_and(|(d, _, _, _)| d.to_ascii_lowercase().contains("ubo"));
            let logical_id = if is_ublock {
                "ublock".to_string()
            } else {
                ext_id.clone()
            };
            let name = if is_ublock {
                "uBlock Origin Lite + ABPVN".to_string()
            } else if com_name.trim().is_empty() {
                matched.map(|(_, n, _, _)| n.clone()).unwrap_or_else(|| ext_id.clone())
            } else {
                com_name
            };
            let desc = if is_ublock {
                "Khiên chặn quảng cáo 3 lớp".to_string()
            } else {
                "Tiện ích WebView2".to_string()
            };
            let (popup_path, options_path) = matched
                .map(|(_, _, p, o)| (p.clone(), o.clone()))
                .unwrap_or_else(|| {
                    if is_ublock {
                        ("popup.html".to_string(), "dashboard.html".to_string())
                    } else {
                        (String::new(), String::new())
                    }
                });
            next.push(LoadedExtensionMeta {
                id: logical_id,
                ext_id,
                name,
                desc,
                popup_path,
                options_path,
                enabled,
            });
        }
        if !next.iter().any(|e| e.id == "ublock") {
            if let Some((_, fname, p, o)) = manifests
                .into_iter()
                .find(|(d, _, _, _)| d.to_ascii_lowercase().contains("ubo"))
            {
                next.push(LoadedExtensionMeta {
                    id: "ublock".to_string(),
                    ext_id: String::new(),
                    name: fname,
                    desc: "Khiên chặn quảng cáo 3 lớp".to_string(),
                    popup_path: p,
                    options_path: o,
                    enabled: self.adblock_enabled,
                });
            }
        }
        next.push(LoadedExtensionMeta {
            id: "ai_cowork".to_string(),
            ext_id: String::new(),
            name: "TP AI Cowork".to_string(),
            desc: "Trợ lý tự động hóa tác vụ".to_string(),
            popup_path: String::new(),
            options_path: String::new(),
            enabled: true,
        });
        self.loaded_extensions = next;
    }

    fn extensions_state_json(&self) -> String {
        if self.loaded_extensions.is_empty() {
            return format!(
                "[{{\"id\":\"ublock\",\"ext_id\":\"\",\"name\":\"uBlock Origin Lite + ABPVN\",\"desc\":\"Khiên chặn quảng cáo 3 lớp\",\"enabled\":{},\"has_popup\":false,\"has_options\":false}},{{\"id\":\"ai_cowork\",\"ext_id\":\"\",\"name\":\"TP AI Cowork\",\"desc\":\"Trợ lý tự động hóa tác vụ\",\"enabled\":true,\"has_popup\":true,\"has_options\":false}}]",
                self.adblock_enabled
            );
        }
        let items: Vec<String> = self
            .loaded_extensions
            .iter()
            .map(|e| {
                format!(
                    "{{\"id\":\"{}\",\"ext_id\":\"{}\",\"name\":\"{}\",\"desc\":\"{}\",\"enabled\":{},\"has_popup\":{},\"has_options\":{}}}",
                    Self::escape_json(&e.id),
                    Self::escape_json(&e.ext_id),
                    Self::escape_json(&e.name),
                    Self::escape_json(&e.desc),
                    e.enabled,
                    !e.popup_path.is_empty() || e.id == "ai_cowork",
                    !e.options_path.is_empty(),
                )
            })
            .collect();
        format!("[{}]", items.join(","))
    }

    fn refresh_browser_extensions(&self) {
        #[cfg(target_os = "windows")]
        {
            let proxy = self.proxy.clone();
            let core = self.tab_shell.webview();
            let Ok(wv13) = <ICoreWebView2 as Interface>::cast::<ICoreWebView2_13>(&core) else {
                return;
            };
            let Ok(profile) = (unsafe { wv13.Profile() }) else {
                return;
            };
            let Ok(profile7) = <ICoreWebView2Profile as Interface>::cast::<ICoreWebView2Profile7>(&profile) else {
                return;
            };
            let handler = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(move |err, list| {
                if err.is_err() {
                    return Ok(());
                }
                let mut out = Vec::new();
                if let Some(list) = list {
                    let mut count = 0u32;
                    if unsafe { list.Count(&mut count) }.is_ok() {
                        for idx in 0..count {
                            if let Ok(ext) = unsafe { list.GetValueAtIndex(idx) } {
                                let mut id_ptr = PWSTR::null();
                                let mut name_ptr = PWSTR::null();
                                let mut en = BOOL::default();
                                let id = if unsafe { ext.Id(&mut id_ptr) }.is_ok() {
                                    take_pwstr(id_ptr)
                                } else {
                                    String::new()
                                };
                                let name = if unsafe { ext.Name(&mut name_ptr) }.is_ok() {
                                    take_pwstr(name_ptr)
                                } else {
                                    String::new()
                                };
                                let enabled = unsafe { ext.IsEnabled(&mut en) }.is_ok() && en.as_bool();
                                if !id.is_empty() {
                                    out.push((id, name, enabled));
                                }
                            }
                        }
                    }
                }
                let _ = proxy.send_event(AppCmd::BrowserExtensionsLoaded(out));
                Ok(())
            }));
            let _ = unsafe { profile7.GetBrowserExtensions(&handler) };
        }
    }

    fn compute_mru_candidate_ids<I>(
        active_id: Option<u64>,
        mru_tab_ids: &[u64],
        workspace_tabs: I,
    ) -> Vec<u64>
    where
        I: IntoIterator<Item = u64>,
    {
        let valid_ids: Vec<u64> = workspace_tabs.into_iter().collect();
        let mut out: Vec<u64> = Vec::with_capacity(valid_ids.len());
        if let Some(id) = active_id.filter(|id| valid_ids.contains(id)) {
            out.push(id);
        }
        for &id in mru_tab_ids {
            if valid_ids.contains(&id) && !out.contains(&id) {
                out.push(id);
            }
        }
        for id in valid_ids {
            if !out.contains(&id) {
                out.push(id);
            }
        }
        out
    }

    fn mru_candidate_ids(&self) -> Vec<u64> {
        let active_id = self
            .tabs
            .get(self.active_idx)
            .filter(|t| t.workspace_id == self.active_workspace_id && t.worker_kind.is_none())
            .map(|t| t.id);
        Self::compute_mru_candidate_ids(
            active_id,
            &self.mru_tab_ids,
            self.tabs
                .iter()
                .filter(|t| t.workspace_id == self.active_workspace_id && t.worker_kind.is_none())
                .map(|t| t.id),
        )
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
            return self.settings_shell.as_ref().ok_or_else(|| "Settings WebView unavailable".to_string());
        }
        if t == "ai" {
            return self.ai_shell.as_ref().ok_or_else(|| "AI WebView unavailable".to_string());
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
        let rec_val = parse_json(&self.window_recovery_json).unwrap_or_else(|_| JsonVal::obj(vec![]));
        let window_count = rec_val
            .get("open_windows")
            .and_then(|v| v.as_u64())
            .unwrap_or(1)
            .max(1);
        let windows_val = rec_val
            .get("windows")
            .cloned()
            .unwrap_or_else(|| JsonVal::Arr(vec![]));
        JsonVal::obj(vec![
            ("active_index", JsonVal::Num(self.active_idx as f64)),
            ("tab_count", JsonVal::Num(self.tabs.len() as f64)),
            ("window_count", JsonVal::Num(window_count as f64)),
            ("view_mode", JsonVal::Str(self.view_mode.as_str().to_string())),
            ("ai_mode", JsonVal::Str(self.ai_mode.as_str().to_string())),
            (
                "chat_channel",
                JsonVal::Str(self.chat_channel.as_str().to_string()),
            ),
            ("tabs", JsonVal::Arr(arr)),
            ("windows", windows_val),
            ("recovery", rec_val),
        ])
        .to_json()
    }

    fn handle_mcp_msg(&mut self, msg: McpBridgeMsg) {
        match msg {
            McpBridgeMsg::WakeWindow => {
                self.wake_and_show_window();
            }
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
                    "tpbrowser://windows" => Ok(self.window_recovery_json.clone()),
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
                    "tp_window_recovery_status" => {
                        let _ = reply.send(Ok(self.window_recovery_json.clone()));
                    }
                    "tp_recover_windows" => {
                        let _ = self.proxy.send_event(AppCmd::RecoverWindows);
                        let state = parse_json(&self.window_recovery_json)
                            .unwrap_or_else(|_| JsonVal::obj(vec![]));
                        let out = JsonVal::obj(vec![
                            ("ok", JsonVal::Bool(true)),
                            ("recovery_triggered", JsonVal::Bool(true)),
                            ("state", state),
                        ])
                        .to_json();
                        let _ = reply.send(Ok(out));
                    }
                    "tp_discover_shopee_shops" => {
                        let tab = match shopee_discovery::parse_tab_id(args.get("tab_id")) {
                            Ok(Some(id)) => self.tabs.iter().find(|t| t.id == id),
                            Ok(None) => self.tabs.get(self.active_idx),
                            Err(()) => { let _ = reply.send(Ok(shopee_discovery::unavailable())); return; }
                        };
                        let Some(tab) = tab else { let _ = reply.send(Ok(shopee_discovery::unavailable())); return; };
                        let tab_id = tab.id;
                        let Some(saved_navigation_id) = tab.navigation_readiness.ready_navigation_id() else {
                            let _ = reply.send(Ok(shopee_discovery::unavailable())); return;
                        };
                        if !shopee_discovery::admit_navigation(&tab.navigation_readiness, Some(saved_navigation_id))
                            || !tab.webview.url().ok().is_some_and(|u| shopee_discovery::valid_url(&u))
                        {
                            let _ = reply.send(Ok(shopee_discovery::unavailable())); return;
                        }
                        let req_id = self.next_mcp_req_id;
                        self.next_mcp_req_id = self.next_mcp_req_id.wrapping_add(1).max(1);
                        if let Err(reply) = shopee_discovery::insert_pending(&mut self.shopee_pending, req_id, (tab_id, saved_navigation_id, Instant::now() + shopee_discovery::TIMEOUT, reply)) {
                            let _ = reply.send(Ok(shopee_discovery::unavailable())); return;
                        }
                        let proxy = self.proxy.clone();
                        let eval = tab.webview.evaluate_script_with_callback(shopee_discovery::SCRIPT, move |payload| {
                            let payload = if payload.len() <= 128 * 1024 { Some(payload) } else { None };
                            let _ = proxy.send_event(AppCmd::ShopeeNativeResult { req_id, tab_id, payload: ShopeePayload(payload) });
                        });
                        if eval.is_err() {
                            if let Some((_, _, _, reply)) = self.shopee_pending.remove(&req_id) { let _ = reply.send(Ok(shopee_discovery::unavailable())); }
                        }
                    }
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
                    "tp_get_scan_state" => {
                        let logs_arr: Vec<JsonVal> = self
                            .ecom_logs
                            .iter()
                            .rev()
                            .take(30)
                            .rev()
                            .map(|s| JsonVal::Str(s.clone()))
                            .collect();
                        let queue_arr: Vec<JsonVal> = self
                            .multi_shop_queue
                            .iter()
                            .map(|s| JsonVal::Str(s.clone()))
                            .collect();
                        let current_shop = self
                            .shopee_shop_scan_current
                            .as_ref()
                            .map(|(id, name)| format!("{} ({})", name, id))
                            .unwrap_or_default();
                        let res = JsonVal::obj(vec![
                            ("multi_shop_step", JsonVal::Str(self.multi_shop_step.clone())),
                            ("multi_shop_idx", JsonVal::Num(self.multi_shop_idx as f64)),
                            ("multi_shop_queue", JsonVal::Arr(queue_arr)),
                            ("multi_shop_tab_id", JsonVal::Num(self.multi_shop_tab_id.unwrap_or(0) as f64)),
                            ("shopee_shop_scan_phase", JsonVal::Str(self.shopee_shop_scan_phase.clone())),
                            ("shopee_shop_scan_current", JsonVal::Str(current_shop)),
                            ("multi_shop_auth_pending", JsonVal::Bool(self.multi_shop_auth_pending)),
                            ("logs", JsonVal::Arr(logs_arr)),
                        ]);
                        let _ = reply.send(Ok(res.to_json()));
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct WindowBounds {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    maximized: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WorkAreaRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecoveryTabSnapshot {
    url: String,
    title: String,
    profile: String,
    container_id: String,
    workspace_id: String,
    pinned: bool,
    essential: bool,
    pinned_origin_url: String,
    tab_group_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecoveryWindowSnapshot {
    role: String,
    window_idx: usize,
    bounds: WindowBounds,
    active_workspace_id: String,
    active_idx: usize,
    view_mode: String,
    tabs: Vec<RecoveryTabSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecoverySnapshot {
    version: u32,
    clean_shutdown: bool,
    updated_at_epoch: u64,
    primary: RecoveryWindowSnapshot,
    secondary: Vec<RecoveryWindowSnapshot>,
}

fn clamp_window_bounds_to_work_area(bounds: WindowBounds, work: WorkAreaRect) -> WindowBounds {
    let work_w = (work.right - work.left).max(860) as u32;
    let work_h = (work.bottom - work.top).max(560) as u32;
    let width = bounds.width.clamp(860, work_w);
    let height = bounds.height.clamp(560, work_h);
    let max_x = (work.right - width as i32).max(work.left);
    let max_y = (work.bottom - height as i32).max(work.top);
    WindowBounds {
        x: bounds.x.clamp(work.left, max_x),
        y: bounds.y.clamp(work.top, max_y),
        width,
        height,
        maximized: bounds.maximized,
    }
}

fn is_window_offscreen(bounds: &WindowBounds, work: &WorkAreaRect) -> bool {
    let right = bounds.x.saturating_add(bounds.width as i32);
    let overlap_w = right.min(work.right) - bounds.x.max(work.left);
    overlap_w < 120 || bounds.y < work.top - 16 || bounds.y > work.bottom - 48
}

#[cfg(target_os = "windows")]
fn work_area_for_point(x: i32, y: i32) -> Option<WorkAreaRect> {
    const MONITOR_DEFAULTTONEAREST: u32 = 2;
    unsafe {
        let monitor = MonitorFromPoint(WinPoint { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut info = MonitorInfo {
            cb_size: std::mem::size_of::<MonitorInfo>() as u32,
            monitor: WinRect { left: 0, top: 0, right: 0, bottom: 0 },
            work: WinRect { left: 0, top: 0, right: 0, bottom: 0 },
            flags: 0,
        };
        if monitor != 0 && GetMonitorInfoW(monitor, &mut info) != 0 {
            Some(WorkAreaRect {
                left: info.work.left,
                top: info.work.top,
                right: info.work.right,
                bottom: info.work.bottom,
            })
        } else {
            None
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn work_area_for_point(_x: i32, _y: i32) -> Option<WorkAreaRect> {
    None
}

#[cfg(target_os = "windows")]
fn clamp_live_window_to_monitor(window: &tao::window::Window) -> bool {
    if window.is_maximized() {
        return false;
    }
    let Ok(pos) = window.outer_position() else {
        return false;
    };
    let size = window.inner_size();
    let current = WindowBounds {
        x: pos.x,
        y: pos.y,
        width: size.width.max(860),
        height: size.height.max(560),
        maximized: false,
    };
    let anchor_x = pos.x.saturating_add((size.width as i32) / 2);
    let anchor_y = pos.y.saturating_add(24);
    let Some(work) = work_area_for_point(anchor_x, anchor_y) else {
        return false;
    };
    let work_w = (work.right - work.left).max(860) as u32;
    let work_h = (work.bottom - work.top).max(560) as u32;
    let needs_clamp = is_window_offscreen(&current, &work)
        || current.width > work_w
        || current.height > work_h;
    if !needs_clamp {
        return false;
    }
    let clamped = clamp_window_bounds_to_work_area(current, work);
    window.set_outer_position(PhysicalPosition::new(clamped.x, clamped.y));
    if clamped.width != size.width || clamped.height != size.height {
        window.set_inner_size(PhysicalSize::new(clamped.width, clamped.height));
    }
    true
}

#[cfg(not(target_os = "windows"))]
fn clamp_live_window_to_monitor(_window: &tao::window::Window) -> bool {
    false
}

fn recovery_window_to_json(win: &RecoveryWindowSnapshot) -> JsonVal {
    let tabs_arr: Vec<JsonVal> = win
        .tabs
        .iter()
        .map(|tab| {
            JsonVal::obj(vec![
                ("url", JsonVal::Str(tab.url.clone())),
                ("title", JsonVal::Str(tab.title.clone())),
                ("profile", JsonVal::Str(tab.profile.clone())),
                ("container_id", JsonVal::Str(tab.container_id.clone())),
                ("workspace_id", JsonVal::Str(tab.workspace_id.clone())),
                ("pinned", JsonVal::Bool(tab.pinned)),
                ("essential", JsonVal::Bool(tab.essential)),
                ("pinned_origin_url", JsonVal::Str(tab.pinned_origin_url.clone())),
                ("tab_group_id", JsonVal::Num(tab.tab_group_id as f64)),
            ])
        })
        .collect();
    JsonVal::obj(vec![
        ("role", JsonVal::Str(win.role.clone())),
        ("window_idx", JsonVal::Num(win.window_idx as f64)),
        (
            "bounds",
            JsonVal::obj(vec![
                ("x", JsonVal::Num(win.bounds.x as f64)),
                ("y", JsonVal::Num(win.bounds.y as f64)),
                ("width", JsonVal::Num(win.bounds.width as f64)),
                ("height", JsonVal::Num(win.bounds.height as f64)),
                ("maximized", JsonVal::Bool(win.bounds.maximized)),
            ]),
        ),
        (
            "active_workspace_id",
            JsonVal::Str(win.active_workspace_id.clone()),
        ),
        ("active_idx", JsonVal::Num(win.active_idx as f64)),
        ("view_mode", JsonVal::Str(win.view_mode.clone())),
        ("tab_count", JsonVal::Num(win.tabs.len() as f64)),
        ("tabs", JsonVal::Arr(tabs_arr)),
    ])
}

fn serialize_recovery_snapshot(snapshot: &RecoverySnapshot) -> String {
    let sec_arr: Vec<JsonVal> = snapshot.secondary.iter().map(recovery_window_to_json).collect();
    JsonVal::obj(vec![
        ("version", JsonVal::Num(snapshot.version as f64)),
        ("clean_shutdown", JsonVal::Bool(snapshot.clean_shutdown)),
        (
            "updated_at_epoch",
            JsonVal::Num(snapshot.updated_at_epoch as f64),
        ),
        ("primary", recovery_window_to_json(&snapshot.primary)),
        ("secondary", JsonVal::Arr(sec_arr)),
    ])
    .to_json()
}

fn parse_recovery_window(val: &JsonVal, default_role: &str, default_idx: usize) -> Option<RecoveryWindowSnapshot> {
    let b = val.get("bounds")?;
    let x = b.get("x").and_then(JsonVal::as_i64).unwrap_or(80) as i32;
    let y = b.get("y").and_then(JsonVal::as_i64).unwrap_or(50) as i32;
    let width = b.get("width").and_then(JsonVal::as_u64).unwrap_or(1480).clamp(860, 3840) as u32;
    let height = b.get("height").and_then(JsonVal::as_u64).unwrap_or(900).clamp(560, 2160) as u32;
    let maximized = b.get("maximized").and_then(JsonVal::as_bool).unwrap_or(false);
    let tabs = val
        .get("tabs")
        .and_then(JsonVal::as_arr)
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let url = item.get("url").and_then(JsonVal::as_str)?.trim().to_string();
                    if url.is_empty() {
                        return None;
                    }
                    Some(RecoveryTabSnapshot {
                        url,
                        title: item.get("title").and_then(JsonVal::as_str).unwrap_or("").to_string(),
                        profile: item.get("profile").and_then(JsonVal::as_str).unwrap_or("default").to_string(),
                        container_id: item.get("container_id").and_then(JsonVal::as_str).unwrap_or("default").to_string(),
                        workspace_id: item.get("workspace_id").and_then(JsonVal::as_str).unwrap_or(DEFAULT_WORKSPACE_ID).to_string(),
                        pinned: item.get("pinned").and_then(JsonVal::as_bool).unwrap_or(false),
                        essential: item.get("essential").and_then(JsonVal::as_bool).unwrap_or(false),
                        pinned_origin_url: item.get("pinned_origin_url").and_then(JsonVal::as_str).unwrap_or("").to_string(),
                        tab_group_id: item.get("tab_group_id").and_then(JsonVal::as_u64).unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(RecoveryWindowSnapshot {
        role: val
            .get("role")
            .and_then(JsonVal::as_str)
            .unwrap_or(default_role)
            .to_string(),
        window_idx: val
            .get("window_idx")
            .and_then(JsonVal::as_u64)
            .map(|n| n as usize)
            .unwrap_or(default_idx),
        bounds: WindowBounds {
            x,
            y,
            width,
            height,
            maximized,
        },
        active_workspace_id: val
            .get("active_workspace_id")
            .and_then(JsonVal::as_str)
            .unwrap_or(DEFAULT_WORKSPACE_ID)
            .to_string(),
        active_idx: val
            .get("active_idx")
            .and_then(JsonVal::as_u64)
            .map(|n| n as usize)
            .unwrap_or(0),
        view_mode: val
            .get("view_mode")
            .and_then(JsonVal::as_str)
            .unwrap_or("single")
            .to_string(),
        tabs,
    })
}

fn parse_recovery_snapshot(raw: &str) -> Option<RecoverySnapshot> {
    let val = parse_json(raw).ok()?;
    let primary = parse_recovery_window(val.get("primary")?, "primary", 0)?;
    let secondary = val
        .get("secondary")
        .and_then(JsonVal::as_arr)
        .map(|arr| {
            arr.iter()
                .enumerate()
                .filter_map(|(idx, item)| parse_recovery_window(item, "secondary", idx + 1))
                .collect()
        })
        .unwrap_or_default();
    Some(RecoverySnapshot {
        version: val.get("version").and_then(JsonVal::as_u64).unwrap_or(1) as u32,
        clean_shutdown: val.get("clean_shutdown").and_then(JsonVal::as_bool).unwrap_or(true),
        updated_at_epoch: val.get("updated_at_epoch").and_then(JsonVal::as_u64).unwrap_or(0),
        primary,
        secondary,
    })
}

fn recovery_needed(snapshot: &RecoverySnapshot) -> bool {
    !snapshot.clean_shutdown || !snapshot.secondary.is_empty()
}

fn capture_window_snapshot(app: &TpBrowserApp, role: &str, window_idx: usize) -> RecoveryWindowSnapshot {
    let (x, y) = app
        .window
        .outer_position()
        .map(|p| (p.x, p.y))
        .unwrap_or((80, 50));
    let sz = app.window.inner_size();
    let user_tabs: Vec<&BrowserTab> = app
        .tabs
        .iter()
        .filter(|t| t.worker_kind.is_none())
        .collect();
    let active_tab_id = app.tabs.get(app.active_idx).map(|t| t.id);
    let active_idx = active_tab_id
        .and_then(|id| user_tabs.iter().position(|t| t.id == id))
        .unwrap_or(0);
    let tabs = user_tabs
        .into_iter()
        .map(|t| RecoveryTabSnapshot {
            url: t.url.clone(),
            title: t.title.clone(),
            profile: t.profile_name.clone(),
            container_id: t.container_id.clone(),
            workspace_id: t.workspace_id.clone(),
            pinned: t.pinned,
            essential: t.essential,
            pinned_origin_url: t.pinned_origin_url.clone().unwrap_or_default(),
            tab_group_id: t.tab_group_id.unwrap_or(0),
        })
        .collect();
    RecoveryWindowSnapshot {
        role: role.to_string(),
        window_idx,
        bounds: WindowBounds {
            x,
            y,
            width: sz.width.max(860),
            height: sz.height.max(560),
            maximized: app.window.is_maximized(),
        },
        active_workspace_id: app.active_workspace_id.clone(),
        active_idx,
        view_mode: app.view_mode.as_str().to_string(),
        tabs,
    }
}

fn restore_window_tabs(app: &mut TpBrowserApp, snap: &RecoveryWindowSnapshot, replace_blank_newtab: bool) {
    if snap.tabs.is_empty() {
        return;
    }
    let has_non_newtab = snap.tabs.iter().any(|t| t.url != "tp://newtab");
    if replace_blank_newtab
        && has_non_newtab
        && app.tabs.len() == 1
        && app.tabs[0].url == "tp://newtab"
    {
        app.tabs.clear();
        app.active_idx = 0;
    }
    for (i, item) in snap.tabs.iter().enumerate() {
        if !replace_blank_newtab
            && (item.url == "tp://newtab" || app.tabs.iter().any(|t| t.url == item.url))
        {
            continue;
        }
        let isolated = item.profile != "default";
        let lazy = !app.tabs.is_empty() && i != snap.active_idx;
        let cid = Some(TpBrowserApp::normalize_container_id(&item.container_id));
        app.add_tab_internal(&item.url, isolated, false, None, None, lazy, cid);
        if let Some(tab) = app.tabs.last_mut() {
            tab.workspace_id = if app.workspaces.iter().any(|w| w.id == item.workspace_id) {
                item.workspace_id.clone()
            } else {
                DEFAULT_WORKSPACE_ID.to_string()
            };
            tab.pinned = item.pinned;
            tab.essential = item.essential;
            if !item.pinned_origin_url.is_empty() {
                tab.pinned_origin_url = Some(item.pinned_origin_url.clone());
            } else if tab.pinned || tab.essential {
                tab.pinned_origin_url = Some(tab.url.clone());
            }
            if item.tab_group_id > 0 {
                tab.tab_group_id = Some(item.tab_group_id);
                app.next_tab_group_id = app.next_tab_group_id.max(item.tab_group_id.saturating_add(1));
            }
        }
    }
    if app.tabs.is_empty() {
        app.add_tab("tp://newtab", false);
    } else if snap.active_idx < app.tabs.len() {
        app.execute(AppCmd::FocusIndex(snap.active_idx));
    }
    if app.workspaces.iter().any(|w| w.id == snap.active_workspace_id) {
        app.active_workspace_id = snap.active_workspace_id.clone();
    }
    app.view_mode = ContentViewMode::from_str(&snap.view_mode);
    app.relayout();
    app.sync_all_ui();
}

struct WindowRegistry {
    primary_window_id: WindowId,
    secondary: HashMap<WindowId, TpBrowserApp>,
    detach_in_flight: HashSet<(WindowId, u64)>,
    pending_recovery: Option<RecoverySnapshot>,
    crash_detected: bool,
    recovery_dirty_since: Option<Instant>,
    last_sync_epoch: u64,
}

impl WindowRegistry {
    fn new(
        primary_window_id: WindowId,
        pending_recovery: Option<RecoverySnapshot>,
        crash_detected: bool,
    ) -> Self {
        Self {
            primary_window_id,
            secondary: HashMap::new(),
            detach_in_flight: HashSet::new(),
            pending_recovery,
            crash_detected,
            recovery_dirty_since: None,
            last_sync_epoch: 0,
        }
    }

    fn mark_recovery_dirty(&mut self) {
        if self.recovery_dirty_since.is_none() {
            self.recovery_dirty_since = Some(Instant::now());
        }
    }

    fn recovery_deadline(&self) -> Option<Instant> {
        self.recovery_dirty_since
            .map(|t| t + std::time::Duration::from_millis(650))
    }

    fn recovery_due(&self) -> bool {
        self.recovery_dirty_since
            .is_some_and(|t| t.elapsed() >= std::time::Duration::from_millis(650))
    }

    fn snapshot_live(&self, primary: &TpBrowserApp, clean_shutdown: bool, now_epoch: u64) -> RecoverySnapshot {
        let primary_snap = capture_window_snapshot(primary, "primary", 0);
        let mut secondary_snaps: Vec<RecoveryWindowSnapshot> = self
            .secondary
            .values()
            .enumerate()
            .map(|(i, win)| capture_window_snapshot(win, "secondary", i + 1))
            .collect();
        secondary_snaps.sort_by_key(|w| (w.bounds.x, w.bounds.y));
        RecoverySnapshot {
            version: 1,
            clean_shutdown,
            updated_at_epoch: now_epoch,
            primary: primary_snap,
            secondary: secondary_snaps,
        }
    }

    fn sync_window_summary_into(&self, primary: &mut TpBrowserApp, snapshot: &RecoverySnapshot) {
        let mut windows_json = Vec::with_capacity(1 + snapshot.secondary.len());
        windows_json.push(recovery_window_to_json(&snapshot.primary));
        for sec in &snapshot.secondary {
            windows_json.push(recovery_window_to_json(sec));
        }
        let open_windows = 1 + snapshot.secondary.len();
        let sec_tabs: usize = snapshot.secondary.iter().map(|w| w.tabs.len()).sum();
        let total_tabs = snapshot.primary.tabs.len() + sec_tabs;
        let pending_win = self
            .pending_recovery
            .as_ref()
            .map(|p| p.secondary.len())
            .unwrap_or(0);
        let pending_tabs: usize = self
            .pending_recovery
            .as_ref()
            .map(|p| p.secondary.iter().map(|w| w.tabs.len()).sum::<usize>() + p.primary.tabs.len())
            .unwrap_or(0);
        primary.window_recovery_json = JsonVal::obj(vec![
            ("open_windows", JsonVal::Num(open_windows as f64)),
            ("secondary_windows", JsonVal::Num(snapshot.secondary.len() as f64)),
            ("primary_tabs", JsonVal::Num(snapshot.primary.tabs.len() as f64)),
            ("total_tabs", JsonVal::Num(total_tabs as f64)),
            ("clean_shutdown", JsonVal::Bool(snapshot.clean_shutdown)),
            ("crash_detected", JsonVal::Bool(self.crash_detected)),
            ("pending_recovery_windows", JsonVal::Num(pending_win as f64)),
            ("pending_recovery_tabs", JsonVal::Num(pending_tabs as f64)),
            ("updated_at_epoch", JsonVal::Num(self.last_sync_epoch as f64)),
            ("windows", JsonVal::Arr(windows_json)),
        ])
        .to_json();
    }

    fn persist_recovery_snapshot(&mut self, primary: &mut TpBrowserApp, clean_shutdown: bool) {
        self.recovery_dirty_since = None;
        let now_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_sync_epoch = now_epoch;
        let live_snap = self.snapshot_live(primary, clean_shutdown, now_epoch);
        self.sync_window_summary_into(primary, &live_snap);
        if !primary.persist_session {
            return;
        }
        let mut disk_snap = live_snap;
        if !clean_shutdown && disk_snap.secondary.is_empty() {
            if let Some(ref pending) = self.pending_recovery {
                if !pending.secondary.is_empty() {
                    disk_snap.secondary = pending.secondary.clone();
                }
            }
        }
        let body = serialize_recovery_snapshot(&disk_snap);
        let _ = fs::write(primary.exe_dir.join("window_recovery.json"), body);
    }

    fn dismiss_recovery(&mut self, primary: &mut TpBrowserApp) {
        self.pending_recovery = None;
        self.crash_detected = false;
        self.persist_recovery_snapshot(primary, false);
        primary.sync_settings_modal(None);
    }

    fn sync_all_window_geometries(&mut self, primary: &mut TpBrowserApp) {
        let _ = clamp_live_window_to_monitor(&primary.window);
        primary.relayout();
        primary.sync_all_ui();
        for secondary in self.secondary.values_mut() {
            let _ = clamp_live_window_to_monitor(&secondary.window);
            secondary.relayout();
            secondary.sync_all_ui();
        }
        self.persist_recovery_snapshot(primary, false);
        primary.sync_settings_modal(None);
    }

    fn recover_windows(
        &mut self,
        target: &EventLoopWindowTarget<AppEvent>,
        event_loop: &EventLoopProxy<AppEvent>,
        exe_dir: &PathBuf,
        primary: &mut TpBrowserApp,
    ) -> usize {
        let mut restored_count = 0usize;
        if let Some(snapshot) = self.pending_recovery.take() {
            let replace_blank = primary.tabs.len() == 1 && primary.tabs[0].url == "tp://newtab";
            restore_window_tabs(primary, &snapshot.primary, replace_blank);
            if snapshot.primary.bounds.maximized && !primary.window.is_maximized() {
                primary.window.set_maximized(true);
            }
            for sec_snap in snapshot.secondary {
                let fallback_work = work_area_for_point(sec_snap.bounds.x, sec_snap.bounds.y)
                    .unwrap_or(WorkAreaRect { left: 0, top: 0, right: 1920, bottom: 1080 });
                let clamped = clamp_window_bounds_to_work_area(sec_snap.bounds.clone(), fallback_work);
                if let Ok(sec_id) = self.create_secondary(
                    target,
                    event_loop.clone(),
                    exe_dir.clone(),
                    (clamped.width, clamped.height),
                    false,
                    Some(WinPoint { x: clamped.x, y: clamped.y }),
                ) {
                    if let Some(sec_app) = self.secondary.get_mut(&sec_id) {
                        restore_window_tabs(sec_app, &sec_snap, true);
                        if clamped.maximized {
                            sec_app.window.set_maximized(true);
                        }
                        let _ = clamp_live_window_to_monitor(&sec_app.window);
                    }
                    restored_count += 1;
                }
            }
        }
        self.crash_detected = false;
        self.sync_all_window_geometries(primary);
        primary.save_session();
        restored_count
    }

    fn create_secondary(
        &mut self,
        target: &EventLoopWindowTarget<AppEvent>,
        event_loop: EventLoopProxy<AppEvent>,
        exe_dir: PathBuf,
        size: (u32, u32),
        staging: bool,
        position: Option<WinPoint>,
    ) -> wry::Result<WindowId> {
        let started = Instant::now();
        let mut builder = WindowBuilder::new()
            .with_title("TPbrowser — Cửa sổ mới")
            .with_decorations(false)
            .with_visible(!staging)
            .with_window_icon(TpBrowserApp::build_window_icon("ribbon", "obsidian-gold"))
            // `size` and monitor work areas are physical pixels. Do not convert
            // source dimensions to logical units before clamping/placement.
            .with_inner_size(PhysicalSize::new(size.0.max(860), size.1.max(560)))
            .with_min_inner_size(PhysicalSize::new(860, 560));
        if let Some(position) = position.map(|point| Self::clamp_secondary_position(point, size)) {
            builder = builder.with_position(PhysicalPosition::new(position.x, position.y));
        }
        #[cfg(target_os = "windows")]
        { builder = builder.with_undecorated_shadow(true); }
        let window = builder.build(target)
            .map_err(|err| wry::Error::Io(std::io::Error::other(err.to_string())))?;
        let window_id = window.id();
        let proxy = WindowProxy::new(window_id, event_loop);
        let mut app = TpBrowserApp::new(window, proxy, exe_dir, false, true)?;
        if !staging {
            app.add_tab("tp://newtab", false);
        }
        self.secondary.insert(window_id, app);
        TpBrowserApp::log_perf(
            if staging { "secondary staging" } else { "secondary ready" },
            started.elapsed().as_secs_f64() * 1000.0,
        );
        Ok(window_id)
    }

    #[cfg(target_os = "windows")]
    fn clamp_secondary_position(cursor: WinPoint, size: (u32, u32)) -> WinPoint {
        const MONITOR_DEFAULTTONEAREST: u32 = 2;
        unsafe {
            let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
            let mut info = MonitorInfo {
                cb_size: std::mem::size_of::<MonitorInfo>() as u32,
                monitor: WinRect { left: 0, top: 0, right: 0, bottom: 0 },
                work: WinRect { left: 0, top: 0, right: 0, bottom: 0 },
                flags: 0,
            };
            if monitor != 0 && GetMonitorInfoW(monitor, &mut info) != 0 {
                let max_x = (info.work.right - size.0 as i32).max(info.work.left);
                let max_y = (info.work.bottom - size.1 as i32).max(info.work.top);
                return WinPoint {
                    x: cursor.x.clamp(info.work.left, max_x),
                    y: cursor.y.clamp(info.work.top, max_y),
                };
            }
        }
        cursor
    }

    #[cfg(not(target_os = "windows"))]
    fn clamp_secondary_position(cursor: WinPoint, _size: (u32, u32)) -> WinPoint { cursor }

    #[cfg(target_os = "windows")]
    fn managed_window_at(&self, primary: &TpBrowserApp, cursor: WinPoint) -> Option<WindowId> {
        unsafe {
            let hit = WindowFromPoint(cursor);
            let root = if hit != 0 { GetAncestor(hit, 2) } else { 0 }; // GA_ROOT
            if root == primary.window.hwnd() {
                return Some(self.primary_window_id);
            }
            self.secondary.iter().find_map(|(id, runtime)| {
                (root == runtime.window.hwnd()).then_some(*id)
            })
        }
    }

    fn close_secondary(&mut self, window_id: WindowId) -> bool {
        self.detach_in_flight.retain(|(source_id, _)| *source_id != window_id);
        if let Some(mut app) = self.secondary.remove(&window_id) {
            // Secondary runtimes never install the tray or compact watcher, but keep
            // cleanup explicit so their child WebViews are dropped with their window.
            app.remove_system_tray();
            app.tabs.clear();
            app.chat_header_shell = None;
            app.ai_shell = None;
            app.dock_resizer_shell = None;
            app.sidebar_resizer_shell = None;
            app.settings_shell = None;
            app.fb_shell = None;
            app.zalo_shell = None;
            app.custom_popup_shell = None;
            app.glance_backdrop_shell = None;
            app.glance_header_shell = None;
            app.glance_body_shell = None;
            true
        } else {
            false
        }
    }

    fn begin_detach(&mut self, source_id: WindowId, tab_id: u64) -> bool {
        self.detach_in_flight.insert((source_id, tab_id))
    }

    fn finish_detach(&mut self, source_id: WindowId, tab_id: u64) {
        self.detach_in_flight.remove(&(source_id, tab_id));
    }

    /// Move the actual WebView2 child HWND, not a URL clone.  Do not mutate the
    /// source collection until Wry has accepted the new parent, so an unavailable
    /// destination leaves the source tab and its live session untouched.
    #[cfg(target_os = "windows")]
    fn move_live_tab(source: &mut TpBrowserApp, target: &mut TpBrowserApp, tab_id: u64, source_is_primary: bool) -> bool {
        let Some(source_index) = source.tabs.iter().position(|tab| tab.id == tab_id && tab.worker_kind.is_none()) else {
            return false;
        };
        if target.tabs.len() >= 14 || target.tabs.iter().any(|tab| tab.id == tab_id) {
            return false;
        }
        if source.tabs[source_index].webview.reparent(target.window.hwnd()).is_err() {
            return false;
        }

        let previous_active_id = source.tabs.get(source.active_idx).map(|tab| tab.id);
        let mut tab = source.tabs.remove(source_index);
        // Append/activate without disturbing unrelated destination tabs. The
        // transferred BrowserTab keeps its live WebView2 history, form state,
        // profile/container and private identity.
        // A window may have a different active Space. Reconcile before activation:
        // retain the matching Space when it exists, otherwise place the tab in the
        // destination's active Space so shell filtering cannot render blank content.
        if target.workspaces.iter().any(|space| space.id == tab.workspace_id) {
            target.active_workspace_id = tab.workspace_id.clone();
        } else {
            tab.workspace_id = target.active_workspace_id.clone();
        }
        target.tabs.push(tab);
        target.active_idx = target.tabs.len() - 1;
        target.mru_tab_ids.retain(|id| *id != tab_id);
        target.mru_tab_ids.insert(0, tab_id);
        target.mru_cycle_ids = None;
        // The transfer is now committed: only now may a staged secondary create
        // its optional chat, AI, settings and resizer shells.
        target.hydrate_secondary_shells();

        source.selected_tab_ids.remove(&tab_id);
        source.mru_tab_ids.retain(|id| *id != tab_id);
        source.mru_cycle_ids = None;
        if source.split_paired_tab_id == Some(tab_id) {
            source.split_paired_tab_id = None;
            source.view_mode = ContentViewMode::SingleTab;
        }
        if let Some(active_id) = previous_active_id.filter(|id| *id != tab_id) {
            source.active_idx = source.tabs.iter().position(|tab| tab.id == active_id).unwrap_or(0);
        } else if let Some(next_index) = source.tabs.iter().position(|tab| tab.worker_kind.is_none()) {
            source.active_idx = next_index;
        } else if source_is_primary {
            // Only the primary receives a fallback tab; an empty secondary closes.
            let _ = source.add_tab_internal("tp://newtab", false, false, None, None, false, None);
        }

        target.relayout();
        target.sync_all_ui();
        source.relayout();
        source.sync_all_ui();
        source.save_session();
        // Secondary persistence is intentionally a no-op. Inbound secondary ->
        // primary transfers must still commit the primary session immediately.
        if target.persist_session {
            target.save_session();
        }
        true
    }

    #[cfg(target_os = "windows")]
    fn detach_from_primary(&mut self, primary: &mut TpBrowserApp, target_id: WindowId, tab_id: u64) -> bool {
        self.secondary
            .get_mut(&target_id)
            .map(|target| Self::move_live_tab(primary, target, tab_id, true))
            .unwrap_or(false)
    }

    #[cfg(target_os = "windows")]
    fn detach_from_secondary(&mut self, source_id: WindowId, target_id: WindowId, tab_id: u64) -> bool {
        if source_id == target_id {
            return false;
        }
        let Some(mut source) = self.secondary.remove(&source_id) else {
            return false;
        };
        let moved = self.secondary
            .get_mut(&target_id)
            .map(|target| Self::move_live_tab(&mut source, target, tab_id, false))
            .unwrap_or(false);
        self.secondary.insert(source_id, source);
        moved
    }

    #[cfg(target_os = "windows")]
    fn detach_to_primary(&mut self, primary: &mut TpBrowserApp, source_id: WindowId, tab_id: u64) -> bool {
        let Some(mut source) = self.secondary.remove(&source_id) else { return false; };
        let moved = Self::move_live_tab(&mut source, primary, tab_id, false);
        self.secondary.insert(source_id, source);
        moved
    }
}

fn main() -> wry::Result<()> {
    // Tối ưu hóa bộ nhớ và tài nguyên cho WebView2 (tiết kiệm RAM, khởi động nhanh)
    if env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").is_err() {
        env::set_var(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            "--disable-features=Translate,OptimizationHints,MediaRouter --disable-backgrounding-occluded-windows --disable-renderer-backgrounding --disable-background-timer-throttling",
        );
    }

    if env::args().any(|a| a == "--mcp") {
        std::process::exit(mcp_server::run_stdio_bridge());
    }

    #[cfg(target_os = "windows")]
    {
        use std::io::{Read, Write};
        let mutex_name = b"TPbrowser_SingleInstance_Mutex\0";
        unsafe {
            let handle = CreateMutexA(std::ptr::null_mut(), 0, mutex_name.as_ptr());
            if handle != 0 && GetLastError() == 183 {
                // A mutex is authoritative: a failed /wake may mean the existing
                // instance is still starting, but must never permit a second host.
                if let Ok(mut stream) = std::net::TcpStream::connect("127.0.0.1:17777") {
                    let _ = stream.write_all(b"POST /wake HTTP/1.1
\nHost: 127.0.0.1:17777
\nContent-Length: 0
\nConnection: close
\n
\n");
                    let mut buf = [0u8; 128];
                    let _ = stream.read(&mut buf);
                }
                std::process::exit(0);
            }
        }
    }

    let is_silent = env::args().any(|a| a == "--silent");
    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    let event_proxy = event_loop.create_proxy();

    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let saved_window_size = fs::read_to_string(exe_dir.join("config.json"))
        .map(|raw| TpBrowserApp::persisted_window_size(&raw))
        .unwrap_or((1480, 900));
    let prev_recovery = fs::read_to_string(exe_dir.join("window_recovery.json"))
        .ok()
        .and_then(|raw| parse_recovery_snapshot(&raw));
    let crash_detected = prev_recovery.as_ref().is_some_and(|s| !s.clean_shutdown);
    let has_crashed_secondary = prev_recovery
        .as_ref()
        .is_some_and(|s| !s.clean_shutdown && !s.secondary.is_empty());
    let pending_recovery = prev_recovery.as_ref().filter(|s| recovery_needed(s)).cloned();

    let mut window_builder = WindowBuilder::new()
        .with_title("TPbrowser — Trình Duyệt Cá Nhân & AI Cowork")
        .with_decorations(false)
        .with_visible(!is_silent)
        .with_window_icon(TpBrowserApp::build_window_icon("ribbon", "obsidian-gold"))
        .with_min_inner_size(LogicalSize::new(860, 560));

    if let Some(ref prev) = prev_recovery {
        let mut bounds = prev.primary.bounds.clone();
        if let Some(work) = work_area_for_point(bounds.x + (bounds.width as i32) / 2, bounds.y + 24) {
            bounds = clamp_window_bounds_to_work_area(bounds, work);
        }
        window_builder = window_builder
            .with_inner_size(PhysicalSize::new(bounds.width.max(860), bounds.height.max(560)))
            .with_position(PhysicalPosition::new(bounds.x, bounds.y));
    } else {
        window_builder = window_builder.with_inner_size(LogicalSize::new(saved_window_size.0, saved_window_size.1));
    }

    #[cfg(target_os = "windows")]
    let window_builder = window_builder.with_undecorated_shadow(true);

    let window = window_builder
        .build(&event_loop)
        .expect("Failed to create TPbrowser window");

    if is_silent {
        let _ = window.set_skip_taskbar(true);
    }

    let primary_window_id = window.id();
    let mut app = TpBrowserApp::new(
        window,
        WindowProxy::new(primary_window_id, event_proxy.clone()),
        exe_dir.clone(),
        true,
        false,
    )?;
    if prev_recovery.as_ref().is_some_and(|p| p.primary.bounds.maximized) && !is_silent {
        app.window.set_maximized(true);
    }
    let _ = clamp_live_window_to_monitor(&app.window);
    let mut windows = WindowRegistry::new(primary_window_id, pending_recovery, crash_detected);
    windows.persist_recovery_snapshot(&mut app, false);
    if has_crashed_secondary && !is_silent {
        let _ = app.proxy.send_event(AppCmd::RecoverWindows);
    }
    let mut modifiers = ModifiersState::empty();
    let mut alt_keys = AltKeyState::default();

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

    event_loop.run(move |event, target, control_flow| {
        let expired_shopee = shopee_discovery::expire_pending(&mut app.shopee_pending, Instant::now());
        if expired_shopee.iter().any(|id| app.shopee_workspace_pending.remove(id)) {
            app.shopee_discovery_json = shopee_discovery::unavailable();
            app.sync_workspace_view();
        }
        *control_flow = shopee_wait_flow(*control_flow, shopee_discovery::nearest_deadline(&app.shopee_pending));

        match event {
            Event::MainEventsCleared => {
                if windows.recovery_due() {
                    windows.persist_recovery_snapshot(&mut app, false);
                }
                if app.session_due() {
                    app.save_session();
                }
            }
            Event::WindowEvent { window_id, event, .. } if window_id != windows.primary_window_id => {
                let mut close_secondary = false;
                let mut moved_or_resized = false;
                if let Some(secondary) = windows.secondary.get_mut(&window_id) {
                    match event {
                        WindowEvent::CloseRequested => close_secondary = true,
                        WindowEvent::Moved(_) => moved_or_resized = true,
                        WindowEvent::Resized(_) => {
                            secondary.relayout();
                            secondary.sync_all_ui();
                            moved_or_resized = true;
                        }
                        WindowEvent::ScaleFactorChanged { .. } => {
                            let _ = clamp_live_window_to_monitor(&secondary.window);
                            secondary.relayout();
                            secondary.sync_all_ui();
                            moved_or_resized = true;
                        }
                        _ => {}
                    }
                }
                if moved_or_resized {
                    windows.mark_recovery_dirty();
                }
                if close_secondary {
                    windows.close_secondary(window_id);
                    windows.persist_recovery_snapshot(&mut app, false);
                    app.sync_settings_modal(None);
                }
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    app.execute(AppCmd::WinClose);
                    if app.should_exit {
                        windows.persist_recovery_snapshot(&mut app, true);
                        *control_flow = ControlFlow::Exit;
                    } else {
                        windows.persist_recovery_snapshot(&mut app, false);
                    }
                }
                WindowEvent::Moved(_) => {
                    windows.mark_recovery_dirty();
                }
                WindowEvent::Resized(_) => {
                    app.relayout();
                    app.sync_all_ui();
                    windows.mark_recovery_dirty();
                }
                WindowEvent::ScaleFactorChanged { .. } => {
                    let _ = clamp_live_window_to_monitor(&app.window);
                    app.relayout();
                    app.sync_all_ui();
                    windows.mark_recovery_dirty();
                }
                WindowEvent::ModifiersChanged(new_mods) => {
                    modifiers = new_mods;
                }
                WindowEvent::Focused(false) => {
                    alt_keys = AltKeyState::default();
                    if app.mru_hud_open {
                        app.execute(AppCmd::CommitMruTab(None));
                    }
                }
                WindowEvent::KeyboardInput { event: KeyEvent { physical_key, state, .. }, .. } => {
                    let pressed = state == ElementState::Pressed;
                    if alt_keys.should_open_command_bar(physical_key, pressed) {
                        let _ = app.proxy.send_event(AppCmd::OpenCommandBar);
                    }
                    alt_keys.update(physical_key, pressed);
                    if !pressed {
                        if matches!(physical_key, KeyCode::ControlLeft | KeyCode::ControlRight) {
                            if app.mru_hud_open {
                                app.execute(AppCmd::CommitMruTab(None));
                            } else {
                                app.mru_cycle_ids = None;
                                if let Some(id) = app.tabs.get(app.active_idx).filter(|tab| tab.worker_kind.is_none()).map(|tab| tab.id) {
                                    app.mru_tab_ids.retain(|existing| *existing != id);
                                    app.mru_tab_ids.insert(0, id);
                                }
                            }
                        }
                        return;
                    }
                    if let Some(shortcut) = TpBrowserApp::shortcut_from_key_input(physical_key, modifiers) {
                        if let Some(command) = app.shortcut_command_for(&shortcut) {
                            app.run_custom_shortcut(command);
                            return;
                        }
                    }
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
                    } else if physical_key == KeyCode::Escape && app.mru_hud_open {
                        app.execute(AppCmd::CancelMruTab);
                    } else if matches!(physical_key, KeyCode::Enter | KeyCode::NumpadEnter)
                        && app.mru_hud_open
                    {
                        app.execute(AppCmd::CommitMruTab(None));
                    } else if physical_key == KeyCode::Escape && app.settings_open {
                        app.execute(AppCmd::CloseSettingsModal);
                    } else if physical_key == KeyCode::Escape && app.glance_open {
                        app.execute(AppCmd::CloseGlance);
                    } else if physical_key == KeyCode::Escape && app.command_bar_open {
                        app.execute(AppCmd::CloseCommandBar);
                    } else if modifiers.control_key() && !modifiers.alt_key() {
                        match physical_key {
                            KeyCode::KeyT if modifiers.shift_key() => {
                                app.execute(AppCmd::ReopenClosedTab)
                            }
                            KeyCode::KeyT if !modifiers.shift_key() => app.execute(AppCmd::NewTab {
                                url: "tp://newtab".to_string(),
                                isolated: false,
                                container_id: None,
                            }),
                            KeyCode::KeyW | KeyCode::F4 if !modifiers.shift_key() => {
                                app.execute(AppCmd::CloseActiveTab)
                            }
                            KeyCode::Tab if modifiers.shift_key() => {
                                app.execute(AppCmd::StepMruTab(-1))
                            }
                            KeyCode::Tab => app.execute(AppCmd::StepMruTab(1)),
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
                            KeyCode::KeyC if modifiers.shift_key() => {
                                app.execute(AppCmd::CopyActiveUrl)
                            }
                            KeyCode::KeyK if !modifiers.shift_key() => {
                                app.execute(AppCmd::OpenCommandBar)
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
                            KeyCode::KeyP if !modifiers.shift_key() => {
                                app.execute(AppCmd::PrintActive)
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
                    } else if physical_key == KeyCode::F12 {
                        app.execute(AppCmd::OpenDevTools);
                    } else if modifiers.control_key() && modifiers.alt_key() && !modifiers.shift_key() {
                        match physical_key {
                            KeyCode::KeyC => app.execute(AppCmd::ToggleCompactMode),
                            KeyCode::ArrowLeft => app.execute(AppCmd::PrevWorkspace),
                            KeyCode::ArrowRight => app.execute(AppCmd::NextWorkspace),
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
                                container_id: None,
                            }),
                            KeyCode::KeyW => {},
                            _ => {}
                        }
                    }
                }
                _ => {}
            },
            Event::UserEvent(AppEvent::WindowCommand { window_id, command: AppCmd::NewWindow, .. }) => {
                let size = if window_id == windows.primary_window_id {
                    let size = app.window.inner_size();
                    Some((size.width, size.height))
                } else {
                    windows.secondary.get(&window_id).map(|secondary| {
                        let size = secondary.window.inner_size();
                        (size.width, size.height)
                    })
                };
                if let Some(size) = size {
                    let _ = windows.create_secondary(target, event_proxy.clone(), exe_dir.clone(), size, false, None);
                    windows.persist_recovery_snapshot(&mut app, false);
                    app.sync_settings_modal(None);
                }
            }
            Event::UserEvent(AppEvent::WindowCommand { command: AppCmd::RecoverWindows, .. }) => {
                let _ = windows.recover_windows(target, &event_proxy, &exe_dir, &mut app);
            }
            Event::UserEvent(AppEvent::WindowCommand { command: AppCmd::DismissWindowRecovery, .. }) => {
                windows.dismiss_recovery(&mut app);
            }
            Event::UserEvent(AppEvent::WindowCommand { command: AppCmd::SyncWindowGeometry, .. }) => {
                windows.sync_all_window_geometries(&mut app);
            }
            Event::UserEvent(AppEvent::WindowCommand { window_id, command: AppCmd::DetachTabToNewWindow { tab_id, intent, fallback_cursor }, .. }) => {
                let eligible = if window_id == windows.primary_window_id {
                    app.tabs.iter().any(|tab| tab.id == tab_id && tab.worker_kind.is_none())
                } else {
                    windows.secondary.get(&window_id).is_some_and(|source| {
                        source.tabs.iter().any(|tab| tab.id == tab_id && tab.worker_kind.is_none())
                    })
                };
                if !eligible || !windows.begin_detach(window_id, tab_id) {
                    return;
                }
                let size = if window_id == windows.primary_window_id {
                    let size = app.window.inner_size();
                    Some((size.width, size.height))
                } else {
                    windows.secondary.get(&window_id).map(|source| {
                        let size = source.window.inner_size();
                        (size.width, size.height)
                    })
                };
                let Some(size) = size else {
                    windows.finish_detach(window_id, tab_id);
                    return;
                };
                #[cfg(target_os = "windows")]
                let hovered_managed_window = fallback_cursor
                    .and_then(|cursor| windows.managed_window_at(&app, cursor));
                let existing_target = hovered_managed_window
                    .filter(|target_id| *target_id != window_id);
                #[cfg(not(target_os = "windows"))]
                let hovered_managed_window: Option<WindowId> = None;
                #[cfg(not(target_os = "windows"))]
                let existing_target: Option<WindowId> = None;

                // A drag which ends over its source window is neither a transfer
                // nor a detach. Context-menu detach intentionally has no location.
                if intent == TabDetachIntent::DragOut && hovered_managed_window == Some(window_id) {
                    windows.finish_detach(window_id, tab_id);
                    return;
                }

                let target_id = if let Some(target_id) = existing_target {
                    target_id
                } else {
                    let Ok(target_id) = windows.create_secondary(
                        target, event_proxy.clone(), exe_dir.clone(), size, true, fallback_cursor,
                    ) else {
                        windows.finish_detach(window_id, tab_id);
                        return;
                    };
                    target_id
                };
                #[cfg(not(target_os = "windows"))]
                let moved = false;
                #[cfg(target_os = "windows")]
                let moved = if window_id == windows.primary_window_id {
                    windows.detach_from_primary(&mut app, target_id, tab_id)
                } else if target_id == windows.primary_window_id {
                    windows.detach_to_primary(&mut app, window_id, tab_id)
                } else {
                    windows.detach_from_secondary(window_id, target_id, tab_id)
                };
                if !moved {
                    // Roll back only a staging destination; an existing destination
                    // retains its unrelated tabs when a transfer is rejected.
                    if existing_target.is_none() {
                        windows.close_secondary(target_id);
                    }
                } else {
                    if window_id != windows.primary_window_id
                        && windows.secondary.get(&window_id).is_some_and(|source| source.tabs.is_empty())
                    {
                        windows.close_secondary(window_id);
                    }
                    if target_id == windows.primary_window_id {
                        app.window.set_focus();
                    } else if let Some(target) = windows.secondary.get_mut(&target_id) {
                        target.window.set_visible(true);
                        target.window.set_focus();
                    }
                }
                windows.finish_detach(window_id, tab_id);
                windows.persist_recovery_snapshot(&mut app, false);
                app.sync_settings_modal(None);
            }
            Event::UserEvent(AppEvent::WindowCommand { window_id, origin_tab_id, command: cmd }) => {
                let is_quiet_bg = matches!(
                    &cmd,
                    AppCmd::CheckReminders
                        | AppCmd::UiShellReady(_)
                        | AppCmd::ShopeeNativeResult { .. }
                        | AppCmd::FocusContentTab(_)
                );
                let cmd_desc = if matches!(&cmd, AppCmd::ShopeeNativeResult { .. }) {
                    "ShopeeNativeResult [redacted]".to_string()
                } else { format!("{:?}", cmd) };
                let t_evt = Instant::now();
                // A reparented BrowserTab keeps Wry callbacks and IPC closures
                // created under its former WindowId. Every such callback carries
                // `origin_tab_id`, so route *all* of its commands by current stable
                // ownership (not only lifecycle/meta/media variants). Window-shell
                // commands have no origin and retain their original routing.
                if let Some(tab_id) = origin_tab_id {
                    if app.tabs.iter().any(|tab| tab.id == tab_id) {
                        app.execute(cmd);
                    } else if let Some(owner) = windows.secondary.values_mut().find(|runtime| {
                        runtime.tabs.iter().any(|tab| tab.id == tab_id)
                    }) {
                        owner.execute(cmd);
                    }
                } else if window_id == windows.primary_window_id {
                    app.execute(cmd);
                } else if let Some(secondary) = windows.secondary.get_mut(&window_id) {
                    secondary.execute(cmd);
                }
                if !is_quiet_bg {
                    windows.mark_recovery_dirty();
                }
                let dur = t_evt.elapsed().as_secs_f64() * 1000.0;
                if dur > 10.0 {
                    TpBrowserApp::log_perf(&format!("CMD: {}", cmd_desc), dur);
                }
                if app.should_exit {
                    windows.persist_recovery_snapshot(&mut app, true);
                    *control_flow = ControlFlow::Exit;
                }
            }
            _ => {}
        }
        let next_deadline = [
            shopee_discovery::nearest_deadline(&app.shopee_pending),
            windows.recovery_deadline(),
            app.session_deadline(),
        ]
        .into_iter()
        .flatten()
        .min();
        *control_flow = shopee_wait_flow(*control_flow, next_deadline);
    });
}

fn shopee_wait_flow(current: ControlFlow, deadline: Option<Instant>) -> ControlFlow {
    if current == ControlFlow::Exit {
        return ControlFlow::Exit;
    }
    match (current, deadline) {
        (ControlFlow::WaitUntil(existing), Some(next)) => {
            ControlFlow::WaitUntil(existing.min(next))
        }
        (_, Some(next)) => ControlFlow::WaitUntil(next),
        _ => ControlFlow::Wait,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn tiktok_ads_status_contract_fails_closed() {
        use super::{mark_tiktok_cache_restored, tiktok_ads_status, tiktok_status_messages, TIKTOK_ADS_STATUSES};
        for s in TIKTOK_ADS_STATUSES {
            assert_eq!(tiktok_ads_status(&format!("{{\"status\":\"{}\"}}", s)), s);
        }
        assert_eq!(tiktok_ads_status("{\"status\":\"fresh\"}"), "error");
        assert_eq!(tiktok_ads_status("not json"), "error");
        assert_eq!(tiktok_ads_status("{\"logged_in\":true,\"captcha_active\":true}"), "captcha");
        assert_eq!(tiktok_ads_status("{\"logged_in\":false,\"need_login\":true}"), "needs_login");
        assert_eq!(tiktok_ads_status("{\"logged_in\":true,\"error\":\"x\"}"), "error");
        // Legacy payload without provenance is never promoted to "ok".
        assert_eq!(tiktok_ads_status("{\"logged_in\":true,\"cost_ads\":1}"), "partial");
        for s in ["loading", "needs_login", "captcha", "unavailable", "error"] {
            assert_ne!(tiktok_status_messages(s).1, "Đã lấy xong");
        }
        let restored = mark_tiktok_cache_restored("{\"status\":\"ok\",\"cache_state\":\"live\"}").unwrap();
        assert!(restored.contains("\"cache_state\":\"restored_from_disk\""));
        assert!(!restored.contains("\"live\""));
        assert_eq!(tiktok_ads_status(&restored), "partial", "restored ok must not stay fresh");
        let restored = mark_tiktok_cache_restored(
            "{\"status\":\"ok\",\"status_reasons\":[],\"period\":{\"start\":\"2026-11-16\",\"end\":\"2026-11-16\",\"is_today\":true}}",
        )
        .unwrap();
        assert!(restored.contains("\"status_reasons\":[\"restored_from_disk\"]"));
        assert!(restored.contains("\"is_today\":false"));
        assert!(!restored.contains("\"is_today\":true"));
        assert!(restored.contains("\"start\":\"2026-11-16\""), "original period kept as provenance");
        let restored_partial = mark_tiktok_cache_restored("{\"status\":\"partial\"}").unwrap();
        assert_eq!(tiktok_ads_status(&restored_partial), "partial");
        assert!(mark_tiktok_cache_restored("[1]").is_none());
    }

    #[test]
    fn tiktok_ads_collector_envelope_is_read_only_and_null_preserving() {
        let js = super::TIKTOK_PASSIVE_SCAN_JS;
        for key in [
            "\"tp.tiktok.ads.v1\"", "status:", "source_url:", "captured_at:", "period:", "metrics:",
            "campaigns_provenance:", "provenance:", "raw:", "\"needs_login\"", "\"captcha\"",
            "\"loading\"", "\"unavailable\"", "\"partial\"", "\"error\"", "\"ok\"", "read_only: true",
        ] {
            assert!(js.contains(key), "missing envelope key {key}");
        }
        for forbidden in [".click(", ".submit(", "dispatchEvent", "location.href =", "location.assign", "location.replace", "fetch(", "XMLHttpRequest"] {
            assert!(!js.contains(forbidden), "collector must stay passive: {forbidden}");
        }
        assert!(!js.contains("if (!s) return 0;"));
        assert!(!js.contains("return isNaN(v) ? 0 : v;"));
        assert!(!js.contains("name: \"GMV Max Toàn Cửa Hàng\""), "no synthesized campaign rows");
        assert!(js.contains("is_today: a === b && a === today"));
        assert!(js.contains("return \"Không xác định\";"));
        // Derived core metrics can never yield "ok" and are labelled.
        assert!(js.contains("derived.length === 0 && campProv.source !== \"unavailable\""));
        assert!(js.contains("reasons.push(k + \"_derived\")"));
        assert!(js.contains("derived_fields: derivedFields"));
        // Async steps are guarded and scroll is restored on failure.
        assert!(js.contains("setTimeout(guarded(extractAndSend, \"retry\"), 1100)"));
        assert!(js.contains("setTimeout(guarded(sweepNext, \"sweep\"), 220)"));
        assert!(js.contains("guarded(extractAndSend, \"extract\")()"));
        assert!(!js.contains("setTimeout(extractAndSend") && !js.contains("setTimeout(sweepNext"));
        assert!(js.contains("function postError(err, where) {\n    restoreScroll();"));
    }

    #[test]
    fn navigation_registration_failure_rolls_back_partial_tokens() {
        let mut state = super::NavigationRegistrationState::new();
        state.tokens = [Some(11), Some(22), None];
        state.fail(super::NavigationRegistrationPhase::NavigationCompleted, [true, true, true]);

        assert_eq!(state.failure.map(|failure| failure.phase), Some(super::NavigationRegistrationPhase::NavigationCompleted));
        assert_eq!(state.failure.map(|failure| failure.hresult), Some(-1));
        assert_eq!(state.tokens, [None, None, None]);
        assert!(!state.is_usable());
    }

    #[test]
    fn navigation_registration_teardown_is_idempotent_and_retains_failed_removals() {
        let mut state = super::NavigationRegistrationState::new();
        state.tokens = [Some(31), Some(32), Some(33)];
        state.fail(super::NavigationRegistrationPhase::ContentLoading, [true, false, false]);

        assert_eq!(state.tokens, [None, Some(32), Some(33)]);
        let first = state.take_tokens_for_teardown();
        assert_eq!(first, [None, Some(32), Some(33)]);
        assert_eq!(state.take_tokens_for_teardown(), [None, None, None]);
    }

    #[test]
    fn shopee_wait_schedule_preserves_exit_and_selects_deadline() {
        let deadline = Instant::now() + shopee_discovery::TIMEOUT;
        assert_eq!(shopee_wait_flow(ControlFlow::Wait, Some(deadline)), ControlFlow::WaitUntil(deadline));
        assert_eq!(shopee_wait_flow(ControlFlow::WaitUntil(deadline), None), ControlFlow::Wait);
        assert_eq!(shopee_wait_flow(ControlFlow::Exit, Some(deadline)), ControlFlow::Exit);
    }
    #[test]
    fn shopee_wait_schedule_does_not_delay_an_earlier_pending_wakeup() {
        let now = Instant::now();
        let earlier = now + std::time::Duration::from_secs(1);
        let later = now + std::time::Duration::from_secs(2);
        assert_eq!(
            shopee_wait_flow(ControlFlow::WaitUntil(earlier), Some(later)),
            ControlFlow::WaitUntil(earlier),
        );
    }
    #[test]
    fn shopee_callback_debug_does_not_expose_payload() {
        let cmd = super::AppCmd::ShopeeNativeResult { req_id: 1, tab_id: 2, payload: super::ShopeePayload(Some("secret-shop-name".into())) };
        assert!(!format!("{:?}", cmd).contains("secret-shop-name"));
    }
    #[test]
    fn right_alt_k_signal_requires_right_alt_only() {
        let mut state = super::AltKeyState::default();
        assert!(!state.should_open_command_bar(super::KeyCode::KeyK, true));
        state.update(super::KeyCode::AltLeft, true);
        assert!(!state.should_open_command_bar(super::KeyCode::KeyK, true));
        state.update(super::KeyCode::AltLeft, false);
        state.update(super::KeyCode::AltRight, true);
        assert!(state.should_open_command_bar(super::KeyCode::KeyK, true));
        assert!(!state.should_open_command_bar(super::KeyCode::KeyK, false));
        state.update(super::KeyCode::AltRight, false);
        assert!(!state.should_open_command_bar(super::KeyCode::KeyK, true));
        state.update(super::KeyCode::AltRight, true);
        state.update(super::KeyCode::AltLeft, true);
        assert!(!state.should_open_command_bar(super::KeyCode::KeyK, true));
    }

    use super::*;

    #[cfg(any())]
    #[test]
    fn selected_shops_same_platform_round_trip_with_distinct_ids() {
        let shops = vec![
            ShopSelection { platform: "shopee".into(), shop_id: "shop-1".into() },
            ShopSelection { platform: "shopee".into(), shop_id: "shop-2".into() },
        ];
        let config = format!("{{\"selected_shops\":{}}}", serialize_shop_selections(&shops));
        assert_eq!(parse_shop_selections(&config), shops);
        assert!(parse_shop_selections("{}").is_empty());
        assert!(parse_shop_selections(r#"{"selected_shops":"unknown"}"#).is_empty());
    }

    #[cfg(any())]
    #[test]
    fn config_payload_and_ordered_writes_preserve_latest_shop_snapshot() {
        let root = std::env::temp_dir().join(format!("tpbrowser-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("snapshot.json");
        let first = vec![ShopSelection { platform: "shopee".into(), shop_id: "one".into(), display_name: "A\"".into(), selected: true }];
        let mut latest = first.clone();
        latest.push(ShopSelection { platform: "shopee".into(), shop_id: "two".into() });
        let old_payload = config_payload_with_selections("{\"ui_theme\":\"dark\"}", &first);
        let new_payload = config_payload_with_selections("{\"ui_theme\":\"dark\"}", &latest);
        write_config_snapshot(&path, &old_payload).unwrap();
        write_config_snapshot(&path, &new_payload).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert_eq!(parse_shop_selections(&saved), latest);
        assert_eq!(parse_json(&saved).unwrap().get("ui_theme").and_then(JsonVal::as_str), Some("dark"));
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[cfg(any())]
    #[test]
    fn selected_shops_ignore_invalid_records_and_default_optional_fields() {
        let raw = r#"{"selected_shops":[null,{}, {"platform":"other","account_id":"a","shop_id":"b"}, {"platform":"shopee","account_id":" ","shop_id":"b"}, {"platform":"tiktok","account_id":17,"shop_id":"b"}, {"platform":"tiktok","account_id":"a","shop_id":""}, {"platform":"tiktok","account_id":"a","shop_id":"b","selected":"yes","display_name":42}]}"#;
        assert_eq!(parse_shop_selections(raw), vec![ShopSelection { platform: "tiktok".into(), account_id: "a".into(), shop_id: "b".into(), display_name: String::new(), selected: false }]);
        for raw in ["", "null", "[]", "{", r#"{"selected_shops":null}"#, r#"{"selected_shops":{}}"#] {
            assert!(parse_shop_selections(raw).is_empty());
        }
    }

    #[cfg(any())]
    #[test]
    fn selected_shops_reject_malformed_unicode_identity() {
        for field in ["platform", "account_id", "shop_id"] {
            for invalid in [r"\uD800", r"\uDC00", r"\uD800\u0041", r"\u12xz", r"\u123"] {
                let mut fields = [("platform", "shopee"), ("account_id", "account"), ("shop_id", "shop")];
                fields.iter_mut().find(|(key, _)| *key == field).unwrap().1 = match field {
                    "platform" => match invalid { r"\uD800" => r"shopee\uD800", r"\uDC00" => r"shopee\uDC00", r"\uD800\u0041" => r"shopee\uD800\u0041", r"\u12xz" => r"shopee\u12xz", _ => r"shopee\u123" },
                    _ => match invalid { r"\uD800" => r"id\uD800", r"\uDC00" => r"id\uDC00", r"\uD800\u0041" => r"id\uD800\u0041", r"\u12xz" => r"id\u12xz", _ => r"id\u123" },
                };
                let raw = format!(r#"{{"selected_shops":[{{"platform":"{}","account_id":"{}","shop_id":"{}"}}]}}"#, fields[0].1, fields[1].1, fields[2].1);
                assert!(parse_shop_selections(&raw).is_empty(), "accepted {field}: {invalid}");
            }
        }
        let malformed_hex = r#"{"selected_shops":[{"platform":"shopee","account_id":"id\u+123","shop_id":"b"}]}"#;
        assert!(parse_shop_selections(malformed_hex).is_empty());
        let valid = r#"{"selected_shops":[{"platform":"shopee","account_id":"a\uD83D\uDE00","shop_id":"b"}]}"#;
        assert_eq!(parse_shop_selections(valid)[0].account_id, "a😀");
    }

    #[test]
    fn selection_only_model_round_trip_dedupes_and_rejects_legacy_records() {
        let raw = r#"{"selected_shops":[{"platform":"shopee","shop_id":"one"},{"platform":"shopee","shop_id":"one"},{"platform":"tiktok","shop_id":"same"},{"platform":"shopee","shop_id":"ok","account_id":"legacy"}]}"#;
        let selections = parse_shop_selections(raw);
        assert_eq!(selections, vec![ShopSelection { platform: "shopee".into(), shop_id: "one".into() }]);
        let serialized = serialize_shop_selections(&selections);
        assert_eq!(parse_shop_selections(&format!("{{\"selected_shops\":{}}}", serialized)), selections);
        assert!(!serialized.contains("account_id"));
        assert_ne!(
            ShopSelection { platform: "shopee".into(), shop_id: "same".into() },
            ShopSelection { platform: "tiktok".into(), shop_id: "same".into() },
        );
    }

    #[test]
    fn persisted_window_size_restores_and_clamps_geometry() {
        assert_eq!(TpBrowserApp::persisted_window_size(
            r#"{"window_width": 1920, "window_height": 1080}"#
        ), (1920, 1080));
        assert_eq!(TpBrowserApp::persisted_window_size(
            r#"{"window_width": 640, "window_height": 300}"#
        ), (860, 560));
        assert_eq!(TpBrowserApp::persisted_window_size("{}"), (1480, 900));
    }

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
    fn test_settings_modal_html_has_font_marker_and_seven_tabs() {
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
            "pane_about",
        ] {
            assert!(SETTINGS_MODAL_HTML.contains(tab), "missing {}", tab);
        }
        assert!(SETTINGS_MODAL_HTML.contains("orb_manager_list"));
        assert!(SETTINGS_MODAL_HTML.contains("sw_always_show_orbs"));
        assert!(SETTINGS_MODAL_HTML.contains("btn_check_update"));
        assert!(SETTINGS_MODAL_HTML.contains("check_app_update"));
        assert!(SETTINGS_MODAL_HTML.contains("about_brand_icon"));
        assert_eq!(APP_VERSION, "0.5.6");
        assert_eq!(APP_FULL_VERSION, "v0.5.6-browser-only");
        assert!(injected.contains("Motion"));
    }

    #[test]
    fn test_glance_preview_assets_and_actions() {
        assert!(GLANCE_HEADER_HTML.contains("glance_open_in_tab"));
        assert!(GLANCE_HEADER_HTML.contains("close_glance"));
        assert!(GLANCE_HEADER_HTML.contains("Escape"));
        assert!(GLANCE_BACKDROP_HTML.contains("close_glance"));
        assert!(CONTENT_AGENT_JS.contains("open_glance"));
        assert!(CONTENT_AGENT_JS.contains("pointerdown"));
        assert!(TAB_SHELL_HTML.contains("fallbackWebFavicon"));
        assert!(NEWTAB_HTML.contains("repeat(auto-fit, minmax(120px, 1fr))"));
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
    fn test_custom_shortcuts_normalize_persisted_values_and_fail_closed_on_conflicts() {
        assert_eq!(TpBrowserApp::normalize_custom_shortcut("shift + ctrl + c"), Some("Ctrl+Shift+C".to_string()));
        assert_eq!(TpBrowserApp::normalize_custom_shortcut("C"), None);
        assert!(TpBrowserApp::reserved_browser_shortcut("Ctrl+T"));
        let mut shortcuts = TpBrowserApp::default_custom_shortcuts();
        shortcuts.insert("command_bar".to_string(), "Alt+A".to_string());
        assert_eq!(TpBrowserApp::shortcut_command_for_map(&shortcuts, "Alt+A"), None);
        let parsed = parse_json(r#"{"custom_shortcuts":{"command_bar":"Ctrl+T","copy_url":"Shift+Ctrl+C"}}"#).unwrap();
        let loaded = TpBrowserApp::parse_custom_shortcuts(parsed.get("custom_shortcuts").unwrap());
        assert_eq!(loaded.get("command_bar").map(String::as_str), Some("Ctrl+K"));
        assert_eq!(loaded.get("copy_url").map(String::as_str), Some("Ctrl+Shift+C"));
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
    fn test_browser_shell_has_no_workspace_entrypoint() {
        assert!(!TITLEBAR_HTML.contains("open_workspace"));
        assert!(!NEWTAB_HTML.contains("open_workspace"));
        assert!(TITLEBAR_HTML.contains("tb-worker-badge"));
        assert!(TAB_SHELL_HTML.contains("tab-worker-badge"));
        assert!(AI_PANEL_HTML.contains("quick_worker_tab"));
        assert_eq!(TpBrowserApp::normalize_url("tp://workspace"), "tp://workspace");
        assert_eq!(TpBrowserApp::title_from_url("tp://workspace"), "Không gian TMĐT");
    }

    #[test]
    fn test_normalize_url_classifies_domains_localhost_ipv4_and_schemes() {
        assert_eq!(TpBrowserApp::normalize_url("example.com/path"), "https://example.com/path");
        assert_eq!(TpBrowserApp::normalize_url("localhost:3000/status"), "https://localhost:3000/status");
        assert_eq!(TpBrowserApp::normalize_url("127.0.0.1:8080"), "https://127.0.0.1:8080");
        assert_eq!(TpBrowserApp::normalize_url("ftp://example.com/file"), "ftp://example.com/file");
        assert_eq!(TpBrowserApp::normalize_url("tp://history"), "tp://history");
        assert_eq!(TpBrowserApp::normalize_url("rust browser"), "https://www.google.com/search?q=rust+browser");
    }

    #[test]
    fn test_workspace_session_roundtrip_and_legacy_default() {
        let session = format!(
            "{{\"active_workspace_id\":\"research\",\"tabs\":[{},{}]}}",
            TpBrowserApp::serialize_session_tab("https://example.com", "default", "default", "research", false, Some(7), false, None),
            TpBrowserApp::serialize_session_tab("https://example.org", "default", "shopping", "personal", false, None, false, None),
        );
        let value = parse_json(&session).expect("workspace session should parse");
        assert_eq!(
            value.get("active_workspace_id").and_then(|v| v.as_str()),
            Some("research")
        );
        let tabs = value.get("tabs").and_then(|v| v.as_arr()).expect("tabs array");
        assert_eq!(
            tabs[0].get("container_id").and_then(|v| v.as_str()),
            Some("default")
        );
        assert_eq!(
            tabs[1].get("container_id").and_then(|v| v.as_str()),
            Some("shopping")
        );
        assert_eq!(
            tabs[0].get("workspace_id").and_then(|v| v.as_str()),
            Some("research")
        );
        assert_eq!(tabs[0].get("tab_group_id").and_then(JsonVal::as_u64), Some(7));
        assert_eq!(tabs[1].get("tab_group_id").and_then(JsonVal::as_u64), Some(0));
        assert_eq!(
            tabs[1].get("workspace_id").and_then(|v| v.as_str()),
            Some("personal")
        );

        let persisted_pins = parse_json(r#"{"tabs":[{"url":"https://example.com","pinned":true},{"url":"https://example.org","pinned":false}]}"#)
            .expect("pinned tab session should parse");
        let persisted_tabs = persisted_pins.get("tabs").and_then(|v| v.as_arr()).expect("tabs array");
        assert_eq!(persisted_tabs[0].get("pinned").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(persisted_tabs[1].get("pinned").and_then(|v| v.as_bool()), Some(false));

        let legacy = parse_json(r#"{"tabs":[{"url":"https://example.com"}]}"#)
            .expect("legacy session should parse");
        assert_eq!(
            legacy
                .get("tabs")
                .and_then(|v| v.as_arr())
                .and_then(|tabs| tabs[0].get("container_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("default"),
            "default"
        );
        assert!(legacy.get("active_workspace_id").is_none());
        assert!(legacy
            .get("tabs")
            .and_then(|v| v.as_arr())
            .and_then(|tabs| tabs[0].get("workspace_id"))
            .is_none());
        assert_eq!(DEFAULT_WORKSPACE_ID, "default");
    }

    #[test]
    fn test_container_id_validation_and_stable_profile_mapping() {
        assert_eq!(TpBrowserApp::normalize_container_id("work-1"), "work-1");
        assert_eq!(TpBrowserApp::normalize_container_id(""), "default");
        assert_eq!(TpBrowserApp::normalize_container_id("bad/name"), "default");
        assert_eq!(TpBrowserApp::normalize_container_id("  "), "default");
        assert_eq!(TpBrowserApp::container_profile_name("default"), "default");
        assert_eq!(TpBrowserApp::container_profile_name("work-1"), "container_work-1");
        assert_eq!(TpBrowserApp::container_profile_name("work-1"), TpBrowserApp::container_profile_name("work-1"));
        assert_ne!(TpBrowserApp::container_profile_name("work-1"), TpBrowserApp::container_profile_name("personal"));
    }

    #[test]
    fn workspace_discovery_command_accepts_only_optional_valid_tab_id() {
        assert_eq!(parse_workspace_discovery_tab_id(r#"{"cmd":"discover_shopee_shops"}"#), Ok(None));
        assert_eq!(parse_workspace_discovery_tab_id(r#"{"cmd":"discover_shopee_shops","tab_id":7}"#), Ok(Some(7)));
        assert_eq!(parse_workspace_discovery_tab_id(r#"{"cmd":"discover_shopee_shops","tab_id":0}"#), Err(()));
        assert_eq!(parse_workspace_discovery_tab_id(r#"{"cmd":"discover_shopee_shops","tab_id":"7"}"#), Err(()));
    }

    #[test]
    fn workspace_discovery_target_skips_workers_and_invalid_or_unready_urls() {
        let tabs = vec![(1, false, "https://banhang.shopee.vn/portal/shop".to_string(), true),
            (2, true, "https://banhang.shopee.vn/portal/shop".to_string(), true),
            (3, false, "https://example.test".to_string(), true),
            (4, false, "https://banhang.shopee.vn/portal/shop?x".to_string(), false),
            (5, false, "https://banhang.shopee.vn/portal/shop#x".to_string(), true)];
        assert_eq!(select_workspace_discovery_tab(&tabs, None), Some(1));
        assert_eq!(select_workspace_discovery_tab(&tabs, Some(2)), None);
        assert_eq!(select_workspace_discovery_tab(&tabs, Some(5)), Some(5));
    }

    #[test]
    fn workspace_discovery_payload_exposes_only_safe_shape() {
        let payload = workspace_discovery_payload(r#"{"status":"ok","account_identity":"unavailable","shops":[{"shop_id":"1","display_name":"A"}]}"#);
        assert!(payload.contains("account_identity"));
        assert!(payload.contains("shop_id"));
        assert!(payload.contains("display_name"));
        assert!(!payload.contains("\"account_id\""));
    }

    #[test]
    fn workspace_config_validation_fails_closed_for_malformed_state() {
        let default = Workspace::default_workspace();
        let work = Workspace {
            id: "workspace-17".to_string(),
            name: "Research".to_string(),
            icon: "✦".to_string(),
            accent: String::new(),
            container_id: "default".to_string(),
        };
        assert!(valid_workspace_list(&[default.clone(), work.clone()]));
        assert!(!valid_workspace_list(&[work.clone()]));
        assert!(!valid_workspace_list(&[default.clone(), Workspace {
            id: "default".to_string(),
            name: "Duplicate".to_string(),
            icon: "✦".to_string(),
            accent: String::new(),
            container_id: "default".to_string(),
        }]));
        assert!(!valid_workspace_list(&[default, Workspace {
            id: "bad/id".to_string(),
            name: "Invalid".to_string(),
            icon: "✦".to_string(),
            accent: String::new(),
            container_id: "default".to_string(),
        }]));
        assert!(!valid_workspace_name("\n"));
    }

    #[test]
    fn spaces_picker_contract_exposes_crud_and_active_space_state() {
        assert!(TAB_SHELL_HTML.contains("workspaces"));
        assert!(TAB_SHELL_HTML.contains("set_active_workspace"));
        assert!(TAB_SHELL_HTML.contains("create_workspace"));
        assert!(TAB_SHELL_HTML.contains("rename_workspace"));
        assert!(TAB_SHELL_HTML.contains("delete_workspace"));
    }

    #[test]
    fn task_selection_json_and_mcp_auth_token_roundtrip() {
        let payload = JsonVal::obj(vec![
            ("text", JsonVal::Str("line1\n\"quoted\"\\path".to_string())),
            ("source", JsonVal::Str("web".to_string())),
            ("contact", JsonVal::Str("A & \"B\"".to_string())),
        ])
        .to_json();
        let parsed = parse_json(&payload).unwrap();
        assert_eq!(parsed.get("text").and_then(JsonVal::as_str), Some("line1\n\"quoted\"\\path"));
        assert_eq!(parsed.get("contact").and_then(JsonVal::as_str), Some("A & \"B\""));

        let tok = mcp_server::generate_auth_token();
        assert!(tok.starts_with("tp_"));
        assert!(tok.len() >= 35);
        let cfg = format!("{{\"mcp_auth_token\":\"{}\"}}", tok);
        assert_eq!(TpBrowserApp::extract_field_str(&cfg, "mcp_auth_token"), Some(tok));
    }

    #[test]
    fn normalize_bookmarks_json_preserves_legacy_and_normalizes_folders_tags_keywords() {
        let raw = r##"[
            {"title":"Google Sheets","url":"https://docs.google.com/spreadsheets"},
            {"title":"YouTube","url":" https://www.youtube.com ","folder":" Giải trí ","tags":["#video","Music","video"],"keyword":" Y T "},
            {"title":"Invalid","url":"   "}
        ]"##;
        let normalized = TpBrowserApp::normalize_bookmarks_json(raw).unwrap();
        let parsed = parse_json(&normalized).unwrap();
        let arr = parsed.as_arr().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].get("folder").and_then(JsonVal::as_str), Some(""));
        assert_eq!(arr[0].get("keyword").and_then(JsonVal::as_str), Some(""));
        assert_eq!(arr[1].get("folder").and_then(JsonVal::as_str), Some("Giải trí"));
        assert_eq!(arr[1].get("keyword").and_then(JsonVal::as_str), Some("yt"));
        let tags: Vec<&str> = arr[1]
            .get("tags")
            .and_then(JsonVal::as_arr)
            .unwrap()
            .iter()
            .filter_map(JsonVal::as_str)
            .collect();
        assert_eq!(tags, vec!["video", "Music"]);
        assert!(TpBrowserApp::normalize_bookmarks_json("{bad").is_none());
    }

    #[test]
    fn normalize_boosts_json_normalizes_domains_selectors_and_prunes_empty_entries() {
        let raw = r##"{
            "https://WWW.YouTube.com/watch?v=1": {
                "css": " ytd-masthead { opacity: 0.9; } ",
                "zap_selectors": [" #secondary ", "#secondary", ""],
                "enabled": true
            },
            "example.com": {
                "css": "   ",
                "zap_selectors": []
            }
        }"##;
        let norm = TpBrowserApp::normalize_boosts_json(raw).unwrap();
        let parsed = parse_json(&norm).unwrap();
        let yt = parsed.get("youtube.com").unwrap();
        assert_eq!(yt.get("css").and_then(JsonVal::as_str), Some("ytd-masthead { opacity: 0.9; }"));
        assert_eq!(yt.get("enabled").and_then(JsonVal::as_bool), Some(true));
        let zaps: Vec<&str> = yt
            .get("zap_selectors")
            .and_then(JsonVal::as_arr)
            .unwrap()
            .iter()
            .filter_map(JsonVal::as_str)
            .collect();
        assert_eq!(zaps, vec!["#secondary"]);
        assert!(parsed.get("example.com").is_none());
        assert!(TpBrowserApp::normalize_boosts_json("[]").is_none());
    }

    #[test]
    fn permission_policy_maps_to_wry_response_and_safe_download_path_rejects_traversal() {
        assert_eq!(TpBrowserApp::permission_policy_code("allow"), 1);
        assert_eq!(TpBrowserApp::permission_policy_code("deny"), 2);
        assert_eq!(TpBrowserApp::permission_policy_code("anything"), 0);
        assert_eq!(
            TpBrowserApp::permission_response_from_code(0),
            super::PermissionResponse::Default
        );
        assert_eq!(
            TpBrowserApp::permission_response_from_code(1),
            super::PermissionResponse::Allow
        );
        assert_eq!(
            TpBrowserApp::permission_response_from_code(2),
            super::PermissionResponse::Deny
        );

        let root = std::env::temp_dir().join(format!("tpbrowser-dl-test-{}", std::process::id()));
        let dl_dir = root.join("Downloads");
        let outside = root.join("secret.txt");
        let inside = dl_dir.join("report.pdf");
        std::fs::create_dir_all(&dl_dir).unwrap();
        std::fs::write(&outside, "nope").unwrap();
        std::fs::write(&inside, "ok").unwrap();
        assert!(TpBrowserApp::is_safe_download_path(&inside.to_string_lossy(), &dl_dir).is_some());
        assert!(TpBrowserApp::is_safe_download_path(&outside.to_string_lossy(), &dl_dir).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn window_recovery_clamps_bounds_and_roundtrips_crash_state() {
        use super::{
            clamp_window_bounds_to_work_area, is_window_offscreen, parse_recovery_snapshot,
            recovery_needed, serialize_recovery_snapshot, RecoverySnapshot, RecoveryTabSnapshot,
            RecoveryWindowSnapshot, WindowBounds, WorkAreaRect,
        };

        let work = WorkAreaRect { left: 0, top: 0, right: 1920, bottom: 1040 };
        let offscreen = WindowBounds {
            x: 3400,
            y: 1500,
            width: 2400,
            height: 1600,
            maximized: false,
        };
        assert!(is_window_offscreen(&offscreen, &work));
        let clamped = clamp_window_bounds_to_work_area(offscreen, work.clone());
        assert_eq!(clamped.width, 1920);
        assert_eq!(clamped.height, 1040);
        assert_eq!(clamped.x, 0);
        assert_eq!(clamped.y, 0);
        assert!(!is_window_offscreen(&clamped, &work));

        let shifted = clamp_window_bounds_to_work_area(
            WindowBounds { x: -400, y: -120, width: 1100, height: 700, maximized: false },
            WorkAreaRect { left: 1920, top: 0, right: 3840, bottom: 1080 },
        );
        assert_eq!(shifted.x, 1920);
        assert_eq!(shifted.y, 0);

        let snap = RecoverySnapshot {
            version: 1,
            clean_shutdown: false,
            updated_at_epoch: 1_760_000_000,
            primary: RecoveryWindowSnapshot {
                role: "primary".to_string(),
                window_idx: 0,
                bounds: clamped.clone(),
                active_workspace_id: "default".to_string(),
                active_idx: 0,
                view_mode: "single".to_string(),
                tabs: vec![RecoveryTabSnapshot {
                    url: "https://example.com".to_string(),
                    title: "Example".to_string(),
                    profile: "default".to_string(),
                    container_id: "default".to_string(),
                    workspace_id: "default".to_string(),
                    pinned: false,
                    essential: false,
                    pinned_origin_url: String::new(),
                    tab_group_id: 0,
                }],
            },
            secondary: vec![RecoveryWindowSnapshot {
                role: "secondary".to_string(),
                window_idx: 1,
                bounds: WindowBounds { x: 100, y: 80, width: 1000, height: 720, maximized: true },
                active_workspace_id: "work".to_string(),
                active_idx: 0,
                view_mode: "columns".to_string(),
                tabs: vec![RecoveryTabSnapshot {
                    url: "https://banhang.shopee.vn".to_string(),
                    title: "Shopee".to_string(),
                    profile: "isolated-1".to_string(),
                    container_id: "shop".to_string(),
                    workspace_id: "work".to_string(),
                    pinned: true,
                    essential: false,
                    pinned_origin_url: "https://banhang.shopee.vn".to_string(),
                    tab_group_id: 3,
                }],
            }],
        };
        assert!(recovery_needed(&snap));
        let json = serialize_recovery_snapshot(&snap);
        let parsed = parse_recovery_snapshot(&json).expect("recovery json must parse");
        assert_eq!(parsed, snap);

        let mut clean = parsed.clone();
        clean.clean_shutdown = true;
        clean.secondary.clear();
        assert!(!recovery_needed(&clean));
    }

    #[test]
    fn cached_newtab_html_reuses_static_allocation_and_theme_variants() {
        use super::{cached_history_html, cached_newtab_html};

        let dark1 = cached_newtab_html("obsidian-gold");
        let dark2 = cached_newtab_html("obsidian-gold");
        assert!(std::ptr::eq(dark1.as_ptr(), dark2.as_ptr()));
        assert!(dark1.contains("data-theme=\"obsidian-gold\""));

        let light1 = cached_newtab_html("carrara-light");
        let light2 = cached_newtab_html("warm-paper");
        assert!(std::ptr::eq(light1.as_ptr(), light2.as_ptr()));
        assert!(light1.contains("data-theme=\"carrara-light\""));

        let hist1 = cached_history_html("obsidian-gold");
        let hist2 = cached_history_html("obsidian-gold");
        assert!(std::ptr::eq(hist1.as_ptr(), hist2.as_ptr()));
    }

    #[test]
    fn context_menu_prefs_normalizes_and_cleans_tracking_urls() {
        let def = TpBrowserApp::normalize_context_menu_prefs("{}");
        let parsed = parse_json(&def).unwrap();
        assert_eq!(parsed.get("hide_separators").and_then(JsonVal::as_bool), Some(true));
        assert_eq!(parsed.get("clean_copy_link").and_then(JsonVal::as_bool), Some(true));
        assert_eq!(parsed.get("web_menu_enabled").and_then(JsonVal::as_bool), Some(true));
        assert_eq!(parsed.get("hide_mute").and_then(JsonVal::as_bool), Some(false));

        let custom = TpBrowserApp::normalize_context_menu_prefs(r#"{"hide_separators":false,"hide_mute":true,"web_menu_enabled":false}"#);
        let c_parsed = parse_json(&custom).unwrap();
        assert_eq!(c_parsed.get("hide_separators").and_then(JsonVal::as_bool), Some(false));
        assert_eq!(c_parsed.get("hide_mute").and_then(JsonVal::as_bool), Some(true));
        assert_eq!(c_parsed.get("web_menu_enabled").and_then(JsonVal::as_bool), Some(false));

        assert_eq!(
            TpBrowserApp::clean_tracking_url("https://example.com/watch?v=abc&utm_source=fb&fbclid=123&si=xyz#t=10"),
            "https://example.com/watch?v=abc#t=10"
        );
        assert_eq!(
            TpBrowserApp::clean_tracking_url("https://shopee.vn/item?utm_medium=cpc"),
            "https://shopee.vn/item"
        );
        assert_eq!(TpBrowserApp::clean_tracking_url("tp://newtab"), "tp://newtab");
    }

    #[test]
    fn mru_candidate_ordering_puts_active_then_mru_then_fallback() {
        let candidates = TpBrowserApp::compute_mru_candidate_ids(
            Some(30),
            &[10, 999, 20, 10],
            vec![10, 20, 30, 40],
        );
        assert_eq!(candidates, vec![30, 10, 20, 40]);

        // Wrapping step +1 and -1 across 4 candidates
        assert_eq!(((0isize + 1).rem_euclid(4)) as usize, 1);
        assert_eq!(((0isize - 1).rem_euclid(4)) as usize, 3);
    }

}
