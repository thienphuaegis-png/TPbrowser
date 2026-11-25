# [E-Commerce Engine] 2-Level Multi-Shop Sequential Queue, Captcha Escalation & Real Snapshot Pipeline

> **For agentic workers:** REQUIRED SUB-SKILL: Use subagent-driven-development or executing-plans.

**Goal:** Giải quyết triệt để 2 vấn đề:
1. Trình duyệt bị đơ khi bấm quét (thay thế việc spawn đồng thời 3 WebView2 bằng Hàng đợi 2 cấp Shop -> Module, mỗi shop chạy trên 1 WebView2 duy nhất gắn profile `container_id`, chạy tuần tự Orders -> Ads -> Health, xong là destroy WebView2 giải phóng sạch RAM).
2. Dữ liệu Dashboard không đúng do gán cứng mock data (xóa bỏ 100% mock data, bóc tách dữ liệu thật theo VNĐ nguyên bản, phân tách `today_stats` lọc theo `create_time >= startOfTodayLocal()` [UTC+7] vs `pending_issues` toàn thời gian, tổng hợp vào `snap_YYYY-MM-DD.json`, hỗ trợ Interactive Escalation khi kẹt Captcha/Login kèm nút Bỏ Qua Shop Này, và format hiển thị động trên UI).

---

## 1. Yêu cầu & Quyết định kỹ thuật đã chốt
- **Hàng đợi 2 cấp (Shop -> Module) trong `src/main.rs`:**
  - Vòng ngoài: Duyệt tuần tự từng `shop_id` trong danh sách `shopIds`.
  - Vòng trong: Với mỗi `shop_id`, khởi tạo 1 WebView2 worker duy nhất gắn profile `container_id` (`shop_<id>`), điều hướng tuần tự:
    * Shopee: `Orders` (`/portal/sale/order`) -> `Ads` (`/portal/marketing/pas/...`) -> `Health` (`/portal/accounthealth/home`).
    * TikTok: `Identity & Ads` (`seller-vn.tiktok.com`).
  - Quét xong từng shop -> Gọi `close_tab_by_id` / `destroy` tab để giải phóng sạch 100% RAM trước khi sang shop kế tiếp.
- **Interactive Escalation khi kẹt Captcha / Login:**
  - `ecom_agents.js` phát hiện trang login hoặc captcha iframe/overlay -> gửi `scan_need_auth` về Rust kèm `shop_id`, `platform`, `reason`.
  - Rust tạm dừng timeout, chuyển worker webview sang hiển thị nổi (`set_visible(true)` và focus), đồng thời báo lên UI Workspace.
  - UI Workspace hiện thanh thông báo vàng: `[Shop X] Cần xác thực Captcha/Login...` kèm nút **"Bỏ qua shop này"**.
  - Script `ecom_agents.js` polling 1.5s/lần; khi anh giải Captcha/đăng nhập xong và trang vào portal, tự động gửi `scan_auth_resolved` để Rust ẩn/chạy tiếp.
  - Nếu anh bấm nút **"Bỏ qua shop này"**: Rust đóng tab shop hiện tại, đánh dấu `is_stale: true`, và tiếp tục shop tiếp theo trong hàng đợi.
- **Tránh lặp vô tận & SPA State Pollution trong cùng 1 tab:**
  - Khi điều hướng sang module tiếp theo, Rust set biến ngữ cảnh `window.__tpCurrentScanModule = "<module>"` và gọi trực tiếp `window.__tpRunShopeeScan("<module>")` với độ trễ timeout hợp lý.
- **Chuẩn hóa dữ liệu gốc VNĐ & Lọc ngày theo UTC+7 trong `src/ecom_agents.js`:**
  - Giữ nguyên số nguyên / f64 VNĐ (không chia 1.000.000).
  - Viết `startOfTodayLocal()` tính theo giờ địa phương Việt Nam (UTC+7):
    `var d = new Date(); d.setHours(0,0,0,0); return Math.floor(d.getTime() / 1000);` (so sánh chuẩn với Unix timestamp 10 chữ số của API Shopee).
  - Phân tách: `today_stats` (`revenue_vnd`, `order_count`, `cancelled_count`) vs `pending_issues` (`to_ship`, `express_orders`, `anomalies`, `bad_reviews`).
- **Tổng hợp Snapshot thật trong `src/main.rs` (`save_workspace_snapshot`):**
  - Xóa bỏ toàn bộ dữ liệu mock (`34.8`, `118`, `25.1`, `AuraHome`, `minhanh_94`...).
  - Cộng dồn dữ liệu thật từ các shop quét thành công, lưu vào `snap_YYYY-MM-DD.json` và `snap_latest.json`.
  - Hỗ trợ cờ `"is_stale": true` và trạng thái giữ số liệu cũ nếu shop bị skip hoặc gặp lỗi.
- **Cập nhật hiển thị Dashboard trong `src/workspace.html`:**
  - Helper `formatVND(amount, compact)` format tiền tệ VNĐ linh hoạt (ví dụ: `34.850.000 ₫` hoặc `34.8 Tr ₫` có tooltip).
  - Mini Progress Bar đa shop chi tiết: `[Shop 1/3 - Gia Dụng A] Bước 2/3: Đang quét Shopee Ads...`. Khóa nút `🚀 Quét tất cả` khi đang chạy.

---

## 2. Kế hoạch triển khai (Implementation Tasks)

### Task 1: Chuẩn hóa `src/ecom_agents.js` (UTC+7 startOfTodayLocal, Raw VNĐ, Captcha Poller, Context Module)
- [ ] Thêm `startOfTodayLocal()` trả về Unix timestamp (giây).
- [ ] Phân tách `today_stats` và `pending_issues` trong payload `shopee_orders`.
- [ ] Giữ nguyên số tiền VNĐ nguyên bản (không chia 100k hay 1tr).
- [ ] Bổ sung cơ chế phát hiện Captcha / Login -> gửi `scan_need_auth` và polling 1.5s/lần cho đến khi giải xong -> gửi `scan_auth_resolved`.
- [ ] Đồng bộ theo biến ngữ cảnh `window.__tpCurrentScanModule`.

### Task 2: Tái cấu trúc Hàng đợi 2 cấp & Escalation trong `src/main.rs`
- [ ] Xây dựng struct quản lý hàng đợi: `MultiShopScanQueue` với danh sách `shop_ids`, index hiện tại, và module hiện tại (`Orders -> Pas -> Health`).
- [ ] Triển khai hàm chạy từng shop trên 1 tab worker duy nhất với `container_id: "shop_<id>"`.
- [ ] Triển khai xử lý IPC:
  - `scan_need_auth`: Tạm dừng timeout, đưa tab lên nổi, gửi sự kiện cảnh báo lên Workspace UI.
  - `scan_auth_resolved`: Đưa tab về ẩn, tiếp tục quét.
  - `skip_current_shop`: Đóng tab hiện tại, gán cờ `is_stale = true` cho shop đó, chuyển sang shop kế tiếp.
- [ ] Khi chuyển module trong cùng shop: evaluate script với biến ngữ cảnh `__tpCurrentScanModule`.
- [ ] Khi kết thúc một shop: gọi `CloseTabById` để destroy WebView2 giải phóng 100% RAM, rồi kích hoạt shop kế tiếp.

### Task 3: Viết lại `save_workspace_snapshot` bằng Dữ liệu Thật trong `src/main.rs`
- [ ] Xóa bỏ toàn bộ mock data AuraHome, 34.8, 118, 25.1.
- [ ] Tổng hợp từ các payload shop thật đã thu thập: cộng doanh thu, số đơn, gom danh sách `anomalies`, `bad_reviews`, `adsAlerts`.
- [ ] Lưu file `snap_YYYY-MM-DD.json` theo ngày hiện tại và cập nhật `snap_latest.json`.
- [ ] Gửi snapshot thật về Dashboard qua `sync_workspace_command_center()`.

### Task 4: Nâng cấp Giao diện Dashboard `src/workspace.html`
- [ ] Bổ sung helper `formatVND(amount, compact)` chuẩn format tiếng Việt.
- [ ] Cập nhật các hàm `renderPlatformStrip`, `renderKpiCards`, `renderAlertsAndReviews`, `renderAdsAndPromos` để đọc đúng cấu trúc dữ liệu VNĐ thật và cờ `is_stale`.
- [ ] Thêm nút **"Bỏ qua shop này"** trên thanh cảnh báo xác thực.
- [ ] Cập nhật Mini Progress Bar đa shop và disable nút quét khi hàng đợi đang chạy.

### Task 5: Kiểm thử & Nghiệm thu
- [ ] Chạy `cargo test --bin TPbrowser` (bảo đảm 50/50 unit tests PASS).
- [ ] Chạy `pytest -q tests/` (bảo đảm 100% contract tests PASS).
- [ ] Chạy `pytest -q tests/test_checkpoint_integrity.py` (bảo đảm 4/4 PASS).
- [ ] Biên dịch `cargo build --release` ra bản `TPbrowser.exe` hoàn chỉnh.
