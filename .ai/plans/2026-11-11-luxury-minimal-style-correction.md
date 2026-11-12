# PLAN BRAINSTORMING: SỬA LẠI GIAO DIỆN TPBROWSER — ÁP DỤNG PHONG CÁCH ĐÁ CẨM THẠCH & VÀNG GOLD VÀO TRÌNH DUYỆT THẬT (KHÔNG BÊ NGUYÊN ẢNH MẪU LÀM TRANG CHỦ)

- **Ngày lập**: 2026-11-11
- **Trạng thái**: ĐÃ DUYỆT QUA `clarify` — ĐANG THỰC THI

---

## 1. Bảng Lỗi Hiện Tại (Vì sao giao diện đang sai)

| Vị trí (`file:line`) | Hiện trạng đang sai | Hậu quả thực tế khi dùng trình duyệt | Hướng sửa đúng ("Lấy phong cách, giữ đúng chức năng trình duyệt") |
| :--- | :--- | :--- | :--- |
| `src/newtab.html:516-607` | Bê nguyên chữ và bố cục poster thời trang trong ảnh mẫu (`AURELIA`, menu `COLLECTIONS / THE BRAND / JOURNAL / CONTACT`, cột dọc `GALLERY`, khẩu hiệu `THE ART OF MINIMALISM`). | Trang chủ trình duyệt biến thành trang quảng cáo thương hiệu ảo; mất Đồng hồ, mất nút `GIAO AI COWORK`, mất thẻ Tài khoản Google, và lưới Bookmark bị giấu vào popup `COLLECTIONS`. | Khôi phục đầy đủ **bố cục Trang Chủ TPbrowser thực thụ** (Đồng hồ + Lời chào anh Phú, Thanh địa chỉ/Tìm kiếm + nút `Giao AI Cowork`, Lưới Bookmark hiển thị sẵn có `+ Thêm trang` & nút xóa `×`, Thẻ Google Account) nhưng thiết kế bằng **phong cách Đen Cẩm Thạch & Vàng Gold / Sáng Ngọc Trắng & Gold**. |
| `src/tab_shell.html:25-75` & `src/ai_panel.html:25-75` | Khi chuyển sang giao diện Sáng (`warm-paper`), thanh Tab và ngăn AI lại đổi sang màu xanh lá `#4A7C59` thay vì tông Ngọc Trắng & Vàng Champagne. | Lệch tông hoàn toàn giữa Trang chủ (Vàng Champagne) và Thanh Tab / Ngăn AI (Xanh lá). | Đồng bộ hệ màu toàn trình duyệt về đúng 2 chế độ chuẩn của phong cách này: **Tối (Đen Cẩm Thạch & Vàng Gold `#D4AF37`)** và **Sáng (Ngọc Trắng Carrara `#F7F4EE` & Vàng Champagne `#9E762C`)**. |
| `src/content_agent.js:242-350` | Bong bóng AI nổi (`#__tp_bubble_pill`, `#__tp_bubble_popover`) trên các trang web vẫn giữ viền vuông dày `2px solid #10b981` và bóng cứng `4px 4px 0px` kiểu cũ. | Lạc quẻ với phong cách sang trọng, tối giản của toàn bộ trình duyệt. | Chuyển Bong bóng AI nổi và hộp thoại thu gọn sang thiết kế **viền mảnh Vàng Gold, nền Đen Nhung / Ngọc Trắng bo góc mềm**, đồng bộ với vỏ trình duyệt. |

---

## 2. Bản Đồ Ánh Xạ Dữ Liệu & Phong Cách (Data & Style Mapping)

| Thành phần trình duyệt | Nguồn dữ liệu / IPC thật (`src/main.rs`) | File vị trí | Áp dụng phong cách từ ảnh mẫu như thế nào |
| :--- | :--- | :--- | :--- |
| **Nền & Chất liệu (Background & Surface)** | `ui_theme` (`obsidian-gold` / `carrara-light`) trong `config.json` | `src/newtab.html`, `src/tab_shell.html`, `src/ai_panel.html` | Nền đá cẩm thạch tối (`#0B0C0E`) hoặc ngọc trắng (`#F7F4EE`) kèm vân sóng lụa vàng mờ tinh tế ở lớp nền; thẻ nội dung dùng kính mờ (`backdrop-filter: blur(10px)`) và viền vàng mảnh `1px`. |
| **Thanh đầu Trang Chủ (Header)** | `google_user`, `toggle_tab_orientation`, `open_ai_settings`, `set_ui_theme` | `src/newtab.html` | Logo `TP` viền vàng kim, công tắc chuyển nhanh **Tối / Sáng**, nút **Đổi Tab Dọc/Ngang**, nút **Cài đặt API Key**, và **Thẻ Tài khoản Google** (mở modal đăng nhập Google thật). |
| **Khu vực Trung tâm (Đồng hồ & Thanh Tìm Kiếm)** | `navigate_active`, `run_ai_prompt`, `focus_ai_panel` | `src/newtab.html` | Đồng hồ số lớn font Serif thanh lịch (`Cormorant Garamond` / `Georgia`) màu vàng kim, đường kẻ mảnh 1px đính nút hình thoi (`◆`) chính giữa, bên dưới là ô nhập URL / Tìm kiếm / `@ai` kèm nút `MỞ TRANG` và `GIAO AI COWORK`. |
| **Lưới Trang Ghim Nhanh (Bookmarks Grid)** | `cfg.bookmarks` + lệnh `save_bookmarks` (thêm/xóa lưu thật vào `config.json`) | `src/newtab.html` | Hiển thị trực tiếp 4 cột ngay trên trang chủ (không giấu trong popup), mỗi thẻ viền vàng mảnh `1px`, rê chuột sáng nhẹ ánh kim và hiện nút `×` xóa, kèm nút `+ Thêm trang mới` mở modal thêm URL. |
| **Lối tắt nhanh Messenger / Zalo / AI** | `toggle_ai_mode`, `navigate_active` | `src/newtab.html` | Tuỳ chọn giữ 3 nút tròn Vàng Gold nổi góc dưới phải (AI, Messenger, Zalo) hoặc gộp gọn vào lưới Bookmark theo lựa chọn của anh. |
| **Thanh Tab Dọc/Ngang & Ngăn AI Cowork** | `__tpSyncShellState`, `__tpLoadAiConfigFromRust`, `skills/skills.json` | `src/tab_shell.html`, `src/ai_panel.html`, `src/content_agent.js` | Viền mảnh vàng kim, chấm trạng thái vàng/ngọc, giữ nguyên 100% tính năng Hover Reveal, Tự học Skill (`Làm mẫu` + dạy bằng lời) và `Test Key` cổng `8045`. |

---

## 3. Ma Trận Tác Động Sự Kiện (Event Impact Matrix)

1. **Trigger (UI/Payload)**:
   - Đổi chế độ Sáng/Tối từ Trang chủ hoặc Thanh Tab gửi `{cmd: "set_ui_theme", theme: "obsidian-gold" | "carrara-light"}`.
   - Thêm/xóa Bookmark gửi `{cmd: "save_bookmarks", bookmarks_json: "..."}`.
   - Cập nhật tên/đăng nhập Google gửi `{cmd: "set_google_user", google_user: "..."}`.
2. **Entity State**:
   - `TpBrowserApp` trong `src/main.rs` cập nhật `self.ui_theme`, `self.bookmarks_json`, `self.google_user` và lưu bền vững xuống `config.json`.
3. **Đồng bộ liên khung (Cross-WebView Sync)**:
   - Khi đổi `ui_theme`, `main.rs` gọi đồng thời `__tpSyncShellState` (Thanh Tab), `__tpSyncNewTab` (mọi tab đang ở `tp://newtab`), và `__tpLoadAiConfigFromRust` (Ngăn AI) để cả 3 vùng đổi màu Tối/Sáng tức thì không lệch tông.
4. **Reversal & Backup**:
   - Bản sao lưu `src/*.bak_before_calm_ui` vẫn giữ nguyên làm mốc đối chiếu an toàn.

---

## 4. Bảng Đáp Án Đã Chốt (`clarify`)

| # | Câu hỏi | Đáp án đã chốt |
| :--- | :--- | :--- |
| **1** | Về bố cục khu vực trung tâm của Trang chủ TPbrowser (khi áp dụng phong cách Đá Cẩm Thạch & Vàng Gold vào công cụ trình duyệt thật), anh chọn kiểu nào? | **Mặc định theo đề xuất (1a): 1 trục trung tâm thoáng đãng** (Đồng hồ vàng kim + Lời chào + Ô tìm kiếm/Giao AI + Lưới 8 Bookmark hiển thị sẵn bên dưới; có nút chuyển đổi nhanh sang 2 cột nếu muốn). |
| **2** | Về cụm 3 nút tròn Vàng Gold ở góc dưới phải (Robot AI, Messenger, Zalo), anh muốn giữ làm lối tắt nhanh hay lược bỏ? | **Phương án A + Mở rộng:** Giữ 3 nút tròn Vàng Gold ở góc dưới phải làm **3 nút bật/tắt cửa sổ chat nổi dạng Facebook (Facebook Chat Heads / Mini Chat Box)** cho **AI Cowork**, **Facebook Messenger** và **Zalo**. |
| **3** | Về bộ chủ đề màu đồng bộ trên toàn bộ trình duyệt (Thanh Tab + Trang chủ + Ngăn AI + Bong bóng nổi), anh chọn phương án nào? | **Phương án A (Chỉ 2 chế độ Tối & Sáng):** **Tối (Đen Cẩm Thạch & Vàng Gold)** và **Sáng (Ngọc Trắng & Vàng Champagne)**; đặc biệt **tăng độ đậm và tương phản của màu chữ ở Chế độ Sáng** (`#1A1610` / `#5C3D08`) để nhìn rõ nét, không bị nhạt/mờ. |
