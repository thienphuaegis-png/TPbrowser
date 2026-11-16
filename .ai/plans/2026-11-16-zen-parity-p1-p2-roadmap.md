# TPbrowser — Zen parity P1/P2 implementation roadmap

**Nguồn:** buổi rà soát Zen Browser ngày 2026-11-15, session `local-2af93eb53e45bbb4372f3ee2def8b6874c7b57f20eee7dcd87e07876a9e30918`.
**Mục tiêu:** tiếp tục bổ sung các workflow Zen còn thiếu sau khi P0 đã hoàn tất; không trộn với backlog Shopee/TikTok.

## Trạng thái hiện tại

| Nhóm | Trạng thái | Bằng chứng |
|---|---|---|
| Workspaces thật | Đã làm P0 | `src/main.rs`, `src/tab_shell.html`, unit tests |
| Essentials/Pinned Tabs | Đã làm P0 | `src/main.rs`, persistence/session tests |
| Container Tabs | Đã làm P0 | container identity/profile mapping + tests |
| Glance preview | Đã làm code, live pass còn thiếu | `src/content_agent.js`, `src/glance_header.html`; chưa xác nhận Alt+Click live ổn định |
| Window Sync & Recovery | Chưa lập card implementation | Chưa có task P1 riêng |
| Command Bar | Chưa làm | Chưa có task/card riêng |
| Recently-used Tab Switcher | Chưa làm | Chưa có task/card riêng |
| URL bar nâng cao | Chưa làm đầy đủ | Chưa có task/card riêng |
| Bookmark folders/tags/keywords | Chưa làm | Chưa có task/card riêng |
| Split View nâng cao | Mới có BSP cơ bản | Cần đối chiếu với Zen: chọn tab đích danh, layout, unsplit |
| Downloads / PiP / Reader / Translation | Chưa đánh giá khả năng WebView2 và chưa làm | Cần feasibility card riêng |
| Spaces routing / Boosts / Mods | Chưa làm | Cần thiết kế riêng, không gộp vào browser core |

## Thứ tự triển khai bắt buộc

Mỗi card chỉ có một hành vi người dùng hoàn chỉnh, tối đa 2–4 file source và 1–3 nhóm test.

1. **Window Sync & Recovery** — khôi phục cửa sổ/tab sau crash, resize/display change, single-instance wake; không làm command bar cùng lượt.
2. **Command Bar** — một command palette mở bằng phím tắt riêng, tìm lệnh/tab/workspace; không làm URL bar nâng cao cùng lượt.
3. **Recently-used Tab Switcher** — Ctrl+Tab/Alt+Tab theo MRU, giữ mapping native index/visible index.
4. **URL bar nâng cao** — omnibox history/search/suggestions và route SPA; tách khỏi command bar.
5. **Bookmark folders/tags/keywords** — model + persistence trước, UI quản lý sau.
6. **Split View nâng cao** — chọn đúng hai tab, 2 cột/2 hàng/spiral, unsplit và ẩn widget trùng.
7. **Downloads/PiP/Reader/Translation feasibility** — khảo sát API WebView2 trước, chỉ triển khai capability có bằng chứng.
8. **Spaces routing/Boosts/Mods** — phase cuối, không giả lập Firefox extension ecosystem.

## Ngoài phạm vi lượt đầu

- Không sửa Shopee/TikTok/E-commerce trong các card Zen parity này.
- Không đánh dấu Glance done lần nữa; chỉ tạo card live verification riêng nếu cần.
- Không tạo một card lớn kiểu “làm toàn bộ Zen parity”.
- Không commit/push/reset/stash working tree.

## Acceptance của phase kế tiếp

Card đầu tiên phải trả lời được: bấm/trigger Window Sync & Recovery trong một tình huống mô phỏng → trạng thái cửa sổ/tab được khôi phục đúng → MCP/state payload phản ánh đúng → test pass → release build không lỗi.
