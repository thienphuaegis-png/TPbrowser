# STATE.md — TPbrowser (`D:\Projects_Hermes\TPbrowser`)

**Checkpoint:** `v0.6.0-edge-style-startup-boost-and-system-tray` (2026-11-13)
**Dự án:** `TPbrowser` — Trình duyệt cá nhân Rust (`tao` + `wry` / WebView2), giao diện **AURELIA**, **Cơ chế Startup Boost chuẩn Microsoft Edge (Khay Hệ Thống Native Win32, Single-Instance Mutex & Local /wake Endpoint, Chạy ngầm mở tức thì < 0.05s, Tự khởi động cùng Windows)**, Thanh Bookmark Tab Ngang (28px viền vàng, Ctrl+Shift+B, Titlebar động 40px/68px), Tối ưu 70% RAM & Lazy Sleeping Tabs.

---

## 1. Phiên Hiện Tại

- **Cơ Chế Khởi Động Tức Thì Chuẩn Microsoft Edge (Startup Boost & Single-Instance)**:
  - **Single-Instance Mutex & Endpoint `/wake` (`src/main.rs`, `src/mcp_server.rs`)**: Sử dụng Win32 `CreateMutexA("TPbrowser_SingleInstance_Mutex")`. Khi bấm mở lại từ Desktop / Start Menu, tiến trình mới phát hiện app đã chạy trong RAM, gửi yêu cầu `POST http://127.0.0.1:17777/wake` rồi thoát sau 2ms; cửa sổ nền lập tức gọi `ShowWindow(SW_RESTORE)` và `SetForegroundWindow` hiển thị trong **< 0.05s**.
  - **Khay Hệ Thống Native Win32 (System Tray & Subclassing)**: Gọi trực tiếp `Shell_NotifyIconW` kết hợp `SetWindowSubclass` (`comctl32` + `shell32`). Click trái icon bật app; click phải mở Menu Win32 với hai tùy chọn `⚡ Mở TPbrowser` và `🚪 Thoát hoàn toàn`.
  - **Chế độ Chạy Ngầm Giữ Chỗ (`background_mode`)**: Khi bấm nút `[X]` hoặc `Alt+F4`, app tự lưu cấu hình & phiên tab rồi ẩn cửa sổ xuống khay hệ thống, nhân WebView2 duy trì ở trạng thái nghỉ với mức tiêu thụ RAM cực thấp (~120MB).
  - **Khởi động cùng Windows (`startup_boost`)**: Bổ sung hàm `sync_windows_autostart` tự ghi nhận đường dẫn `TPbrowser.exe --silent` vào Registry `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
- **Cập Nhật Giao Diện Cài Đặt (Alt+,) (`src/settings_modal.html`)**:
  - Thêm 2 nút gạt: *Tốc Độ Mở Siêu Tốc (Chạy Ngầm Khay Hệ Thống)* và *Tự Động Nạp Cùng Windows (Startup Boost)* tại Tab 2 (Tabs & Layout), hỗ trợ đồng bộ tức thì.
- **Biên Dịch & Cập Nhật Binary Gốc**:
  - Biên dịch Release tối ưu `target/release/TPbrowser.exe` (2.26MB) và copy đè sang `TPbrowser.exe` thư mục gốc.

---

## 2. Bằng Chứng Kiểm Thử

- **Rust Unit Tests (`cargo test`)**: `17 passed; 0 failed` (toàn bộ test layout, bsp, settings, mcp server, mini json).
- **Build Release (`cargo build --release`)**: `0 errors, 0 warnings`, biên dịch thành công file thực thi `TPbrowser.exe`.
- **Kiểm tra Checkpoint Trio (`uv run pytest -q tests/test_checkpoint_integrity.py`)**: `4 passed in 0.04s`.

---

## 3. Kiến Trúc Cốt Lõi

- `src/main.rs`: Nhân Win32 & Multi-WebView2, Native System Tray, Single-Instance Mutex, Background Resident Controller, MCP Server HTTP :17777 + stdio.
- `src/mcp_server.rs`: Server MCP nội bộ + endpoint `/wake` và `/health`.
- `src/layout.rs`: Tính toán hình học 3 vùng (TabOrientation, AiDisplayMode, ContentViewMode).
- `src/titlebar.html`: Thanh trên hợp nhất 40px/68px, Bookmarks Bar 28px, Zen Split Popover, Copy Native Win32.
- `src/settings_modal.html`: Bảng Cài Đặt Tập Trung 7 Tab (Startup Boost, Background mode, Themes, AI Key).
- `src/workspace.html` & `src/ecom_agents.js`: Landing Page TMĐT Shopee Audit / Live / Đánh giá 1-3★ & TikTok Seller.

---

## 4. Việc Kế Tiếp

- [x] Tối ưu tốc độ mở app tức thì chuẩn Edge: System Tray Native Win32, Single-Instance Mutex, Chạy ngầm mở tức thì <0.05s, Startup Boost cùng Windows.
- [x] Bổ sung thanh Bookmark Tab Ngang chuẩn AURELIA (Ctrl+Shift+B, dynamic titlebar height).
- [ ] Bổ sung thêm các bộ Skill mẫu theo yêu cầu công việc hằng ngày của anh Phú.
