# Cập nhật cho AI Studio — TPbrowser (sau khi áp Đợt 1 phần 1)

## Đã áp dụng xong (verify sạch, không cần sửa lại)
1. `src/workspace.html`: đã tách khối JS "SHOPEE EXECUTIVE 3-COLUMN DASHBOARD LOGIC" ra khỏi thuộc tính `style` của `<button id="tp_skip_intro">`, đặt vào `<script>` riêng đúng vị trí. Đã sửa `"stat_rev"` → `"stat-rev"`.
2. `src/layout.rs`: `compute_columns_in_area` / `compute_rows_in_area` đã nhận `master_ratio` và dùng khi `count == 2` (như patch đề xuất).
3. `tests/test_webview_ecom_scan.py`: đã cập nhật id `stat-rev` cho khớp HTML thật.

Verify: `cargo test` 50/50 pass, `pytest` 41/41 pass, `cargo build --release` thành công.

**KHÔNG cần AI Studio làm lại 2 bước này.**

## QUAN TRỌNG — patch Bước 3-5 (Command Bar / Context Menu / Split) bị SAI giả định kiến trúc

Patch Đợt 1 mà AI Studio gửi giả định phải **xây mới toàn bộ Command Bar** (`#__tp_cmd_modal`, hàm `window.__tpOpenZenCommandBar`) bên trong `src/content_agent.js` (script chạy trên mọi trang web đang mở).

Nhưng thực tế codebase **đã có sẵn 1 Command Bar hoạt động đầy đủ** nằm trong `src/titlebar.html`:
- HTML: `<div id="command_bar_overlay">` chứa `#command_bar_input`, `#command_bar_results` (dòng ~467-470).
- JS: `openCommandBar()`, `closeCommandBar()`, `renderCommandBar()`, `activateCommandBar()` — đã có fuzzy filter tab/workspace/lệnh, bắt phím Escape/ArrowUp/ArrowDown/Enter (dòng ~967-980).
- Được gắn vào `window.__tpOpenCommandBar = openCommandBar;`
- `AppCmd::OpenCommandBar` (trong `src/main.rs`) hiện chỉ gọi `self.titlebar_shell.evaluate_script("window.__tpOpenCommandBar()")`.

**Lỗi thật** (đúng như patch mô tả đúng triệu chứng): `titlebar_shell` chỉ **hiện khi ở chế độ Tab Ngang** (`TabOrientation::HorizontalTop`). Khi đang ở Tab Dọc (mặc định), `titlebar_shell` bị ẩn (`is_visible = false`) → gọi script vào một webview đang ẩn không hiển thị gì → `Ctrl+K` / `Alt+K` ở Tab Dọc không có phản hồi.

**Yêu cầu cho patch mới:** đừng viết lại Command Bar trong `content_agent.js`. Hãy sửa đúng gốc:
1. Nhân bản/tái dùng cấu trúc Command Bar sẵn có (`#command_bar_overlay`, logic trong `renderCommandBar`/`activateCommandBar`) sang `src/tab_shell.html` — file này đã có sẵn `sendRust()`, `window.__tpSyncShellState`, và input `#v_url_input` dùng khi Tab Dọc.
2. Sửa `AppCmd::OpenCommandBar` trong `src/main.rs` (dòng ~5716) để gọi đúng shell đang hiển thị: nếu `self.tab_orientation == TabOrientation::VerticalLeft` → gọi script trên `self.tab_shell`; ngược lại gọi `self.titlebar_shell` như hiện tại.
3. Tương tự, `AppCmd::FocusUrlBar` (dòng ~6725) đã có logic đúng hướng (ưu tiên titlebar focus) nhưng cần thêm nhánh: khi `VerticalLeft` và sidebar đang collapsed, phải mở Command Bar thay vì focus vào ô input đã ẩn.

Giữ nguyên toàn bộ các `AppCmd` mới đề xuất ở Bước 3.1 (`CopyActiveUrl`, `DuplicateTab`, `PinTabById`, `CloseOtherTabs`, `MoveTabToWorkspace`, `ReorderTab`, `OpenInSplit`, `AdjustSplitRatio`, `ResetSplitRatio`) và IPC parsing ở Bước 3.2 — phần này không liên quan Command Bar, đã hợp lý, có thể giữ.

## File đính kèm
Bản full export mới nhất (đã có fix Bước 1+2) nằm ở: `.ai/FULL_PROJECT_EXPORT.md` (~1.68MB) — anh sẽ tải lại file này lên AI Studio để AI Studio đọc đúng state hiện tại, đặc biệt đọc kỹ `src/titlebar.html` trước khi viết patch Command Bar mới.
