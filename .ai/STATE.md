# STATE.md — TPbrowser (`D:\Projects_Hermes\TPbrowser`)

**Checkpoint:** `v0.5.6-shopee-landing-audit-live-ratings-promos` (2026-11-12)
**Dự án:** `TPbrowser` — Trình duyệt cá nhân Rust (`tao` + `wry` / WebView2), giao diện **AURELIA**, **Màn hình làm việc riêng TMĐT (`tp://workspace`) dạng Landing Page**, **Worker Tabs chạy ngầm không chiếm chuột**, **Thanh trên Icon-First tích hợp Tab Ngang + URL ngắn**, **Favicon thật trên từng Tab**, **Kéo giãn Tab Dọc tự do**, **Cửa sổ Chat tối giản**, **Bộ Logo & App Icon**, **Universal MCP Server**, AI Cowork & `uBlock Origin Lite`.

---

## 1. Phiên Hiện Tại

- **Nâng Cấp Màn Hình TMĐT `tp://workspace` Dạng Landing Page & Kiểm Toán Toàn Diện Shopee (`src/ecom_agents.js`, `src/workspace.html`, `src/main.rs`)**:
  - **Sửa lỗi kiểm toán đơn giá bất thường & đếm tab Shopee v3**: Đọc tổng đơn từ `payment_info.total_price` (micro-VND / 100.000), khách từ `buyer_info.username`, trạng thái `status_info.status`, vận chuyển `fulfilment_info.masked_channel_name`; chuẩn hóa mã tab `200=Chờ XN, 300=Chờ lấy, 400=Đang giao, 500=Đã giao`; chunk batch `5` đơn/request + `next_page_sentinel` để quét ~200 đơn mới nhất; tính trung vị theo đúng cặp `tên SP + phân loại` (không chia đều combo khi thiếu giá lẻ).
  - **Bổ sung 4 nguồn dữ liệu thật Shopee**: Đánh giá `1★/2★/3★` (200 đánh giá gần nhất qua `cursor = last_comment_id` + phát hiện từ khóa **Hàng giả / Hàng nhái / Mùi lạ / Hết hạn**), **Shopee Live** (`liveList/v2`), **Khuyến mãi Shop** (`discount/list` `limit<=10`), **Mã giảm giá** (`voucher/list`), **Flash Sale** (`get_shop_flash_sale_list` + cảnh báo hết hạn `≤ 24h`).
  - **Giao diện Landing Page & Mở trực tiếp**: Thanh điều hướng nổi 1 hàng, 6 thẻ KPI Hero, bảng chi tiết đơn bất thường, lọc đánh giá 1–3★ + search, lưới 3 cột Khuyến mãi/Voucher/Flash Sale, tích hợp đầy đủ vào nút **Copy Báo Cáo**, và **click vào bất kỳ dòng/thẻ nào sẽ mở Tab đi thẳng tới đúng đơn/chương trình để xử lý**.
- **TikTok Seller Center (`TIKTOK_PASSIVE_SCAN_JS`) & Worker Tab Ngầm (`v0.5.5`)**: Đã hoàn tất trước đó.

---

## 2. Bằng Chứng Kiểm Thử

- **Rust Unit Tests (`cargo test`)**: `16 passed; 0 failed` (bao gồm `test_workspace_and_background_worker_tabs`, `test_settings_modal_and_startup_stability`, `test_compact_icon_ui_and_sidebar_resizer`, `test_brand_logo_and_icon_assets`).
- **Build Release (`cargo build --release`)**: `0 errors, 0 warnings`, đã deploy ra `TPbrowser.exe`.
- **Kiểm chứng sống & thị giác (`TPbrowser.exe` + MCP + `PrintWindow` + `vision_analyze`)**: Xác nhận trên tài khoản thật (`mastercareforpet`): `Chờ lấy 118 | Đang giao 2738 | 172–200 đơn | 200 đánh giá 1–3★ (5 cảnh báo giả/nhái/mùi lạ) | 4 phiên Live | 11 KM | 9 Voucher | 11 Flash Sale`; kiểm tra trực tiếp `live_v2_hero.png`, `live_v3_audit.png`, `live_v3_promos.png` và click dòng mở thẳng `/portal/sale/order/...`, `/portal/marketing/add-on-deal/...`, `/portal/marketing/shop-flash-sale/...`.
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
