# STATE.md — TPbrowser (`D:\Projects_Hermes\TPbrowser`)

**Checkpoint:** `v0.9.11-2026-11-19-ecommerce-hierarchy-overview-cards` (2026-11-19)
**Dự án:** `TPbrowser` — Trình duyệt cá nhân Rust (`tao` + `wry` / WebView2), giao diện **AURELIA**, **Dashboard Shopee Executive 3 Cột Hiện Đại**, **Đổi Logo & Toàn Bộ Icon Giao Diện Sáng Sang Tối Sắc Nét**, **Cơ chế Startup Boost chuẩn Microsoft Edge (Native Win32 Tray, Single-Instance Mutex & Local /wake Endpoint, Mở tức thì < 0.05s, Tự khởi động cùng Windows)**, Thanh Bookmark Tab Ngang (28px viền vàng, Ctrl+Shift+B), Tối ưu 70% RAM & Lazy Sleeping Tabs.

---

## 1. Phiên Hiện Tại
- **Platform workspace wide/theme-first**: `src/workspace.html` dùng toàn chiều rộng khả dụng; Shopee/TikTok đi theo theme sáng/tối chung nhưng giữ accent riêng. Tổng quan Shopee lấy KPI thật hiện có làm chuẩn (đơn, rủi ro giá, đánh giá, Live, PAS, khuyến mãi); TikTok dùng cấu trúc tương tự và giữ `—` khi chưa có provenance.
- **Tổ chức Sub-Tabs Shopee (1a)**: Phân tách 4 góc nhìn mượt mà: Tổng quan (3 cột executive), Kiểm toán rủi ro (đơn bất thường & đánh giá 1★-3★), Vận hành Live & Ads, Khuyến mãi & Vouchers.
- **Shop Selection Panel (2a)**: Danh sách shop hiển thị cố định bên dưới thanh thao tác, có checkbox/click cả dòng, đếm số shop đã chọn, nút Chọn tất cả và hai CTA quét tuần tự; bỏ popup và chuông thông báo không dùng.
- **Discovery worker isolation**: Worker `shopee_discovery` chỉ lấy danh sách shop; không bị route collector đơn hàng nhận nhầm khi PageFinished, tránh log quét sai và giảm tải WebView khi chuyển tab.
- **Cinematic Workspace Intro**: GPU-accelerated Animated WebP intro (`assets/workspace_intro.webp`, 395 KB) với auto fade-out sau 1.9s và nút bỏ qua tức thì.
- **Vertical Sidebar Layout**: Nút `+` mở `tp://newtab` trực tiếp không bị chặn prompt; thanh URL chuyển lên trên; cho phép đóng tab cuối cùng trở về NewTab; nét gạch phân tách chuẩn.
- **Shopee + Tab Shell actions**: Bỏ TP SELLER khỏi header; thêm CTA Quét Shop luôn hiện; Worker nằm cuối sidebar, tách khỏi Tab ghim; Ctrl/Shift-click chọn nhiều tab; kéo thả để nhóm/tách; worker không được ghim; group/selection IPC được scope theo workspace và tab sender.
- **Tương thích toàn diện**: 27 focused/checkpoint tests pass và 50 Rust unit tests pass; PAS/Live giữ trạng thái `—` khi thiếu dữ liệu, TikTok dark tokens giữ tương phản, và dashboard không còn phụ thuộc Chart.js CDN.

---

## 2. Việc Tiếp Theo
- [x] Khung 3 tầng Tổng quan → Nền tảng → Shop: thêm 3 thẻ điều hướng Shopee/TikTok/Pancake ở tab Tổng quan (`.platform-switch-row`), mở đúng pane qua `activateWorkspaceTab`.
- [x] Shopee chọn shop bind đúng `shopee_shop_orders[shop_id]` qua `resolveShopeeSnapshot` (đã có từ lượt trước, verify lại ở lượt này); PAS/Health giữ nhãn "chỉ có dữ liệu cấp nền tảng" khi đang xem 1 shop.
- [x] Build release `cargo build --release`, copy `TPbrowser.exe` ở gốc (hash khớp target/release), chạy thử PID xác nhận đúng artifact.
- [ ] Mở WebView2 kiểm tra trực quan 3 thẻ Shopee/TikTok/Pancake và việc đổi shop Shopee cập nhật đúng số liệu theo shop (chưa có vision/screenshot pass cho WebView2 cửa sổ thật).
- [ ] TikTok per-shop discovery vẫn chưa có — giữ TikTok ở cấp nền tảng, hiện rõ "shop discovery chưa có" (đúng như đã duyệt, chưa làm lượt sau).

---

## 3. Rủi Ro & Thận Trọng
- Không được làm gãy các test contract thương mại điện tử trong `test_webview_ecom_scan.py`.
- Tránh phụ thuộc các thư viện CDN bên ngoài để đảm bảo tính năng chạy ngoại tuyến 100% trong môi trường WebView2.
- Giữ nguyên cấu trúc IPC native giữa WebView2 và Rust runtime.

---

## 4. Quyết Định Kiến Trúc
- Áp dụng mô hình Hybrid Canvas: Vẽ biểu đồ bằng HTML5 Canvas 2D Retina thuần túy, tự động thích ứng kích thước DPI màn hình và hoạt động hoàn hảo khi máy không có kết nối internet.
- Tái sử dụng trọn vẹn 100% IPC native `AppCmd::RunEcomScan` và cơ chế lưu trữ shop selection đã được kiểm thử.
