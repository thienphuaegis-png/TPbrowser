# Brainstorming: Khắc phục Phiên đăng nhập Shop, Hủy quét, Focus Tab khi vướng Captcha & Thanh tiến trình cao cấp

**Ngày lập:** 2026-11-20  
**Tác giả:** AI Assistant (theo yêu cầu của anh Phú)  
**Trạng thái:** Chờ anh duyệt (Brainstorming Phase)

---

## 1. Phân Tích Nguyên Nhân Gốc (Root Cause Analysis từ Log thật)

Dựa vào log `user_data/logs/ecom_scanner.log`:
```
[2026-11-20 09:00:55] [INFO] [QUEUE] Bắt đầu hàng đợi đa shop với 2 shop
[2026-11-20 09:00:55] [INFO] [SHOPEE] Bắt đầu quét shop [1/2] PetsCare Sài Gòn (1556409510) - Bước 1: Đơn hàng
```
Và sau đó dừng lại, mở tab chưa đăng nhập:

| Hiện tượng | Nguyên nhân mã nguồn | Vị trí code (`src/main.rs`) |
|---|---|---|
| **Mở tab chưa đăng nhập dù trình duyệt đã login** | Trong `start_next_shop_in_multi_queue`, code gán `container = Some(format!("shop_{}", s_id))`. Điều này ép WebView2 tạo ra thư mục profile cô lập hoàn toàn mới (`user_data/containers/shop_<id>`) chưa có bất kỳ cookie đăng nhập nào của anh Phú. Trong khi thực tế anh đăng nhập Shopee Sub-account/Main Account trên profile mặc định (`container: None`). | `src/main.rs:5424` |
| **Thiếu chức năng hủy quét để bắt đầu lại** | Khi hàng đợi đang chạy hoặc kẹt một shop, UI chỉ có nút quét bị disable, không có cơ chế hủy (`cancel_workspace_scan`) để đóng worker tab, giải phóng RAM và reset state hàng đợi về 0. | `src/workspace.html`, `src/main.rs:8880` |
| **Vướng Captcha nhưng không tự chuyển sang tab đó** | Khi `ecom_agents.js` phát hiện trang login/captcha, Rust đã nhận được `AppCmd::ScanNeedAuth` nhưng chưa chuyển tab active của cửa sổ chính (`set_visible(true)` và `focus()` trên WebView2), khiến tab worker vẫn nằm ngầm hoặc không nhảy lên trước mắt người dùng để giải Captcha. | `src/main.rs:8888-8905` |
| **Thanh tiến trình đơn điệu** | Chỉ là 1 thanh kẻ 4px đổi % width, thiếu hiệu ứng shimmer animation, thiếu badge shop trực quan và thiếu nút thao tác nhanh (Bỏ qua / Hủy). | `src/workspace.html:711-719` |

---

## 2. Giải Pháp Thiết Kế Chi Tiết

### Vấn đề 1: Dùng chung Profile đã đăng nhập & Luồng chuyển shop Shopee chuẩn
* **Bỏ ép profile riêng**: Đổi `container = None` (hoặc tái sử dụng hàm `add_worker_tab(url, "bot", ...)`). Khi đó tab worker sẽ chia sẻ 100% cookie và session đăng nhập của trình duyệt.
* **Luồng chuyển đổi shop Shopee**:
  - Với tài khoản Shopee cha quản lý nhiều shop:
    - Bắt đầu: mở `https://banhang.shopee.vn/portal/shop` (danh sách shop).
    - Dùng `SHOPEE_OPEN_SHOP_JS` tìm đúng dòng `shop_id` của shop cần quét và click `Chi tiết` để Shopee chuyển ngữ cảnh vào shop đó.
    - Chờ chuyển trang sang `/portal/sale/order` để cào đơn.
    - Xong shop đó, quay lại `/portal/shop` để mở shop kế tiếp.

### Vấn đề 2: Chức năng "🛑 Hủy quét" (Cancel & Reset Queue)
* **Phía Frontend (`src/workspace.html`)**:
  - Thêm nút `🛑 Hủy quét` nổi bật trên thanh tiến trình.
  - Hàm `cancelScan()` gửi lệnh `{ cmd: "cancel_workspace_scan" }`.
  - Khôi phục trạng thái nút "🚀 Quét tất cả" (bật lại clickable) và ẩn thanh tiến trình.
* **Phía Rust Core (`src/main.rs`)**:
  - Nhận `AppCmd::CancelWorkspaceScan`.
  - Đóng sạch tab worker đang mở (`self.execute(AppCmd::CloseTabById(tid))`).
  - Xóa sạch danh sách hàng đợi (`self.multi_shop_queue.clear()`, `self.multi_shop_idx = 0`).
  - Reset toàn bộ cờ: `self.multi_shop_auth_pending = false`, `self.multi_shop_step = String::new()`.
  - Đồng bộ lại tiến trình UI: `active: false, status: "cancelled", label: "Đã hủy phiên quét."`.
  - Ghi log rõ ràng: `[INFO] [QUEUE] Người dùng đã bấm hủy phiên quét.`.

### Vấn đề 3: Tự động chuyển tab (Auto-Focus Tab) khi vướng Captcha/Login
* Khi `ecom_agents.js` phát hiện trang login hoặc iframe Captcha (`AppCmd::ScanNeedAuth`):
  1. Gỡ cờ worker: `tab.worker_kind = None`.
  2. Đảm bảo tab thuộc workspace hiện tại: `tab.workspace_id = self.active_workspace_id.clone()`.
  3. Bật hiển thị: `tab.webview.set_visible(true)`.
  4. Chuyển tab active của trình duyệt: `self.execute(AppCmd::FocusIndex(pos))`.
  5. Focus con trỏ vào WebView2: `tab.webview.focus()`.
  6. Đồng bộ lại thanh tab bar: `self.sync_tab_bar(); self.sync_active_tab();`.
  7. **Kết quả**: Cửa sổ trình duyệt nhảy ngay lập tức vào tab Shopee/TikTok đó để anh Phú gõ mật khẩu, nhận OTP hoặc kéo thanh trượt giải Captcha mà không cần phải tự đi tìm tab!
  8. Sau khi anh xác thực xong, script tự động bắt được và tiếp tục cào dữ liệu mà không làm gián đoạn hàng đợi.

### Vấn đề 4: Nâng cấp Thanh tiến trình Cao Cấp (Polished Progress Bar)
* Giao diện kính mờ Glassmorphism bo tròn góc (`border-radius: 12px`, padding thoáng đãng).
* **Header thông tin**:
  - Chip trạng thái: `● Đang quét` có hiệu ứng nhấp nháy phát sáng (Pulse animation).
  - Tên shop hiện tại hiển thị bằng badge màu thương hiệu nổi bật.
  - Số % tiến độ font monospaced to bản, dễ nhìn.
  - Nút `⏭️ Bỏ qua shop này` + Nút `🛑 Hủy quét` đặt ngay góc phải tiện dụng.
* **Thanh Progress Bar**:
  - Dày 6px, bo tròn 3px.
  - Dải màu Gradient rực rỡ: Cam Shopee $\rightarrow$ Đỏ $\rightarrow$ Tím $\rightarrow$ Xanh Accent.
  - Hiệu ứng vệt sáng chuyển động không ngừng (`@keyframes shimmer`).
  - Đổi sang màu Xanh Ngọc Emerald rực rỡ khi hoàn tất 100%.

---

## 3. Ma Trận Tác Động Sự Kiện (Event Impact Matrix)

| Hành động | Trigger | Rust State | UI Workspace | Tab Worker |
|---|---|---|---|---|
| **Bấm "Hủy quét"** | `cancelScan()` qua IPC `cancel_workspace_scan` | Reset `multi_shop_queue`, `idx=0`, `auth_pending=false` | Ẩn progress bar, mở lại nút "Quét tất cả", hiện toast báo đã hủy | Tab worker lập tức bị đóng (`CloseTabById`), giải phóng RAM |
| **Gặp Captcha** | `scan_need_auth` từ JS | `multi_shop_auth_pending = true` | Hiện banner vàng kèm nút "Bỏ qua", thanh tiến trình hiện cảnh báo | Tab worker biến thành tab thường, trình duyệt tự switch sang tab này và focus |
| **Xác thực xong** | `scan_auth_resolved` từ JS | `multi_shop_auth_pending = false` | Tắt banner vàng, tiếp tục chạy % tiến độ | Tự động cào dữ liệu và tiếp tục module tiếp theo |
| **Bỏ qua shop** | `skipCurrentShop()` qua IPC `skip_current_shop` | Gắn cờ `is_stale: true`, đóng tab hiện tại, tăng `idx += 1` | Cập nhật nhãn sang shop tiếp theo | Đóng tab cũ, mở shop kế tiếp |
