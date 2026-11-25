# Custom Keyboard Shortcuts — TPbrowser

## Phạm vi lượt này
- Thêm chỉnh sửa phím tắt cho các lệnh riêng của TPbrowser/Zen: Compact Mode, Command Bar, Settings, Glance, Split View, Space trước/sau, Copy URL, AI panel, Sidebar collapse.
- Không thay đổi shortcut chuẩn trình duyệt (`Ctrl+T`, `Ctrl+W`, `Ctrl+P`, `Ctrl+L`...).

## Acceptance
1. Settings > Shortcuts hiển thị ô chỉnh sửa thay vì badge tĩnh cho các lệnh riêng.
2. Nhập tổ hợp bằng phím thật, chuẩn hóa modifier + key, báo trùng với lệnh khác và cho phép xóa/reset mặc định.
3. Lưu vào `config.json`, khôi phục sau restart, phát sự kiện sync UI.
4. Native keyboard dispatcher đọc map đã lưu và thực thi đúng AppCmd; shortcut xung đột bị từ chối fail-closed.

## Giới hạn
- Không làm Window Sync, Container UX hay Compact Mode mở rộng trong lượt này.
- Không đọc/ghi secret; không đụng `user_data/`.

## Verification
- `cargo test`
- focused browser/contract/checkpoint pytest
- `cargo build --release`
