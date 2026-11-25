# ĐẶC TẢ KỸ THUẬT & KẾ HOẠCH TRIỂN KHAI LOGIC: MULTI-PLATFORM COMMAND CENTER
> **File tài liệu:** `.ai/specs/WORKSPACE_DATA_AND_LOGIC_PLAN.md`  
> **Dự án:** TPbrowser (`src/workspace.html` + Rust Core `src/main.rs` + E-Commerce Workers)  
> **Kiến trúc giao diện:** Hybrid Landing Page Command Center (100% Offline Native, Auto Light/Dark Theme, Dynamic Platform Branding)  
> **Mục tiêu:** Nối dữ liệu thực tế từ Shopee (tài khoản mẹ đa shop con), TikTok Shop và Pancake POS (qua REST API) vào giao diện điều hành; hỗ trợ lưu lịch sử phiên quét, mở tab xử lý trực tiếp đúng profile shop, xuất báo cáo chốt ca và chuẩn bị hook tự động hóa quảng cáo (Auto-Ads).

---

## 1. NGUYÊN TẮC KỸ THUẬT BẮT BUỘC (STRICT GUARDRAILS)

Mọi AI Agent và Developer khi triển khai code **BẮT BUỘC** tuân thủ 5 nguyên tắc bất di bất dịch sau:

1. **Bảo Toàn 100% Contract Tests (`pytest -q tests/` — 43/43 PASS):**
   * Tuyệt đối **không xóa, đổi tên hoặc thay đổi cấu trúc** của các thành phần ngầm trong `src/workspace.html`:
     * `<nav class="island-nav">` và đóng `</nav>`.
     * `<header class="shopee-subnav-bar">` và đóng `</header>`.
     * Thẻ comment `<!-- Đơn hàng & Tác vụ gần đây -->` trong `#shopee-view-tong-quan`.
     * Các ID anchor: `sp_toship`, `sp_shipping_sub`, `sp_anom_count`, `sp_bad_kpi`, `sp_live_kpi`, `sp_pas_sales`, `sp_discounts_list`, `sp_vouchers_list`, `sp_flashsales_list`, `tt_sales`, `tt_kpi_sub`, `tt_status_pill`, `tt_provenance_tbody`.
     * Rule CSS `.platform-kpi-grid` với `grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));`.
   * Sau mỗi lần sửa code, bắt buộc chạy `pytest -q tests/` và xác nhận đạt **43/43 PASS**.

2. **Bảo Mật Trusted Types & 100% Offline:**
   * Tuyệt đối **cấm** `.innerHTML`, `.outerHTML`, `insertAdjacentHTML`. Mọi thao tác dựng hoặc cập nhật DOM phải sử dụng DOM API chuẩn (`document.createElement`, `textContent`, `createEl()`, `clearEl()`).
   * Tuyệt đối không nhúng thư viện CDN ngoài; biểu đồ vẽ bằng HTML5 Canvas 2D Retina thuần túy.

3. **Quy Tắc Hiển Thị Khi Thiếu Dữ Liệu (`Unknown / Not Scanned`):**
   * Khi worker chưa quét hoặc sàn chưa kết nối, các chỉ số bắt buộc hiển thị ký tự gạch ngang `—`. Không tự ý điền số `0` giả.

4. **Đồng Bộ Giao Diện Sáng / Tối (`Light / Dark Mode`):**
   * Giao diện hỗ trợ tự động nhận diện `@media (prefers-color-scheme: light)` của trình duyệt và đồng bộ trực tiếp khi Rust Core phát sự kiện:
     ```javascript
     window.dispatchEvent(new CustomEvent("tp_config_changed", { detail: { theme: "light" | "dark" | "auto" } }));
     ```
   * Khi chuyển đổi giữa Sáng và Tối, hệ thống tự cập nhật biến CSS và vẽ lại biểu đồ Canvas 2D với độ tương phản phù hợp.

---

## 2. GIAO THỨC GIAO TIẾP IPC HAI CHIỀU (WEBVIEW2 $\leftrightarrow$ RUST CORE)

### 2.1 Lệnh từ Webview gửi lên Rust (`chrome.webview.postMessage`)

| Lệnh (`cmd`) | Payload JSON Chuẩn | Nhiệm Vụ Xử Lý Ở Rust Core |
|---|---|---|
| `shopee_discover_shops` | `{ "cmd": "shopee_discover_shops" }` | Quét tài khoản Shopee đang đăng nhập (Main Account / Sub-account) để lấy toàn bộ danh sách `shop_id` con được phân quyền, lưu registry và đẩy lại xuống Webview. |
| `save_pancake_api` | `{ "cmd": "save_pancake_api", "apiKey": "..." }` | Lưu Pancake API Key vào cấu hình local, gọi thử endpoint xác thực Pancake POS, lấy danh sách kho/shop và đẩy trạng thái kết nối xuống Webview. |
| `workspace_scan` | `{ "cmd": "workspace_scan", "platform": "all", "mode": "selected", "shopIds": ["sp_01", "tt_01", "pk_01"] }` | Kích hoạt worker thu thập dữ liệu phiên hiện tại của các gian hàng được chọn, chuẩn hóa dữ liệu, lưu file Snapshot mới và gọi `window.onWorkspaceData()`. |
| `load_snapshot` | `{ "cmd": "load_snapshot", "snapshotKey": "snap_20261119_2130" }` | Đọc file JSON phiên quét lịch sử từ thư mục lưu trữ và đẩy lại xuống Webview để xem lại số liệu. |
| `open_tab` | `{ "cmd": "open_tab", "url": "https://...", "shopId": "sp_01", "source": "workspace_command_center" }` | Mở tab trình duyệt mới trong TPbrowser **gắn đúng Cookie/Profile của `shopId`** để nhân viên thao tác xử lý ngay trên sàn. |
| `export_csv` | `{ "cmd": "export_csv", "snapshot": "snap_latest", "platform": "all" }` | Xuất file CSV tổng hợp doanh thu, đối soát lệch tồn SKU và cảnh báo vận hành ra thư mục Reports/Downloads. |
| `copy_report` | `{ "cmd": "copy_report", "text": "..." }` | Ghi nội dung báo cáo chốt ca vào System Clipboard. |

### 2.2 Schema Dữ Liệu Chuẩn Rust Đẩy Xuống Webview (`window.onWorkspaceData(payload)`)

```json
{
  "snapshotKey": "snap_20261119_2130",
  "availableSnapshots": [
    { "key": "snap_20261119_2130", "label": "🕒 Phiên mới nhất (19/11 - 21:30)" },
    { "key": "snap_20261118_2200", "label": "📁 Chốt ca tối qua (18/11 - 22:00)" }
  ],
  "pancakeStatus": {
    "connected": true,
    "shopsCount": 1,
    "label": "Đã kết nối (1 kho)"
  },
  "shops": [
    { "id": "sp_01", "platform": "shopee", "name": "AuraHome Official Mall", "selected": true },
    { "id": "sp_02", "platform": "shopee", "name": "AuraHome Gia Dụng Xanh", "selected": true },
    { "id": "tt_01", "platform": "tiktok", "name": "AuraHome Live Studio VN", "selected": true },
    { "id": "pk_01", "platform": "pancake", "name": "Pancake Kho Tổng", "selected": true }
  ],
  "snapshotData": {
    "timestamp": "19/11/2026 21:30",
    "platforms": [
      {
        "key": "shopee",
        "name": "Shopee (2 shop)",
        "rev": 34.8,
        "orders": 118,
        "cir": "10.2%",
        "liveRev": 9.4,
        "expressPct": "21.2%",
        "color": "#EE4D2D"
      }
    ],
    "kpis": {
      "netRev": 68.4,
      "revNote": "+12.4% so với phiên trước",
      "netProfit": 15.2,
      "marginPct": "22.2%",
      "adsSpend": 7.45,
      "cir": "10.9%",
      "roas": "9.18x",
      "totalOrders": 214,
      "aov": "319.600 đ",
      "expressPct": "19.1%",
      "expressUrgent": "41 đơn Hỏa tốc (3 chờ lấy gấp)",
      "cancelRate": "1.6%",
      "toShipCount": 19
    },
    "hours": ["00h", "04h", "08h", "12h", "16h", "20h", "21h30"],
    "revSeries": [3.2, 2.1, 7.8, 14.5, 11.9, 19.4, 9.5],
    "profitSeries": [0.7, 0.5, 1.8, 3.2, 2.6, 4.3, 2.1],
    "shopHealth": [
      {
        "shopId": "sp_01",
        "platform": "shopee",
        "shop": "AuraHome Official Mall",
        "points": "0/12 Sao Quả Tạ",
        "status": "healthy",
        "note": "Không hạn chế — Đủ điều kiện Flash Sale & Mega Campaign",
        "expressOnTime": "99.4%",
        "url": "https://banhang.shopee.vn/portal/settings/shop/health"
      }
    ],
    "liveSessions": [
      {
        "shopId": "tt_01",
        "platform": "tiktok",
        "shop": "AuraHome Live Studio VN",
        "title": "Xả Kho Đèn Ngủ & Decor Cuối Tuần",
        "status": "LIVE",
        "rev": 11.6,
        "sharePct": "58.2%",
        "viewers": 428,
        "orders": 39,
        "url": "https://seller-vn.tiktok.com/live/dashboard"
      }
    ],
    "priceAnomalies": [
      {
        "shopId": "sp_01",
        "orderId": "#261119A8X9",
        "platform": "shopee",
        "shop": "AuraHome Official",
        "sku": "AH-DEN-01",
        "soldPrice": "89.000 đ",
        "basePrice": "245.000 đ",
        "reason": "Chồng mã Voucher Shop + Flash Sale",
        "url": "https://banhang.shopee.vn/portal/sale/order/123456"
      }
    ],
    "badReviews": [
      {
        "shopId": "sp_01",
        "platform": "shopee",
        "shop": "AuraHome Official",
        "stars": 1,
        "customer": "minhanh_94",
        "content": "Giao thiếu dây nguồn đèn, nhắn tin chưa thấy shop rep",
        "orderId": "#261117K21",
        "url": "https://banhang.shopee.vn/portal/settings/shop/rating"
      }
    ],
    "crossSku": [
      {
        "sku": "AH-DEN-01",
        "name": "Đèn Ngủ Mặt Trăng 3D Cảm Ứng",
        "shopee": 0,
        "tiktok": 24,
        "pancake": 18,
        "mismatch": true,
        "note": "Shopee HẾT HÀNG nhưng TikTok & Kho vẫn còn — Mở lại tồn Shopee ngay",
        "url": "https://banhang.shopee.vn/portal/product/list/all"
      }
    ],
    "adsAlerts": [
      {
        "shopId": "sp_01",
        "campaignId": "camp_9981",
        "platform": "shopee",
        "shop": "AuraHome Official",
        "campaign": "Quảng cáo Tìm Kiếm - Đèn Ngủ",
        "dailyBudgetVnd": 500000,
        "spentVnd": 470000,
        "budgetPct": "94%",
        "cirNumeric": 9.8,
        "cir": "9.8%",
        "status": "CHẠM NGÂN SÁCH",
        "badge": "pill-warning",
        "url": "https://banhang.shopee.vn/portal/marketing/pas/index"
      }
    ],
    "promos": [
      {
        "shopId": "sp_01",
        "platform": "shopee",
        "shop": "AuraHome Official",
        "type": "Shop Flash Sale",
        "name": "Khung giờ 21h - Giảm 35% Đèn Ngủ",
        "usage": "42/50 suất (84%)",
        "url": "https://banhang.shopee.vn/portal/marketing/shop-flash-sale"
      }
    ]
  }
}
```
