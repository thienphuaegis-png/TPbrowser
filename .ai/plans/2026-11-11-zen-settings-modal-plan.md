# KẾ HOẠCH THIẾT KẾ & ÁNH XẠ: BẢNG CÀI ĐẶT TẬP TRUNG (SETTINGS MODAL) CHO TPBROWSER THEO CHUẨN ZEN BROWSER

- **Ngày lập**: 2026-11-11
- **Trạng thái**: ĐÃ TRIỂN KHAI HOÀN TẤT (`v0.3.0-zen-settings`, 2026-11-11)
- **Nguồn cảm hứng**: Khảo sát thực tế Zen Browser v1.24.1b (`C:\Program Files\Zen Browser`) trên máy anh Phú

---

## 1. Kết Quả Khảo Sát Zen Browser Thực Tế Trên Máy Anh
- **Cấu hình phát hiện**:
  - Bản cài: Zen Browser v1.24.1b (Firefox Gecko base) tại `C:\Program Files\Zen Browser`.
  - Profile chính: `C:\Users\thien\AppData\Roaming\zen\Profiles\nri8zwwp.Default (release)`.
  - Tiện ích mở rộng đã cài: *Sheets Row Highlighter, Chrome Remote Desktop, Authenticator, Bitwarden, AdGuard, Proton Pass, Open in Popup Window*.
  - Bộ Zen Mods đang hoạt động: **Transparent Zen**, **Better Find Bar**, **Custom MenuButton**, **Zen Context Menu**.
  - Các thiết lập Zen đặc trưng: `zen.urlbar.behavior: "float"` (thanh URL nổi), `zen.workspaces` (không gian làm việc), `zen.view.compact` (chế độ tự ẩn thanh điều khiển).

---

## 2. Bản Đồ Ánh Xạ Dữ Liệu (Data Mapping Matrix)

| Nhóm Cài Đặt (Zen-style) | Khóa Cấu Hình TPbrowser (`config.json`) | Nơi Xử Lý Backend (`src/main.rs`) | Giao Diện Hiển Thị (`UI Component`) | Trạng Thái Hiện Tại |
|---|---|---|---|---|
| **Giao diện: Theme** | `ui_theme` (`"obsidian-gold"` / `"carrara-light"`) | `set_theme_mode` | Segmented Pill `999px` | Đã có, chuyển vào Modal |
| **Giao diện: Floating Omnibox** | `floating_omnibox` (boolean) | IPC broadcast | Toggle Switch | Mặc định bật |
| **Giao diện: Compact Titlebar** | `compact_titlebar` (boolean) | `src/titlebar.html` (auto-hide) | Toggle Switch | Đã có trong titlebar |
| **Bố cục: Tab Dọc / Ngang** | `tab_orientation` (`"vertical"` / `"horizontal"`) | `toggle_tab_orientation` | Segmented Pill `999px` | Đã có, chuyển vào Modal |
| **Bố cục: Chia khung BSP** | `view_mode` (`"single"` / `"spiral"`) | `cycle_view_mode` | Segmented Pill `999px` | Đã có, chuyển vào Modal |
| **AI: Provider / Key / Model** | `ai_provider`, `ai_api_key`, `ai_model`, `ai_base_url` | `save_ai_config`, `test_ai_key` | Input Pill + Dropdown | Trước ở drawer nhỏ, gom vào Modal |
| **AI: Thu nhỏ khi giao việc** | `auto_minimize_on_task` (boolean) | `send_cowork_task` | Toggle Switch | Đã có |
| **Chat: Kiểu Cửa Sổ** | `ai_mode` (`"popup"` / `"dock"`) | `toggle_chat_dock_style` | Segmented Pill `999px` | Đã có |
| **Chat: Unread Badges** | `show_unread_badges` (boolean) | `CHANNEL_UNREAD_WATCHER_JS` | Toggle Switch | Mặc định bật |
| **Chat: Độ Rộng Ngăn Phải** | `ai_dock_width` (integer, 260–75%) | `update_ai_dock_width` | Input / Hiển thị số px | Đã có thanh kéo `6px` |
| **Bảo Mật: uBlock Lite & ABPVN** | `adblock_enabled` (boolean) | `extensions/uBOLite.chromium` | Toggle Switch | Tích hợp sẵn |
| **Tài Khoản: Google Profile** | `google_user` (string) | Google Account Pill | Thẻ Avatar + Nút Đăng nhập | Đã có |
| **Phím Tắt (Shortcuts)** | Danh mục tĩnh + phím tắt tùy biến | IPC Global Key handler | Bảng hiển thị phím tắt | Đã có `Alt+N, Alt+T...` |

---

## 3. Ma Trận Tác Động Sự Kiện (Event Impact Matrix)
1. **Trigger (UI/Payload)**:
   - Người dùng bấm nút `Cài Đặt` trên thanh điều khiển hoặc phím tắt `Alt+,` (hoặc `Ctrl+,`).
   - Mở Modal Cài Đặt tập trung với 6 tab bên trái (`Look & Feel`, `Tabs`, `AI Cowork`, `Side Panels`, `Privacy`, `Shortcuts`).
2. **Entity State**:
   - Khi người dùng thay đổi 1 thiết lập (vd chuyển Theme, đổi Model AI, đổi bố cục Tab), modal gửi lệnh IPC `save_setting { key, value }`.
   - Cập nhật trực tiếp `config.json`.
3. **Broadcast & Đồng Bộ WebViews**:
   - `main.rs` nhận IPC và lập tức broadcast event `tp_config_changed` đến tất cả các WebViews đang chạy: `newtab`, `tab_shell`, `ai_panel`, `titlebar`.
   - Không cần tải lại toàn bộ trình duyệt (Zero reload).
4. **Reversal & Đóng Modal**:
   - Bấm `X`, bấm ra ngoài nền tối (backdrop click), hoặc nhấn phím `Esc` để đóng modal mượt mà với hiệu ứng thu nhỏ `scale(0.96)`.

---

## 4. Các Bước Thực Hiện Cụ Thể
- **Bước 1**: Tạo component `src/settings_modal.html` (hoặc nhúng trực tiếp modal template vào `newtab.html` và `tab_shell.html`) theo đúng chuẩn thiết kế Vàng Đồng + Pill `999px` + SVG nét mảnh `1.75px`.
- **Bước 2**: Đổi nút `Cài Đặt API Key` ở header thành nút `Cài Đặt` đa năng (mở Modal toàn diện).
- **Bước 3**: Bổ sung IPC handler trong `src/main.rs` để đọc/ghi tập trung `config.json` và broadcast thay đổi cấu hình.
- **Bước 4**: Kiểm thử biên dịch `cargo test`, `cargo build --release`, xác nhận qua ảnh chụp màn hình headless Edge.
