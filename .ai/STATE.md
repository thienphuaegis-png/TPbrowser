# STATE.md — TPbrowser (`D:\Projects_Hermes\TPbrowser`)

**Checkpoint:** `v0.9.5-2026-11-16-camoufox-scan-toggle` (2026-11-16)
**Dự án:** `TPbrowser` — Trình duyệt cá nhân Rust (`tao` + `wry` / WebView2), giao diện **AURELIA**, **Đổi Logo & Toàn Bộ Icon Giao Diện Sáng Sang Tối Sắc Nét & Nổi Bật Tối Đa**, **Cơ chế Startup Boost chuẩn Microsoft Edge (Khay Hệ Thống Native Win32, Single-Instance Mutex & Local /wake Endpoint, Chạy ngầm mở tức thì < 0.05s, Tự khởi động cùng Windows)**, Thanh Bookmark Tab Ngang (28px viền vàng, Ctrl+Shift+B, Titlebar động 40px/68px), Tối ưu 70% RAM & Lazy Sleeping Tabs.

---

## 1. Phiên Hiện Tại

- Thêm công tắc "Chế Độ Quét An Toàn Nâng Cao" (field `camoufox_enabled: bool`, default `false`) vào Cài Đặt → tab Bảo Mật & uBlock, chuẩn bị cho tích hợp Camoufox (Firefox vá fingerprint, server mode, chạy ngầm) làm engine quét TikTok/Shopee thay thế DOM-scrape WebView2 khi người dùng chủ động bật. Chưa nối logic spawn/kill tiến trình Camoufox thật — đánh dấu `// TODO` tại `AppCmd::SaveSetting::"camoufox_enabled"` trong `src/main.rs`.
- Toggle đi đúng pattern field cấu hình có sẵn (`adblock_enabled`): persist `config.json`, đọc lại lúc khởi động (`load_config`), đồng bộ 2 chiều với `settings_modal.html` qua `build_config_payload_json`/`__tpSyncSettingsModal` (merge qua `Object.assign`, không cần sửa thêm JS phía nhận).
- Quyết định kiến trúc đã thống nhất với người dùng (chưa code): Camoufox sẽ chạy như 1 tiến trình phụ (giống `cloudflared.exe` đang có), launch server mode giữ sống suốt session khi bật — không spawn-kill mỗi lần quét để tránh cold-start vài giây; mặc định OFF để không tốn RAM/khởi động chậm cho người chỉ lướt web.
- Hoàn tất chuỗi Command Bar MVP theo 3 card: native Right Alt+K state/signal, titlebar overlay với tìm tab/workspace bằng stable ID, và các lệnh Back/Forward/Reload/New Tab/Close Tab qua IPC hiện hữu.
- Test focused Right Alt+K, `cargo check`, `git diff --check` và `node --check` đều pass; live delivery khi child WebView giữ focus và overlay vượt khung titlebar 40/68px vẫn chưa xác minh.
- MRU `t_ab0a18c5` đã hoàn tất: Ctrl+Tab chạy theo runtime MRU bằng stable `tab_id`, lọc workspace hiện tại, resolve native index chỉ lúc focus; Ctrl+Shift+Tab giữ đường cũ.
- Đã sửa updater cho máy đã đóng gói: kiểm tra commit nguồn qua GitHub API, tải thẳng `TPbrowser.exe`, tự thoát/thay binary/mở lại; không còn phụ thuộc Git/Cargo/source tree trên máy khách.
- Build mới đã push: `a5bf425`; commit nguồn updater `f99eead` + `df28735`; checksum `TPbrowser.exe`: `1c2ac1ddbc359b102b7951918ad85f6640ba4726e59ff248cd1036fdf1285c10`.
- Bổ sung Window Geometry Recovery: lưu `window_width/window_height` vào `config.json`, khôi phục khi mở lại, clamp theo kích thước tối thiểu/tối đa và đồng bộ UI sau resize/DPI change; chưa claim live restart trên máy khách.
- URL bar nâng cao lượt đầu: titlebar nhận gợi ý thật từ tab đang mở, bookmarks và history; dedupe tối đa 30 URL qua native state, giữ nguyên `@ai ...` và `navigate_active`; chưa claim live WebView2 UI. Đã commit/push `2488a07`; binary mới checksum `623bdc06fb7af065fcedab3befa2d8aebafa92b09406688577037cf6bf1338ef`.
- Triển khai và vừa harden Zen Glance (Xem Nhanh Nổi): Alt+Click bắt ở `pointerdown` capture để không bị WebView nuốt, 3 webview con `glance_backdrop_shell`/`glance_header_shell`/`glance_body_shell`, Escape/click backdrop đóng và nút mở Tab lớn.
- Chuỗi Zen parity vẫn tạm dừng các phase khác theo yêu cầu người dùng; không tự tạo hoặc dispatch task mới ngoài Glance/Camoufox toggle.
- Các thay đổi updater và binary đã commit/push; chỉ còn file runtime tunnel chưa theo dõi (`cloudflared.exe`, `scripts/*_log`) và không đưa vào Git.

---

## 2. Bằng Chứng Kiểm Thử

- **Camoufox scan toggle (UI + persisted config)**: full Rust suite `23 passed; 0 failed`; `cargo check --release` sạch; `git diff --check` sạch (chỉ warning CRLF/LF bình thường trên Windows). Chưa có logic Camoufox thật nên không cần live verify process — chỉ verify lưu/đọc đúng field qua test hiện có + đọc lại luồng `save_config`/`load_config`/`build_config_payload_json` bằng code review.
- **Command Bar chain**: focused Right Alt+K test `1 passed`; `cargo check` pass; `node --check` titlebar script pass; `git diff --check` pass. Chưa live smoke verify child-WebView focus/overlay extent.
- **MRU**: full Rust suite `22 passed; 0 failed`; focused Right Alt test `1 passed`; `cargo check` và `git diff --check` pass. Không thay đổi persistence.
- **GitHub updater**: `cargo test` `22 passed; 0 failed`, `cargo check`, release build pass; PowerShell GitHub API probe trả về commit nguồn `df287358bdc0ae3f8ec8b4348a3937963e6c537a`; binary mới đã push và remote `origin/main` trùng `a5bf425`.
- **Window Geometry Recovery**: focused geometry test `1 passed`; full Rust suite `23 passed; 0 failed`; `cargo check`, `cargo build --release`, `git diff --check` pass. Binary checksum mới: `fc751a00e056078ecb9f830a1a1d72067f87af4b7545c3d136ebc93a81e81244`.
- **URL bar history suggestions**: full Rust suite `23 passed; 0 failed`; `cargo check`, `node --check src/content_agent.js`, `git diff --check` pass. Datalist lấy dữ liệu từ titlebar state thật, không thêm schema mới.
- **Checkpoint Integrity**: `codegraph.cmd sync . && python scripts/generate_checkpoint.py && pytest -q tests/test_checkpoint_integrity.py` → `4 passed; 0 failed`.
- Đã live smoke verify qua MCP: `/health` OK, `tools/list` OK, mở `tp://workspace` đọc được KPI/đơn bất thường/đánh giá/worker tab với dữ liệu thật, sau đó đóng đúng tab test. Chưa claim live pass cho toàn bộ Settings/Glance/TikTok CAPTCHA/Camoufox flow.

---

## 3. Kiến Trúc Cốt Lõi

- `src/main.rs`: Nhân Win32 & Multi-WebView2, Native System Tray, Single-Instance Mutex, Background Resident Controller, MCP Server HTTP :17777 + stdio.
- `src/mcp_server.rs`: Server MCP nội bộ + endpoint `/wake` và `/health`.
- `src/layout.rs`: Tính toán hình học 3 vùng (TabOrientation, AiDisplayMode, ContentViewMode).
- `src/titlebar.html`: Thanh trên hợp nhất 40px/68px, Bookmarks Bar 28px, Zen Split Popover, Copy Native Win32.
- `src/settings_modal.html`: Bảng Cài Đặt Tập Trung 7 Tab (Startup Boost, Background mode, Themes, AI Key, Camoufox scan toggle).
- `src/workspace.html` & `src/ecom_agents.js`: Landing Page TMĐT Shopee Audit / Live / Đánh giá 1-3★ & TikTok Seller.

---

## 4. Việc Kế Tiếp

- [ ] Camoufox: nối logic thật cho toggle `camoufox_enabled` — spawn/kill tiến trình server mode, lazy-download engine (~470MB) lần đầu bật, route nút "Quét TikTok" ở `workspace.html` qua Camoufox khi bật.
- [x] Zen MRU: Ctrl+Tab theo thứ tự dùng gần đây, stable `tab_id`, chỉ workspace hiện tại (`t_ab0a18c5`).
- [ ] Live verify Command Bar: Right Alt+K khi child WebView focus và overlay không bị cắt bởi titlebar WebView.
- [x] Workspace state/persistence, switcher, filtering và create workspace.
- [x] Pinned tab state/persistence.
- [x] Container identity/persistence và mở tab theo container.
- [x] Glance preview — Alt+Click mở thẻ nổi xem nhanh, ESC/click-outside đóng, nút mở ra Tab lớn; pointerdown capture đã harden.
- [ ] Chưa test manual UI Glance thật trên WebView2 (chỉ verify qua cargo test + cargo check + release build).
- [ ] Xác minh live updater trên một máy khách sạch (không Git/Rust).
- [ ] Live verify Window Geometry Recovery: đổi kích thước/DPI, đóng/mở lại và kiểm tra config/session thật.
- [ ] Live verify URL bar suggestions: gõ từ khóa, chọn gợi ý bằng Enter và kiểm tra `@ai ...`.
- [ ] Các phase Zen khác — tiếp tục từ URL bar sau khi live verify.
