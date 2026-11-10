# PLAN BRAINSTORMING: ĐỔI GIAO DIỆN TPBROWSER TINH TẾ, TỐI GIẢN & THƯ GIÃN (CALM MINIMAL UI)

- **Ngày lập**: 2026-11-10
- **Trạng thái**: CHỜ ANH PHÚ CHỌN PHONG CÁCH TRÊN DEMO (`clarify`)

---

## Bước 1: Quét hiện trạng giao diện `TPbrowser` & Nguyên nhân gây cảm giác "cứng / căng thẳng"

| File hiện tại (`file:line`) | Thiết kế hiện tại (Brutalist / Viền cứng) | Vì sao chưa tạo cảm giác thư giãn | Hướng đổi sang Tinh Tế & Thư Giãn (Calm Minimal) |
| :--- | :--- | :--- | :--- |
| `src/newtab.html:16-18`, `99-106`, `122-128` | Nền trắng gắt `#ffffff` kèm lưới chấm bi `radial-gradient`, khung trung tâm viền đen dày `2px solid #0f172a` và bóng đổ cứng `box-shadow: 6px 6px 0px`. | Viền đen dày và bóng vuông sắc cạnh tạo cảm giác công cụ kỹ thuật nặng nề, nhìn lâu dễ mỏi mắt. | Chuyển sang nền **Giấy Ấm (`#FAF8F5`)** hoặc **Cát Thiền (`#F5F2EB`)**, bo góc đồng tâm (`14px` ngoài / `9px` trong), viền mờ `1px rgba(0,0,0,0.08)` và bóng sương mờ tự nhiên. |
| `src/tab_shell.html:101-162`, `195-212` | Thanh điều hướng chứa 3 hàng nút đóng khung vuông sát nhau, Tab đang chọn tô nền đen đặc `#0f172a`, chữ IN HOA dày. | Quá nhiều đường kẻ chia ô làm chật thanh bên trái. | Gom thanh địa chỉ và nút điều hướng nhẹ nhàng, Tab đang chọn nổi lên như **thẻ trắng kem mềm mại** có chấm tròn xanh trà (`#4A7C59`), ẩn các nút phụ khi chưa rê chuột tới. |
| `src/ai_panel.html:21-68`, `120-154` | Nhiều nút viền màu gắt (đỏ, xanh dương, đen) và lưới 4 ô vuông lớn. | Khi mở bên cạnh trang web dễ gây phân tán sự chú ý. | Đổi thành các **viên thuốc nhỏ (Soft Pills)** cùng tông màu dịu, khung hội thoại bo tròn nhẹ, giữ nguyên 100% tính năng (`Làm mẫu`, `Skills`, `Key`, `Bong bóng`). |

---

## Bước 2: Bản đồ ánh xạ dữ liệu & Token thiết kế (Data & Style Mapping)

| Thành phần giao diện | File cập nhật trong `TPbrowser` | Giữ nguyên 100% tính năng | Thay đổi thẩm mỹ |
| :--- | :--- | :--- | :--- |
| **Thanh Quản lý Tab (Dọc & Ngang)** | `src/tab_shell.html` | Đổi Dọc/Ngang (`Alt+T`), Thu gọn (`«`), Mở Tab/Profile (`Alt+N`/`Alt+P`), `uBlock: BẬT`, Google Account | Bo góc `9px`, nền `#F3EFEA` (Giấy ấm), Tab active nền sáng nổi nhẹ thay vì khối đen đặc |
| **Trang Bắt Đầu Thư Giãn** | `src/newtab.html` | Đồng hồ, Lời chào tên Google anh Phú, Tìm kiếm/`@ai`, Thêm/Xóa Bookmark | Đồng hồ nét thanh (`font-weight: 300`, `tabular-nums`), thẻ Bookmark thoáng đãng, bỏ lưới chấm bi |
| **Ngăn AI Cowork & Bong Bóng Nổi** | `src/ai_panel.html`, `src/content_agent.js` | Tự thêm API Key (`8045`), Tự thu gọn thành Bong bóng, `🔴 Làm mẫu` & `📚 Skills` | Bong bóng dạng viên thuốc tròn (`border-radius: 999px`) êm dịu, nút pill tinh gọn |

---

## Bước 3: Bảng đáp án đã chốt (`clarify`)

| # | Câu hỏi | Đáp án đã chốt |
| :--- | :--- | :--- |
| **1** | Khi bấm xem thử 3 phong cách trên bản Demo, anh thích tông màu thư giãn nào nhất cho `TPbrowser`? | **Tích hợp bộ chủ đề thư giãn (Giấy Ấm, Sương Sớm, Cát Thiền) VÀ thêm Giao diện Tối: Đen + Vàng Gold (`obsidian-gold`: nền đen nhung `#0E0F12` / `#16181D` + điểm nhấn vàng Champagne Gold `#D4AF37`).** |
| **2** | Về cách bố trí nút bấm trên Thanh Tab và Ngăn AI để tạo cảm giác nhẹ nhàng, không rối mắt, anh chọn kiểu nào? | **Tối giản thoáng mắt: Chỉ hiện thanh chính, còn các nút phụ (`← → ↻ ★`, nút `×` đóng tab) chỉ hiện nhẹ khi rê chuột vào (`Hover Reveal`).** |
