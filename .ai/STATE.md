# STATE.md — TPbrowser (`D:\Projects_Hermes\TPbrowser`)

**Checkpoint:** `v0.9.1-2026-11-16-command-bar-mru-done` (2026-11-16)
**Dự án:** `TPbrowser` — Trình duyệt cá nhân Rust (`tao` + `wry` / WebView2), giao diện **AURELIA**, **Đổi Logo & Toàn Bộ Icon Giao Diện Sáng Sang Tối Sắc Nét & Nổi Bật Tối Đa**, **Cơ chế Startup Boost chuẩn Microsoft Edge (Khay Hệ Thống Native Win32, Single-Instance Mutex & Local /wake Endpoint, Chạy ngầm mở tức thì < 0.05s, Tự khởi động cùng Windows)**, Thanh Bookmark Tab Ngang (28px viền vàng, Ctrl+Shift+B, Titlebar động 40px/68px), Tối ưu 70% RAM & Lazy Sleeping Tabs.

---

## 1. Phiên Hiện Tại

- Hoàn tất chuỗi Command Bar MVP theo 3 card: native Right Alt+K state/signal, titlebar overlay với tìm tab/workspace bằng stable ID, và các lệnh Back/Forward/Reload/New Tab/Close Tab qua IPC hiện hữu.
- Test focused Right Alt+K, `cargo check`, `git diff --check` và `node --check` đều pass; live delivery khi child WebView giữ focus và overlay vượt khung titlebar 40/68px vẫn chưa xác minh.
- MRU `t_ab0a18c5` đã hoàn tất: Ctrl+Tab chạy theo runtime MRU bằng stable `tab_id`, lọc workspace hiện tại, resolve native index chỉ lúc focus; Ctrl+Shift+Tab giữ đường cũ.
- Triển khai và vừa harden Zen Glance (Xem Nhanh Nổi): Alt+Click bắt ở `pointerdown` capture để không bị WebView nuốt, 3 webview con `glance_backdrop_shell`/`glance_header_shell`/`glance_body_shell`, Escape/click backdrop đóng và nút mở Tab lớn.
- Sửa UI theo live screenshot: bookmark grid New Tab responsive khi có AI dock; tab web giữ favicon thật nếu có và dùng globe SVG fallback thay vì chấm xám khi favicon lỗi.
- Đã hoàn tất chuỗi Zen P0 được phép triển khai: Workspaces (state, persistence, picker, filtering, create), Essentials/Pinned Tabs (pin/unpin + persistence), và Container Tabs (identity/persistence + mở tab theo container).
- Đã harden đường đi tab sau filtering workspace: native index/visible index tách riêng cho focus-close, next/previous, split pair và layout pane; tab workspace khác không còn chiếm pane hiện tại.
- Container ID hợp lệ ánh xạ ổn định sang profile WebView riêng; `default` dùng profile mặc định; session cũ có fallback an toàn.
- `tp://workspace` được giữ nguyên. Zen Glance đã triển khai xong (xem trên).
- Chuỗi Zen parity vẫn tạm dừng các phase khác theo yêu cầu người dùng; không tự tạo hoặc dispatch task mới ngoài Glance.
- Thay đổi code vẫn nằm trong working tree, chưa commit/push/reset/stash; không tự ý dọn các thay đổi UI trước đó.

---

## 2. Bằng Chứng Kiểm Thử

- **Command Bar chain**: focused Right Alt+K test `1 passed`; `cargo check` pass; `node --check` titlebar script pass; `git diff --check` pass. Chưa live smoke verify child-WebView focus/overlay extent.
- **MRU**: full Rust suite `22 passed; 0 failed`; focused Right Alt test `1 passed`; `cargo check` và `git diff --check` pass. Không thay đổi persistence.
- **Checkpoint Integrity**: `codegraph.cmd sync . && python scripts/generate_checkpoint.py && pytest -q tests/test_checkpoint_integrity.py` → `4 passed; 0 failed`.
- **Rust Unit Tests (`cargo test`)**: `21 passed; 0 failed` (thêm kiểm thử hình học, Glance action, pointerdown, favicon fallback và grid responsive).
- **JavaScript syntax (`node --check src/content_agent.js`)**: pass.
- **`cargo check`**: sạch, không lỗi/cảnh báo mới sau khi wiring Zen Glance.
- **`cargo build --release`**: biên dịch thành công bản mới sau các fix Glance/UI; đã copy sang `TPbrowser.exe` ở gốc repo và xác minh process chạy từ đường dẫn này.
- **Diff hygiene (`git diff --check`)**: pass cho phạm vi đã kiểm tra.
- **Checkpoint Integrity (`uv run --with pytest python -m pytest -q tests/test_checkpoint_integrity.py`)**: `4 passed; 0 failed`.
- **Review Kanban**: card container mở tab đã được approve và đóng.
- Đã live smoke verify qua MCP: `/health` OK, `tools/list` OK, mở `tp://workspace` đọc được KPI/đơn bất thường/đánh giá/worker tab với dữ liệu thật, sau đó đóng đúng tab test. Chưa claim live pass cho toàn bộ Settings/Glance/TikTok CAPTCHA flow.

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

- [x] Zen MRU: Ctrl+Tab theo thứ tự dùng gần đây, stable `tab_id`, chỉ workspace hiện tại (`t_ab0a18c5`).
- [ ] Live verify Command Bar: Right Alt+K khi child WebView focus và overlay không bị cắt bởi titlebar WebView.
- [x] Workspace state/persistence, switcher, filtering và create workspace.
- [x] Pinned tab state/persistence.
- [x] Container identity/persistence và mở tab theo container.
- [x] Glance preview — Alt+Click mở thẻ nổi xem nhanh, ESC/click-outside đóng, nút mở ra Tab lớn; pointerdown capture đã harden.
- [ ] Chưa test manual UI Glance thật trên WebView2 (chỉ verify qua cargo test + cargo check + release build).
- [ ] Các phase Zen khác — chưa lập/dispatch card khi đang pause.
