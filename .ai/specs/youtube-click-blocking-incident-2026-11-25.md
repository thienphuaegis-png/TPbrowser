# Báo cáo lỗi YouTube: click không phản hồi và log quảng cáo bị chặn

- **Ngày:** 2026-11-25
- **Ứng dụng:** TPbrowser.exe
- **Trang liên quan:** YouTube
- **Trạng thái:** Đã khoanh vùng; cần tiếp tục đối chiếu trên thao tác thực tế của người dùng nếu lỗi tái diễn.

## 1. Triệu chứng anh gặp

1. DevTools liên tục ghi nhiều dòng:

   ```text
   Failed to load resource: net::ERR_BLOCKED_BY_CLIENT
   ```

2. Các URL xuất hiện trong log gồm:

   - `youtube.com/youtubei/v1/log_event`
   - `youtube.com/api/stats/qoe`
   - `youtube.com/ptracking?html5=1...`
   - `google.com/pagead...`
   - `google.com.vn/pagead...`
   - `generate_204?...`

3. DevTools ghi lặp lại cảnh báo:

   ```text
   Blocked script execution in 'about:blank' because the document's frame is sandboxed and the 'allow-scripts' permission is not set.
   ```

4. Anh báo click vào thumbnail/video không phản hồi; hiện tượng được mô tả là gần như mọi click trên YouTube đều không ăn.

## 2. Phân loại nguyên nhân

### 2.1. `ERR_BLOCKED_BY_CLIENT`

Đây là kết quả của bộ lọc quảng cáo/privacy chặn request quảng cáo hoặc tracking. Nó **không tự động có nghĩa là YouTube bị hỏng**.

Các request telemetry của YouTube đã được allow riêng trong ruleset nội bộ:

- `/youtubei/v1/log_event`
- `/api/stats/qoe`
- `/ptracking`

Các endpoint quảng cáo như `pagead` và `generate_204` vẫn bị chặn có chủ đích để không bật lại quảng cáo/tracking.

### 2.2. Cảnh báo sandbox `about:blank`

Đây là iframe phụ do YouTube/Google tạo cho lớp quảng cáo hoặc tracking. Frame bị sandbox nhưng không có quyền `allow-scripts`. Không được mở `allow-scripts` toàn cục vì sẽ làm yếu sandbox bảo mật và có thể cho quảng cáo chạy lại.

### 2.3. Click không phản hồi

Giả thuyết có cơ sở nhất là một iframe/lớp phủ lớn của quảng cáo vẫn nằm trên nội dung YouTube và nhận pointer event, dù nội dung quảng cáo đã bị chặn.

Đã thêm xử lý riêng cho YouTube:

- Tìm iframe/layer lớn `fixed` hoặc `absolute`.
- Chỉ xử lý layer trong suốt, `about:blank`, hoặc lớp z-index cao.
- Đặt `pointer-events: none` cho lớp phủ nghi ngờ.
- Bỏ qua `#movie_player`, `#masthead` và `ytd-app`.
- Không whitelist quảng cáo.

## 3. Thao tác xác minh đã thực hiện

- Restart TPbrowser sau khi build.
- Capture trạng thái trước và sau click.
- Click thử thumbnail YouTube bằng thao tác desktop.
- Xác minh một lần click đã chuyển từ trang chủ sang trang video.
- Xác minh video tải và phát; accessibility hiển thị nút `Phát / Tạm dừng`.
- Chạy contract/checkpoint tests.

Kết quả test gần nhất:

- Contract + checkpoint tests: **39 passed**
- Checkpoint integrity: **4 passed**
- `node --check src/content_agent.js`: pass
- Ruleset JSON: hợp lệ
- `git diff --check`: pass
- Release build: pass

## 4. Trạng thái hiện tại

- Bộ chặn quảng cáo cũ vẫn giữ nguyên.
- Heuristic betting động và whitelist thử nghiệm trước đó đã rollback.
- Chỉ còn allowlist telemetry YouTube cần thiết để giảm lỗi `ERR_BLOCKED_BY_CLIENT`.
- `pagead`, `generate_204` và các request quảng cáo khác vẫn bị chặn.
- Một lần live test trong TPbrowser đã click được và phát video.
- Tuy nhiên, vì anh vẫn báo lỗi khi thao tác bằng chuột thật, cần đối chiếu thêm đúng cửa sổ đang nhận focus và đúng video/URL tái hiện lỗi.

## 5. Cách tái hiện cần ghi lại nếu lỗi còn

Khi lỗi xuất hiện lại, cần ghi đủ:

1. URL đầy đủ của video.
2. Cửa sổ đang foreground là TPbrowser hay DevTools.
3. Click vào thumbnail, tiêu đề, nút Play hay vùng player.
4. Sau click URL có đổi không.
5. Video có hiện quảng cáo đầu video hay đứng trắng.
6. Chụp ảnh TPbrowser sau click và DevTools ngay sau cùng thời điểm.

## 6. Không nên làm

- Không allow toàn bộ `google.com`, `google.com.vn` hoặc `youtube.com` chỉ để xóa log đỏ.
- Không bật `allow-scripts` cho mọi iframe `about:blank`.
- Không tắt toàn bộ uBlock chỉ vì DevTools hiển thị `ERR_BLOCKED_BY_CLIENT`.
- Không dùng heuristic ẩn mọi phần tử `fixed/absolute`, vì có thể làm hỏng player và giao diện YouTube.
