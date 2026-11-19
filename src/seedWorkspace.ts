export interface BookmarkItem {
  title: string;
  url: string;
}

export interface HistoryItem {
  title: string;
  url: string;
  ts: number;
}

export interface WorkspaceSpace {
  id: string;
  name: string;
}

export interface FloatingOrbItem {
  id: string;
  label: string;
  title: string;
  url: string;
  color: string;
  kind: 'builtin' | 'popup' | 'tab';
  enabled: boolean;
}

export interface ShopeeShopInfo {
  shop_id: string;
  display_name: string;
  region?: string;
}

export interface SelectedShop {
  platform: string;
  shop_id: string;
}

export interface ShopeeOrderAnomaly {
  order_sn: string;
  buyer: string;
  status: string;
  item_name: string;
  model_name: string;
  qty: number;
  effective_price: number;
  median_price: number;
  order_total: number;
  reasons: string[];
  url: string;
}

export interface ShopeeReviewItem {
  id: string;
  order_sn: string;
  buyer: string;
  user_name: string;
  stars: number;
  rating_star: number;
  product_name: string;
  model_name?: string;
  comment: string;
  replied: boolean;
  is_fake_suspect: boolean;
  matched_keywords: string[];
  alert_keywords: string[];
  ctime: number;
  created_at: string;
  url: string;
}

export interface ShopeeLiveSession {
  id: string;
  title: string;
  is_live: boolean;
  start_time_ms: number;
  duration_ms: number;
  viewers: number;
  avg_ccu: number;
  atc: number;
  placed_orders: number;
  confirmed_orders: number;
  confirmed_sales: number;
  url: string;
}

export interface ShopeeDiscountItem {
  id: string;
  name: string;
  kind_label: string;
  item_count: number;
  time_status: number;
  expiring_24h: boolean;
  start_time: number;
  end_time: number;
  url: string;
}

export interface ShopeeVoucherItem {
  id: string;
  name: string;
  voucher_code: string;
  discount_pct: number;
  max_value: number;
  value: number;
  min_price: number;
  current_usage: number;
  usage_limit: number;
  time_status: number;
  expiring_24h: boolean;
  start_time: number;
  end_time: number;
  url: string;
}

export interface ShopeeFlashSaleItem {
  id: string;
  name: string;
  enabled_item_count: number;
  item_count: number;
  time_status: number;
  expiring_24h: boolean;
  start_time: number;
  end_time: number;
  url: string;
}

export interface ShopeeSnapshot {
  ts: string;
  shop_id?: string;
  shop_name?: string;
  need_login: boolean;
  to_ship: number;
  shipping: number;
  unpaid: number;
  express_orders?: number;
  scanned_orders: number;
  anomalies: ShopeeOrderAnomaly[];
  rating_summary: {
    to_reply_count: number;
    one_star_count: number;
    two_star_count: number;
    three_star_count: number;
    good_rating_rate: number;
  };
  reviews: ShopeeReviewItem[];
  fake_reviews: ShopeeReviewItem[];
  live: {
    status: string;
    is_live_now: boolean;
    sessions: ShopeeLiveSession[];
  };
  discounts: ShopeeDiscountItem[];
  vouchers: ShopeeVoucherItem[];
  flash_sales: ShopeeFlashSaleItem[];
}

export const DEFAULT_BOOKMARKS: BookmarkItem[] = [
  { title: 'Shopee Seller', url: 'https://banhang.shopee.vn' },
  { title: 'TikTok Seller', url: 'https://seller-vn.tiktok.com' },
  { title: 'Google Sheets', url: 'https://docs.google.com/spreadsheets' },
  { title: 'Gmail', url: 'https://mail.google.com' },
  { title: 'Zalo Web', url: 'https://chat.zalo.me' },
  { title: 'Pancake POS', url: 'https://pos.pages.fm' },
];

export const DEFAULT_SPACES: WorkspaceSpace[] = [
  { id: 'default', name: 'Default' },
  { id: 'ecom', name: 'TMĐT Shopee & TikTok' },
  { id: 'ops', name: 'Vận hành & CSKH' },
];

export const DEFAULT_FLOATING_ORBS: FloatingOrbItem[] = [
  { id: 'ai', label: 'AI', title: 'AI Cowork', url: '', color: '#E2BE68', kind: 'builtin', enabled: true },
  { id: 'fb', label: 'FB', title: 'Messenger', url: 'https://www.messenger.com', color: '#38BDF8', kind: 'builtin', enabled: true },
  { id: 'zalo', label: 'ZL', title: 'Zalo Web', url: 'https://chat.zalo.me', color: '#60A5FA', kind: 'builtin', enabled: true },
];

export function createSeedHistory(): HistoryItem[] {
  const nowSec = Math.floor(Date.now() / 1000);
  return [
    { title: 'Shopee Kênh Người Bán — Quản Lý Đơn Hàng', url: 'https://banhang.shopee.vn/portal/sale/order', ts: nowSec - 3600 },
    { title: 'Shopee PAS — Chiến Dịch Quảng Cáo', url: 'https://banhang.shopee.vn/portal/marketing/pas/index', ts: nowSec - 2400 },
    { title: 'TikTok Shop Seller Center VN', url: 'https://seller-vn.tiktok.com/ads-creation/dashboard', ts: nowSec - 1200 },
    { title: 'Google Sheets — Báo Cáo Doanh Thu Tháng 11', url: 'https://docs.google.com/spreadsheets', ts: nowSec - 480 },
  ];
}

export function createSeedShopeeDiscovery() {
  return {
    status: 'ok',
    account_identity: 'aurelia_group_vn (Main Sub-account)',
    shops: [
      { shop_id: '882410912', display_name: 'Aurelia Official Store', region: 'VN' },
      { shop_id: '904155238', display_name: 'TP Beauty & Home', region: 'VN' },
    ] as ShopeeShopInfo[],
  };
}

export function createSeedShopSnapshots(): {
  platform: ShopeeSnapshot;
  byShop: Record<string, ShopeeSnapshot>;
} {
  const nowMs = Date.now();
  const shop1Anomalies: ShopeeOrderAnomaly[] = [
    {
      order_sn: '261118A9K4M2QX',
      buyer: 'minhanh_94',
      status: 'Chờ lấy hàng',
      item_name: 'Serum Vitamin C 15% Sáng Da 30ml',
      model_name: 'Combo 2 Chai',
      qty: 2,
      effective_price: 118000,
      median_price: 289000,
      order_total: 236000,
      reasons: ['Giá thực bán thấp hơn 59% so với trung vị', 'Nghi lỗi chồng mã voucher'],
      url: 'https://banhang.shopee.vn/portal/sale/order',
    },
    {
      order_sn: '261118B7T2N8PL',
      buyer: 'hoangnam.store',
      status: 'Chờ lấy hàng',
      item_name: 'Kem Chống Nắng Kiềm Dầu SPF50+ 50ml',
      model_name: 'Tuýp 50ml',
      qty: 3,
      effective_price: 92000,
      median_price: 215000,
      order_total: 276000,
      reasons: ['Giá đơn vị giảm bất thường (-57%)'],
      url: 'https://banhang.shopee.vn/portal/sale/order',
    },
  ];

  const shop2Anomalies: ShopeeOrderAnomaly[] = [
    {
      order_sn: '261118C5R9W1VD',
      buyer: 'lanphuong_decor',
      status: 'Đang chuẩn bị',
      item_name: 'Máy Xông Tinh Dầu Vân Gỗ 500ml',
      model_name: 'Bản Remote + 2 Tinh Dầu',
      qty: 1,
      effective_price: 145000,
      median_price: 320000,
      order_total: 145000,
      reasons: ['Lệch giá Flash Sale (-55%)'],
      url: 'https://banhang.shopee.vn/portal/sale/order',
    },
  ];

  const nowSec = Math.floor(nowMs / 1000);
  const shop1Reviews: ShopeeReviewItem[] = [
    {
      id: 'rev-101',
      order_sn: '261116D2K8X1AB',
      buyer: 'thuha_88',
      user_name: 'thuha_88',
      stars: 1,
      rating_star: 1,
      product_name: 'Serum Vitamin C 15% Sáng Da 30ml',
      model_name: 'Combo 2 Chai',
      comment: 'Mùi hơi lạ, hộp hơi móp góc, nghi hàng giả hay hàng tồn kho cũ.',
      replied: false,
      is_fake_suspect: true,
      matched_keywords: ['hàng giả'],
      alert_keywords: ['hàng giả'],
      ctime: nowSec - 14400,
      created_at: '18/11/2026 14:22',
      url: 'https://banhang.shopee.vn/portal/settings/shop/rating',
    },
    {
      id: 'rev-102',
      order_sn: '261117E4M9Q3CD',
      buyer: 'quanghuy_vn',
      user_name: 'quanghuy_vn',
      stars: 2,
      rating_star: 2,
      product_name: 'Kem Chống Nắng Kiềm Dầu SPF50+ 50ml',
      model_name: 'Tuýp 50ml',
      comment: 'Giao hàng chậm 2 ngày so với dự kiến, đóng gói tạm ổn.',
      replied: false,
      is_fake_suspect: false,
      matched_keywords: [],
      alert_keywords: [],
      ctime: nowSec - 25200,
      created_at: '18/11/2026 11:05',
      url: 'https://banhang.shopee.vn/portal/settings/shop/rating',
    },
  ];

  const shop2Reviews: ShopeeReviewItem[] = [
    {
      id: 'rev-201',
      order_sn: '261117F8P3Z9EF',
      buyer: 'ngocmai_hcm',
      user_name: 'ngocmai_hcm',
      stars: 3,
      rating_star: 3,
      product_name: 'Máy Xông Tinh Dầu Vân Gỗ 500ml',
      model_name: 'Bản Remote',
      comment: 'Phun sương đều nhưng dây nguồn hơi ngắn so với mô tả.',
      replied: false,
      is_fake_suspect: false,
      matched_keywords: [],
      alert_keywords: [],
      ctime: nowSec - 32400,
      created_at: '18/11/2026 09:40',
      url: 'https://banhang.shopee.vn/portal/settings/shop/rating',
    },
  ];

  const shop1Live: ShopeeLiveSession[] = [
    {
      id: 'live-01',
      title: 'Săn Deal 11.18 — Combo Serum & Chống Nắng Giảm Sâu',
      is_live: true,
      start_time_ms: nowMs - 75 * 60 * 1000,
      duration_ms: 75 * 60 * 1000,
      viewers: 4820,
      avg_ccu: 146,
      atc: 312,
      placed_orders: 94,
      confirmed_orders: 88,
      confirmed_sales: 26840000,
      url: 'https://banhang.shopee.vn/portal/marketing/live',
    },
    {
      id: 'live-02',
      title: 'Phiên Trưa Aurelia — Voucher Độc Quyền 15%',
      is_live: false,
      start_time_ms: nowMs - 8 * 3600 * 1000,
      duration_ms: 120 * 60 * 1000,
      viewers: 3190,
      avg_ccu: 98,
      atc: 184,
      placed_orders: 61,
      confirmed_orders: 57,
      confirmed_sales: 15920000,
      url: 'https://banhang.shopee.vn/portal/marketing/live',
    },
  ];

  const shop2Live: ShopeeLiveSession[] = [
    {
      id: 'live-03',
      title: 'Decor Phòng Ngủ & Tinh Dầu Thiên Nhiên',
      is_live: false,
      start_time_ms: nowMs - 6 * 3600 * 1000,
      duration_ms: 95 * 60 * 1000,
      viewers: 1890,
      avg_ccu: 64,
      atc: 112,
      placed_orders: 38,
      confirmed_orders: 35,
      confirmed_sales: 10850000,
      url: 'https://banhang.shopee.vn/portal/marketing/live',
    },
  ];

  const shop1Discounts: ShopeeDiscountItem[] = [
    {
      id: 'disc-1',
      name: 'Ưu đãi tháng 11 — Chăm sóc da',
      kind_label: 'Chương trình của Shop',
      item_count: 18,
      time_status: 2,
      expiring_24h: false,
      start_time: nowSec - 3 * 86400,
      end_time: nowSec + 12 * 86400,
      url: 'https://banhang.shopee.vn/portal/marketing/discount',
    },
  ];

  const shop1Vouchers: ShopeeVoucherItem[] = [
    {
      id: 'vch-1',
      name: 'Voucher Toàn Shop 30K',
      voucher_code: 'AURELIA30K',
      discount_pct: 0,
      max_value: 30000,
      value: 30000,
      min_price: 299000,
      current_usage: 412,
      usage_limit: 1000,
      time_status: 2,
      expiring_24h: false,
      start_time: nowSec - 7 * 86400,
      end_time: nowSec + 10 * 86400,
      url: 'https://banhang.shopee.vn/portal/marketing/vouchers/list',
    },
  ];

  const shop1FlashSales: ShopeeFlashSaleItem[] = [
    {
      id: 'fs-1',
      name: 'Khung 20:00 Hôm nay',
      enabled_item_count: 6,
      item_count: 6,
      time_status: 1,
      expiring_24h: true,
      start_time: nowSec + 3600,
      end_time: nowSec + 3 * 3600,
      url: 'https://banhang.shopee.vn/portal/marketing/shop-flash-sale',
    },
  ];

  const shop2Discounts: ShopeeDiscountItem[] = [
    {
      id: 'disc-2',
      name: 'Combo Nhà Thơm — Giảm 18%',
      kind_label: 'Combo Khuyến Mãi',
      item_count: 12,
      time_status: 2,
      expiring_24h: true,
      start_time: nowSec - 5 * 86400,
      end_time: nowSec + 18 * 3600,
      url: 'https://banhang.shopee.vn/portal/marketing/discount',
    },
  ];

  const shop2Vouchers: ShopeeVoucherItem[] = [
    {
      id: 'vch-2',
      name: 'Voucher Nhà Cửa 20K',
      voucher_code: 'HOME20K',
      discount_pct: 10,
      max_value: 20000,
      value: 20000,
      min_price: 199000,
      current_usage: 198,
      usage_limit: 500,
      time_status: 2,
      expiring_24h: false,
      start_time: nowSec - 6 * 86400,
      end_time: nowSec + 11 * 86400,
      url: 'https://banhang.shopee.vn/portal/marketing/vouchers/list',
    },
  ];

  const shop2FlashSales: ShopeeFlashSaleItem[] = [];

  const shop1: ShopeeSnapshot = {
    ts: '18/11/2026 18:20',
    shop_id: '882410912',
    shop_name: 'Aurelia Official Store',
    need_login: false,
    to_ship: 48,
    shipping: 112,
    unpaid: 7,
    express_orders: 9,
    scanned_orders: 140,
    anomalies: shop1Anomalies,
    rating_summary: {
      to_reply_count: 2,
      one_star_count: 1,
      two_star_count: 1,
      three_star_count: 0,
      good_rating_rate: 98.6,
    },
    reviews: shop1Reviews,
    fake_reviews: shop1Reviews.filter((r) => r.is_fake_suspect),
    live: { status: 'ok', is_live_now: true, sessions: shop1Live },
    discounts: shop1Discounts,
    vouchers: shop1Vouchers,
    flash_sales: shop1FlashSales,
  };

  const shop2: ShopeeSnapshot = {
    ts: '18/11/2026 18:20',
    shop_id: '904155238',
    shop_name: 'TP Beauty & Home',
    need_login: false,
    to_ship: 23,
    shipping: 59,
    unpaid: 4,
    express_orders: 4,
    scanned_orders: 60,
    anomalies: shop2Anomalies,
    rating_summary: {
      to_reply_count: 1,
      one_star_count: 0,
      two_star_count: 0,
      three_star_count: 1,
      good_rating_rate: 99.1,
    },
    reviews: shop2Reviews,
    fake_reviews: [],
    live: { status: 'ok', is_live_now: false, sessions: shop2Live },
    discounts: shop2Discounts,
    vouchers: shop2Vouchers,
    flash_sales: shop2FlashSales,
  };

  const platform: ShopeeSnapshot = {
    ts: '18/11/2026 18:20',
    need_login: false,
    to_ship: shop1.to_ship + shop2.to_ship,
    shipping: shop1.shipping + shop2.shipping,
    unpaid: shop1.unpaid + shop2.unpaid,
    express_orders: (shop1.express_orders || 0) + (shop2.express_orders || 0),
    scanned_orders: shop1.scanned_orders + shop2.scanned_orders,
    anomalies: [...shop1Anomalies, ...shop2Anomalies],
    rating_summary: {
      to_reply_count: 3,
      one_star_count: 1,
      two_star_count: 1,
      three_star_count: 1,
      good_rating_rate: 98.8,
    },
    reviews: [...shop1Reviews, ...shop2Reviews],
    fake_reviews: [...shop1.fake_reviews, ...shop2.fake_reviews],
    live: {
      status: 'ok',
      is_live_now: true,
      sessions: [...shop1Live, ...shop2Live],
    },
    discounts: [...shop1Discounts, ...shop2Discounts],
    vouchers: [...shop1Vouchers, ...shop2Vouchers],
    flash_sales: [...shop1FlashSales, ...shop2FlashSales],
  };

  return {
    platform,
    byShop: {
      '882410912': shop1,
      '904155238': shop2,
    },
  };
}

export function createSeedShopeePas() {
  return {
    ts: '18/11/2026 18:18',
    sales_ads: 48620000,
    cost_ads: 7140000,
    roas_ads: 6.81,
    orders_ads: 164,
    campaigns: [
      {
        name: 'GMV Max — Serum Vitamin C 15%',
        status: 'Đang chạy',
        budget: 3500000,
        budget_raw: '3.500.000₫/ngày',
        cost: 2890000,
        sales: 22400000,
        roas: 7.75,
        acos: 12.9,
        url: 'https://banhang.shopee.vn/portal/marketing/pas/index',
      },
      {
        name: 'Tìm Kiếm — Kem Chống Nắng SPF50+',
        status: 'Đang chạy',
        budget: 2500000,
        budget_raw: '2.500.000₫/ngày',
        cost: 2410000,
        sales: 15820000,
        roas: 6.56,
        acos: 15.2,
        url: 'https://banhang.shopee.vn/portal/marketing/pas/index',
      },
      {
        name: 'Khám Phá — Máy Xông Tinh Dầu',
        status: 'Đang chạy',
        budget: 2000000,
        budget_raw: '2.000.000₫/ngày',
        cost: 1840000,
        sales: 10400000,
        roas: 5.65,
        acos: 17.7,
        url: 'https://banhang.shopee.vn/portal/marketing/pas/index',
      },
    ],
  };
}

export function createSeedShopeeHealth() {
  return {
    status: 'ok',
    ts: '18/11/2026 18:18',
    penalty_points: 0,
    lsr_pct: 0.42,
    nfr_pct: 0.18,
    crr_pct: 97.4,
    overall_status: 'Xuất sắc',
    tier: 'Xuất sắc',
    late_shipment_rate: '0.42%',
    non_fulfillment_rate: '0.18%',
    chat_response_rate: '97.4%',
  };
}

export function createSeedTikTok() {
  const today = new Date().toISOString().slice(0, 10);
  return {
    status: 'ok',
    period_label: 'Hôm nay',
    period_date: today,
    ts: '18/11/2026 18:19',
    captured_at: new Date().toISOString(),
    source_url: 'https://seller-vn.tiktok.com/ads-creation/dashboard',
    sales_ads: 31450000,
    cost_ads: 5620000,
    orders_ads: 118,
    roas_ads: 5.6,
    campaigns_provenance: { status: 'ok', count: 2 },
    campaigns: [
      {
        name: 'Product Shopping — Aurelia Skincare Set',
        status: 'Đang phân phối',
        budget: 3500000,
        cost: 3180000,
        orders: 74,
        sales: 19850000,
        roas: 6.24,
      },
      {
        name: 'LIVE Shopping — Phiên Tối 18/11',
        status: 'Đang phân phối',
        budget: 3000000,
        cost: 2440000,
        orders: 44,
        sales: 11600000,
        roas: 4.75,
      },
    ],
  };
}

export function createSeedTikTokHealth() {
  return {
    status: 'ok',
    module: 'account_health',
    schema: 'tp.tiktok.health.v1',
    source_url: 'https://seller-vn.tiktok.com/health',
    read_only: true,
    captured_at: new Date().toISOString(),
    update_text: 'Đồng bộ từ chế độ Web Preview',
    account_score_raw: '200 / 200 (Tốt)',
    store_score_raw: '4.9 / 5.0',
    raw_rows: [
      'Điểm vi phạm: 0 điểm',
      'Tỷ lệ giao hàng trễ: 0.6%',
      'Tỷ lệ phản hồi trong 24h: 96.4%',
    ],
  };
}
