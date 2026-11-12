# Plan: Tích Hợp Bộ Logo & App Icon Thương Hiệu Cho TPbrowser

**Ngày:** 2026-11-12  
**Trạng thái:** Đã hoàn tất triển khai, build & kiểm chứng sống  
**Mockup so sánh:** `.ai/sketches/2026-11-12-logo-icon-selection-mockup.html`

---

## 1. Đáp án đã chốt qua `clarify`

| # | Câu hỏi | Đáp án đã chốt |
|---|---|---|
| 1 | Chọn bộ Logo & App Icon | **Cài cả 2 bộ, cho phép đổi qua lại trong Cài Đặt (mặc định Bộ 1: Dải lụa 3D `ribbon`)** |
| 2 | Hiển thị đầu Trang Bắt Đầu (`newtab.html`) | **Thay ô `TP` bằng Logo không chữ + giữ dòng chữ serif `TPBROWSER // KHÔNG GIAN CÁ NHÂN`** |
| 3 | Nút nhỏ ở Thanh trên cùng (`titlebar.html` 24px) & Cài Đặt (`settings_modal.html` 28px) | **Dùng App Icon tròn (cân đối, rõ nét ở cỡ nhỏ 24–28px)** |
| 4 | Icon file `TPbrowser.exe` & Icon cửa sổ/Taskbar Windows | **File `.exe` dùng App Icon Tối (`toi1`); icon cửa sổ & Taskbar tự đổi Sáng/Tối theo `ui_theme` & `logo_style`** |

---

## 2. Khảo sát hiện trạng & Xử lý tài nguyên gốc

- **4 ảnh nguồn (`2752×1536` JPG, nền ô caro giả trong suốt + nhãn chú thích bên dưới):**
  - **Bộ 1 (`ribbon`):** `sang1.jpg` (Sáng) & `toi1.jpg` (Tối) — Chữ lồng `TP` dải lụa 3D uốn lượn.
  - **Bộ 2 (`arrow`):** `sang 2.jpg` (Sáng) & `toi 2.jpg` (Tối) — Chữ lồng `TP` mũi tên góc cạnh nghiêng động.
- **Chuẩn hóa tài nguyên sạch (`assets/icons/`):** Tách nền caro 100%, bỏ nhãn chữ, cắt vừa khít vòng tròn App Icon và giữ nguyên dải sáng bạc của bản Sáng:
  - 4 Logo không chữ PNG trong suốt (`160px` — `~21 KB`/file): `logo_ribbon_light.png`, `logo_ribbon_dark.png`, `logo_arrow_light.png`, `logo_arrow_dark.png`.
  - 4 App Icon tròn PNG trong suốt (`128×128` — `~24 KB`/file): `app_ribbon_light.png`, `app_ribbon_dark.png`, `app_arrow_light.png`, `app_arrow_dark.png`.
  - 4 file RGBA thô `64×64` (`16 KB`/file) nhúng `include_bytes!` để `tao::window::Icon::from_rgba` đổi icon cửa sổ/Taskbar tức thì mà **không cần thêm crate ảnh ngoài**.
  - 1 file đa phân giải `assets/icons/tpbrowser.ico` (`16, 24, 32, 48, 64, 128, 256` từ `app_ribbon_dark.png`, `~123 KB`) để `build.rs` đóng gói thành `.res` và nhúng thẳng vào `TPbrowser.exe`.

---

## 3. Bản đồ ánh xạ dữ liệu (Data Mapping)

| Vị trí đích | Hiện trạng code (`file:line`) | Thay đổi dự kiến | Đồng bộ động |
|---|---|---|---|
| **Khóa cấu hình `logo_style`** | *(Chưa có)* trong `TpBrowserApp` (`src/main.rs:580`) & `config.json` | Thêm `logo_style: String` (`"ribbon"` mặc định \| `"arrow"`), lưu/nạp `config.json`, đọc ở `SaveSetting` & `build_config_payload_json` | Có (`tp_config_changed`) |
| **Chọn Bộ Logo trong Cài Đặt** | `pane_look_feel` (`src/settings_modal.html:604-642`) | Thêm hàng chọn **Phong cách Logo TPbrowser** (`Dải Lụa 3D (Bộ 1)` \| `Mũi Tên Góc Cạnh (Bộ 2)`) | Có (lưu ngay, zero-reload) |
| **Icon file `TPbrowser.exe`** | Chưa có `build.rs` / Windows `.rsrc` | Thêm `build.rs` chuyển `assets/icons/tpbrowser.ico` thành `.res` và gắn `cargo:rustc-link-arg` (đã test `rustc` + `ExtractIconExW` ra đủ icon lớn/nhỏ) | Cố định `app_ribbon_dark` theo Q4 |
| **Icon Cửa sổ & Taskbar Windows** | `WindowBuilder::new()` (`src/main.rs:3912`) chưa gọi `.with_window_icon(...)` | Tạo helper `sync_window_icon(&self)` gọi `window.set_window_icon(...)` bằng RGBA `64×64` khớp đúng `logo_style` × `ui_theme` | Tự đổi khi đổi `ui_theme` hoặc `logo_style` |
| **Thanh trên cùng (`24×24px`)** | `src/titlebar.html:60-75, 288` đang là ô `<div class="tb-monogram">TP</div>` | Thay bằng `<img>` **App Icon tròn** theo `logo_style` + `ui_theme` (nhúng base64 qua `/*__TP_BRAND_ASSETS__*/`) | Tự đổi qua `__tpSyncTitlebar` & `tp_config_changed` |
| **Đầu Trang Bắt Đầu (`38px`)** | `src/newtab.html:116-132, 758` đang là ô `<div class="tp-monogram">TP</div>` | Thay bằng `<img>` **Logo không chữ** nền trong suốt, giữ nguyên tiêu đề serif `TPBROWSER // KHÔNG GIAN CÁ NHÂN` + cập nhật `<link rel="icon">` | Tự đổi qua `__tpSyncNewTab` & `tp_config_changed` |
| **Bảng Cài Đặt Zen (`28×28px`)** | `src/settings_modal.html:124-139, 549` đang là `<div class="zs-monogram">TP</div>` | Thay bằng `<img>` **App Icon tròn** theo `logo_style` + `ui_theme` | Tự đổi qua `__tpSyncSettingsModal` & `tp_config_changed` |
| **Trang Lịch Sử (`tp://history`)** | `src/history.html:5` chưa có favicon | Gắn `<link rel="icon">` App Icon đồng bộ theme | Tự đổi khi mở trang |
| **Độ rộng biến `TPbrowser`** | `src/main.rs` (`inject_ui_fonts` dòng `358`) | Bổ sung inject `/*__TP_BRAND_ASSETS_JS__*/` cấp hàm `window.__tpResolveBrandAsset(kind, logoStyle, uiTheme)` dùng chung cho mọi HTML | Dùng 1 nguồn duy nhất, không lặp base64 ở 4 file HTML |

---

## 4. Kế hoạch kiểm chứng sau khi anh duyệt
1. `cargo test`: kiểm tra thêm unit test cho `logo_style` parsing, `assets/icons/tpbrowser.ico`, 4 file RGBA `64×64`, và `inject_ui_fonts` thay thế sạch `/*__TP_BRAND_ASSETS_JS__*/`.
2. `cargo build --release`: biên dịch 0 lỗi, 0 warning; chạy `ExtractIconExW` kiểm tra trực tiếp file `TPbrowser.exe` mới có icon nhúng thật.
3. Chạy sống `TPbrowser.exe`, kiểm chứng qua MCP (`127.0.0.1:17777/mcp`) trên `tp://newtab` và kiểm tra trực quan bằng ảnh chụp nếu cần.
4. Cập nhật `.ai/STATE.md`, `.ai/MAP.html`, `python scripts/generate_checkpoint.py`, `pytest -q tests/test_checkpoint_integrity.py`.
