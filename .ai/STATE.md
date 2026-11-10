# STATE.md — TPbrowser (`D:\Projects_Hermes\TPbrowser`)

**Checkpoint:** `v0.2.0-aurelia-ui` (2026-11-10)
**Dự án:** `TPbrowser` — Trình duyệt cá nhân siêu nhẹ viết bằng Rust (`tao` + `wry` / WebView2 ~748 KB), giao diện **AURELIA (Tối Đen Vàng Gold & Sáng Ngọc Trắng)**, Tab Dọc/Ngang, AI Cowork tự học Skill và chặn quảng cáo `uBlock Origin Lite` MV3 + `ABPVN`.

---

## 1. Phiên Hiện Tại

- **Giao diện AURELIA (Tối & Sáng) + Tinh tế tối giản (`src/newtab.html`, `src/tab_shell.html`, `src/ai_panel.html`)**:
  - Dựng nguyên bản 2 thiết kế **AURELIA — THE ART OF MINIMALISM** (Đen Đá Cẩm Thạch & Vàng Gold / Ngọc Trắng Carrara & Vàng Champagne) với dải lụa 3D SVG, trục kim cương chia đôi và 3 bong bóng nổi Vàng Gold góc dưới phải (**Robot AI**, **Facebook Messenger**, **Zalo**).
  - Thanh Tab Dọc/Ngang và Ngăn Trợ lý AI đồng bộ tông màu Đen + Vàng Gold (`obsidian-gold`) và Sáng (`warm-paper`, `airy-cloud`, `zen-sand`), áp dụng **Hover Reveal** (ẩn gọn nút phụ `← → ↻ ⌂ ★` và nút đóng tab `×` cho tới khi rê chuột vào).
  - Bản sao lưu trước khi đổi giao diện: `src/*.bak_before_calm_ui`.
- **Chặn quảng cáo toàn diện (`extensions/uBOLite.chromium` + `src/content_agent.js` + `src/main.rs`)**:
  - Cài đặt Extension **`uBlock Origin Lite` (`v2026.1103.1743`, Manifest V3 `declarativeNetRequest`)** bật sẵn bộ lọc tiếng Việt **`vie-1` (`ABPVN List`)** và `annoyances-overlays` qua `with_browser_extensions_enabled(true)`.
  - Kết hợp Bộ lọc 3 lớp nội tại: khóa `window.open` popunder cá cược, ép CSS `0px` chống Anti-Adblock trên `animevietsub.best`, và tự tua 16x + bấm Skip quảng cáo YouTube sau `< 0.25s`.
- **Hệ thống Tự học Skill kép & Chuẩn hóa API Key (`src/ai_panel.html`, `src/content_agent.js`, `skills/skills.json`)**:
  - Hỗ trợ cả **Dạy bằng lời trong chat** (`learn_skill`) lẫn nút **`Làm mẫu` (Record)** bắt thao tác click/input thực tế trên trang web, lọc theo tên miền (`Domain-Scoped Skills`).
  - Tự động nối `/v1` trong `normalizeOpenAiBase()` và thêm nút `⚡ Test Key` kiểm tra trực tiếp cổng `127.0.0.1:8045/v1` (`gemini-3.8-flash`).
- **Tách công cụ cũ**: Đổi tên `zen-rust-bsp` thành `D:\Projects_Hermes\shopee-scraper` để giữ riêng tính năng `F9` cào đơn Shopee.

---

## 2. Bằng Chứng Kiểm Thử

- **Rust Unit Tests (`cargo test`)**: `3 passed; 0 failed` (`test_vertical_left_with_right_ai_dock`, `test_horizontal_top_with_minimized_bubble`, `test_spiral_split_inside_content_area`).
- **Build Release (`cargo build --release`)**: `0 errors, 0 warnings`, sinh thực thi `TPbrowser.exe` (`748 KB`).
- **Kiểm tra Checkpoint Trio (`pytest -q tests/test_checkpoint_integrity.py`)**: Xác nhận đầy đủ `AGENTS.md`, `CLAUDE.md`, `STATE.md`, `MAP.html`, `PROJECT_STATE.json` và sơ đồ Archify.
- **Kiểm chứng thị giác (`vision_analyze`)**: Đã kiểm tra ảnh chụp `aurelia_dark.png`, `aurelia_light.png`, `ai_panel_obsidian_gold_v2.png` và `tpbrowser_obsidian_gold_demo.png`.

---

## 3. Kiến Trúc Cốt Lõi

- `src/main.rs`: Nhân Win32 & WebView2 (`tab_shell`, `tabs`, `ai_shell`), nạp Extension `uBOLite`, lọc popunder, IPC Router và lưu `config.json` / `session.json` / `skills/skills.json`.
- `src/layout.rs`: Tính toán hình học 3 vùng (`TabOrientation`, `AiDisplayMode`, `ContentViewMode`).
- `src/newtab.html`: Không gian bắt đầu AURELIA (Giao diện Tối & Sáng, 3 bong bóng Vàng Gold AI/Messenger/Zalo, Collections Bookmarks).
- `src/tab_shell.html` & `src/ai_panel.html`: Thanh quản lý Tab Dọc/Ngang Hover-Reveal và Ngăn Trợ lý AI Cowork.
- `src/content_agent.js`: Bong bóng AI nổi trên trang web, bộ ghi nhận thao tác Skill (`__tpRecordingSkill`) và Bộ lọc Quảng cáo 3 Lớp.

---

## 4. Việc Kế Tiếp

- [ ] Thêm tùy chọn phím tắt mở nhanh ngăn `COLLECTIONS` (Bookmarks) từ mọi trang web.
- [ ] Bổ sung thêm các bộ Skill mẫu theo yêu cầu công việc hằng ngày của anh Phú.
