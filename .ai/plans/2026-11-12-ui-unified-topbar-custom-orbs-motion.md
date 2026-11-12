# Plan: Nâng Cấp Giao Diện Hợp Nhất, Bong Bóng Tùy Biến, Motion & Chuẩn Trình Duyệt

**Ngày:** 2026-11-12  
**Trạng thái:** [x] Đã hoàn tất triển khai & kiểm chứng sống (2026-11-12)  
**Mockup trực quan:** `.ai/sketches/2026-11-12-ui-top-bar-floating-orbs-mockup.html`

---

## 1. Các lỗi ngầm phát hiện trên code hiện tại

| Vấn đề | Nguyên nhân gốc (`file:line`) | Cách xử lý triệt để |
|---|---|---|
| Thanh trên cùng bị ẩn mặc định, ở Tab Ngang phải gánh 2-3 thanh chồng | `compact_titlebar: true` (`src/main.rs:852`) làm `titlebar_shell` ẩn; ở Tab Ngang lại có cả `.h-row-tabs` + `.h-row-nav` (`src/tab_shell.html:449-519`), còn Tab Dọc không có thanh địa chỉ trên cùng | Cho `titlebar_shell` **luôn hiện** (cao `40px`), đặt đủ nút điều hướng, thanh địa chỉ, nút cài đặt `⚙`, chia khung, `+ Tab`, và `— □ ×` lên đó; bỏ hẳn `.h-row-nav` thừa (`TOP_TABBAR_HEIGHT` giảm từ `82px` xuống `40px`) |
| Không thấy badge đỏ đếm số tin nhắn mới & AI xong không nhấp nháy | 1) `src/content_agent.js:248` ẩn `#__tp_web_orbs` trên `tp://newtab`; `src/newtab.html:576-589` dùng cụm `.floating-chat-heads` riêng, không đồng bộ badge/nháy.<br>2) `CHANNEL_UNREAD_WATCHER_JS` (`src/main.rs:245-283`) thiếu selector Zalo Web/Messenger mới.<br>3) `SyncBubbleStatus` (`src/main.rs:2781`) tăng `unread_ai` nhưng không phát sự kiện pulse/âm thanh | Hợp nhất **1 bộ bong bóng góc phải duy nhất** qua `content_agent.js` chạy trên MỌI trang (gỡ `.floating-chat-heads` trùng trong `newtab.html`); mở rộng watcher Zalo/FB; thêm hiệu ứng vòng sáng nhấp nháy + âm thanh Web Audio API dịu nhẹ khi AI hoàn thành hoặc có tin mới |
| Cụm 3 nút góc phải cố định cứng AI / FB / Zalo | Render tĩnh trong `src/content_agent.js:377-404` và `src/newtab.html:842-875` | Thêm mảng cấu hình `floating_orbs` vào `config.json` + giao diện quản lý trong `src/settings_modal.html` (bật/tắt từng kênh & `+ Thêm App tùy chọn`) |
| Thiếu các tính năng trình duyệt mặc định | `src/main.rs` chưa lưu `closed_tabs`, chưa có Find-in-page (`Ctrl+F`), Zoom (`Ctrl+/-/0`), Lịch sử (`history.json`) | Bổ sung đầy đủ trong `src/main.rs` + `src/content_agent.js` + `src/titlebar.html` |

---

## 2. Bản đồ ánh xạ dữ liệu (Data Mapping)

| Hạng mục mới | Nguồn / Biến hiện có | File đích | Ghi chú |
|---|---|---|---|
| Thanh trên hợp nhất luôn hiện | `titlebar_pinned` (`src/main.rs:513, 1320`), `NavBack/NavForward/ReloadActive/GoHome/NavigateActive/BookmarkActive` (`src/main.rs:2542-2597`) | `src/titlebar.html`, `src/layout.rs`, `src/tab_shell.html`, `src/main.rs` | Thêm input URL + ổ khóa + nút `☆`, `Copy URL`, `Ctrl+F`, `+ Tab`, `Khung BSP`, `⚙ Cài đặt`, `— □ ×` |
| Cấu hình bong bóng góc phải | `config.json` (`src/main.rs:1708-1813`) + `settings_modal.html` (`pane_side_panels`) | `src/main.rs`, `src/settings_modal.html`, `src/content_agent.js` | Thêm `always_show_floating_orbs: bool` + `floating_orbs_json` (id, label, icon_kind, color, target_type `channel\|tab\|popup`, url, enabled) |
| Thông báo đỏ + nhấp nháy + chuông | `unread_ai`, `unread_fb`, `unread_zalo` (`src/main.rs:518-520`), `playReminderChime` (`src/content_agent.js:775`) | `src/main.rs`, `src/content_agent.js`, `src/newtab.html` | Thêm `ai_finished_flash: bool`, reset khi mở panel AI; chuông Web Audio API nhẹ khi `unread` tăng hoặc AI xong |
| Thư viện Motion offline | Theo chuẩn skill `motion-ai-kit` (`motion@14.9.1` UMD `motion.js`) | `assets/vendor/motion.js` + `include_str!` | Hardware-accelerated spring (`scale`, `y`, `opacity`) cho orbs, badge, modal, find bar |
| Tính năng trình duyệt mặc định | `self.tabs` (`src/main.rs:508`), `WebView::zoom`, `self.exe_dir` | `src/main.rs`, `src/content_agent.js`, `src/titlebar.html` | `closed_tabs: Vec<String>` (`Ctrl+Shift+T`), `zoom_level: f64` (`Ctrl+/-/0`), `history.json` (`Ctrl+H`), Find bar (`Ctrl+F`) |

---

## 3. Lựa chọn anh đã chốt qua `clarify`

| # | Câu hỏi | Đáp án |
|---|---|---|
| 1 | Bố cục thanh trên cùng | *(Bỏ qua — đề xuất dùng Phương án 1: 1 thanh trên 40px luôn hiện + hàng Tab mảnh khi ở Tab Ngang / sidebar khi ở Tab Dọc)* |
| 2 | Cách mở của bong bóng tự thêm | *(Bỏ qua — đề xuất cho chọn từng nút trong Cài Đặt: Mở Tab mới hoặc Cửa sổ nổi)* |
| 3 | Báo tin nhắn mới & AI xong việc | **Đã chốt:** Badge đỏ có số bên trong + vòng tròn AI nhấp nháy sáng khi xong + kèm âm thanh chuông nhẹ |
| 4 | Nhóm tính năng trình duyệt mặc định | **Đã chốt:** Đầy đủ 4 món — Tìm chữ (`Ctrl+F`), Mở lại tab vừa đóng (`Ctrl+Shift+T`), Zoom (`Ctrl+/-/0`), Nút Copy URL & Lịch sử duyệt nhanh |
