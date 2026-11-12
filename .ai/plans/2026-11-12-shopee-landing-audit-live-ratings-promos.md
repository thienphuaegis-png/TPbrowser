# Kế Hoạch Nâng Cấp Màn Hình TMĐT (`tp://workspace`) Dạng Landing Page + Kiểm Toán Toàn Diện Shopee

**Ngày:** 2026-11-12
**Trạng thái:** ĐÃ CHỐT — ĐANG TRIỂN KHAI

## 1. Lỗi Ngầm Hiện Tại Đã Phát Hiện Khi Probe API Thật (`banhang.shopee.vn`)

| Lỗi hiện tại (`src/ecom_agents.js`) | Nguyên nhân thật trên API Shopee v3 | Hậu quả thực tế |
|---|---|---|
| `order_total = 0`, `item_price = 0`, `anomalies = []` | `get_order_list_card_list` trả tổng tiền tại `c.payment_info.total_price` (`micro-VND / 100.000`), không phải `c.total_price` | Toàn bộ đơn hiển thị 0đ nên không phát hiện được đơn giá bất thường (ví dụ đơn `26111254VBFBMJ` Cần câu mèo `13.000đ`) |
| `to_ship = 323,215` | `order_list_tab: 100` là **Tất cả**, không phải Chờ lấy hàng (`300` mới là Chờ lấy hàng `~116`, `400` là Đang giao `~2.750`, `200` là Chờ xác nhận `11`) | Số liệu Chờ lấy hàng bị cộng thành tổng lịch sử toàn shop |
| `buyer = "Khách Shopee"`, `status = ""`, `shipping = ""` | Trường thật lần lượt là `card_header.buyer_info.username`, `status_info.status`, `fulfilment_info.masked_channel_name` | Bảng đơn hàng và CSV xuất thiếu tên khách, trạng thái và kênh vận chuyển |

## 2. Bản Đồ Ánh Xạ Dữ Liệu Thật (Data Mapping — Đã Kiểm Chứng 100% Trên `banhang.shopee.vn`)

| Hạng mục | Endpoint API thật trên Worker Shopee (`credentials: "include"`) | Trường JSON thực tế |
|---|---|---|
| Đơn hàng & Giá bất thường | `POST /api/v3/order/search_order_list_index` + `get_order_list_card_list` (`tab 200, 300, 400, 500`) | `payment_info.total_price`, `card_header.buyer_info.username`, `status_info.status`, `fulfilment_info.masked_channel_name`, `item_info_group` |
| Đánh giá 1★ / 2★ / 3★ + Cảnh báo hàng giả/nhái | `GET /api/v3/settings/get_rating_dashboard/` + `search_shop_rating_comments_new/?rating_star=1,2,3&page_size=40` | `counts.{one,two,three}_star_count`, `to_reply_count`, `comment`, `rating_star`, `user_name`, `order_sn`, `product_name`, `model_name`, `ctime`, `reply`, `low_rating_reasons` |
| Shopee Live | `GET /api/supply/lm/sellercenter/liveList/v2?page=1&pageSize=10&timeDim=7d&endDate=YYYY-MM-DD` | `status` (`1` Đang Live / `2` Đã kết thúc), `title`, `startTime`, `duration`, `viewers`, `avgEngagedCcu`, `atc`, `placedOrders`, `confirmedOrders`, `confirmedSales` |
| Khuyến mãi của Shop (Giảm giá / Combo / Deal sốc) | `POST /api/marketing/v3/public/discount/list/` (`{offset:0, limit:30, discount_type:0, time_status:2|1}`) | `discounts[].{discount_type, seller_discount, add_on_deal, bundle_deal}` (`name`, `start_time`, `end_time`, `time_status`, `item_preview.item_count`) |
| Mã Giảm Giá của Shop (Voucher) | `GET /api/marketing/v3/voucher/list/?offset=0&limit=30&promotion_type=2` (Đang chạy) & `promotion_type=1` (Sắp chạy) | `voucher_list[].{name, voucher_code, start_time, end_time, discount, value, min_price, max_value, current_usage, usage_limit, fe_status}` |
| Flash Sale của Shop | `GET /api/marketing/v4/shop_flash_sale/get_shop_flash_sale_list/?offset=0&limit=30&type=2|1|0` | `flash_sale_list[].{flash_sale_id, start_time, end_time, enabled_item_count, item_count, status, type}` |

## 3. Quyết Định Đã Chốt (Qua `clarify`)
| # | Hạng mục | Đáp án đã chốt |
|---|---|---|
| 1 | Độ sâu quét | Quét sâu: **200 đơn mới nhất** + **200 đánh giá 1★–3★ gần nhất** |
| 2 | Bố cục & Báo cáo | Giao diện **Landing Page** AURELIA (Sáng/Tối đồng bộ theme app) + tự động gom Cảnh báo Hàng giả/Nhái, Đơn giá bất thường, Live, Flash Sale vào nút **Copy Báo Cáo** |
| 3 | Khuyến mãi & Tương tác | Hiện cả **Đang diễn ra + Sắp diễn ra**, tô nổi bật chương trình **sắp hết hạn trong 24h**, và **click vào mục nào sẽ mở thẳng tab làm việc trực tiếp của mục đó** |
