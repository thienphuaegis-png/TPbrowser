# PLAN BRAINSTORMING: ĐỒNG BỘ MÀU VÀNG ĐỒNG & HỆ BO GÓC THỐNG NHẤT TOÀN GIAO DIỆN TPBROWSER

- **Ngày lập**: 2026-11-11
- **Trạng thái**: `[x] FIXED` — ĐÃ HOÀN TẤT & KIỂM CHỨNG THỊ GIÁC (`cargo test`: 4 passed, `cargo build --release`: 0 errors, `vision_analyze`: Light & Dark pass)
- **Áp dụng skill**: `brainstorming`, `redesign-existing-projects`, `ui-style-vocab`, `make-interfaces-feel-better`

---

## 1. Bảng Kiểm Toán Lỗi Hiện Tại (Design Audit — Code thật)

| Hạng mục lỗi | Hiện trạng thực tế (`file:line`) | Hậu quả thị giác | Hướng đồng bộ đề xuất |
| :--- | :--- | :--- | :--- |
| **1. Lệch hệ bo góc (lộn xộn `pill 999px` với chữ nhật `5–16px`)** | `src/newtab.html:170,220` (`999px`) đứng cạnh `:201` (`8px`) và `:319` (`11px` bọc nút `0px`); `src/tab_shell.html:167` (`999px`) vs `:116` (`7px`), `:140` (`9px`); `src/ai_panel.html:108,241` (`999px`) vs `:304` (`11px` bọc nút `8px`); `src/chat_header.html:63` (`999px`) vs `:115` (`6px`). | Thanh trên cùng và hộp chat bị "cái tròn vo, cái chữ nhật góc nhọn", thiếu đồng nhất thiết kế. | Quy về **1 ngôn ngữ hình khối thống nhất** (3 bậc bo đồng tâm `outer = inner + gap`), trừ 3 bóng chat nổi `50%` ở góc dưới phải. |
| **2. Màu vàng Champagne sáng bị chói / nhạt chữ** | `src/newtab.html:17-54`, `src/tab_shell.html:19-41`, `src/ai_panel.html:19-41`, `src/chat_header.html:13-31`, `src/content_agent.js:263,350` đang dùng `#FFF4C2 → #F1CF73 → #CFA037`. | Trên nền Ngọc Trắng và Đen Cẩm Thạch, nút Champagne bị chói sáng, mất nét trầm ấm của **Vàng Đồng**. | Quay lại bảng **Vàng Đồng Cổ Điển (Bronze Gold)**: Tối (`#E2BE68 → #B5872E`, chữ tối `#140F05`), Sáng (`#C79535 → #9A6B16`, chữ trắng `#FFFFFF`; chữ nhấn `#8A5E10`). |
| **3. Gradient nền & `transition: all` / thiếu cảm giác bấm (`scale(0.96)`)** | `src/newtab.html:183,205,223,426` dùng `transition: all`; `src/newtab.html:277` dùng text gradient trên clock; nút chưa có `:active { transform: scale(0.96); }` & `:focus-visible`. | Thiếu độ chắc chắn khi bấm nút; `transition: all` kém mượt hơn `cubic-bezier(0.22, 1, 0.36, 1)`. | Đưa về màu phẳng/chuyển sắc đồng nhẹ chuẩn mực, thay `transition: all` bằng thuộc tính cụ thể + `cubic-bezier(0.22, 1, 0.36, 1)` + `scale(0.96)` khi nhấn. |
| **4. Lệch ngôn ngữ icon (emoji lẫn SVG)** | `src/newtab.html:721-726` (`🌙`, `☀️`), `src/ai_panel.html:352,424` (`📚`, `📄`, `📑`), `src/tab_shell.html:525` (`◐`) vs `src/titlebar.html` (SVG mảnh 1.7px). | Emoji Windows có màu vàng/xanh lạc tông Vàng Đồng trên nền tối/sáng. | Có thể thay emoji trên các nút điều hướng chính bằng SVG nét mảnh `currentColor` đồng bộ với `titlebar.html` (hỏi anh chốt). |

---

## 2. Bản Đồ Ánh Xạ Token Thiết Kế (Design Token Mapping)

| Token / Nhóm | Giá trị đề xuất — Tối (`obsidian-gold`) | Giá trị đề xuất — Sáng (`carrara-light`) | Áp dụng đồng loạt tại |
| :--- | :--- | :--- | :--- |
| `--bg-base` / `--bg-side` / `--bg-card` | `#0B0C0E` / `#111318` / `#181B22` | `#F6F2EA` / `#EFE9DC` / `#FFFFFF` | `newtab`, `tab_shell`, `ai_panel`, `chat_header`, `titlebar`, `dock_resizer` |
| `--gold-accent` (chữ, icon tiêu đề) | `#E2BE68` | `#8A5E10` | Tiêu đề, giờ đồng hồ, viền nhấn |
| `--cta-bg` / `--cta-fg` (nút chính Vàng Đồng) | `linear-gradient(135deg, #E2BE68, #B5872E)` / `#140F05` | `linear-gradient(135deg, #C79535, #9A6B16)` / `#FFFFFF` | Nút `GIAO AI COWORK`, tab active, switch active, nút `GỬI`, `Đi tới` |
| `--border-col` / `--border-strong` | `rgba(212, 175, 55, 0.30)` / `rgba(226, 190, 104, 0.72)` | `rgba(154, 107, 22, 0.34)` / `rgba(138, 94, 16, 0.75)` | Toàn bộ card, ô nhập, đường phân cách |
| Hệ bo góc đồng tâm (`--r-lg / --r-md / --r-sm / --r-xs`) | Phương án A: `16px / 10px / 8px / 7px` (Hoặc Phương án B: `12px` card + `999px` nút) | Giống nhau 100% ở cả Tối & Sáng | Toàn bộ 6 giao diện HTML + `content_agent.js` |

---

## 3. Đáp Án Đã Chốt (`clarify`)

| # | Câu hỏi | Đáp án đã chốt |
| :--- | :--- | :--- |
| **1** | Kiểu bo góc đồng bộ cho toàn bộ 6 giao diện TPbrowser | **Phương án B: Viên thuốc bo tròn `999px`** cho toàn bộ nút bấm, thanh chuyển Sáng/Tối, thẻ Google, ô tìm kiếm (`omnibox`, `url-form`, `prompt-form`), chip, tab Chat; card nội dung & modal giữ `12px–16px` đồng tâm (`inner = outer - gap`). |
| **2** | Màu sắc của 3 bóng chat nổi góc dưới phải (AI, Messenger, Zalo) | **Chế độ Sáng**: nền trắng ngọc viền Vàng Đồng + icon `#8A5E10`. **Chế độ Tối**: nền Đen Nhung + icon Vàng Đồng `#E2BE68`. |
| **3** | Ngôn ngữ icon trên các nút chính | **Đổi emoji trên các nút chính sang bộ SVG nét mảnh `1.75px` (`currentColor`)** đồng bộ cùng `titlebar.html`. |
