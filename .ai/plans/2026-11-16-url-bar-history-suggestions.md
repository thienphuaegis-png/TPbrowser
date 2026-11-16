# URL Bar History Suggestions — Implementation Plan

**Goal:** Khi anh focus URL bar và gõ, TPbrowser cung cấp gợi ý từ tab đang mở, bookmark và history thật; Enter vẫn điều hướng/ tìm kiếm theo luồng hiện tại.

**Scope:** Một hành vi hoàn chỉnh trong lượt đầu: dữ liệu gợi ý được đồng bộ từ Rust state sang titlebar; không đổi cơ chế normalize URL, navigation hay route SPA.

| File | Responsibility |
|---|---|
| `src/main.rs` | Thêm `history` vào payload `tb_state` gửi cho titlebar |
| `src/titlebar.html` | Gắn datalist vào URL input và render gợi ý dedupe từ tabs/bookmarks/history |
| `src/main.rs` tests | Kiểm tra payload helper/giữ history JSON hợp lệ qua test hiện có |

## Acceptance

- Mở URL bar, gõ một phần URL/title: Chromium hiển thị gợi ý từ dữ liệu thật đang có.
- Gợi ý không trùng URL và không làm thay đổi `@ai ...` hoặc `navigate_active` hiện tại.
- `cargo test`, `cargo check`, `git diff --check` pass.

## Ngoài phạm vi lượt này

- Không thêm database/schema mới.
- Không đổi ranking/fuzzy search nâng cao.
- Không sửa bookmark folders/tags.
- Không live-claim WebView2 UI trước khi anh test binary.
