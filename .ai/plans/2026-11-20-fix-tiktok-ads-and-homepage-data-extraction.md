# Kế hoạch Brainstorming: Khắc phục Triệt để Bóc tách Dữ liệu TikTok Shop (Trang chủ GMV & TikTok Ads)

**Ngày lập:** 2026-11-20  
**Tác giả:** Antigravity (Google DeepMind Team)  
**Trạng thái:** Chờ anh duyệt (BRAINSTORMING PROPOSAL)  
**Tài liệu tham chiếu:** [`AGENTS.md`](file:///d:/Projects_Hermes/TPbrowser/AGENTS.md), [`SKILL.md`](file:///C:/Users/thien/.gemini/config/plugins/hermes/skills/brainstorming/SKILL.md)

---

## 1. Nguyên nhân gốc rễ (Root Cause Analysis từ File Log & Code thật)

### A. Bước 1: Trang chủ TikTok (`TIKTOK_HOMEPAGE_SCAN_JS`) trả về 0 GMV & 0 Đơn
1. **Bẫy DOM `parentElement` hạn hẹp**:
   - Code hiện tại (`src/main.rs:394-417`): Tìm element có chữ `"doanh thu"`, sau đó chỉ lấy `parent = nodes[i].parentElement`.
   - Trên TikTok Seller Center (sử dụng Arco Design / React SPA): Thẻ text `"Doanh thu bán hàng"` và con số tiền (`₫ 120.000`) thường là hai thẻ `div` ngang hàng (siblings) nằm trong một component card chung (`.overview-item` hoặc `.arco-statistic`). Do đó `parentElement` chỉ là wrapper của nhãn mà không chứa số tiền.
   - Regex `var m = pTxt.match(/₫?\s*([\d.,]+)\s*₫?/);` có thể bắt nhầm vào các con số khác như `+15%` hoặc `24h` nếu card có dòng so sánh trước con số doanh thu.
2. **Thời gian chờ quá ngắn (2.2s)**:
   - Trang chủ TikTok Seller Center rất nặng, tải nhiều bundle JS và gọi các API bất đồng bộ (`/api/v1/seller/home/...`). Sau 2.2 giây, các ô doanh thu thường vẫn đang ở trạng thái Skeleton/Loading, dẫn đến script cào ra 0 và gửi payload về ngay.

### B. Bước 2: Quảng cáo TikTok Ads (`TIKTOK_PASSIVE_SCAN_JS`) trả về 0 & trạng thái `partial`
1. **Bẫy điều kiện `parts.length === 2` với cụm từ "so với"**:
   - Code hiện tại (`src/main.rs:636-647`):
     ```javascript
     var parts = cTxt.split(/so với/i);
     if (parts.length === 2) { ... }
     ```
   - Nếu dashboard Ads của TikTok không hiển thị chữ "so với" (ví dụ: giao diện tiếng Anh, tài khoản mới bật ads chưa có kỳ so sánh trước, hoặc có 2 mốc so sánh khiến `parts.length > 2`), hàm `fromOverviewCard` sẽ **bỏ qua toàn bộ** và trả về `overview_card_not_found`.
   - Điều này khiến toàn bộ các chỉ số `cost`, `sales`, `roas`, `orders` đều bị `null` (hiển thị 0 ₫ trên giao diện).
2. **Từ khóa Label quá cứng nhắc**:
   - Hiện tại chỉ bắt các từ cố định như `chi phí`, `doanh thu gộp`, `roi`. Nếu giao diện hiển thị `Chi phí quảng cáo`, `GMV từ quảng cáo`, `ROAS`, `Lợi tức đầu tư` thì không khớp.

### C. Bước 3: Lỗi bắt nhầm Product ID 19 chữ số thành số lượng tồn kho
- Trong `TIKTOK_PRODUCTS_SCAN_JS`: Biểu thức fallback `rTxt.match(/(\d+)\s*(?:cái|sản phẩm|chiếc)?/)` có cụm đơn vị là tùy chọn (`?`), nên vô tình khớp với dãy số ID sản phẩm 19 số của TikTok (`1731920003160312320`) thay vì số tồn kho thật.

---

## 2. Bảng Ánh Xạ Dữ Liệu (Data Mapping)

| Chỉ số cần lấy | Nguồn DOM trên TikTok Seller | Cơ chế bóc tách mới đề xuất | Giá trị dự kiến |
|---|---|---|---|
| **Tên shop TikTok** | Header profile, `.arco-dropdown-trigger`, avatar menu | Bỏ qua chuỗi "Trung tâm nhà bán hàng", lấy tên shop thật (MasterCare) | `MasterCare Việt Nam` |
| **GMV Trang chủ** | Thẻ card Doanh thu trên `seller-vn.tiktok.com/homepage` | Dùng `el.closest(...)`, tìm số tiền có ký hiệu `₫` hoặc số lớn nhất trong card | Số nguyên VNĐ (vd `1.250.000 ₫`) |
| **Số đơn Trang chủ** | Thẻ card Đơn hàng hôm nay | Dùng `el.closest(...)`, tìm số nguyên trong card sau nhãn Đơn hàng | Số nguyên (vd `15 đơn`) |
| **Đơn chờ vận chuyển** | Thẻ "Chờ vận chuyển" / "Chờ lấy hàng" | Regex bắt số lượng đơn đang chờ giao | Số nguyên (vd `3 đơn`) |
| **Chi phí TikTok Ads** | Thẻ Chi phí / Cost trên dashboard Ads | Tìm card chứa "Chi phí" (không phụ thuộc "so với"), bóc tách số tiền VNĐ | Số nguyên VNĐ |
| **Doanh thu TikTok Ads** | Thẻ Doanh thu / GMV Ads | Tìm card chứa "Doanh thu", bóc tách số tiền VNĐ | Số nguyên VNĐ |
| **ROAS TikTok Ads** | Thẻ ROI / ROAS trên dashboard Ads | Bóc tách số thập phân hoặc tính `Doanh thu / Chi phí` | Số thực (vd `3.25x`) |
| **Tồn kho sản phẩm** | Cột "Tồn kho" trên `/product/manage` | Ưu tiên ô chứa chữ "tồn kho", loại bỏ số `> 100.000` (tránh ID) | Số nguyên (vd `250 cái`) |

---

## 3. Ma Trận Tác Động Sự Kiện (Event Impact Matrix)

1. **Trigger (UI / Worker)**:
   - Khi chạy hàng đợi đa shop, worker mở lần lượt 3 URL:
     * `seller-vn.tiktok.com/homepage` (Trang chủ)
     * `seller-vn.tiktok.com/ads-creation/dashboard...` (Ads)
     * `seller-vn.tiktok.com/product/manage` (Tồn kho)
2. **Entity State**:
   - Dữ liệu `tiktok_json`, `tiktok_ads`, `tiktok_products_json` trong Rust được cập nhật các con số thực tế thay vì 0.
3. **Ledger / KPI**:
   - `kpis.netRev` và `kpis.totalOrders` trong snapshot được cộng dồn chính xác doanh thu TikTok Shop vào tổng doanh thu hệ thống.
4. **Kho hàng & Lệch kho (Cross-SKU)**:
   - Tồn kho sản phẩm TikTok chuẩn xác (không còn con số rác 19 chữ số), đối chiếu chính xác với tồn kho Shopee để phát hiện hết hàng thật sự.
5. **UI & Logs**:
   - Log `[DATA] [TIKTOK]` ghi nhận đầy đủ Doanh thu hôm nay, đơn hôm nay, chi phí Ads, tồn kho thật và hiển thị lên Terminal cũng như Dashboard Workspace.

---

## 4. Các Phương Án Đề Xuất Giải Quyết

### Phương án 1: Cải tiến Bộ bóc tách DOM thông minh + Cơ chế Polling Retry (Khuyên dùng)
- **Cơ chế:**
  1. Thay thế cách duyệt `parentElement` bằng `el.closest("[class*='card'], [class*='statistic'], [class*='item'], [class*='module'], div")` để gom trọn vẹn component thẻ dữ liệu.
  2. Bỏ hẳn điều kiện bắt buộc `so với` trong TikTok Ads; trích xuất con số doanh thu/chi phí trực tiếp từ thẻ.
  3. Thêm cơ chế **Polling kiểm tra mỗi 1.2 giây (tối đa 4 lần = 5 giây)**: Nếu DOM chưa load xong số liệu thì đợi, ngay khi số liệu xuất hiện là cào và gửi về Rust ngay lập tức (không bắt người dùng chờ lâu nếu mạng nhanh).
  4. Lọc chặt chẽ tồn kho sản phẩm: Chỉ nhận số `< 100.000` và ưu tiên thẻ có class `stock` hoặc text `tồn kho`.
  5. Reset biến `tiktok_shop_name` trong `config.json` nếu đang là "Trung tâm nhà bán hàng" để cập nhật tên shop thật.
- **Ưu điểm:** Bóc tách dữ liệu sạch sẽ, thích ứng linh hoạt với mọi thay đổi nhỏ của giao diện TikTok, không còn bị số 0 hay số rác.

### Phương án 2: Bóc tách thẳng từ API Network/State ngầm (Fetch Interceptor)
- **Cơ chế:** Tiêm script can thiệp vào `window.fetch` hoặc `XMLHttpRequest` để bắt gói tin JSON trả về từ backend của TikTok (`/api/v1/...`).
- **Nhược điểm:** Phức tạp, dễ bị TikTok phát hiện và chặn bảo mật, rủi ro cao hơn phương án 1.

---

## 5. Câu hỏi cần anh chốt

Anh duyệt giúp em:
1. **Anh có đồng ý triển khai theo Phương án 1 (Cải tiến DOM Selector thông minh + Polling Retry 1.2s)** để cào chuẩn Doanh thu Trang chủ & Ads TikTok không ạ?
2. **Về tên shop TikTok:** Em sẽ xóa chữ *"Trung tâm nhà bán hàng"* đang lưu trong config để tự động nhận diện lại tên shop thật (như MasterCare), anh duyệt nhé?
