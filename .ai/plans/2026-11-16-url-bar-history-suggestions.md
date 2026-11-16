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

## Next after live verification

1. **URL bar phase 2:** fuzzy ranking and keyboard-selected suggestion rows, while preserving `@ai ...` and direct URL/search normalization.
2. **Bookmarks folders/tags/keywords:** model + persistence first, then management UI; do not mix with split view.
3. **Split View nâng cao:** chọn đúng hai tab, 2 cột/2 hàng/spiral, unsplit and duplicate-widget suppression.
4. **Feasibility riêng:** Downloads/PiP/Reader/Translation trên WebView2; chỉ code capability có API evidence.

Tomorrow's first test is the current binary: focus URL bar, type a known part of a tab/bookmark/history URL, choose a suggestion with Enter, then verify `@ai ...` still dispatches as an AI prompt.