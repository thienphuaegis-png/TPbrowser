# [E-Commerce Scanner] Single Sequential Pipeline, Live Progress Bar & AI Logger Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use subagent-driven-development or executing-plans.

**Goal:** Giải quyết triệt để 4 vấn đề vận hành quét sàn: Chống đơ toàn bộ trình duyệt bằng cơ chế 1-Worker tuần tự (Single Sequential Pipeline), hiển thị thanh tiến trình trực quan (Mini Progress Bar + Step Label) trên Command Center, nhận diện shop TikTok (1 account 1 shop), và ghi file log chuyên dụng chuẩn hóa tại `user_data/logs/ecom_scanner.log` để AI phân tích gỡ lỗi tức thì.

**Architecture:** 
- Thay thế hoàn toàn cơ chế khởi tạo đồng loạt 5 WebView2 worker tab bằng **Pipeline hàng đợi tuần tự (Sequential Worker)**: Chỉ duy trì đúng 1 Worker Tab duy nhất, tái sử dụng và giải phóng sạch sau khi quét xong.
- Rust Core (`src/main.rs`) cung cấp hàm `log_ecom()` ghi file đĩa `user_data/logs/ecom_scanner.log` có xoay vòng (log rotation).
- Bổ sung bước phát hiện TikTok Shop Identity trên `seller-vn.tiktok.com` trước khi trích xuất Ads.
- Command Center (`src/workspace.html`) bổ sung Mini Progress Bar tinh tế (chuẩn Aurelia UI, không dùng `.innerHTML`), nhận sự kiện IPC `window.onWorkspaceProgress()`.

**Tech Stack:** Rust (Tao, Wry WebView2, std::fs), JavaScript (DOM API Trusted Types, Canvas 2D), Python pytest.

---

## Global Constraints & Strict Guardrails
1. **Bảo toàn 100% Contract Tests (`pytest -q tests/` — 43/43 PASS):** Tuyệt đối không xóa hay đổi tên các ID, thẻ comment, hoặc anchor CSS trong `src/workspace.html`.
2. **Không khóa UI Thread của Tao:** Mọi network call ngoài (Pancake curl) chạy trên background thread (`std::thread::spawn`). Worker WebView2 chỉ mở từng bước tuần tự.
3. **Trusted Types & 100% Offline:** Không dùng `.innerHTML`, không nạp CDN ngoài.
4. **Log Format chuẩn cho AI:** Mọi dòng log tuân thủ cú pháp `[YYYY-MM-DD HH:MM:SS] [LEVEL] [PLATFORM] Message` để AI grep và phân tích tự động.

---

## ĐÁP ÁN ĐÃ CHỐT TỪ BRAINSTORMING

| # | Câu hỏi từ Brainstorming | Đáp án đã chốt từ anh |
|---|---|---|
| 1 | Về thanh hiển thị tiến trình quét (Visual Progress Tracker) | **Phương án A**: Mini Progress Bar và dòng trạng thái chi tiết ngay dưới tiêu đề Dashboard, tự động ẩn/chuyển trạng thái khi xong. |
| 2 | Về số lượng shop TikTok trên tài khoản | **1 tài khoản 1 shop** (nhận diện trực tiếp shop đang đăng nhập, không cần luồng chọn đa shop con). |
| 3 | Về mục đích sử dụng file log | **Phục vụ AI đọc phân tích gỡ lỗi khi anh gửi yêu cầu**, lưu tại `user_data/logs/ecom_scanner.log`, không cần thêm nút UI mở cho người dùng. |

---

## File Structure & Responsibilities

| File | Responsibility |
|---|---|
| `src/main.rs` | Triển khai `log_ecom()`, chuyển `RunEcomScan` sang hàng đợi tuần tự (Sequential Queue), phát hiện TikTok Shop, và gửi tiến độ IPC `onWorkspaceProgress`. |
| `src/workspace.html` | Thêm Mini Progress Bar & Step Label (DOM thuần), hàm `window.onWorkspaceProgress()`. |
| `src/ecom_agents.js` | Điều chỉnh script Shopee để đồng bộ với bước chuyển tuần tự. |
| `tests/test_ecom_logger_and_pipeline.py` | Unit tests kiểm tra file log ghi đúng format và progress payload an toàn. |

---

## Implementation Tasks

### Task 1: Hệ thống ghi file Log chuyên dụng (`user_data/logs/ecom_scanner.log`)
**Files:**
- Modify: `src/main.rs` (thêm hàm `log_ecom(&self, level: &str, tag: &str, msg: &str)`)
- Create: `tests/test_ecom_logger.py`

- [ ] **Step 1: Viết test kiểm tra format file log** trong `tests/test_ecom_logger.py` (kiểm tra regex `^\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\] \[(INFO|SUCCESS|WARN|ERROR)\] \[[A-Z_]+\]`).
- [ ] **Step 2: Triển khai hàm `log_ecom` trong `src/main.rs`**:
  * Tự tạo thư mục `user_data/logs/` nếu chưa có.
  * Append dòng log mới vào `user_data/logs/ecom_scanner.log`.
  * Tự động kiểm tra nếu kích thước file > 3MB thì rotate (đổi tên thành `ecom_scanner.log.old`).
  * Đồng thời đẩy vào mảng `self.ecom_logs` để giữ tương thích ngược với UI cũ.
- [ ] **Step 3: Gắn `log_ecom` vào các điểm trọng yếu**:
  * Khi bắt đầu quét: ghi danh sách shop được chọn.
  * Khi quét Pancake: ghi URL gọi, kết quả HTTP, số đơn, doanh thu, hoặc mã lỗi.
  * Khi quét Shopee: ghi từng bước (`index`, `click_shop`, `orders`, `done`).
  * Khi quét TikTok: ghi URL, kết quả bóc tách, lỗi CAPTCHA hoặc chưa đăng nhập.
- [ ] **Step 4: Chạy `cargo test --bin TPbrowser` và `pytest -q tests/test_ecom_logger.py`** xác nhận PASS.

---

### Task 2: Hàng đợi quét Tuần tự 1-Worker (Single Sequential Pipeline) chống đơ trình duyệt
**Files:**
- Modify: `src/main.rs` (tái cấu trúc `RunEcomScan`, `shopee_shop_scan_queue`, thêm `EcomScanPipeline`)

- [ ] **Step 1: Thiết kế `EcomScanPipeline` trong `src/main.rs`**:
  * Định nghĩa `enum ScanStage { Pancake, ShopeeShop(String, String), TikTok, Done }`.
  * Thay vì gọi `ensure_ecom_worker` cho 5 loại tab cùng lúc, tạo danh sách các chặng cần chạy tuần tự.
- [ ] **Step 2: Xử lý tuần tự từng chặng (Single Worker Reuse)**:
  * **Chặng 1 (Pancake):** Chạy background thread với `curl.exe` (0% WebView CPU). Bắn tiến độ `[1/N] Đang kéo dữ liệu Pancake POS...`.
  * **Chặng 2 (Shopee):** Tạo **duy nhất 1 tab worker** (`ecom_single_worker`). Duyệt lần lượt từng shop con. Quét xong shop 1 thì cập nhật dữ liệu, bắn tiến độ, điều hướng sang shop 2.
  * **Chặng 3 (TikTok):** Tái sử dụng chính tab worker đó, điều hướng sang `seller-vn.tiktok.com`. Quét xong thì bắn tiến độ hoàn thành.
  * **Chặng 4 (Finish):** Đóng/unload tab worker (`unload_tab_by_id`), tạo Snapshot mới, bắn tín hiệu hoàn thành 100%.
- [ ] **Step 3: Thêm cơ chế Timeout Guard**:
  * Nếu một shop bị kẹt mạng hoặc vướng CAPTCHA quá 15 giây, worker tự động ghi log `[WARN] Shop X timeout sau 15s, chuyển sang shop tiếp theo` và nhảy chặng tiếp theo mà không làm treo app.
- [ ] **Step 4: Kiểm tra `cargo test --bin TPbrowser`** xác nhận không đứt gãy IPC.

---

### Task 3: Nhận diện Shop TikTok (1 Account 1 Shop) & Trích xuất dữ liệu
**Files:**
- Modify: `src/main.rs` (cập nhật script bóc tách TikTok Shop `TIKTOK_SHOP_DISCOVERY_JS` và xử lý kết quả)

- [ ] **Step 1: Viết script JS nhận diện TikTok Shop**:
  * Bóc tách tên shop từ thanh header (`.seller-profile-name`, `[class*='shop-name']`, hoặc DOM header của TikTok Seller Center).
  * Kiểm tra trạng thái đăng nhập: nếu có URL `/login` hoặc xuất hiện chữ "Đăng nhập", trả về `{ logged_in: false, need_login: true }`.
  * Bóc tách Shop ID từ URL hoặc thẻ meta/JSON config của trang.
- [ ] **Step 2: Xử lý kết quả trả về trong Rust Core**:
  * Nhận kết quả từ worker TikTok: nếu lấy được tên shop, cập nhật `ShopSelection { platform: "tiktok", shop_id, name }`.
  * Ghi log chi tiết: `[SUCCESS] [TIKTOK] Đã nhận diện shop: [Tên Shop] (ID: ...)`.
  * Nếu cần đăng nhập hoặc dính CAPTCHA: Ghi log `[WARN] [TIKTOK] Cần đăng nhập / Phát hiện CAPTCHA` và thông báo rõ ràng về tiến trình UI.
- [ ] **Step 3: Kiểm tra biên dịch và test TikTok Ads contract hiện hữu** đảm bảo 100% test TikTok vẫn PASS.

---

### Task 4: Giao diện Tiến trình trực quan (Mini Progress Bar & Step Label) trên `workspace.html`
**Files:**
- Modify: `src/workspace.html` (thêm container `#workspace-scan-progress`, hook `window.onWorkspaceProgress`)

- [ ] **Step 1: Thêm HTML element `#workspace-scan-progress` (ẩn mặc định)**:
  * Đặt ngay dưới thanh Topbar / Header của `#workspace-dashboard-view`.
  * Cấu trúc: Thanh progress bar mảnh (chiều cao 3px, màu accent gradient), kèm dòng chữ nhỏ hiển thị: `[Bước 2/4] ⏳ Đang quét Shopee: AuraHome Mall... (50%)`.
  * 100% dùng CSS biến hệ thống, tương thích tự động Light/Dark Mode.
- [ ] **Step 2: Viết hàm `window.onWorkspaceProgress(payload)` trong `src/workspace.html`**:
  * Nhận `{ active: bool, step: number, total: number, percent: number, label: string, status: string }`.
  * Cập nhật độ rộng của progress bar (`width: percent + '%'`).
  * Cập nhật text content (DOM thuần, không dùng `.innerHTML`).
  * Khi `status === "done"`: Chuyển màu thanh bar sang xanh lá `var(--accent-green)`, hiện text `✅ Đã quét xong toàn bộ gian hàng (100%)`, tự mờ dần sau 4 giây.
- [ ] **Step 3: Chạy Node.js V8 validation** để đảm bảo JS không có lỗi cú pháp.
- [ ] **Step 4: Chạy `pytest -q tests/` xác nhận đạt 43/43 PASS (100%)**.

---

### Task 5: Biên dịch Release, Xác thực thực tế & Cập nhật Checkpoint Trio
**Files:**
- Root: `TPbrowser.exe`
- Modify: `.ai/STATE.md`, `.ai/MAP.html`, `.ai/checkpoints/PROJECT_STATE.json`

- [ ] **Step 1: Chạy toàn bộ test suites**:
  * `cargo test --bin TPbrowser` $\rightarrow$ 50/50 PASS.
  * `pytest -q tests/` $\rightarrow$ 43/43 PASS.
  * `pytest -q tests/test_checkpoint_integrity.py` $\rightarrow$ 4/4 PASS.
- [ ] **Step 2: Biên dịch bản Release**: `cargo build --release` và copy sang `./TPbrowser.exe`.
- [ ] **Step 3: Đồng bộ Trio Checkpoint**:
  * Chạy `codegraph sync .`.
  * Cập nhật `.ai/STATE.md` và `.ai/MAP.html`.
  * Chạy `python scripts/generate_checkpoint.py`.
