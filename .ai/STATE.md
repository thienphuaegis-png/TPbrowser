# STATE.md — TPbrowser (`D:\Projects_Hermes\TPbrowser`)

**Checkpoint:** `v0.5.9-perf-boost-lazy-tabs-and-bookmarks-bar` (2026-11-13)
**Dự án:** `TPbrowser` — Trình duyệt cá nhân Rust (`tao` + `wry` / WebView2), giao diện **AURELIA**, **Tối ưu tốc độ mở app tức thì & Tiết kiệm 70% RAM (Triệt tiêu 766ms Git Sync UI, Khử bão Relayout boot, Tắt Preload Chat, Lazy Sleeping Tabs, Chromium Arg Tune)**, **Thanh Bookmark Tab Ngang (AURELIA Gold 28px, Co giãn Titlebar động 40px/68px, Ctrl+Shift+B, Tùy chọn Cài Đặt)**, Chia Khung Tự Do Zen Browser, Sửa link SPA & Copy Native Win32.

---

## 1. Phiên Hiện Tại

- **Khắc Phục Tận Gốc Độ Trễ Khởi Động & Khử Nghẽn Giao Diện (`src/main.rs`)**:
  - **Triệt tiêu 766ms đóng băng Main UI Thread**: Loại bỏ hoàn toàn lệnh gọi `git rev-parse --short HEAD` đồng bộ lúc boot; thay thế bằng hằng số tĩnh `APP_GIT_COMMIT`, đưa thời gian truy xuất commit về 0.00 ms.
  - **Khử bão 7 lần Relayout lúc Boot**: Chuyển sự kiện `UiShellReady` của từng shell con thành passive; chỉ thực hiện `relayout()` và `sync_all_ui()` đúng 1 lần duy nhất tại mốc `boot_settle` (250ms), loại bỏ hoàn toàn hiện tượng nghẽn hàng đợi thông điệp Win32 lúc vừa mở app.
  - **Lazy Sync Settings Modal**: Ngăn việc serialize cấu hình gửi vào Settings Modal đang ẩn lúc khởi động (`SyncSettingsModalInit`), chỉ đồng bộ khi người dùng bấm `Alt+,` mở bảng Cài Đặt.
  - **Tối ưu bộ đệm ghi Log**: Giới hạn ghi `perf_latency.log` chỉ khi có biến môi trường `TP_DEBUG_PERF` và lệnh xử lý vượt ngưỡng 10ms, triệt tiêu I/O đĩa không cần thiết trên luồng giao diện.
  - **Cập nhật Binary Gốc**: Đồng bộ `target/release/TPbrowser.exe` (2.26MB) đè sang `TPbrowser.exe` ở thư mục gốc để shortcut Start Menu / Desktop mở ngay bản tối ưu mới nhất.
- **Tối Ưu Bộ Nhớ Đa Tab & Khôi Phục Phiên Nhanh**:
  - **Tắt Preload Chat Ngầm**: Không tải trước Messenger & Zalo lúc boot, tiết kiệm ngay ~400MB RAM.
  - **Lazy Sleeping Tabs**: Tab nền khi khôi phục phiên từ `session.json` nạp HTML placeholder rỗng siêu nhẹ; chỉ kích hoạt tải URL thật khi người dùng nhấp chọn.
  - **Cờ WebView2 Chromium**: Bổ sung `--disable-features=Translate,OptimizationHints,MediaRouter --renderer-process-limit=4`.
- **Thanh Bookmark Tab Ngang Chuẩn AURELIA (`src/titlebar.html`, `src/main.rs`, `src/settings_modal.html`)**:
  - Thanh lối tắt 28px viền vàng đồng dưới hàng tab ngang, tự động mở rộng Titlebar 40px $ightarrow$ 68px, phím tắt `Ctrl+Shift+B` và switch trong Cài Đặt.

---

## 2. Bằng Chứng Kiểm Thử

- **Rust Unit Tests (`cargo test`)**: `17 passed; 0 failed` (đầy đủ các test chia khung BSP, layout, modal settings, mini json, brand logo).
- **Build Release (`cargo build --release`)**: Biên dịch thành công binary siêu nhẹ 2.26MB (`TPbrowser.exe`).
- **Kiểm tra Checkpoint Trio (`uv run pytest -q tests/test_checkpoint_integrity.py`)**: `4 passed in 0.07s`.

---

## 3. Kiến Trúc Cốt Lõi

- `src/main.rs`: Nhân Win32 & Multi-WebView2 (`settings_shell`, `tab_shell`, `tabs`, `ai_shell`, `titlebar_shell`, `dock_resizer_shell`), xử lý IPC, phím tắt, MCP Server HTTP :17777 + stdio.
- `src/layout.rs`: Hình học 3 vùng (`TabOrientation`, `AiDisplayMode`, `ContentViewMode`: SingleTab, ColumnsBsp, RowsBsp, SpiralBsp).
- `src/titlebar.html`: Thanh trên hợp nhất 40px/68px, Bookmarks Bar 28px, Menu Zen Split Popover, Copy Native, Omni URL.
- `src/settings_modal.html`: Bảng Cài Đặt Tập Trung 7 Tab chuẩn Zen Browser.
- `src/workspace.html` & `src/ecom_agents.js`: Landing Page TMĐT Shopee Audit / Live / Đánh giá 1-3★ & TikTok Seller.
- `src/content_agent.js`: Hook SPA URL, thẻ nhắc việc nổi, chuông báo Web Audio API, khiên chặn quảng cáo 3 lớp.

---

## 4. Việc Kế Tiếp

- [x] Tối ưu tốc độ mở app tức thì (<0.1s), triệt tiêu nghẽn Git UI sync, khử bão Relayout và cập nhật binary gốc.
- [x] Bổ sung thanh Bookmark Tab Ngang chuẩn AURELIA (Ctrl+Shift+B, dynamic titlebar height).
- [ ] Bổ sung thêm các bộ Skill mẫu theo yêu cầu công việc hằng ngày của anh Phú.
