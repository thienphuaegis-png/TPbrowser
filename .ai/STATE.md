# STATE.md — TPbrowser (`D:\Projects_Hermes\TPbrowser`)

**Checkpoint:** `v0.9.54-2026-11-25-zen-polish-extensions-verified` (2026-11-25)
**Dự án:** `TPbrowser` — Trình duyệt cá nhân Rust (`tao` + `wry` / WebView2), giao diện **AURELIA**.

## 1. Phiên Hiện Tại (2026-11-25)
- **Window Sync & Crash Recovery (`src/main.rs`, `src/mcp_server.rs`, `src/settings_modal.html`)**: Bền vững hóa `window_recovery.json` cho primary/secondary + clamp work-area monitor.
- **Fast New Tab / Warm Standby Pool (`src/main.rs`, `src/newtab.html`, `src/titlebar.html`, `src/content_agent.js`)**: `WarmStandbyTab`, static cache HTML New Tab/History và bỏ blur nặng giúp mở tab mới tức thì.
- **Zen Context Menu & Pill Cleanup (`src/main.rs`, `src/tab_shell.html`, `src/titlebar.html`, `src/content_agent.js`, `src/settings_modal.html`)**: Chuẩn hóa `context_menu_prefs`, `clean_tracking_url` (nối cả `CopyActiveUrl`), đồng bộ menu chuột phải ngang/dọc/web; bỏ pill `Lập Plan Cowork` trên trang web.
- **Centered Command Bar & Ctrl+Tab MRU Switcher HUD (`src/main.rs`, `src/tab_shell.html`, `src/content_agent.js`)**: `tab_shell` dùng `.with_transparent(true)` làm host overlay trung tâm (`#command_bar_overlay` + `#ctrl_tab_hud_overlay`) cho cả tab dọc lẫn ngang; đồng bộ `step_mru_tab` / `commit_mru_tab` / `cancel_mru_tab`.
- **Better Find Bar (`src/content_agent.js`, `src/main.rs`)**: Nâng cấp `#__tp_find_bar` hỗ trợ `obsidian-gold` & `carrara-light`, bộ đếm `x/y`, nút `Aa`, và `CSS.highlights` (`tp-find`/`tp-find-active`) không mất focus ô tìm kiếm.
- **WebView2 Extension Popup & Dashboard (`src/main.rs`, `Cargo.toml`, `src/titlebar.html`, `src/tab_shell.html`)**: Query `ICoreWebView2Profile7::GetBrowserExtensions` lọc extension hệ thống Edge, mở `chrome-extension://<id>/popup.html` qua Zen Glance và `dashboard.html` qua tab mới; hết sạch `dead_code` warnings & 20 file `.bak_before_*`.
- **Kiểm chứng live & test (`TPbrowser.exe`, SHA-256 `C0685547...`)**: `node --check` pass, `pytest -q tests/` **68/68**, `cargo test` **60/60**, `cargo build --release` **0 warnings**, MCP live smoke test xác nhận Extension ID `edginfnmgfcpgieinlabhlgholjccmee`, Command Bar `600px`, MRU HUD `760px`, Find Bar light/count và Split View.

## 2. Việc Tiếp Theo
- [ ] Chọn tính năng kế tiếp theo `/brainstorming` (Screenshot `CapturePreview`, Command Bar `@keyword`/Auto-Route Space, Zen Reader Mode v2, hoặc Extension Manager + Tab Folder UI).
- [ ] Chạy `git commit` khóa mốc `v0.9.54` khi anh xác nhận.

## 3. Rủi Ro & Thận Trọng
- Tuyệt đối tuân thủ Trusted Types trong `src/content_agent.js` (zero `.innerHTML` / `insertAdjacentHTML`).
- Không dùng `<input type="checkbox">` trong chrome UI; giữ nguyên thứ tự section của `src/tab_shell.html`.
- Không sửa/in/commit `config.json`, `user_data/`, `window_recovery.json` hoặc `.env*`.

## 4. Quyết Định Kiến Trúc
- **Single-Host Transparent Overlay (`tab_shell`)**: Dùng chính `tab_shell` phủ toàn cửa sổ khi `full_overlay_active` (`command_bar_open || mru_hud_open`) giúp Command Bar & MRU HUD căn giữa chuẩn xác ở cả tab dọc và tab ngang mà không nhân bản UI sang `titlebar_shell`.
- **Non-Destructive Find Highlights (`CSS.highlights`)**: Tô sáng mọi kết quả tìm kiếm bằng `CSS Custom Highlight API` thay vì chèn thẻ `<mark>`, bảo toàn DOM/React/Trusted Types trên mọi trang web.
- **Contract-Safe Legacy Retention**: Các variant `AppCmd` TMĐT cũ chỉ gắn `#[allow(dead_code)]` để bảo vệ các assertion trong `tests/test_webview_ecom_scan.py` cho tới đợt gỡ bỏ đồng bộ.
