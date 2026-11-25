# Kế hoạch Brainstorming: Khắc phục Triệt để Bóc tách Dữ liệu TikTok Shop, Điểm Sức Khỏe, Khuyến Mãi & Chuẩn Hóa Thương Hiệu Sàn

**Ngày lập:** 2026-11-20  
**Tác giả:** Antigravity (Google DeepMind Team)  
**Trạng thái:** Chờ anh duyệt (BRAINSTORMING PROPOSAL)  
**Tài liệu tham chiếu:** [`AGENTS.md`](file:///d:/Projects_Hermes/TPbrowser/AGENTS.md), [`SKILL.md`](file:///C:/Users/thien/.gemini/config/plugins/hermes/skills/brainstorming/SKILL.md), Ảnh đính kèm: `media_1795150199854_a7a726c7.jpg`

---

## 1. Phân tích Nguyên nhân Gốc rễ (Root Cause Analysis từ Ảnh Thật & Log Thực Tế)

### A. Vấn đề 1: Doanh thu Trang chủ TikTok bị lấy sai (ra 15 ₫) & Quảng cáo Ads ra 0 ₫
1. **Trang chủ TikTok lấy nhầm số ngày/phần trăm thành Doanh thu:**
   - Trong log: `Trang chủ TikTok: Doanh thu hôm nay: 15 ₫`.
   - Nguyên nhân: Regex fallback `match(/(?:Doanh thu|GMV)[^\n\d]{0,25}(?:₫\s*)?([\d.,]+)/i)` đã khớp trúng đoạn text so sánh dạng *"Doanh thu so với 15 ngày trước"* hoặc `+15%`, và gán `15` thành GMV! Tiền VNĐ trên sàn ecom không bao giờ là `15 ₫` (phải từ hàng nghìn đến hàng triệu).
2. **TikTok Ads không lấy được số liệu (mặc dù trên ảnh thật hiển thị rõ ràng `2,035,236 VND`):**
   - Trên ảnh thật của anh (`media_1795150199854_a7a726c7.jpg`):
     * **Chi phí**: `2,035,236 VND` (kèm icon `ⓘ` và checkbox)
     * **Số lượng đơn hàng SKU**: `184` (kèm icon `ⓘ` và checkbox)
     * **Chi phí cho mỗi đơn h...**: `11,061 VND`
     * **Doanh thu gộp**: `13,861,351 VND`
     * **ROI**: `6.81`
   - **Bẫy DOM `indexOf("\n")` và Icon `ⓘ`**:
     * Trong code `fromOverviewCard`: Có dòng chặn `if (!rawLabel || rawLabel.length > 65 || rawLabel.indexOf("\n") !== -1) continue;`. Thẻ tiêu đề trên TikTok Ads chứa icon tooltip `ⓘ` dẫn đến `innerText` có dấu ngắt dòng `\n` hoặc chuỗi `ⓘ`, khiến script **bỏ qua toàn bộ các thẻ card này**!
     * So khớp chuỗi `t === "chi phí"` hoặc `t === "roi"` bị trượt vì dính chữ `ⓘ` hoặc `(?)`.
     * Đơn vị tiền tệ trong ảnh là **`VND`** (chữ in hoa sau số), trong khi một số regex chỉ tìm ký hiệu `₫`.

### B. Vấn đề 2: Điểm tình trạng tài khoản TikTok chưa lấy được
- Trong `multi_shop_queue` (`src/main.rs:5670-5685` và `9270-9280`):
  * Quy trình quét TikTok hiện tại chỉ có 3 bước: Bước 1 (Trang chủ) -> Bước 2 (Ads) -> Bước 3 (Kho sản phẩm).
  * Sau Bước 3, tab bot đóng luôn và chuyển shop tiếp theo mà **không hề mở `seller-vn.tiktok.com/health-center`**.
  * Trong hàm tạo Snapshot `save_workspace_snapshot`: Mảng `shopHealth` chỉ duyệt qua `shopee_sources`, hoàn toàn bỏ quên dữ liệu sức khỏe của TikTok (`self.tiktok_health_json`).

### C. Vấn đề 3: Voucher, Khuyến mãi & Shop Flash Sale chưa lấy được thông tin
- Với Shopee: Đang bóc tách mã ID thô (`527151254819078`) thay vì hiển thị tên chương trình hoặc voucher giảm giá.
- Với TikTok Shop: Chưa có module cào khuyến mãi (`/promotion/campaign` hoặc `/marketing/voucher`), đồng thời hàm tổng hợp Snapshot chưa đưa TikTok promos vào bảng.

### D. Vấn đề 4: Chưa lấy được đơn hàng TikTok để cảnh báo giá bất thường (Price Anomalies)
- Mục `priceAnomalies` trong Snapshot hiện chỉ cào từ đơn hàng Shopee (`/portal/sale/order`).
- Cần bổ sung quét danh sách đơn hàng gần nhất của TikTok (`seller-vn.tiktok.com/order`) để phát hiện các đơn hàng bán giá lệch bất thường (< 15.000đ hoặc giảm sâu bất thường).

### E. Vấn đề 5 & 6: Màu sắc và Icon của TikTok Shop & các sàn bị sai
- **Màu TikTok**: Giao diện Workspace đang gán màu đỏ rực (`#FE2C55` / pill đỏ như Shopee) khi bấm vào tab TikTok. Trong thực tế, nhận diện thương hiệu chuẩn của TikTok là **Nền Đen (#000000 / #111111 / #161823) với viền/ánh Cyan (#25F4EE) & Hồng/Đỏ (#FE2C55)**.
- **Icon các sàn**: Đang dùng emoji rẻ tiền (`🟠 Shopee`, `🎵 TikTok Shop`, `🥞 Pancake`). Cần thay bằng **Vector SVG chuẩn 100% của Shopee, TikTok Shop và Pancake POS** trên toàn bộ Topbar switcher, dropdown danh sách shop, thẻ KPI và bảng thống kê.

---

## 2. Bản đồ Ánh xạ Dữ liệu (Data Mapping)

| Chỉ số / Thành phần | Nguồn DOM / URL thực tế | Cơ chế bóc tách & Chuẩn hóa | Giá trị dự kiến (theo ảnh mẫu của anh) |
|---|---|---|---|
| **TikTok Ads: Chi phí** | Card 1 trên `ads-creation/dashboard` | Lọc nhãn `chi phí` (bỏ qua `ⓘ` và khoảng trắng), trích xuất số trước `VND` | `2,035,236 ₫` |
| **TikTok Ads: Số đơn SKU** | Card 2 trên `ads-creation/dashboard` | Lọc nhãn `số lượng đơn hàng sku` hoặc `số lượng đơn`, lấy số nguyên | `184 đơn` |
| **TikTok Ads: CPA** | Card 3 trên `ads-creation/dashboard` | Lọc nhãn `chi phí cho mỗi đơn`, trích xuất số trước `VND` | `11,061 ₫` |
| **TikTok Ads: Doanh thu gộp**| Card 4 trên `ads-creation/dashboard` | Lọc nhãn `doanh thu gộp` hoặc `gmv`, trích xuất số trước `VND` | `13,861,351 ₫` |
| **TikTok Ads: ROI** | Card 5 trên `ads-creation/dashboard` | Lọc nhãn `roi` hoặc `roas`, bóc tách số thập phân | `6.81x` |
| **TikTok Sức khỏe Shop** | `seller-vn.tiktok.com/health-center` | Thêm Bước 4 trong hàng đợi quét TikTok, bóc tách Điểm trải nghiệm & Trạng thái | Điểm shop / Không vi phạm |
| **Cảnh báo giá bất thường** | `seller-vn.tiktok.com/order` & Shopee orders | Lọc đơn hàng có giá thanh toán `< 15.000 ₫` hoặc lệch >50% giá gốc | Danh sách đơn lệch kèm link xem đơn |
| **Voucher & Khuyến Mãi** | `seller-vn.tiktok.com/promotion` & Shopee promo | Trích xuất tên chương trình Flash sale, Voucher giảm giá đang hoạt động | Tên voucher/CTKM thay vì chuỗi ID thô |
| **Màu sắc TikTok** | CSS Workspace | Nền đen huyền bí `#0a0a0c`, pill `#161823`, accent Cyan `#25F4EE` + Pink `#FE2C55` | Giao diện công nghệ, đúng chất TikTok |
| **Icon sàn thương mại** | SVG Inline chuẩn | Thay thế emoji bằng SVG vector sắc nét của Shopee, TikTok, Pancake | Icon nhận diện chính hãng |

---

## 3. Ma trận Tác động Sự kiện (Event Impact Matrix)

1. **Trigger**:
   - Khi bấm **🚀 Quét tất cả**:
     * Shopee: Quét Đơn hàng -> Shopee Ads -> Sức khỏe shop Shopee.
     * TikTok: Bước 1 (Trang chủ) -> Bước 2 (TikTok Ads) -> Bước 3 (Kho sản phẩm & Đơn hàng) -> Bước 4 (Trung tâm sức khỏe shop).
2. **Entity State**:
   - Cả 4 payload dữ liệu của TikTok (`tiktok_json`, `tiktok_ads`, `tiktok_products_json`, `tiktok_health_json`) được nạp đầy đủ vào bộ nhớ state Rust.
3. **Ledger / Snapshot**:
   - `save_workspace_snapshot`:
     * Đưa điểm sức khỏe TikTok vào mảng `shopHealth`.
     * Đưa các đơn hàng giá thấp/bất thường từ TikTok vào mảng `priceAnomalies`.
     * Bóc tách tên khuyến mãi/voucher thật vào `promos`.
     * Cộng dồn chính xác Doanh thu Ads (`13.86 Tr`) và Chi phí Ads (`2.03 Tr`) vào tổng chỉ số tài chính.
4. **UI Workspace**:
   - Tab chuyển nền tảng đổi sang phong cách sang trọng: Shopee cam chuẩn, TikTok đen huyền bí neon (Cyan/Magenta), Pancake vàng xanh chuyên nghiệp.
   - Các icon SVG chính hãng thay thế hoàn toàn emoji `🟠`, `🎵`, `🥞`.

---

## 4. Các Phương Án Đề Xuất Giải Quyết

### Phương án: Nâng cấp Toàn diện Bộ Parser + Chuẩn hóa Giao diện Thương hiệu (Khuyên dùng)
- **Về Parser:**
  1. Loại bỏ triệt để điều kiện `indexOf("\n")` trong nhãn thẻ card Ads, làm sạch ký tự icon `ⓘ`, `(?)` bằng `.replace(/[ⓘ?]/g, "").trim()`.
  2. Bắt chuẩn đơn vị tiền tệ cả `VND` lẫn `₫`, tách dấu phẩy hàng nghìn `2,035,236` chính xác 100%.
  3. Thêm Bước 4 quét `health-center` cho TikTok trong `multi_shop_queue` và đưa vào Snapshot `shopHealth`.
  4. Lọc bỏ các con số nhỏ (< 1.000) và số ngày so sánh khi bóc tách Doanh thu trang chủ TikTok.
  5. Cào bổ sung đơn hàng TikTok để đẩy vào `priceAnomalies` (Cảnh báo giá bất thường).
- **Về Giao diện Thương hiệu:**
  1. Tích hợp bộ SVG vector chính hãng cho Shopee, TikTok và Pancake POS.
  2. Đổi giao diện tab TikTok từ màu đỏ chói sang màu Đen đẳng cấp `#000000`/`#161823` với dải viền gradient Cyan-Magenta (`#25F4EE` - `#FE2C55`).

---

## 5. Quyết định Đã Chốt

| # | Câu hỏi | Đáp án đã chốt |
|---|---|---|
| 1 | Quy trình quét TikTok 4 bước | **Đồng ý**, quét đầy đủ 4 bước (Trang chủ -> Ads -> Kho & Đơn hàng -> Sức khỏe shop) |
| 2 | Nhận diện thương hiệu sàn & Icon | **Đồng ý**, dùng SVG vector chính hãng cho Shopee, TikTok, Pancake và đổi màu TikTok sang Đen neon (Cyan/Magenta) |
| 3 | Ngưỡng cảnh báo giá bất thường TikTok | **Đơn dưới 15.000 ₫** (đồng bộ với quy chuẩn Shopee) |
