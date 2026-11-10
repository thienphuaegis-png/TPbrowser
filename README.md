# TPbrowser — Trình Duyệt Cá Nhân Siêu Nhẹ & AI Cowork (Rust + WebView2)

Trình duyệt cá nhân viết bằng **Rust (`tao` + `wry`)** dung lượng chỉ **682 KB**, thiết kế riêng cho anh Phú:

1. **Quản lý Tab linh hoạt (Dọc bên tay trái hoặc Ngang phía trên):**
   - Bấm nút **`⬆ Ngang` / `⬅ Tab Dọc`** (hoặc phím tắt **`Alt + T`**) để đổi ngay lập tức giữa Thanh Tab Dọc bên trái và Thanh Tab Ngang phía trên.
   - Bấm **`«`** để thu gọn Thanh Tab Dọc về dạng icon nhỏ gọn (`54px`).
   - Hỗ trợ xem **1 Tab toàn màn hình** hoặc bật **Chia khung BSP (`Khung: Xoắn Ốc` / `Chia Cột` — `Alt + M`)** để xem nhiều Tab cạnh nhau.
2. **Trang Bắt Đầu Trắng Decor (`tp://newtab`) & Quản lý bằng Tài khoản Google:**
   - Giao diện sáng tinh tế, đồng hồ số, thanh tìm kiếm/lệnh `@ai`, và lưới thẻ Bookmark cá nhân (thêm/xóa trực tiếp).
   - Đăng nhập **Tài khoản Google** ngay góc phải để quản lý trình duyệt và đồng bộ phiên làm việc (Gmail, Drive, Sheets, YouTube).
3. **AI Cowork bên tay phải & Bong bóng chat tự thu gọn khi làm việc (`Alt + A`):**
   - **Tự thêm API Key (BYOK):** Bấm **`⚙ API Key`** để người dùng tự nhập Gemini API Key hoặc OpenAI/9Router Endpoint + Key (lưu vào `config.json`).
   - **Tự thu gọn khi giao việc:** Khi anh giao việc cho AI (ví dụ: *"Tóm tắt trang này"*, *"Điền từ khóa vào ô tìm kiếm"*, *"Tổng hợp mọi Tab"*), cửa sổ chat bên phải **tự động thu gọn thành Bong bóng nhỏ (Mini-Pill)** ở góc dưới trang web để không che màn hình, trong khi AI vẫn tiếp tục đọc trang và thao tác ngầm cho đến khi báo hoàn tất.

## Phím tắt nhanh
- **`Alt + T`**: Đổi Thanh Tab Dọc (Trái) ↔ Ngang (Trên)
- **`Alt + A`**: Đổi AI Ngăn Phải ↔ Bong Bóng Nổi
- **`Alt + N`**: Mở Tab mới
- **`Alt + P`**: Mở Tab với Profile tài khoản độc lập (`acc_2`, `acc_3`...)
- **`Alt + W`**: Đóng Tab đang chọn
- **`Alt + M`**: Đổi chế độ khung (`1 Tab` → `Xoắn Ốc BSP` → `Chia Cột`)
- **`Alt + 1..8`**: Chuyển nhanh tới Tab số 1..8
