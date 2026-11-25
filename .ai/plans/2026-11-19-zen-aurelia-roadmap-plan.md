# KẾ HOẠCH TRIỂN KHAI TPBROWSER (ZEN + AURELIA)

> **File:** `.ai/plans/2026-11-19-zen-aurelia-roadmap-plan.md`  
> **Ngày khởi tạo:** 2026-11-19  
> **Trạng thái:** Sẵn sàng thực thi theo thứ tự ưu tiên Đợt A → Đợt B → Đợt C → Đợt D.

---

## 0. Quy Tắc Bắt Buộc Trước Khi Code (Guardrails)
1. **`src/main.rs`:**
   - Mọi `AppCmd` mới **bắt buộc thêm vào cuối enum** và xử lý ở **cuối `execute()`** (không chen giữa `ToggleTabPinned..OpenCommandBar` hay `SetTabSelection..CopyToClipboard`).
   - Không chen IPC mới vào giữa `tab_meta` và `channel_unread` trong `handle_ipc()`.
2. **`src/content_agent.js`:**
   - **Cấm tuyệt đối** `.innerHTML`, `.outerHTML`, `insertAdjacentHTML` (dính Trusted Types trên YouTube/Google). Chỉ dùng `document.createElement` / `textContent` hoặc hàm `domEl()`.
3. **`src/tab_shell.html`:**
   - Giữ đúng thứ tự DOM: `#v_essentials_section` < `#v_pinned_section` < `#v_tab_list` < `#v_media_card` < `.space-section` < `#v_worker_section`.
   - Tuyệt đối không dùng `<input type="checkbox">` (dùng widget `.pill-switch` custom của AURELIA).
4. **`src/workspace.html`:**
   - Offline 100% (không CDN), không bịa số `0` khi thiếu dữ liệu (giữ `—`), TikTok giữ read-only.

---

## Đợt A — Hoàn Thiện Thanh Tiêu Đề Mới & Dọn Giao Diện (ĐÃ HOÀN TẤT 100%)

### A1. Bung HWND Titlebar cho Gợi ý URL & Menu Extension [HOÀN THÀNH]
- **Vấn đề:** WebView `titlebar` trước đây cao cố định `40px` (hoặc `68px` khi bật Bookmark Bar), nên dropdown gợi ý tìm kiếm và popover `#tb_ext_popover` bị cắt ở mép dưới.
- **File đã sửa:** `src/main.rs`, `src/titlebar.html`, `tests/test_titlebar_split_contract.py`.
- **Chi tiết kỹ thuật đã triển khai:**
  - `src/main.rs`: Thêm cờ `titlebar_popover_open: bool`, trong `relayout()` khi `true` tạm bung chiều cao HWND `titlebar_shell` lên 380px và gọi `bring_webview_to_front` để nổi trên nội dung web; khi đóng trả về 40px/68px. Đổi `titlebar_shell` sang `.with_transparent(true)` để nền trong suốt.
  - Thêm IPC: `open_titlebar_popover` / `close_titlebar_popover` (đặt cuối enum `AppCmd` & cuối `execute()`).
  - `src/titlebar.html`: Thay `<datalist>` bằng dropdown `#tb_url_dropdown` custom thông minh, phân loại badge: *Tab đang mở*, *Bookmark*, *Lịch sử*, *Giao việc AI*, *Tìm kiếm Web*. Hỗ trợ phím `↑/↓/Enter/Esc` và click-outside.
  - Quản lý ghim Extension (`pinned_extensions: Vec<String>`) lưu `config.json` qua `save_setting`.
- **Nghiệm thu:** Test contract `test_titlebar_popover_and_url_dropdown_contract` pass 100%.

### A2. Dọn Code Chết trong `src/workspace.html` [HOÀN THÀNH]
- **File đã sửa:** `src/workspace.html`.
- **Chi tiết:** Xóa hơn 1,239 dòng HTML/CSS/Canvas/JS chết của layout 3-cột cũ (`.shopee-grid-3col`), giữ lại contract ecom test.
- **Nghiệm thu:** `pytest -q tests/test_webview_ecom_scan.py` pass 100% (25/25).

---

## Đợt B — Tính Năng Chuẩn Zen Browser Mới (Chạy Thật End-to-End)

### B1. Dịch Tự Động Toàn Trang & Dịch Bôi Đen (Zen Translations)
- **File sửa:** `src/main.rs`, `src/content_agent.js`, `src/settings_modal.html`.
- **Config mới (`config.json`):**
  - `translate_target_lang`: `"vi"` (mặc định Tiếng Việt; hỗ trợ `vi`, `en`, `zh-CN`, `ja`, `ko`).
  - `translate_auto_prompt`: `true` (tự hiện thanh gợi ý dịch khi vào trang ngoại ngữ).
  - `translate_engine`: `"free"` (`"free"` dùng endpoint dịch nhanh không cần key, hoặc `"ai"` dùng LLM Cowork dịch theo ngữ cảnh).
  - `translate_never_domains`: `Vec<String>` (danh sách domain không hỏi dịch).
- **Backend (`src/main.rs`):**
  - Thêm tool/IPC `translate_text` (gọi HTTP từ Rust để tránh lỗi CORS của trang web khi trang có CSP chặt!).
  - Trả kết quả về tab đang gọi qua `evaluate_script`.
- **Content Script (`src/content_agent.js` — thuần DOM API):**
  1. *Tự phát hiện ngoại ngữ:* Kiểm tra `document.documentElement.lang` (hoặc mẫu ký tự) khi load xong; nếu khác `translate_target_lang` và domain không nằm trong `never_domains` → hiện pill/banner nổi gọn ở góc trên phải: `[Dịch sang Tiếng Việt] [Không hỏi trang này] [×]`.
  2. *Dịch toàn trang:* Duyệt `TreeWalker` các `TextNode` hiển thị (bỏ qua `script`, `style`, `code`, `pre`, `input`), lưu bản gốc vào `WeakMap` để bấm **"Xem bản gốc"** là khôi phục tức thì không cần reload. Gom nhóm (batch ~25 đoạn/lượt) để gửi dịch mượt.
  3. *Dịch đoạn bôi đen:* Khi `mouseup` có `window.getSelection().toString().trim()`, hiện nút nổi nhỏ **"Dịch"** ngay cạnh vùng chọn; bấm vào sẽ hiện thẻ kết quả dịch gọn kèm nút Copy.
- **UI (`src/settings_modal.html`):** Thêm nhóm **Dịch Tự Động (Translations)** vào tab *Giao Diện* hoặc *Tab & Khung Hình*.

### B2. Tùy Biến Zen Glance & Công Cụ Tìm Kiếm Mặc Định
- **File sửa:** `src/main.rs`, `src/content_agent.js`, `src/settings_modal.html`, `src/newtab.html`.
- **Config mới:**
  - `glance_enabled`: `true`
  - `glance_trigger`: `"alt_click"` | `"shift_click"`
  - `glance_external_from_pinned`: `true` (khi bấm link trỏ ra domain khác từ **Essentials** hoặc **Pinned Tab**, tự động mở bằng **Zen Glance** thay vì làm trôi Tab Ghim).
  - `default_search_engine`: `"google"` | `"duckduckgo"` | `"bing"` | `"perplexity"`.
- **Chi tiết:**
  - `resolve_url_or_search()` trong `src/main.rs` và thanh Omnibox trong `newtab.html` đọc `default_search_engine` để tạo URL tìm kiếm tương ứng.
  - `content_agent.js` đọc cờ `glance_trigger` & `is_pinned_or_essential` (truyền qua state sync) để bắt click mở Glance chuẩn xác.

### B3. Tự Động Picture-in-Picture (Auto PiP) Khi Chuyển Tab
- **File sửa:** `src/main.rs`, `src/content_agent.js`, `src/settings_modal.html`.
- **Config mới:** `auto_pip_on_switch`: `true`.
- **Chi tiết:**
  - Khi chuyển khỏi tab đang phát `<video>` có kích thước lớn (`videoWidth > 200`, `!paused`) và `auto_pip_on_switch == true` → `main.rs` bắn script sang tab vừa rời khỏi gọi `video.requestPictureInPicture()` (bọc `try/catch`); khi quay lại tab đó thì tự `document.exitPictureInPicture()`.

---

## Đợt C — Vá Bảo Mật & Bug Tồn Đọng

### C1. Bảo Mật MCP HTTP Server (`:17777`)
- **Rủi ro:** `scripts/start_tunnel.bat` & `scripts/start_all.bat` mở public cổng `17777` (có tool `tp_eval_js`) ra `trycloudflare.com` mà chưa có xác thực.
- **File sửa:** `src/main.rs`, `scripts/start_tunnel.bat`, `scripts/start_all.bat`.
- **Chi tiết:**
  - Sinh hoặc đọc `mcp_auth_token` (từ env `TP_MCP_TOKEN` hoặc tự tạo ngẫu nhiên lưu `config.json` lần đầu).
  - Middleware HTTP tại cổng `17777`: nếu request không đi kèm header `Authorization: Bearer <token>` (trừ khi cấu hình cho phép `localhost` nội bộ không qua proxy `CF-Connecting-IP`/`X-Forwarded-For`), trả về `401 Unauthorized`.
  - In sẵn token ra console/log khi chạy tunnel để client MCP copy dán.

### C2. Sửa Bug Escape Chuỗi ở `AppCmd::CreateTaskFromSelection`
- **File sửa:** `src/main.rs`.
- **Chi tiết:** Thay chuỗi escape thủ công `{\\\"text\\\":...}` bị thừa dấu `\` khi `evaluate_script` sang `ai_panel.html` bằng `JsonVal::obj(...)` + `.to_json()`.

---

## Đợt D — Mở Rộng Tùy Chọn (Làm Sau Khi Xong A–C)

1. **Download Manager gọn trong Sidebar (`src/main.rs` + `src/tab_shell.html`):**
   - Hook `DownloadStarting` của WebView2, phát event tiến độ sang `tab_shell.html`, hiện thẻ gọn ngay trên `.space-section` (tên file, `%`, nút mở file / mở thư mục).
2. **Zen Boosts (Custom CSS theo Domain):**
   - Lưu map `domain -> custom_css` trong `config.json`, `content_agent.js` tự chèn `<style id="__tp_boost_css">` khi tải trang + công cụ "Zap" click ẩn phần tử rác.
3. **Little Zen (Quick Lookup Window):**
   - Phím tắt toàn cục Windows (`RegisterHotKey`) bật cửa sổ nhỏ gọn để tra nhanh hoặc hỏi `@ai` mà không cần bung cửa sổ trình duyệt chính.

---

## Bộ Lệnh Kiểm Chứng (Chạy Sau Mỗi Đợt)
```powershell
cargo test
C:\Users\thien\AppData\Local\hermes\hermes-agent\venv\Scripts\python.exe -m pytest -q tests/test_checkpoint_integrity.py tests/test_titlebar_split_contract.py tests/test_webview_ecom_scan.py
cargo build --release
Copy-Item -Force target/release/TPbrowser.exe ./TPbrowser.exe
python scripts/generate_checkpoint.py
```
