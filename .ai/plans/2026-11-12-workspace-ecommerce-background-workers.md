# Plan: Màn Hình Làm Việc Riêng (Shopee / TikTok) & AI Worker Tab Chạy Ngầm

**Ngày:** 2026-11-12  
**Trạng thái:** Đã chốt phương án qua `clarify` — chờ anh Phú gật đầu để triển khai code  
**Mockup trực quan:** `.ai/sketches/2026-11-12-workspace-ecommerce-workers.html`

---

## 1. Khảo sát code thật (Dự án gốc & Tool anh đã làm)

1. **Tool Shopee & TikTok anh đã xây trên máy:**
   - `D:\Projects_Hermes\stz-ai-hub\src\agents.js` + `src\main.rs`:
     - **Shopee Orders API**: Trích `SPC_CDS` từ cookie, gọi `search_order_list_index` + `get_order_list_card_list` (tab `[100, 200, 300]`), chuẩn hóa `micro-VND / 100.000`, kiểm toán đơn giá bất thường đối chiếu `pos_prices.json`.
     - **Shopee PAS**: Đọc tab *"Chiến Dịch Nhóm & Chiến Dịch Đơn"*, radio `"Đang diễn ra"`, ghép 2 bảng `nameT` + `numT`, tính `ROAS`/`ACOS`.
     - **TikTok Seller (`seller-vn.tiktok.com`)**: Dùng `TIKTOK_PASSIVE_SCAN_JS` (IIFE ẩn danh hoàn toàn, giữ nguyên UA WebView2 & `navigator.webdriver` gốc để tránh CAPTCHA `SecSDK`), chọn `"Hôm nay"` qua 2 ô `YYYY-MM-DD`, đọc 5 KPI + bảng chiến dịch.
   - `D:\Projects_Hermes\shopee-scraper\src\`: Giao diện Hub tổng hợp + xuất `shopee_don_hang.csv` + `shopee_pas_latest.json`.
2. **Hiện trạng `TPbrowser` (`D:\Projects_Hermes\TPbrowser`):**
   - `src/main.rs:3938` (`RequestDomContextForAi`) và `src/main.rs:3988` (`ExecuteDomAction`) hiện luôn đánh vào `self.tabs.get(self.active_idx)` — tức là chiếm luôn Tab người dùng đang xem.
   - Chưa có khái niệm `Worker Tab` tách biệt khỏi `User Tabs`.

---

## 2. Bản đồ ánh xạ dữ liệu (Data Mapping)

| Thành phần mới | Nguồn kế thừa từ tool cũ | Vị trí trong `TPbrowser` | Ghi chú |
|---|---|---|---|
| **Màn hình làm việc riêng (`tp://workspace`)** | `shopee-scraper/src/start_hub.html` + `stz-ai-hub` | `src/workspace.html` + nút icon trên `src/titlebar.html` | Giao diện AURELIA Sáng/Tối đồng bộ |
| **Worker Tabs chạy ngầm (`is_worker`)** | `shared_env` (`WebContext`) của `TPbrowser` | `BrowserTab { is_worker, worker_label, ... }` trong `src/main.rs` | Dùng chung cookie đăng nhập; không đổi `active_idx`, không chiếm chuột OS |
| **Agent Shopee Đơn hàng & PAS** | `stz-ai-hub/src/agents.js` (dòng 208–892) + `pos_prices.json` | `src/ecom_agents.js` (chỉ inject vào tab worker Shopee) | Không inject vào TikTok để tránh `SecSDK` |
| **Agent TikTok Passive Scan** | `stz-ai-hub/src/main.rs` (`TIKTOK_PASSIVE_SCAN_JS`) | Rust `evaluate_script` IIFE ẩn danh từ `src/main.rs` | Không đổi UA, không chạm `navigator.webdriver` |
| **AI Cowork không chiếm Tab chính** | `RequestDomContextForAi` / `ExecuteDomAction` | `ai_worker_tab_id` + `CreateWorkerTab` / `CloseWorkerTab` | AI tự mở Worker Tab để thao tác; có tùy chọn đọc Tab hiện tại khi anh yêu cầu |
| **Kho lưu kết quả & CSV** | `exports/*.json`, `shopee_don_hang.csv` | `D:\Projects_Hermes\TPbrowser\exports\` | Cho phép mở lại tức thì & xuất CSV/Copy Zalo |

---

## 3. Đáp án đã chốt qua `clarify`

| # | Câu hỏi | Đáp án đã chốt |
|---|---|---|
| 1 | Vị trí mở Màn hình làm việc riêng (`tp://workspace`) | **Thêm 1 nút icon riêng trên thanh trên cùng (bên phải ô URL)** để bấm mở/nhảy nhanh tới `tp://workspace` mọi lúc |
| 2 | Hiển thị Tab ngầm khi AI / Shopee / TikTok chạy | **Vẫn hiện trên thanh Tab chính có gắn nhãn `BOT` / `AI`**, nhưng **tuyệt đối không tự nhảy focus** làm gián đoạn Tab anh đang xem |
| 3 | Phạm vi dữ liệu đợt 1 | **Shopee (Đơn hàng + Kiểm toán giá + Quảng cáo PAS) & TikTok Seller (KPI Hôm nay + Chiến dịch)**, có **Quét ngay / Tự động** & **Xuất CSV / Copy báo cáo** |

---

## 4. Kiến trúc thực thi & Ma trận tác động (Không chiếm chuột / Không cướp Tab)

1. **`BrowserTab` mở rộng (`src/main.rs`)**:
   - Thêm `worker_kind: Option<String>` (`None` = Tab người dùng, `Some("ai")` / `Some("bot")` = Tab ngầm).
   - `add_worker_tab(url, worker_kind, title)` tạo WebView trong cùng `shared_env` (kế thừa sẵn cookie đăng nhập), **KHÔNG đổi `self.active_idx`**, **KHÔNG gọi `webview.focus()`**, và trong `relayout()` đặt bounds thực (`1366x860`) ở lớp dưới (`HWND_BOTTOM`) + `MemoryUsageLevel::Normal` để SPA React (`Shopee`, `TikTok`) chạy API/DOM mượt mà mà không chiếm chuột hay màn hình của anh.
2. **AI Cowork dùng Worker Tab riêng (`src/ai_panel.html` + `src/main.rs`)**:
   - Thêm `ai_worker_tab_id: Option<u64>` + nút gạt nhỏ *"Làm việc trên Tab ngầm (AI)"* (mặc định **BẬT**) / *"Đọc Tab đang xem"* khi anh muốn AI tóm tắt trang hiện tại.
   - Khi AI cần mở trang web hoặc thao tác (`navigate`, `click`, `type`, `scroll`, `dom_context`), tự tạo hoặc tái sử dụng Tab `AI` (`worker_kind = Some("ai")`) và chạy `RequestDomContextForAi` / `ExecuteDomAction` đúng lên `tab_id` đó.
3. **Màn hình làm việc riêng `tp://workspace` (`src/workspace.html` + `src/ecom_agents.js`)**:
   - Nút icon riêng trên `src/titlebar.html` mở hoặc focus tab `tp://workspace`.
   - Giao diện AURELIA Sáng/Tối đồng bộ gồm: thanh trạng thái các Worker Tab (`Shopee Orders`, `Shopee PAS`, `TikTok Ads`, `AI Worker`), nút **Quét ngay tất cả** / **Quét riêng từng sàn** / **Tự động quét (15p/30p)**, 2 khối KPI Shopee & TikTok, bảng Chiến dịch & Đơn bất thường, nút **Xuất CSV (`exports/shopee_don_hang.csv`)** và **Copy báo cáo chuẩn Zalo**.
   - **An toàn TikTok `SecSDK`**: Giữ nguyên kiến trúc IIFE ẩn danh từ `stz-ai-hub` (không đổi User-Agent, không chạm `navigator.webdriver`, không chèn UI/global vào DOM TikTok). Nếu phát hiện CAPTCHA mảnh ghép, chỉ báo nút **"Mở Tab TikTok để anh kéo"** thay vì tự động đụng chuột.
