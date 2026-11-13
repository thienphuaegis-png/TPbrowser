# STATE.md — TPbrowser (`D:\Projects_Hermes\TPbrowser`)

**Checkpoint:** `v0.5.8-zen-split-view-url-fix-native-copy` (2026-11-13)
**Dự án:** `TPbrowser` — Trình duyệt cá nhân Rust (`tao` + `wry` / WebView2), giao diện **AURELIA**, **Kế thừa Zen Browser: Chia Khung Tự Do (Popover Menu chọn Tab & 3 Bố cục 2 Cột/2 Hàng/Xoắn Ốc)**, **Sửa kẹt link SPA & Nút Copy Win32 Native**, **Khử trùng Bộ 3 nút khi chia khung (1 Cửa sổ = 1 Bộ nút)**, **Tab 7 Cài Đặt About & Cập Nhật**, **Màn hình TMĐT (`tp://workspace`)**, Worker Tabs ngầm & uBlock Origin Lite.

---

## 1. Phiên Hiện Tại

- **Kế Thừa Zen Browser: Chia Khung Tự Do & Menu Popover Ghép Đôi Tab (`src/titlebar.html`, `src/layout.rs`, `src/main.rs`)**:
  - **Menu Popover Zen Thả Xuống**: Bấm nút chia khung trên Titlebar (hoặc `Alt+M`) thả ra Popover Menu thanh lịch, cho phép chọn bố cục (`◫ 2 Cột`, `⊟ 2 Hàng`, `🌀 Xoắn Ốc`, hoặc `◻ Hủy Chia`) và danh sách tick chọn đích danh 2 tab muốn ghép đôi làm việc song song thay vì ép chia tự động toàn bộ tab.
  - **Nhân Bố Cục Chia Hàng Mới & Phân Cặp**: Bổ sung `ContentViewMode::RowsBsp` + `compute_rows_in_area` trong `src/layout.rs`; lưu trữ `split_paired_tab_id` trong Rust core để chỉ chia đôi 2 tab được chọn.
- **Sửa Triệt Để Lỗi Kẹt Link SPA & Nút Copy Clipboard (`src/content_agent.js`, `src/titlebar.html`, `src/main.rs`)**:
  - **Hook SPA Navigation**: Bắt các sự kiện `history.pushState`, `history.replaceState`, `popstate` và `hashchange` trong `src/content_agent.js`; tự động gửi URL thật về Rust qua lệnh `tab_meta` kèm `url`.
  - **Win32 Native Clipboard API**: Viết hàm Rust `copy_text_to_clipboard` gọi Windows API `OpenClipboard`, `EmptyClipboard`, `GlobalAlloc`, `SetClipboardData(CF_UNICODETEXT)`; nút Copy trên Titlebar gọi IPC `copy_to_clipboard` bảo đảm 100% chép thành công.
- **Khử Trùng Bộ 3 Nút AI / FB / Zalo Khi Chia Khung (`src/main.rs:2628`)**:
  - Khi chia khung (`is_split`), Rust gửi cờ `show_orbs_in_tab`: chỉ duy nhất Tab Active được hiển thị cụm nút ở góc cửa sổ. Các tab phụ tự ẩn triệt để, giữ đúng quy tắc 1 cửa sổ = 1 bộ 3 nút.
- **Bổ Sung Tab 7 Thông Tin & Cập Nhật (About) Trong Cài Đặt Tập Trung (`src/settings_modal.html`, `src/main.rs`)**: Hoàn tất trước đó.

---

## 2. Bằng Chứng Kiểm Thử

- **Rust Unit Tests (`cargo test`)**: `17 passed; 0 failed` (bao gồm `test_rows_split_inside_content_area`, `test_spiral_split_inside_content_area`, `test_settings_modal_html_has_font_marker_and_seven_tabs`).
- **Build Release (`cargo build --release`)**: `0 errors, 0 warnings`, đã deploy trực tiếp ra `TPbrowser.exe` (2.2MB).
- **Kiểm tra Checkpoint Trio (`pytest -q tests/test_checkpoint_integrity.py`)**: `4 passed`, CodeGraph AST synced, `STATE.md` (< 100 dòng), `MAP.html` (`v0.5.8-2026-11-13-zen-split-and-url-fix`), `PROJECT_STATE.json`.

---

## 3. Kiến Trúc Cốt Lõi

- `src/main.rs`: Nhân Win32 & Multi-WebView2 (`settings_shell`, `tab_shell`, `tabs`, `ai_shell`, `titlebar_shell`, `dock_resizer_shell`), xử lý IPC `set_split_mode` / `unsplit` / `copy_to_clipboard` / `check_app_update`, MCP Server HTTP :17777 + stdio.
- `src/layout.rs`: Tính toán hình học 3 vùng (`TabOrientation`, `AiDisplayMode`, `ContentViewMode`: SingleTab, ColumnsBsp, RowsBsp, SpiralBsp).
- `src/titlebar.html`: Thanh trên hợp nhất, Menu Zen Split Popover, nút Copy Native, Omni URL.
- `src/settings_modal.html`: Bảng Cài Đặt Tập Trung 7 Tab chuẩn Zen Browser (About & Cập nhật tự động).
- `src/workspace.html` & `src/ecom_agents.js`: Landing Page TMĐT Shopee Audit / Live / Đánh giá 1-3★ & TikTok Seller.
- `src/content_agent.js`: Hook SPA URL, thẻ nhắc việc nổi, chuông báo Web Audio API, khiên chặn quảng cáo 3 lớp.

---

## 4. Việc Kế Tiếp

- [x] Kế thừa Zen Browser: Chia Khung Tự Do (Popover Chọn Tab & 3 Bố Cục), Sửa Link SPA, Copy Native & Khử Trùng Bộ 3 Nút.
- [ ] Bổ sung thêm các bộ Skill mẫu theo yêu cầu công việc hằng ngày của anh Phú.
