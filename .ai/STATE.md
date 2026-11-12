# STATE.md — TPbrowser (`D:\Projects_Hermes\TPbrowser`)

**Checkpoint:** `v0.5.4-settings-fix-and-startup-stability` (2026-11-12)
**Dự án:** `TPbrowser` — Trình duyệt cá nhân Rust (`tao` + `wry` / WebView2), giao diện **AURELIA**, **Thanh trên Icon-First tích hợp Tab Ngang + URL ngắn**, **Favicon thật trên từng Tab**, **Kéo giãn Tab Dọc tự do**, **Cửa sổ Chat tối giản**, **Bộ Logo & App Icon**, **Universal MCP Server**, AI Cowork & `uBlock Origin Lite`.

---

## 1. Phiên Hiện Tại

- **Sửa Toàn Diện Lỗi Nút Cài Đặt & Ổn Định Khởi Động (`src/main.rs`, `src/settings_modal.html`, `src/titlebar.html`, `src/tab_shell.html`, `src/newtab.html`, `src/history.html`, `src/chat_header.html`, `src/dock_resizer.html`, `src/sidebar_resizer.html`)**:
  - **Giữ đúng Tab Cài Đặt đang mở**: `build_config_payload_json(None)` gửi `open_section: ""` thay vì luôn reset về `"look_feel"`; đồng bộ `set_settings_section`; bỏ hẳn `spawn_titlebar_hover_watcher` gây spam `sync_all_ui()`.
  - **Bỏ `sw_compact_titlebar` & Thêm `startup_mode`**: Thanh trên luôn cố định `40px`; thêm lựa chọn `Khi mở TPbrowser (Khởi động)` trong Tab 2 (`Khôi phục Tab cũ` với đúng `active_idx` | `Mở Tab mới sạch`), parse JSON chuẩn bằng `parse_json()` cho `save_setting` và `save_ai_config`.
  - **Sửa CSS Tab 4, Gemini URL & Mô tả Tab 1**: Style `.zs-input`, đổi `.pill-btn.primary` → `.pill-btn.cta`, `var(--border-col)` → `var(--gold-line)`, sửa `.setting-info > b`, giữ nguyên Base URL khi chọn `Gemini Direct`.
  - **Khởi động đồng bộ 0-Flash**: Tính sẵn `compute_shell_layout` trước khi tạo WebView con, tiêm `ui_theme` + `window.__TP_BOOTSTRAP__` vào `titlebar.html` / `tab_shell.html` từ frame đầu tiên, bổ sung bắt tay `ui_shell_ready` + `boot_settle`, ẩn sạch WebView phụ lúc tạo.
- **Tinh gọn Giao Diện Icon-First, Favicon Tab & Kéo Giãn Tab Dọc Tự Do (`v0.5.3`)**: Đã hoàn tất trước đó.

---

## 2. Bằng Chứng Kiểm Thử

- **Rust Unit Tests (`cargo test`)**: `15 passed; 0 failed` (bao gồm `test_settings_modal_and_startup_stability`, `test_compact_icon_ui_and_sidebar_resizer`, `test_brand_logo_and_icon_assets`).
- **Build Release (`cargo build --release`)**: `0 errors, 0 warnings`, đã deploy ra `TPbrowser.exe`.
- **Kiểm chứng sống & thị giác (`TPbrowser.exe` + MCP `tp_eval_js` + `PrintWindow` + `vision_analyze`)**: Xác nhận trên bản chạy thật — Tab 2 (`startup_mode`, `sidebar_collapsed`), Tab 3, Tab 4 (`snap_settings_tab2.png`, `snap_settings_tab4.png`), Tab 5 đều giữ nguyên tab khi bấm nút và lưu trạng thái đúng; cold-startup render đủ theme, favicon, Google profile.
- **Kiểm tra Checkpoint Trio (`pytest -q tests/test_checkpoint_integrity.py`)**: Đầy đủ `AGENTS.md`, `CLAUDE.md`, `STATE.md` (< 100 dòng), `MAP.html`, `PROJECT_STATE.json`.

---

## 3. Kiến Trúc Cốt Lõi

- `src/main.rs`: Nhân Win32 & Multi-WebView2 (`tab_shell`, `tabs`, `ai_shell`, `fb_shell`, `zalo_shell`, `titlebar_shell`, `dock_resizer_shell`, `settings_shell`), Win32 `FlashWindowEx`, timer nhắc việc 15s ngầm, quản lý `config.json` / `tasks.json` / `skills.json` / `session.json`.
- `src/layout.rs`: Tính toán hình học 3 vùng (`TabOrientation`, `AiDisplayMode`, `ContentViewMode`).
- `src/settings_modal.html`: Bảng Cài Đặt Tập Trung 6 phân mục kiểu Zen Browser.
- `src/ai_panel.html`: Không gian Kế Hoạch & Trò Chuyện Cowork kép, checklist phân vai AI/Bạn, phát nháp báo cáo Zalo.
- `src/content_agent.js`: Thẻ nhắc việc nổi đa tầng, chuông báo Web Audio API và bắt việc nhanh từ mọi văn bản web.
- `src/newtab.html`: Khay việc hôm nay tự động ẩn, đồng hồ serif và lưới bookmarks đồng bộ.

---

## 4. Việc Kế Tiếp

- [x] Hoàn tất triển khai kế hoạch **Zalo Cowork, Bảng Kế Hoạch Công Việc & Nhắc Việc Trực Tiếp** (`.ai/plans/2026-11-11-zalo-cowork-task-planner-reminders.md`).
- [x] Chuẩn hóa bộ phím tắt trình duyệt + `Alt` kiểu Zen, thêm khay `Alt+B` Collections & sửa lỗi Trusted Types mất 3 nút trên YouTube.
- [x] Tích hợp bộ Logo & App Icon (Bộ 1 Dải lụa + Bộ 2 Mũi tên, đổi trong Cài đặt & đổi icon Taskbar/Titlebar/NewTab theo theme).
- [x] Tinh gọn giao diện Icon-First, Logo mở Cài đặt, URL ngắn + Tab Ngang trên thanh trên, Favicon website thật trên Tab, bỏ thanh chuyển kênh trên Chat nổi & kéo giãn Tab Dọc tự do.
- [x] Sửa dứt điểm lỗi nút Cài Đặt giật về Tab 1, vỡ CSS Tab 4, thêm tùy chọn khởi động `startup_mode` và ổn định cold-startup `window.__TP_BOOTSTRAP__` + `ui_shell_ready`.
- [ ] Bổ sung thêm các bộ Skill mẫu theo yêu cầu công việc hằng ngày của anh Phú.
