# Brainstorming: Khắc phục Dữ liệu mẫu bị khóa cứng, Phân cụm 3 Nền tảng, Popover Click-Outside & Tối ưu Quét Shop

**Ngày lập:** 2026-11-20  
**Tác giả:** AI Assistant (theo yêu cầu của anh Phú)  
**Trạng thái:** Chờ anh duyệt (Brainstorming Phase)

---

## 1. Phân Tích Hiện Trạng & Nguyên Nhân Gốc (Root Cause)

| Vấn đề | Nguyên nhân mã nguồn | Vị trí code |
|---|---|---|
| **1. Khóa cứng thông tin mẫu (AuraHome, 34.8 Tr, 109 đơn)** | - Biến `snapshotStore` trong `src/workspace.html` khai báo tĩnh toàn bộ mock data cũ của shop `AuraHome Official Mall`, `minhanh_94`, `camp_9981`.<br>- File `user_data/snapshots/snap_latest.json` trên đĩa vẫn là file cũ tạo từ 01:38 AM đêm qua chứa nguyên khối mock này (do phiên quét 9:00 AM bị dừng trước khi kịp ghi đè).<br>- `src/workspace.html` khi tải trang tự nạp `snapshotStore.snap_latest` nên hiển thị toàn bộ số liệu giả. | `src/workspace.html:2296-2360`<br>`user_data/snapshots/snap_latest.json` |
| **2. Danh sách shop đã quét rồi vẫn phải quét lại** | - Nút quét hiện tại chỉ gửi danh sách ID và chạy từ đầu 100% không lưu trạng thái từng shop.<br>- Trên popover danh sách shop không hiển thị badge trạng thái `✅ Đã cập nhật (HH:mm)` hay `⏳ Chưa quét`, khiến người dùng không biết shop nào đã có dữ liệu. | `src/workspace.html:2520`<br>`src/main.rs:8880` |
| **3. Ô sát bên nút chọn shop không có chức năng** | - Thẻ `<select id="snapshot-select">` bị co cụm rỗng, class `.ctrl-select` không có width/styling rõ ràng trên giao diện, option bên trong lại gán cứng text `(19/11 - 21:30)`, không có nhãn làm người dùng tưởng ô rác/nút hỏng. | `src/workspace.html:686-690` |
| **4. Click ra ngoài không đóng popover chọn shop** | - Thiếu sự kiện `document.addEventListener("click", ...)` để kiểm tra nếu click nằm ngoài `#shopee-shop-selection-panel` và `#shopee-shop-pill` thì gỡ bỏ class `.open`. | `src/workspace.html:2514` |
| **5. Cấu hình Pancake API bị nhét đáy popover Shopee** | - Nút `⚙️ Cấu hình API` của Pancake bị nhét chung dưới danh sách shop Shopee.<br>- Không phân chia 3 nền tảng (Shopee, TikTok, Pancake) rõ ràng để người dùng quản lý và chọn quét. | `src/workspace.html:757-766` |

---

## 2. Thiết Kế & Giải Pháp Chi Tiết

### Vấn đề 1: Xóa sạch Mock Data & Chuyển sang Zero-State thân thiện
* **Xóa bỏ vĩnh viễn dữ liệu mẫu `AuraHome`, `minhanh_94`, `34.8 Tr đ`**:
  - `snapshotStore.snap_latest` khởi tạo ở dạng **Empty/Zero State**:
    * Doanh thu: `0 ₫`, Đơn hàng: `0 đơn`, Lợi nhuận: `0 ₫`, Chi phí Ads: `0 ₫`.
    * Cảnh báo giá, đánh giá xấu, radar live: mảng rỗng `[]`.
    * Nhãn thông báo: `"Chưa có dữ liệu phiên quét hôm nay — Bấm '🚀 Quét tất cả' để cập nhật"`.
  - Xóa file `snap_latest.json` cũ kỹ trên đĩa chứa AuraHome.
  - Sau khi quét thành công shop nào, số liệu thật của shop đó sẽ lập tức được hiển thị thay thế cho số 0.

### Vấn đề 2: Lưu trạng thái từng shop & Không bắt quét lại shop đã quét
* **Lưu cache dữ liệu từng shop trong ngày (`user_data/snapshots/shops/<shop_id>.json`)**:
  - Khi một shop quét thành công, lưu lại timestamp và dữ liệu của riêng shop đó.
  - Trên popover danh sách shop, mỗi dòng shop hiển thị:
    * `[x] PetsCare Sài Gòn` — `✅ Đã quét lúc 09:30 (12 đơn • 3.2 Tr)`
    * `[ ] PetMallVN` — `⏳ Chưa quét hôm nay`
  - Nút **"Quét shop chọn"**: Chỉ quét những shop đang được tích chọn mà chưa quét, hoặc người dùng muốn quét lại.
  - Nút **"Quét lại tất cả"**: Làm mới toàn bộ.

### Vấn đề 3: Tinh gọn / Sửa ô Snapshot Select sát nút chọn shop
* Sửa thẻ `<select id="snapshot-select">`:
  - Thêm icon và nhãn trực quan: `🕒 Lịch sử phiên: [Phiên mới nhất ▼]`.
  - Tự động nạp danh sách các file snapshot thật từ thư mục `user_data/snapshots/` (ví dụ: `Hôm nay 20/11`, `Hôm qua 19/11`...).
  - Nếu chỉ có 1 phiên duy nhất, hiển thị nhãn pill gọn gàng `🕒 Hôm nay 20/11` thay vì một ô select rỗng gây hiểu nhầm.

### Vấn đề 4: Đóng Popover khi Click Chuột Ra Ngoài (Click-Outside)
* Thêm event listener toàn cục:
  ```javascript
  document.addEventListener("click", function(e) {
    var panel = document.getElementById("shopee-shop-selection-panel");
    var btn = document.getElementById("shopee-shop-pill");
    if (!panel || !panel.classList.contains("open")) return;
    if (!panel.contains(e.target) && !btn.contains(e.target)) {
      panel.classList.remove("open");
    }
  });
  ```
  Nhấp bất kỳ vị trí nào ngoài bảng là popover đóng lại ngay tức thì, mượt mà.

### Vấn đề 5: Phân chia 3 Nền Tảng (Shopee - TikTok - Pancake) rõ ràng
Thiết kế lại Popover chọn gian hàng hiển thị thành **3 Khối Riêng Biệt**:

```
┌────────────────────────────────────────────────────────┐
│ 🏪 QUẢN LÝ GIAN HÀNG & NỀN TẢNG                      │
├────────────────────────────────────────────────────────┤
│ 🟠 SHOPEE (5 gian hàng)           [Chọn tất cả] [↻ Tìm]│
│   ☑ PetsCare Sài Gòn              ✅ Đã quét 09:30     │
│   ☑ PetMallVN                     ✅ Đã quét 09:32     │
│   ☐ Tui Tiến Pet VN               ⏳ Chưa quét         │
│   ☐ MasterCare Việt Nam           ⏳ Chưa quét         │
│   ☐ Master Care for Pet           ⏳ Chưa quét         │
├────────────────────────────────────────────────────────┤
│ 🎵 TIKTOK SHOP                                         │
│   ☑ TikTok Shop VN (MasterCare)   🟢 Đã kết nối        │
├────────────────────────────────────────────────────────┤
│ 🥞 PANCAKE POS                                         │
│   ☑ Kênh Pancake POS (1 kho)      🟢 Đã kết nối        │
│   [ ⚙️ Cấu hình API Key Pancake ]                      │
└────────────────────────────────────────────────────────┘
```
* **Đồng thời**: Khi người dùng click chuyển tab sang **🥞 Pancake** trên thanh tab bar, màn hình Pancake sẽ hiển thị sẵn form cấu hình API Key và trạng thái đồng bộ các kho của Pancake.

---

## 3. Ma Trận Tác Động Sự Kiện

| Thao tác | Thành phần UI | Tác động Rust/State |
|---|---|---|
| Click ra ngoài popover | `document.click` | `panel.classList.remove('open')` |
| Tick chọn shop/nền tảng | Checkbox Shopee/TikTok/Pancake | Cập nhật `shopRegistry` + lưu `selected_shops` xuống config |
| Bấm "Cấu hình API" | Mở input API Key Pancake | Lưu key vào `config.json` và test kết nối qua `/shops/check` |
| Bấm Quét shop chọn | Nút `⚡ Quét shop chọn` | Chỉ đẩy các shop được tích vào hàng đợi tuần tự |
| Tải trang lần đầu | `DOMContentLoaded` | Tải `snap_latest.json` thật, nếu rỗng hiện Zero-State (không hiện AuraHome) |
