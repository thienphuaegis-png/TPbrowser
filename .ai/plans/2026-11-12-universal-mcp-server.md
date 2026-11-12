# Plan: Universal MCP Server cho TPbrowser (Mọi AI đều dùng được)

**Ngày:** 2026-11-12  
**Mục tiêu:** Biến `TPbrowser.exe` thành một **MCP Server chuẩn mở (Model Context Protocol)** hỗ trợ đồng thời **Streamable HTTP (`127.0.0.1:17777/mcp`)** và **Stdio (`TPbrowser.exe --mcp`)**, cho phép mọi AI Client (Hermes, Claude Desktop, Claude Code, Cursor, Antigravity, VS Code...) kết nối vào **cùng 1 cửa sổ TPbrowser sống** (giữ nguyên tab, cookie, Zalo/Messenger, Tasks), kèm tự động khởi chạy `TPbrowser.exe` nếu chưa mở.

---

## 1. Bản đồ ánh xạ dữ liệu & tái sử dụng code thật

| Thành phần MCP | Nguồn có sẵn trong TPbrowser | File / Vị trí | Thiết kế tích hợp |
|---|---|---|---|
| `tp_list_tabs`, `tp_open_tab`, `tp_focus_tab`, `tp_close_tab`, `tp_navigate` | `tabs: Vec<BrowserTab>`, `add_tab`, `AppCmd::FocusIndex`/`CloseIndex`/`NavigateActive` | `src/main.rs:420-432, 508, 1005-1127, 2500-2580` | Dùng lại trực tiếp logic điều khiển tab, trả kết quả JSON đồng bộ qua `mpsc::Sender` |
| `tp_get_page_context` | `window.__tpCollectDomForAi()` | `src/content_agent.js:541-576` | Tách `window.__tpBuildDomContext(maxChars)` dùng chung cho cả AI Panel & MCP; fallback DOM reader cho trang chưa inject `content_agent.js` (`tp://newtab`, `zalo_shell`, `fb_shell`) |
| `tp_execute_action` | `window.__tpExecuteDomAction(act)` | `src/content_agent.js:613-663` | Tái sử dụng 3-tier locator (`target_id` / `selector` / `label`) và click/fill/scroll |
| `tp_eval_js` | `WebView::evaluate_script` + `window.ipc.postMessage` | `src/main.rs` (`tabs`, `zalo_shell`, `fb_shell`) | Bọc async IIFE + `Promise.resolve` gửi kết quả về qua `cmd: "mcp_callback"` với `req_id` |
| `tp_tasks_list`, `tp_tasks_save`, `tp_task_complete`, `tp_task_snooze` | `tasks_json`, `save_tasks()`, `sync_tasks_to_views()`, `AppCmd::CompleteTask`/`SnoozeReminder` | `src/main.rs:467-473, 1620-1645, 2635-2651` | Đọc/ghi trực tiếp `tasks/tasks.json` và đồng bộ live ra mọi WebView |
| `tp_chat_open`, `tp_chat_scan`, `tp_chat_draft` | `ensure_zalo_shell`, `ensure_fb_shell`, `CHANNEL_UNREAD_WATCHER_JS`, `__tpInsertDraftMessage` | `src/main.rs:194-237, 890-942, 2664-2687` | Tự ensure shell nếu chưa khởi tạo, quét tin nhắn & chèn nháp trả lời qua `mcp_callback` |
| Resources (`tabs`, `tasks`, `skills`) | `self.tabs`, `self.tasks_json`, `self.skills_json` | `src/main.rs:508, 540-545` | Cung cấp qua `resources/list` và `resources/read` |

---

## 2. Kiến trúc 2 lớp không thêm crate ngoài

1. **`src/json_mini.rs`**: Bộ parser & serializer JSON chuẩn RFC 8259 bằng Rust thuần (0 crate ngoài, giữ `TPbrowser.exe` ~1.2 MB).
2. **`src/mcp_server.rs`**:
   - **HTTP Server (`127.0.0.1:17777`)**: `GET /health`, `POST /mcp` (JSON-RPC 2.0: `initialize`, `notifications/initialized`, `ping`, `tools/list`, `tools/call`, `resources/list`, `resources/read`).
   - **Stdio Bridge (`TPbrowser.exe --mcp`)**: Kiểm tra `127.0.0.1:17777/health`; nếu chưa chạy thì tự spawn `TPbrowser.exe` tách tiến trình (detached), đợi tối đa 12s rồi chuyển tiếp JSON-RPC từng dòng giữa `stdin/stdout` và `POST /mcp`.
3. **Hermes & Các AI khác**:
   - Gắn `tpbrowser` vào `C:\Users\thien\.hermes\config.yaml` (kèm bản backup).
   - Cung cấp đoạn JSON chuẩn để dán vào Claude Desktop, Claude Code, Cursor, Antigravity.

---

## 3. Kế hoạch kiểm chứng

1. `cargo test`: kiểm thử `json_mini` + `mcp_server` + `layout` + `main`.
2. `cargo build --release`: 0 errors, 0 warnings.
3. Kiểm chứng thực tế end-to-end với `TPbrowser.exe`:
   - Gọi `GET /health` & `POST /mcp` (`initialize`, `tools/list`, `tp_list_tabs`, `tp_open_tab`, `tp_eval_js`, `tp_get_page_context`, `tp_tasks_list`).
   - Kiểm chứng chạy `TPbrowser.exe --mcp` qua pipe `stdin`/`stdout` (đảm bảo hoạt động dù binary dùng `windows_subsystem = "windows"`).
4. Cập nhật `.ai/STATE.md`, `.ai/MAP.html`, chạy `scripts/generate_checkpoint.py` và `pytest -q tests/test_checkpoint_integrity.py`.
