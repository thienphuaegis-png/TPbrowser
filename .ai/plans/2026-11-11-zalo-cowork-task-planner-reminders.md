# PLAN: PHÁT TRIỂN AI COWORK (CÙNG LÀM VIỆC) & NHẮC VIỆC TRỰC TIẾP TỪ ZALO WEB

- **Ngày lập**: 2026-11-11
- **Trạng thái**: `[x] FIXED` — ĐÃ HOÀN TẤT CẢ 4 GIAI ĐOẠN & KIỂM CHỨNG THỊ GIÁC (`cargo test`: 7 passed, `cargo build --release`: 0 errors)

---

## 1. Chẩn đoán hiện trạng `TPbrowser` & 4 mảnh ghép cần bổ sung

1. **Cầu nối Zalo Web (`src/main.rs:676-734`, `2020-2026`)**: Đã có sẵn `zalo_shell` và `fb_shell` kèm đếm tin chưa đọc (`CHANNEL_UNREAD_WATCHER_JS`), nhưng AI chưa đọc được nội dung hội thoại bên trong `zalo_shell` và chưa có nút bắt việc nhanh khi bôi đen tin nhắn Zalo.
2. **Lưu trữ Kế hoạch & Công việc (`tasks/tasks.json`)**: Đã có `config.json`, `session.json`, `skills/skills.json`, nhưng chưa có `tasks/tasks.json` để lưu bền vững đầu việc, checklist các bước nhỏ, nguồn giao việc Zalo, thời hạn (`due_at`), giờ nhắc (`remind_at`) và lịch sử phiên Cowork.
3. **Chế độ Cùng Làm Việc (`src/ai_panel.html`, `src/content_agent.js`)**: AI hiện xử lý từng câu lệnh rời rạc (one-shot), chưa có chế độ đồng hành theo Checklist kế hoạch trên Bong bóng nổi (`Mini-Pill`) và chưa có bước khép vòng nháp báo cáo gửi lại Zalo.
4. **Hệ thống Nhắc việc Chủ động (`Reminder Engine`)**: Chưa có luồng canh giờ ngầm trong Rust để bung thẻ nhắc việc nổi trực tiếp trên mọi trang web, phát chuông nhẹ và nháy icon Taskbar Windows (`FlashWindowEx`) khi cửa sổ thu nhỏ.

---

## 2. Lộ trình 4 Giai đoạn triển khai

- **Phase 1 — Nhân Lưu Trữ `tasks/tasks.json` & Bộ Canh Giờ Nhắc Việc Rust (`src/main.rs`)**:
  - Quản lý đọc/ghi `tasks/tasks.json` (thêm `tasks/` vào `.gitignore`), giữ nguyên 0 crate phụ thuộc mới trong `Cargo.toml`.
  - Bổ sung Win32 `FlashWindowEx` + luồng kiểm tra lịch nhắc mỗi `15s` và các lệnh IPC quản lý Task/Reminder.
- **Phase 2 — Bắt Việc Trực Tiếp Từ Zalo Web & Messenger (`src/main.rs`, `src/chat_header.html`, `src/content_agent.js`)**:
  - Nút nổi `✨ Lập Plan Cowork` khi bôi đen tin nhắn trên Zalo/Messenger/Web (cả trong `zalo_shell` lẫn Tab chính).
  - Nút `📋 Bắt việc` trên thanh `src/chat_header.html` để quét nhanh đoạn hội thoại Zalo đang mở + hàm điền nháp tin nhắn báo cáo vào ô chat Zalo.
- **Phase 3 — Không Gian Kế Hoạch & Active Cowork (`src/ai_panel.html`)**:
  - Thanh chuyển 2 chế độ `💬 Cowork` | `📋 Kế hoạch (N)` chuẩn bảng màu Vàng Đồng AURELIA (Tối & Sáng).
  - AI tự bóc tách hạn chót tiếng Việt, chia nhỏ Checklist (`🤖 AI hỗ trợ` / `👤 Anh Phú chốt`), mở tài liệu song song (`SpiralBsp`) và đồng bộ tiến độ từng bước xuống `Mini-Pill`.
- **Phase 4 — Thẻ Nhắc Việc Trực Tiếp Đa Tầng & Trang Chủ (`src/content_agent.js`, `src/newtab.html`)**:
  - Thẻ nhắc việc nổi Vàng Đồng trên mọi Tab + âm báo nhẹ `Web Audio API` + nút `Làm ngay` / `+10p` / `+30p` / `Đã xong`.
  - Khay `Việc Hôm Nay` tối giản trên `tp://newtab` (tự ẩn khi không có việc) + đồng bộ Checkpoint Trio (`STATE.md`, `MAP.html`, `PROJECT_STATE.json`).
