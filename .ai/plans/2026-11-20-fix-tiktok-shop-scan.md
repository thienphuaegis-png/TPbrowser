# Kế hoạch Thực Thi: Khắc phục TikTok Shop & Quét Lệch Tồn Kho Đa Sàn (`/brainstorming`)

> **Quyết định đã chốt với anh Phú**: Áp dụng **Phương án A** (Quét đa bước: Doanh thu & Đơn hàng -> Chi phí Ads & ROAS -> Quản lý sản phẩm tồn kho), đồng thời **kích hoạt tính năng quét sản phẩm đa sàn để phát hiện sản phẩm hết hàng bị lệch giữa Shopee, TikTok và Pancake**.

---

## 1. Kiến trúc luồng Quét TikTok Shop Tuần Tự (Không đơ, Không kẹt)

### 3 Chặng quét tuần tự trên 1 tab worker duy nhất:
1. **Chặng 1: Homepage (`seller-vn.tiktok.com/homepage`)**:
   - URL: `https://seller-vn.tiktok.com/homepage`
   - Thu thập:
     * Tên Shop thật: Lấy từ phần tử profile, loại bỏ các từ khóa hệ thống như "Trung tâm nhà bán hàng", "Seller Center", "Trang chủ"...
     * Doanh thu (GMV hôm nay): bóc tách từ thẻ "Hiệu suất hôm nay" / widget GMV.
     * Số đơn hàng hôm nay: bóc tách từ thẻ "Đơn hàng" / "Đơn chờ vận chuyển".
2. **Chặng 2: Ads Dashboard (`seller-vn.tiktok.com/ads-creation/dashboard...`)**:
   - Điều hướng tab worker sang: `https://seller-vn.tiktok.com/ads-creation/dashboard?origin=SC_ads_tab_button_PC&mpa=1&type=product&shop_region=VN`
   - Thu thập:
     * Chi phí Ads (`cost`)
     * Doanh số do Ads mang lại (`sales`)
     * ROAS & CIR
3. **Chặng 3: Quản lý sản phẩm & Tồn kho (`seller-vn.tiktok.com/product/manage`)**:
   - Điều hướng tab worker sang: `https://seller-vn.tiktok.com/product/manage`
   - Thu thập danh sách SKU, Tên sản phẩm, Số lượng tồn kho (`stock`), và các sản phẩm đang có tồn bằng 0 (`stock === 0`).
4. **Hoàn tất TikTok & Chuyển giao hàng đợi**:
   - Đóng tab worker TikTok (`CloseTabById`), giải phóng 100% RAM.
   - Gọi `self.multi_shop_idx += 1; self.start_next_shop_in_multi_queue();` để tiếp tục shop tiếp theo hoặc hoàn thành phiên quét lưu snapshot (`finish_multi_shop_queue`).

---

## 2. Cơ chế So khớp Lệch Tồn Kho Đa Sàn (Cross-Platform SKU Stock Mismatch)

### Nguồn dữ liệu tồn kho:
- **Shopee**: Dữ liệu tồn kho từ đơn hàng & quản lý sản phẩm (`banhang.shopee.vn/portal/product/list/all`).
- **TikTok Shop**: Dữ liệu tồn kho từ trang quản lý sản phẩm (`seller-vn.tiktok.com/product/manage`).
- **Pancake POS**: Dữ liệu tồn kho từ API variations của Pancake (`/api/v1/shops/{id}/variations`).

### Quy tắc phát hiện lệch tồn:
1. So khớp theo mã SKU hoặc Tên sản phẩm tương đồng:
   - Nếu `shopee_stock == 0` và `tiktok_stock > 0` hoặc `pancake_stock > 0`:
     * Đánh dấu `mismatch: true`
     * Ghi chú: `"Shopee HẾT HÀNG nhưng TikTok & Kho còn hàng — Cần mở lại tồn Shopee ngay!"`
   - Nếu `tiktok_stock == 0` và `shopee_stock > 0` hoặc `pancake_stock > 0`:
     * Đánh dấu `mismatch: true`
     * Ghi chú: `"TikTok HẾT HÀNG trong khi Shopee còn hàng — Cần bổ sung tồn TikTok tránh mất đơn Live!"`
   - Nếu tồn kho toàn hệ thống `<= 3`:
     * Đánh dấu `mismatch: true`
     * Ghi chú: `"Tồn kho chạm đáy toàn hệ thống (< 3 cái) — Cần tắt Hỏa tốc!"`

---

## 3. Các bước thực hiện:
- [ ] **Bước 1**: Viết lại `TIKTOK_IDENTITY_JS` để loại bỏ nhãn "Trung tâm nhà bán hàng" và bóc tách đúng Tên Shop thật.
- [ ] **Bước 2**: Viết `TIKTOK_HOMEPAGE_SCAN_JS` để cào GMV & Số đơn hàng trên Homepage.
- [ ] **Bước 3**: Viết `TIKTOK_PRODUCTS_SCAN_JS` để cào danh sách SKU + Tồn kho trên `seller-vn.tiktok.com/product/manage`.
- [ ] **Bước 4**: Nối luồng 3 chặng TikTok trong Rust (`src/main.rs`) và gọi callback `self.multi_shop_idx += 1; self.start_next_shop_in_multi_queue()` để không bao giờ bị kẹt hàng đợi.
- [ ] **Bước 5**: Tích hợp danh sách SKU tồn kho vào `save_workspace_snapshot` -> sinh mảng `crossSku` thật.
- [ ] **Bước 6**: Kiểm thử `cargo test`, build release `TPbrowser.exe` và nghiệm thu.
