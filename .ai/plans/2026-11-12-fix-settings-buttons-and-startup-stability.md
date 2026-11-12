# Plan: Sửa Toàn Diện Lỗi Nút Cài Đặt & Ổn Định Khởi Động TPbrowser

**Ngày:** 2026-11-12  
**Trạng thái:** Chờ anh Phú chốt `clarify` & duyệt  
**Mockup chẩn đoán:** `.ai/sketches/2026-11-12-settings-and-startup-fix.html`

---

## 1. Bảng lỗi ngầm tìm thấy trên code thật

| Lỗi thực tế | Hậu quả khi dùng | Bằng chứng `file:line` |
|---|---|---|
| `build_config_payload_json(None)` luôn gửi `open_section: self.settings_section` (`"look_feel"`), kết hợp `SetTitlebarHover` gọi `sync_all_ui()` dù `titlebar_pinned = true` | Bấm bất kỳ nút nào ở Tab 2–5 (hoặc rê chuột chạm mép trên) là bị giật ngược về Tab 1 (`Look & Feel`) | `src/main.rs:1808`, `src/main.rs:2030`, `src/main.rs:2830`, `src/settings_modal.html:1494` |
| `relayout()` gán cứng `top_reserved = TITLEBAR_HEIGHT` và `titlebar_shell.set_visible(true)` | Công tắc `Tự động ẩn thanh trên cùng` (`sw_compact_titlebar`) bấm đổi UI nhưng thanh trên không hề ẩn | `src/main.rs:1543`, `src/main.rs:1786` |
| Tab 4 (`pane_side_panels`) dùng class `.zs-input`, `.pill-btn.primary`, `var(--border-col)` chưa định nghĩa; `.setting-info b` làm gãy 3 dòng chữ | Form thêm app bong bóng xấu/lệch chuẩn, viền card mất màu | `src/settings_modal.html:321`, `src/settings_modal.html:853-860`, `src/settings_modal.html:1245` |
| 6 WebView shell (`titlebar`, `tab_shell`, `newtab`, `chat_header`, `dock_resizer`, `sidebar_resizer`) không gửi `ui_shell_ready` và hardcode `data-theme="obsidian-gold"` | Lúc mở app `sync_all_ui()` chạy ở 0ms khi WebView chưa parse xong JS → dính theme tối, trống tab, sai tên Google | `src/main.rs:1019-1126`, `src/main.rs:1234`, `src/titlebar.html:2`, `src/tab_shell.html:2`, `src/newtab.html:2` |
| `ensure_fb_shell()` / `ensure_zalo_shell()` thiếu `.with_visible(false)`; `load_session_or_default()` gọi `add_tab()` ghi đè `session.json` từng vòng và bỏ qua `active_idx` | Nháy khung ẩn lúc 1.2s; mở lại app bị sai tab đang chọn hoặc lệch phiên | `src/main.rs:1254-1305`, `src/main.rs:2420-2443` |

---

## 2. Bản đồ ánh xạ sửa đổi (Data Mapping)

| Vị trí | Hiện trạng (`file:line`) | Thay đổi dự kiến |
|---|---|---|
| **Giữ đúng Tab Cài đặt đang mở** | `src/main.rs:1808`, `src/settings_modal.html:1494` | Chỉ gửi `open_section` khi `Some(sec)` (lúc mở modal); `switchTab()` đồng bộ `set_settings_section`; `SetTitlebarHover` bỏ qua khi `titlebar_pinned == true` |
| **Hoạt động thật cho `sw_compact_titlebar`** | `src/main.rs:1543, 1786` | `let tb_visible = self.titlebar_pinned \|\| self.titlebar_hover || self.settings_open;` `top_reserved = if tb_visible { TITLEBAR_HEIGHT } else { 0 };` |
| **CSS & JS sạch trong `settings_modal.html`** | `src/settings_modal.html:321, 853, 1245, 1378` | Đổi `.setting-info > b`; thêm CSS `.zs-input`, sửa `.pill-btn.cta`, `var(--gold-line)`; không ép `/v1` khi `ai_provider === "gemini"`; parse `save_setting` & `save_ai_config` bằng `parse_json` |
| **Ổn định khởi động (Zero Flash / Zero Race)** | `src/main.rs:470-515, 1019-1325, 2420` | Inject `ui_theme` vào `data-theme` lúc tạo HTML (`inject_ui_html(html, &self.ui_theme)`); mọi WebView gửi `ui_shell_ready` để sync tức thì khi load xong; `.with_visible(false)` cho WebView ẩn/chưa active; `load_session_or_default` khôi phục đúng `active_idx` |

---

## 3. Đáp án đã chốt qua `clarify`

| # | Câu hỏi | Đáp án đã chốt |
|---|---|---|
| 1 | Công tắc "Tự động ẩn thanh trên cùng (Compact Titlebar)" | **Luôn cố định thanh trên 40px, bỏ hẳn công tắc "Tự động ẩn" khỏi Cài Đặt cho đỡ rối** |
| 2 | Khôi phục phiên khi mở lại `TPbrowser.exe` | **Thành 1 lựa chọn ở Cài Đặt** (`startup_mode`: **Khôi phục Tab phiên trước** \| **Mở Tab mới sạch `tp://newtab`**) |
| 3 | Phạm vi các nút lỗi cần sửa | **Đúng các lỗi đã chẩn đoán — sửa toàn bộ và kiểm chứng từng nút** |
