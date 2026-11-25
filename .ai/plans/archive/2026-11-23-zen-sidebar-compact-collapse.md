# Zen-style Sidebar Compact Collapse

**Goal:** Kéo splitter sidebar dọc xuống dưới ngưỡng 60px sẽ chuyển sang Compact Mode thay vì bị kẹp ở 140px; sidebar vẫn hiện lại khi rê vào mép trái và tự ẩn khi rời ra.

**Architecture:** Giữ state Compact Mode hiện có trong `TpBrowserApp`, watcher Windows hiện có phát hiện hover mép trái, và bổ sung ngưỡng collapse tại native resize loop. Không đổi shortcut, workspace, tab model hay visual identity AURELIA.

## Mapping đã xác nhận

| Thành phần | Hiện trạng thật | File |
|---|---|---|
| Resize sidebar | `StartSidebarResize` đo con trỏ, clamp 140..440, gửi `SetSidebarWidth` | `src/main.rs:8522-8573` |
| Compact Mode | `ToggleCompactMode` bật state + `COMPACT_MODE_ENABLED`, relayout | `src/main.rs:10894-10907` |
| Reveal khi hover | Windows watcher gửi `SetCompactSidebarHover` ở mép trái, giữ sidebar mở 260ms sau khi rời | `src/main.rs:2889-2967` |
| Layout overlay | Compact + hover đặt tab shell width 180..440, không hover thì ẩn WebView | `src/main.rs:4132-4171` |
| Width persistence | `sidebar_width` lưu config, mặc định 228, giới hạn 140..440 | `src/main.rs:3121-3125`, `src/main.rs:4666-4674` |
| Test contracts | Có titlebar/sidebar shell contract; chưa có assertion cho collapse threshold | `tests/test_titlebar_split_contract.py` |

## Scope boundary

- Chỉ sửa `src/layout.rs`, `src/main.rs`, `tests/test_titlebar_split_contract.py`.
- Không sửa workspace, AI dock, tab ordering, shortcut, màu, logo hoặc config schema.
- Không commit/push.

## Tasks

- [ ] Thêm hằng số `SIDEBAR_COLLAPSE_THRESHOLD` = 60px trong layout module và import vào native shell.
- [ ] Trong resize loop, nếu vị trí con trỏ xuống dưới threshold thì gửi `ToggleCompactMode`, không gửi width 140px và kết thúc drag; Compact Mode tự lưu config và dùng watcher hiện có để reveal.
- [ ] Thêm contract test chứng minh threshold, command path và watcher/reveal path cùng tồn tại.
- [ ] Chạy focused pytest, `cargo test --quiet`, `git diff --check`, sau đó `cargo build --release`.
- [ ] Sync `.ai/STATE.md`, `.ai/MAP.html`, checkpoint integrity sau khi verification pass.

## Acceptance

1. Resize sidebar xuống `< 60px` chuyển `compact_mode=true` và không lưu width 140px như một sidebar thu nhỏ giả.
2. Trong Compact Mode, rê vào mép trái gửi `SetCompactSidebarHover(true)` và tab shell hiện lại; rời mép đủ thời gian gửi `false` và ẩn lại.
3. Resize ở trên threshold vẫn giữ behavior hiện tại, clamp width 140..440 và lưu `sidebar_width`.
4. Test/build thực tế pass.
