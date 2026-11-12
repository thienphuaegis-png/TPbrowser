# STATE.md — TPbrowser (`D:\Projects_Hermes\TPbrowser`)

**Checkpoint:** `v0.5.5-ecommerce-workspace-and-background-workers` (2026-11-12)
**Dự án:** `TPbrowser` — Trình duyệt cá nhân Rust (`tao` + `wry` / WebView2), giao diện **AURELIA**, **Màn hình làm việc riêng TMĐT (`tp://workspace`)**, **Worker Tabs chạy ngầm không chiếm chuột**, **Thanh trên Icon-First tích hợp Tab Ngang + URL ngắn**, **Favicon thật trên từng Tab**, **Kéo giãn Tab Dọc tự do**, **Cửa sổ Chat tối giản**, **Bộ Logo & App Icon**, **Universal MCP Server**, AI Cowork & `uBlock Origin Lite`.

---

## 1. Phiên Hiện Tại

- **Tích hợp Màn Hình Làm Việc Riêng TMĐT & Worker Tab Chạy Ngầm (`src/main.rs`, `src/workspace.html`, `src/ecom_agents.js`, `src/titlebar.html`, `src/tab_shell.html`, `src/ai_panel.html`)**:
  - **Màn hình làm việc riêng `tp://workspace`**: Mở nhanh bằng nút icon trên thanh trên (`Alt+W`), giao diện AURELIA Sáng/Tối đồng bộ, quản lý quét song song **Shopee (Đơn hàng + Kiểm toán giá bất thường + Quảng cáo PAS)** và **TikTok Seller (KPI Hôm nay + Bảng Chiến dịch)**.
  - **Nguyên tắc Không chiếm chuột & Không cướp Tab**:
    - Thêm `worker_kind: Option<String>` vào `BrowserTab` (`BOT` / `AI`), gán nhãn huy hiệu trên cả Tab Dọc lẫn Tab Ngang.
    - Worker Tab chạy ở lớp nền (`HWND_BOTTOM`) với kích thước thật `1366x860` + `MemoryUsageLevel::Normal` để SPA/API chạy mượt mà mà **không đổi `active_idx`**, **không gọi `webview.focus()`** và **không chiếm con trỏ chuột Windows**.
  - **AI Cowork tự mở Worker Tab riêng**: Thêm tùy chọn `quick_worker_tab` (mặc định BẬT) giúp AI Cowork tự tạo/dùng Tab ngầm `AI` để duyệt web, tìm kiếm và thao tác DOM mà không đè lên Tab người dùng đang xem.
  - **An toàn TikTok `SecSDK` & Sửa lỗi 2FA TikTok (`extensions/uBOLite.chromium`, `src/main.rs`, `src/content_agent.js`)**: Thêm rule ưu tiên `priority: 100` (`99001`/`99002`) mở khóa `/ttwid/check/`, `mssdk`, `byteoversea`, `tiktokv` cho TikTok trong `uBOLite`, bỏ ép User-Agent `Chrome/131` cũ trên `add_tab_internal` để dùng đúng `Edge/157` đồng bộ `Sec-CH-UA`, và guard `tiktok.com` trong `content_agent.js` (`zeroXhrCount: 0`, 43/43 endpoint trả HTTP 200).
  - **Kho xuất & báo cáo**: Tự lưu cache `exports/*.json`, xuất file CSV `exports/shopee_don_hang.csv` chuẩn Excel tiếng Việt (UTF-8 BOM), nút Copy báo cáo chuẩn định dạng Zalo.
- **Sửa Lỗi Nút Cài Đặt & Khởi Động Ổn Định (`v0.5.4`)**: Đã hoàn tất trước đó.

---

## 2. Bằng Chứng Kiểm Thử

- **Rust Unit Tests (`cargo test`)**: `16 passed; 0 failed` (bao gồm `test_workspace_and_background_worker_tabs`, `test_settings_modal_and_startup_stability`, `test_compact_icon_ui_and_sidebar_resizer`, `test_brand_logo_and_icon_assets`).
- **Build Release (`cargo build --release`)**: `0 errors, 0 warnings`, đã deploy ra `TPbrowser.exe`.
- **Kiểm chứng sống & thị giác (`TPbrowser.exe` + MCP + `PrintWindow` + `vision_analyze`)**: Xác nhận qua `live_workspace_view.png` (`tp://workspace` hiển thị chuẩn), `live_workspace_scanning.png` (3 tab ngầm `[BOT]` Shopee đơn, Shopee PAS, TikTok Ads chạy song song và cập nhật tiến trình vào dashboard mà không chiếm chuột hay cướp tab đang xem), và `live_user_tab_uninterrupted.png` (Tab 0 của người dùng hoàn toàn nguyên vẹn, giữ trọn focus).
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
