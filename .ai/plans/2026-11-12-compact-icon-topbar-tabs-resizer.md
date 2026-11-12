# Plan: Tinh Gọn Giao Diện Icon-First, Bỏ Thanh Trên Chat, URL Ngắn + Tab Favicon & Kéo Giãn Tab Dọc

**Ngày:** 2026-11-12  
**Trạng thái:** Đã hoàn tất triển khai, build & kiểm chứng sống  
**Mockup đối chiếu:** `.ai/sketches/2026-11-12-compact-icon-topbar-tabs-resizer.html`

---

## 1. Khảo sát hiện trạng thực tế trong code

1. **Thanh trên của cửa sổ Chat (`src/chat_header.html:137-161`, `src/layout.rs:206`, `src/main.rs:1589-1688`)**:
   - Đang tạo thêm 1 WebView `chat_header_shell` (`40px`) hiển thị `AI Cowork | Messenger | Zalo` + `📋 Bắt việc | ⇔ Ngăn phải | ↗ Tab lớn | —`, trùng hoàn toàn với **3 nút tròn nổi** (`AI`, `FB`, `Zalo`).
2. **Nút chữ dài trên Thanh trên (`src/titlebar.html:139-355`) & AI Panel (`src/ai_panel.html:569-683`)**:
   - Thanh trên: `+ Tab`, `1 Tab`, `Cài đặt`, ô URL `max-width: 820px` dài hết thanh; Logo TP (`src/titlebar.html:288`) đang gọi `go_home` thay vì mở Cài Đặt.
   - AI Panel: Nhiều nút chữ (`Làm mẫu`, `Skills (0)`, `Cài đặt`, `Thu gọn`, 4 chip chữ dài).
3. **Chế độ Tab Ngang (`src/layout.rs:153`, `src/tab_shell.html:584`)**:
   - Khi chọn Tab Ngang, `tab_shell` chiếm thêm `36px` riêng bên dưới thanh trên (`TOP_TABBAR_HEIGHT = 36`), còn thanh trên vẫn để ô URL chiếm hết chiều ngang.
4. **Logo website (Favicon) trên Tab (`src/tab_shell.html:739-755`, `src/main.rs:693`)**:
   - `BrowserTab` chưa có trường `favicon_url`; `tab_shell.html` vẽ cứng `<div class="tab-dot"></div>` cho mọi tab.
5. **Kéo giãn chiều rộng Tab Dọc (`src/main.rs:749`, `src/layout.rs:133`, `src/dock_resizer.html`)**:
   - `sidebar_width` mới chỉ đổi qua 3 nút cố định (`180 / 228 / 280`) trong Settings, chưa có thanh kéo chuột (`ew-resize`) như `dock_resizer_shell` ở ngăn AI bên phải.

---

## 2. Bản đồ ánh xạ dữ liệu (Data Mapping)

| Hạng mục | Hiện trạng (`file:line`) | Thay đổi dự kiến |
|---|---|---|
| **Logo TP ở góc trên trái** | `src/titlebar.html:288` gọi `go_home` | Đổi thành `open_settings_modal` (`look_feel`), bỏ nút chữ `Cài đặt` thừa bên phải |
| **Thanh trên Chat nổi** | `chat_header_shell` (`40px`, `src/main.rs:1589`, `src/chat_header.html`) | Bỏ dải tab `AI Cowork \| Messenger \| Zalo` trùng với 3 nút tròn; dời nút `⇔ Ngăn phải` (icon) vào `ai_panel.html` và tinh gọn theo lựa chọn bên dưới |
| **Ưu tiên Biểu tượng (Icon-first)** | `src/titlebar.html`, `src/tab_shell.html`, `src/ai_panel.html` | Đổi nút chữ (`+ Tab`, `1 Tab`, `Cài đặt`, `Làm mẫu`, `Thu gọn`, chip gợi ý) sang **SVG icon + tooltip** |
| **URL ngắn + Tab Ngang trên 1 thanh** | `src/titlebar.html:140` (`max-width:820px`), `src/layout.rs:153` | Thu ngắn ô URL (`clamp(200px, 26vw, 340px)`); chế độ Tab Ngang hiện danh sách Tab (Favicon + tên gọn + `+`) ngay cạnh URL trên thanh trên (ẩn khi bật Tab Dọc) |
| **Favicon thật của Tab** | `BrowserTab` (`src/main.rs:693`), `src/tab_shell.html:739` | Bổ sung `favicon_url: String` vào `BrowserTab`; trích xuất `<link rel="icon">` / `/favicon.ico` từ trang thực (và App Icon TP cho `tp://newtab`, `tp://history`) |
| **Kéo giãn Tab Dọc tự do** | `sidebar_width` (`src/main.rs:749`), `src/layout.rs:133` | Bổ sung `sidebar_resizer_shell` `5px` bên mép phải Sidebar (`StartSidebarResize` / `SetSidebarWidth` / `ResetSidebarWidth`, kéo mượt `150..440px`, nhấp đúp về `228px`, lưu `config.json`) |

---

## 3. Đáp án đã chốt qua `clarify` (1a 2a 3a 4a)

| # | Câu hỏi | Đáp án đã chốt |
|---|---|---|
| 1 | Thanh trên cửa sổ Chat nổi (`AI / Messenger / Zalo`) | **(1a)** Bỏ hẳn thanh trên (`0px`) khi mở **AI Cowork** (chuyển nút `Ngăn phải ⇔` vào thanh tiêu đề AI); **Messenger / Zalo** dùng thanh mini icon `28px` không chữ cho 4 nút tiện ích (`Bắt việc`, `Ngăn phải`, `Tab lớn`, `Thu gọn`) |
| 2 | URL ngắn + Vị trí Tab Ngang vs Tab Dọc | **(2a)** Khi bật **Tab Ngang**, đặt Tab (`Favicon + Tên gọn + +`) ngay cạnh thanh URL ngắn trên cùng 1 hàng `40px` (ẩn hẳn thanh tab phụ dưới); khi bật **Tab Dọc**, thanh trên không hiện Tab |
| 3 | Mức độ ưu tiên Biểu tượng (Icon-first) | **(3a)** Đổi toàn bộ nút công cụ & tiêu đề sang **SVG Icon + tooltip**; giữ chữ ngắn ở 2 tab `Chat` / `Kế hoạch` và nút `GỬI` |
| 4 | Hiển thị Tab kèm Favicon website | **(4a)** Hiện **Favicon website + Tên trang gọn** (thu gọn sidebar chỉ còn Favicon) |
