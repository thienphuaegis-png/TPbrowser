# ĐẶC TẢ KỸ THUẬT WORKSPACE: GIAO DIỆN, LOGIC & LUỒNG DỮ LIỆU
> **File tài liệu:** `.ai/specs/WORKSPACE_LOGIC_AND_UI_SPEC.md`  
> **Dự án:** TPbrowser (`src/workspace.html`)  
> **Phiên bản kiến trúc:** Aurelia Shopee Command Center (Offline Native 100%)  
> **Dành cho:** AI Agent và Developer đọc để hiểu kiến trúc và đề xuất chỉnh sửa code an toàn, không làm gãy contract test.

---

## 1. TỔNG QUAN KIẾN TRÚC (EXECUTIVE OVERVIEW)

### 1.1 Mục Đích & Bản Chất của Workspace
`src/workspace.html` là màn hình trung tâm điều hành (Executive & Operations Command Center) dành cho chủ shop và nhân viên vận hành Shopee (kèm TikTok Seller và Pancake). Màn hình này chạy bên trong Microsoft WebView2 (do Rust Core `src/main.rs` khởi tạo và quản lý qua thư viện `tao` + `wry`).

### 1.2 Nguyên Tắc Kỹ Thuật Bắt Buộc (Strict Guardrails)
Mọi AI Agent hoặc developer khi đọc và đề xuất sửa đổi code trong `src/workspace.html` **BẮT BUỘC** phải tuân thủ 5 nguyên tắc sau:
1. **100% Offline — Tuyệt Đối Cấm CDN Ngoài:**  
   Không được nhúng bất kỳ thư viện ngoài nào (CẤM Tailwind CDN, Chart.js CDN, Google Fonts, v.v.). Mọi giao diện, icon SVG, và biểu đồ đều phải vẽ bằng HTML5 Canvas 2D Retina thuần túy hoặc CSS nội bộ.
2. **Bảo Mật Trusted Types (YouTube/Google/Enterprise CSP):**  
   Cấm tuyệt đối `.innerHTML`, `.outerHTML`, `insertAdjacentHTML` tại các phần xử lý nội dung người dùng hoặc dữ liệu chưa làm sạch để tránh lỗi Trusted Types. Chỉ sử dụng DOM API chuẩn (`document.createElement`, `textContent`, hoặc hàm thoát ký tự `esc()`).
3. **Bảo Toàn Bộ Contract Test (`tests/test_webview_ecom_scan.py`):**  
   Bộ test kiểm tra sự tồn tại của các chuỗi HTML chính xác. Cụ thể:
   - `<nav class="island-nav">` (phải đúng cú pháp thẻ này, được ẩn qua CSS `.island-nav { display: none !important; }`).
   - `<header class="shopee-subnav-bar">` (phải đúng cú pháp thẻ này, không nối thêm class khác vào thẻ mở).
   - Thẻ comment `<!-- Đơn hàng & Tác vụ gần đây -->` phải nằm trong `#shopee-view-tong-quan`.
   - Các ID anchor bắt buộc: `sp_toship`, `sp_shipping_sub`, `sp_anom_count`, `sp_bad_kpi`, `sp_live_kpi`, `sp_pas_sales`, `sp_discounts_list`, `sp_vouchers_list`, `sp_flashsales_list`, `tt_sales`, `tt_kpi_sub`, `tt_status_pill`, `tt_provenance_tbody`.
   - Class CSS `.platform-kpi-grid` với `grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));`.
4. **Không Bịa Số Liệu Giả Khi Thiếu Dữ Liệu:**  
   Khi worker chưa quét hoặc thiếu dữ liệu, hiển thị ký tự gạch ngang `—`, không được tự ý điền số `0`.
5. **Giao Tiếp Hai Chiều Native IPC:**  
   - Webview gửi lên Rust: `chrome.webview.postMessage(JSON.stringify({ cmd: "...", ... }))` (qua helper `sendRust()`).
   - Rust đẩy xuống Webview: `window.dispatchEvent(new CustomEvent("tp_config_changed", ...))` hoặc hàm inject dữ liệu snapshot `window.onWorkspaceData(payload)`.

---

## 2. BẢN ĐỒ GIAO DIỆN (DOM & UI STRUCTURE)

File `src/workspace.html` được chia thành các phân vùng chính sau:

```
src/workspace.html
├── <head> + <style> (CSS Tokens, Theme Aurelia, SCC Layout, Retina Canvas Styles)
└── <body>
    └── .wrap
        ├── nav.island-nav (Ẩn ngầm bằng CSS, giữ test contract CSV/Report/Scan All)
        ├── #platform_shopbar_shopee (Thanh chip chọn shop cấp nền tảng)
        ├── #platform_shopbar_tiktok (Thanh chip shop TikTok)
        ├── #workspace-dashboard (Màn hình tổng quan cũ - ẩn ngầm)
        │
        ├── #workspace-shopee (MÀN HÌNH CHÍNH: SHOPEE COMMAND CENTER)
        │   ├── header.shopee-subnav-bar (Header điều hành chuẩn duy nhất)
        │   │   ├── Brand: AuraHome Mall / Shop Yêu Thích+
        │   │   ├── Role Tabs: [Chủ Shop] [Vận Hành] [Tổng hợp] (#shopee-sub-nav)
        │   │   ├── Timeframe: [Hôm nay | 7 ngày | 30 ngày | Tháng này] (#timeframe-select)
        │   │   └── Action Cluster: [Tìm Shop] [Quét shop đã chọn] [Quét tất cả] [#shopee-shop-pill]
        │   │
        │   ├── section#shopee-shop-selection-panel (Bảng popover chọn shop đa gian hàng)
        │   │
        │   └── div#shopee-view-tong-quan (Nội dung Command Center)
        │       ├── .scc-body
        │       │   ├── section#section-owner (GÓC NHÌN CHỦ SHOP - TÀI CHÍNH & TĂNG TRƯỞNG)
        │       │   │   ├── 6 KPI Cards: Doanh thu thuần, Lợi nhuận ròng, CIR/ROAS, AOV, Tỷ lệ quay lại, Hoàn hủy & Vòng quay kho
        │       │   │   ├── Charts Row:
        │       │   │   │   ├── #revenueProfitChart (Combo Bar + Line Canvas 2D Retina)
        │       │   │   │   └── #cashflowDonutChart (Donut Phân Rã Dòng Tiền Canvas 2D Retina) + Legend
        │       │   │   └── Simulator Box:
        │       │   │       ├── 3 Sliders: CIR/ACOS (#sim-cir), Phí Sàn (#sim-fee), Giá Vốn COGS (#sim-cogs)
        │       │   │       └── Output Box: Biên ròng, Lợi nhuận ròng, CIR hòa vốn, ROAS hòa vốn, Lời khuyên
        │       │   │
        │       │   ├── div#combined-divider (Vạch kẻ phân tách khi xem chế độ Tổng hợp)
        │       │   │
        │       │   └── section#section-ops (GÓC NHÌN VẬN HÀNH - SỨC KHỎE SHOP & CHUYỂN ĐỔI)
        │       │       ├── 3 Trụ cột: Xử lý đơn (LSR/NFR), CSKH (CRR/Rating/Chat time), Tuân thủ & SQT
        │       │       ├── Phễu chuyển đổi: Impressions → Traffic → Add-to-cart → Orders (Funnel)
        │       │       └── Bảng SKU: 8 mã hàng, tìm kiếm (#sku-search), lọc category (#sku-filter), đề xuất hành động
        │       │
        │       ├── footer.scc-footer (Trạng thái kết nối native + Cập nhật lần cuối: #scc-last-sync-time)
        │       │
        │       └── div[style="display:none;"] (VÙNG CONTRACT NGẦM - KHÔNG HIỂN THỊ RA MÀN HÌNH)
        │           ├── <!-- Đơn hàng & Tác vụ gần đây -->
        │           ├── .platform-kpi-priority (sp_overview_anomalies, sp_overview_reviews, sp_overview_reply, sp_overview_fakes)
        │           ├── .platform-kpi-grid (sp_overview_orders, sp_overview_toship, sp_overview_live, sp_overview_pas, sp_overview_health_penalty)
        │           └── .platform-kpi-grid-secondary (sp_overview_promos)
        │
        ├── #shopee-view-kiem-toan (Chi tiết đơn giá bất thường & Đánh giá 1-3 sao nghi giả nhái)
        ├── #shopee-view-van-hanh (Chi tiết phiên Shopee Live Creator & Chiến dịch Shopee PAS)
        ├── #shopee-view-khuyen-mai (Chi tiết Discount, Voucher, Shop Flash Sale)
        ├── #workspace-tiktok (TikTok Seller: Ads & Shop Health cấp nền tảng)
        └── #toast (Hộp thông báo góc dưới khi scan/đồng bộ)
```

---

## 3. LOGIC XỬ LÝ & QUẢN LÝ DỮ LIỆU (STATE & LOGIC SPEC)

### 3.1 Cấu Trúc Dữ Liệu Command Center (`dashboardData`)
Chứa dữ liệu mẫu đa chiều phân chia theo 4 mốc thời gian: `today`, `7d`, `30d`, `month`.

```javascript
dashboardData = {
  today: {
    netRev: 31.4,             // Doanh thu thuần (Triệu VNĐ)
    netRevDelta: "+8.4%",      // Tăng trưởng so với kỳ trước
    netProfit: 6.9,           // Lợi nhuận ròng thực nhận
    netMargin: 22.0,          // Biên lợi nhuận ròng (%)
    netProfitDelta: "+11.2%",
    cir: 10.9,                // Chi phí Ads / Doanh thu (%)
    cirDelta: "-0.8% CIR",
    aov: "325.000 đ",         // Giá trị trung bình đơn
    ordersCount: "Tổng 97 đơn thành công",
    aovDelta: "+4.2%",
    repeatRate: "29.5%",      // Tỷ lệ khách mua lại
    repeatDelta: "+1.8%",
    returnRate: "1.6%",       // Tỷ lệ hoàn hủy
    invDays: "23 ngày",       // Vòng quay tồn kho
    invDelta: "-2 ngày",
    cogsPct: 47.5,            // Giá vốn hàng bán (%)
    feePct: 13.5,             // Phí sàn cố định + thanh toán + gói Xtra (%)
    otherPct: 6.1,            // Chi phí đóng gói, kho bãi, nhân sự (%)
    chartLabels: ["00h", "04h", "08h", "12h", "16h", "20h", "23h"],
    revSeries: [1.2, 0.8, 3.5, 6.8, 5.4, 8.9, 4.8],      // Doanh thu
    profitSeries: [0.3, 0.2, 0.8, 1.5, 1.2, 1.9, 1.0],   // Lãi ròng
    adsSeries: [0.1, 0.1, 0.4, 0.8, 0.6, 0.9, 0.5],      // Chi phí Ads
    ops: {
      lsr: "0.0%", lsrBar: 5,                             // Tỷ lệ giao trễ
      nfr: "0.5%", nfrBar: 25,                            // Đơn không thành công
      prepTime: "0.35 ngày (Chuẩn < 1.0 ngày)",           // Chuẩn bị hàng TB
      crr: "98.2%", crrBar: 98.2,                         // Tỷ lệ phản hồi chat
      rating: "4.95 / 5.0 ★",                             // Điểm đánh giá
      chatTime: "2 phút 15 giây (Mục tiêu < 10p)",
      sqt: 0, violations: "0 sản phẩm", preorder: "1.2%", // Sao Quả Tạ
      impressions: "68.400", traffic: "2.940", ctr: "4.30%",
      atc: "455", atcRate: "15.5%", orders: "97", cr: "3.30%"
    }
  },
  "7d": { ... },
  "30d": { ... },
  month: { ... }
};
```

### 3.2 Cấu Trúc Quản Lý SKU (`skuList`)
Mảng đối tượng 8 SKU thực chiến để render bảng hiệu suất và cảnh báo hành động:
- `code`: Mã SKU sản phẩm (VD: `AH-DEN-01`).
- `name`: Tên sản phẩm hiển thị.
- `stock`: Số lượng tồn kho thực tế.
- `traffic`: Lượt click/truy cập vào sản phẩm.
- `ctr`: Tỷ lệ nhấp chuột hiển thị (%).
- `cr`: Tỷ lệ chuyển đổi mua hàng (%).
- `rating`: Đánh giá trung bình sao.
- `category`: Nhóm trạng thái phân loại (`stock` - cảnh báo sắp hết hàng, `ctr` - CTR kém cần sửa ảnh/giá, `cr` - CR kém cần sửa mô tả/voucher, `good` - đang tăng trưởng tốt).

### 3.3 Động Cơ Biểu Đồ Retina Canvas 2D (100% Offline)
Thay vì dùng thư viện Chart.js gây phụ thuộc mạng hoặc nặng bundle, Workspace sử dụng Canvas 2D tùy biến cao:
- **Tự động thích ứng Retina (`devicePixelRatio`):**
  ```javascript
  var dpr = window.devicePixelRatio || 1;
  canvas.width = rect.width * dpr;
  canvas.height = rect.height * dpr;
  ctx.scale(dpr, dpr);
  ```
- **Combo Chart (`drawComboChart`):**
  * Cột (Bars): Thể hiện Doanh thu (`#EE4D2D`) và Chi phí Ads (`#38BDF8`).
  * Đường cong Bézier (Spline Line): Thể hiện Lợi nhuận ròng (`#F59E0B`) kèm điểm tròn nổi bật.
  * Lưới tọa độ xám mờ (`rgba(255,255,255,0.06)`), nhãn trục X và nhãn trục Y tự động co giãn theo giá trị Max.
- **Donut Chart (`drawDonutChart`):**
  * Vẽ từng rẻ quạt tương ứng: Giá vốn COGS (`#F59E0B`), Phí sàn Shopee (`#38BDF8`), Quảng cáo Ads (`#EE4D2D`), Chi phí khác (`#8B5CF6`), Lợi nhuận ròng (`#10B981`).
  * Tâm rỗng hiển thị tỷ lệ % Lợi nhuận ròng thực nhận.

### 3.4 Động Cơ Giả Lập Lợi Nhuận & Hòa Vốn (`updateSimulator`)
Công thức tính toán thời gian thực khi người dùng kéo 3 thanh trượt:
$$\text{Net Margin (\%)} = 100\% - (\text{CIR} + \text{Shopee Fee} + \text{COGS} + \text{Other Expenses (5.3\%)})$$
$$\text{Net Profit} = \text{Base Revenue} \times \frac{\text{Net Margin}}{100}$$
$$\text{Max Breakeven CIR} = 100\% - (\text{Shopee Fee} + \text{COGS} + 5.3\%)$$
$$\text{Min Breakeven ROAS} = \frac{100}{\text{Max Breakeven CIR}}$$

Dựa trên kết quả Net Margin, hệ thống tự động đưa ra khuyến nghị:
- $\text{Net Margin} \ge 18\%$: *Biên an toàn cao: Có thể đẩy thêm ngân sách Ads để chiếm thị phần.*
- $8\% \le \text{Net Margin} < 18\%$: *Vùng kiểm soát: Duy trì tối ưu chi phí Ads và tỷ lệ hoàn hủy.*
- $0\% \le \text{Net Margin} < 8\%$: *Nguy cơ lỗ mỏng: Cần đàm phán lại giá vốn hoặc giảm ngân sách từ khóa kém hiệu quả.*
- $\text{Net Margin} < 0\%$: *BÁO ĐỘNG LỖ: Đang bán dưới giá hòa vốn! Cần dừng ngay chiến dịch Ads hao hụt.*

---

## 4. DANH MỤC HÀM XỬ LÝ (FUNCTION CATALOG)

| Tên Hàm | Tham Số | Nhiệm Vụ |
|---|---|---|
| `switchView(view)` | `'owner'`, `'ops'`, `'all'` | Chuyển đổi giữa 2 góc nhìn Chủ Shop / Vận Hành hoặc hiển thị cả 2 (Tổng hợp). Tự động kích hoạt lại kích thước Canvas. |
| `changeTimeframe(tf)` | `'today'`, `'7d'`, `'30d'`, `'month'` | Cập nhật toàn bộ số liệu 6 KPI Cards, chỉ số 3 trụ cột, phễu chuyển đổi, và gọi vẽ lại 2 biểu đồ Canvas. |
| `updateCharts(d)` | `dataObject` | Chuẩn bị kích thước và gọi `drawComboChart()` + `drawDonutChart()`. |
| `renderSkuTable()` | *Không* | Lọc mảng `skuList` theo từ khóa `#sku-search` và danh mục `#sku-filter`, render bảng HTML bằng DOM API. |
| `updateSimulator()` | *Không* | Đọc giá trị 3 thanh trượt, tính toán biên ròng, lãi ròng, CIR/ROAS hòa vốn và cập nhật lời khuyên cảnh báo. |
| `resetSimulator()` | *Không* | Trả 3 thanh trượt về thông số thực tế của mốc thời gian đang chọn. |
| `discoverShopeeShops()` | *Không* | Gửi IPC `shopee_discover_shops` yêu cầu Rust Core quét danh sách shop từ cookie/tab đang đăng nhập. |
| `runShopeeShopScan(mode)` | `'selected'`, `'all'` | Kích hoạt worker quét tuần tự từng shop đã chọn hoặc tất cả shop phát hiện được. |
| `toggleShopeeShop(id)` | `shopId` | Chọn / bỏ chọn một shop cụ thể trong panel chọn shop. |
| `toggleSelectAllShopeeShops()`| *Không* | Chọn tất cả hoặc bỏ chọn tất cả các shop. |
| `activateWorkspaceTab(tab)` | `'shopee'`, `'tiktok'`, v.v. | Điều hướng giữa các pane nền tảng (mặc định khởi tạo là `'shopee'`). |
| `showShopeeToast(msg)` | `string` | Hiện popup toast góc dưới và cập nhật nhãn `#scc-last-sync-time` thành giờ hiện tại. |
| `renderData(st)` | `statePayload` | Tiếp nhận gói dữ liệu đẩy từ Rust, cập nhật snapshot đơn hàng, live, ads và provenance. |
| `renderUnknownShopeeSnapshot()` | *Không* | Đặt lại toàn bộ các thẻ hiển thị thành `—` khi chưa có dữ liệu thật (đảm bảo không bịa số `0`). |

---

## 5. MA TRẬN TEST CONTRACTS CẦN BẢO TOÀN

Khi đề xuất sửa code, hãy kiểm tra các điều kiện này để đảm bảo lệnh `pytest -q tests/` luôn đạt **43/43 PASS**:

1. **Test `test_workspace_shopee_cleanup_keeps_one_scan_and_shop_context`:**
   - Phải có chính xác `<nav class="island-nav">` và đóng `</nav>`.
   - Phải có chính xác `<header class="shopee-subnav-bar">` và đóng `</header>`.
   - Trong `common_header` phải có `onclick="runScan('all')"`, `onclick="exportShopeeCsv()"`, `onclick="copyReportText()"`.
   - Trong `shopee_header` KHÔNG ĐƯỢC CÓ `exportShopeeCsv()` hoặc `copyReportText()`.
   - Trong `#shopee-view-tong-quan` trước `<!-- Đơn hàng & Tác vụ gần đây -->` không được chứa `workspace-search`, `workspace-play-btn`, `workspace-status-text`, `workspace-progress-bar`.
2. **Test `test_workspace_shopee_header_has_no_tp_seller_brand_and_visible_scan_cta`:**
   - Không chứa `TP <span>SELLER</span>` hay `Shopee Operations`.
   - Phải chứa `id="shopee_workspace_scan"`, `onclick="runShopeeShopScan('selected')"`, `onclick="runShopeeShopScan('all')"`, `id="shopee-sub-nav"`, `id="shopee-shop-pill"`.
3. **Test `test_workspace_platform_dashboard_uses_theme_tokens_and_unknown_kpis_until_sync`:**
   - Phải giữ comment CSS: `/* Platform dashboard: theme-first, no dark island or fabricated KPIs. */`.
   - Phải giữ CSS rule: `grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));` trên `.platform-kpi-grid`.
   - Phải có đầy đủ các ID anchor: `sp_toship`, `sp_shipping_sub`, `sp_anom_count`, `sp_bad_kpi`, `sp_live_kpi`, `sp_pas_sales`, `sp_discounts_list`, `sp_vouchers_list`, `sp_flashsales_list`, `tt_sales`, `tt_kpi_sub`, `tt_status_pill`, `tt_provenance_tbody`.

---

## 6. HƯỚNG DẪN DÀNH CHO AI KHI ĐỀ XUẤT CODE SỬA TIẾP THEO

Nếu cần phát triển thêm tính năng cho Workspace, hãy ưu tiên các hướng sau:
1. **Kết Nối Dữ Liệu Quét Thật Vào Command Center:**  
   Hiện tại `dashboardData` đang chứa dữ liệu mô phỏng. Khi worker Shopee quét xong 200 đơn gần nhất hoặc chiến dịch PAS (`lastData.shopee_orders`, `lastData.shopee_pas`), hãy viết thêm hàm adapter `mapRealShopeeDataToSCC(lastData)` để gán doanh thu thực, đơn thực vào các KPI card của `#section-owner` và `#section-ops`.
2. **Thêm Phân Trang Cho Bảng SKU:**  
   Khi danh mục hàng hóa vượt quá 20 SKU, bổ sung phân trang nhẹ nhàng (5-10 SKU/trang) mà không dùng thư viện ngoài.
3. **Mở Rộng Xuất Báo Cáo:**  
   Bổ sung nút xuất báo cáo tài chính dạng text tóm tắt hoặc CSV tương thích với hàm `exportShopeeCsv()` hiện hữu.
4. **Giữ Vững Phong Cách AURELIA Obsidian Gold:**  
   Sử dụng màu nền `#0F1117`, card `#161B26`, viền `#262D3D` hoặc `var(--gold-line)`, màu nhấn vàng hổ phách `#F59E0B`, đỏ Shopee `#EE4D2D`, xanh lá `#34D399`, xanh dương `#38BDF8`.
