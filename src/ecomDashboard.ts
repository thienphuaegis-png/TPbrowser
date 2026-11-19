export type PlatformId = 'all' | 'shopee' | 'tiktok';
export type ConcretePlatformId = 'shopee' | 'tiktok';
export type TimeframeId = 'today' | '7d' | '30d' | 'month';
export type SkuActionCategory = 'stock' | 'ctr' | 'cr' | 'good';

export interface SkuItem {
  code: string;
  name: string;
  platform: ConcretePlatformId;
  shopId: string;
  shopName: string;
  stock: number;
  traffic: number;
  ctr: number;
  cr: number;
  rating: number;
  revenue30d: number;
}

export interface OpsMetrics {
  lsr: number;
  nfr: number;
  prepDays: number;
  crr: number;
  rating: number;
  chatRespSeconds: number;
  sqt: number;
  violations: number;
  preorderPct: number;
  impressions: number;
  traffic: number;
  atc: number;
  orders: number;
}

export interface TimeframeMetrics {
  netRev: number; // million VND
  netRevDeltaPct: number;
  netProfit: number; // million VND
  netProfitDeltaPct: number;
  cir: number; // % of netRev
  cirDeltaPct: number;
  cogsPct: number;
  feePct: number;
  otherPct: number;
  orders: number;
  aovDeltaPct: number;
  repeatRate: number;
  repeatDeltaPct: number;
  returnRate: number;
  invDays: number;
  invDeltaDays: number;
  chartLabels: string[];
  revSeries: number[];
  profitSeries: number[];
  adsSeries: number[];
  ops: OpsMetrics;
}

export interface ShopProfile {
  id: string;
  platform: ConcretePlatformId;
  name: string;
  tierLabel: string;
  category: string;
  sellerUrl: string;
  timeframes: Record<TimeframeId, TimeframeMetrics>;
  skus: SkuItem[];
}

export interface AggregatedMetrics {
  netRev: number;
  netRevDeltaPct: number;
  netProfit: number;
  netMargin: number;
  netProfitDeltaPct: number;
  cir: number;
  roas: number;
  breakevenCir: number;
  cirDeltaPct: number;
  cogsPct: number;
  feePct: number;
  otherPct: number;
  orders: number;
  aov: number; // VND
  aovDeltaPct: number;
  repeatRate: number;
  repeatDeltaPct: number;
  returnRate: number;
  invDays: number;
  invDeltaDays: number;
  chartLabels: string[];
  revSeries: number[];
  profitSeries: number[];
  adsSeries: number[];
  ops: {
    lsr: number;
    nfr: number;
    prepDays: number;
    crr: number;
    rating: number;
    chatRespSeconds: number;
    sqt: number;
    violations: number;
    preorderPct: number;
    impressions: number;
    traffic: number;
    ctr: number;
    atc: number;
    atcRate: number;
    orders: number;
    cr: number;
  };
}

export interface BreakdownRow {
  id: string;
  kind: 'platform' | 'shop';
  platform: ConcretePlatformId;
  shopId: string;
  name: string;
  subtitle: string;
  shopCount?: number;
  netRev: number;
  netProfit: number;
  netMargin: number;
  cir: number;
  roas: number;
  orders: number;
  rating: number;
  sqt: number;
  alertCount: number;
}

export interface DashboardSnapshot {
  platform: PlatformId;
  shopId: string;
  timeframe: TimeframeId;
  scopeTitle: string;
  scopeSubtitle: string;
  breadcrumb: string[];
  platforms: Array<{
    id: ConcretePlatformId;
    name: string;
    shopCount: number;
    netRev: number;
    netProfit: number;
    netMargin: number;
    orders: number;
  }>;
  shopsInPlatform: Array<{
    id: string;
    platform: ConcretePlatformId;
    name: string;
    tierLabel: string;
    category: string;
    sellerUrl: string;
    netRev: number;
    netMargin: number;
    orders: number;
    alertCount: number;
  }>;
  metrics: AggregatedMetrics;
  breakdownKind: 'platform' | 'shop';
  breakdownRows: BreakdownRow[];
  skus: Array<SkuItem & { category: SkuActionCategory; actionLabel: string }>;
  updatedAt: string;
}

export interface SimulatorInput {
  baseRev: number;
  cir: number;
  feePct: number;
  cogsPct: number;
  fixedOpsCost?: number;
}

export interface SimulatorResult {
  maxCir: number;
  netMargin: number;
  netProfit: number;
  breakevenRoas: number | null;
  status: 'safe' | 'thin' | 'loss';
  advice: string;
}

function round1(v: number): number {
  return Math.round(v * 10) / 10;
}

function round2(v: number): number {
  return Math.round(v * 100) / 100;
}

function buildTimeframe(
  base30d: {
    netRev: number;
    netRevDeltaPct: number;
    cogsPct: number;
    feePct: number;
    otherPct: number;
    cir: number;
    cirDeltaPct: number;
    aovVnd: number;
    aovDeltaPct: number;
    repeatRate: number;
    repeatDeltaPct: number;
    returnRate: number;
    invDays: number;
    invDeltaDays: number;
    lsr: number;
    nfr: number;
    prepDays: number;
    crr: number;
    rating: number;
    chatRespSeconds: number;
    sqt: number;
    violations: number;
    preorderPct: number;
    ctr: number;
    atcRate: number;
    cr: number;
  }
): Record<TimeframeId, TimeframeMetrics> {
  const make = (
    scale: number,
    labels: string[],
    weights: number[],
    cirAdj: number,
    deltaAdj: number,
    opsAdj: { lsr?: number; nfr?: number; crr?: number; ctr?: number; cr?: number }
  ): TimeframeMetrics => {
    const netRev = round1(base30d.netRev * scale);
    const cir = round1(Math.max(4, base30d.cir + cirAdj));
    const margin = round1(100 - base30d.cogsPct - base30d.feePct - base30d.otherPct - cir);
    const netProfit = round1((netRev * margin) / 100);
    const orders = Math.max(1, Math.round((netRev * 1_000_000) / base30d.aovVnd));
    const cr = round2(Math.max(1.2, base30d.cr + (opsAdj.cr || 0)));
    const ctr = round2(Math.max(1.5, base30d.ctr + (opsAdj.ctr || 0)));
    const traffic = Math.max(orders, Math.round(orders / (cr / 100)));
    const impressions = Math.max(traffic, Math.round(traffic / (ctr / 100)));
    const atc = Math.max(orders, Math.round(traffic * (base30d.atcRate / 100)));

    const totalW = weights.reduce((a, b) => a + b, 0);
    const revSeries = weights.map((w) => round1((netRev * w) / totalW));
    const profitSeries = weights.map((w, idx) => {
      const mVar = idx % 2 === 1 ? 0.5 : -0.3;
      return round1((revSeries[idx] * (margin + mVar)) / 100);
    });
    const adsSeries = weights.map((w, idx) => {
      const cVar = idx % 2 === 1 ? 0.4 : -0.2;
      return round1((revSeries[idx] * (cir + cVar)) / 100);
    });

    return {
      netRev,
      netRevDeltaPct: round1(base30d.netRevDeltaPct + deltaAdj),
      netProfit,
      netProfitDeltaPct: round1(base30d.netRevDeltaPct + deltaAdj + 3.8),
      cir,
      cirDeltaPct: round1(base30d.cirDeltaPct + cirAdj * 0.25),
      cogsPct: base30d.cogsPct,
      feePct: base30d.feePct,
      otherPct: base30d.otherPct,
      orders,
      aovDeltaPct: round1(base30d.aovDeltaPct + deltaAdj * 0.3),
      repeatRate: round1(base30d.repeatRate + deltaAdj * 0.15),
      repeatDeltaPct: base30d.repeatDeltaPct,
      returnRate: round1(Math.max(0.5, base30d.returnRate + (opsAdj.nfr || 0) * 0.5)),
      invDays: Math.max(10, Math.round(base30d.invDays - deltaAdj * 0.2)),
      invDeltaDays: base30d.invDeltaDays,
      chartLabels: labels,
      revSeries,
      profitSeries,
      adsSeries,
      ops: {
        lsr: round1(Math.max(0, base30d.lsr + (opsAdj.lsr || 0))),
        nfr: round1(Math.max(0.2, base30d.nfr + (opsAdj.nfr || 0))),
        prepDays: round2(Math.max(0.25, base30d.prepDays + (opsAdj.lsr || 0) * 0.05)),
        crr: round1(Math.min(99.5, base30d.crr + (opsAdj.crr || 0))),
        rating: base30d.rating,
        chatRespSeconds: base30d.chatRespSeconds,
        sqt: base30d.sqt,
        violations: base30d.violations,
        preorderPct: base30d.preorderPct,
        impressions,
        traffic,
        atc,
        orders,
      },
    };
  };

  return {
    today: make(
      0.0372,
      ['00h', '04h', '08h', '12h', '16h', '20h', '23h'],
      [1.2, 0.8, 3.5, 6.8, 5.4, 8.9, 4.8],
      -0.8,
      -4.5,
      { lsr: -0.4, nfr: -0.4, crr: 1.4, ctr: 0.18, cr: -0.1 }
    ),
    '7d': make(
      0.252,
      ['T2', 'T3', 'T4', 'T5', 'T6', 'T7', 'CN'],
      [26.5, 28.0, 29.4, 27.8, 33.1, 36.2, 31.8],
      0.5,
      -1.8,
      { lsr: 0.1, nfr: 0.1, crr: -0.6, ctr: 0.04, cr: 0.08 }
    ),
    '30d': make(
      1.0,
      ['Tuần 1', 'Tuần 2 (Sale đôi)', 'Tuần 3', 'Tuần 4 (Lương về)'],
      [175.4, 248.6, 189.2, 232.0],
      0,
      0,
      {}
    ),
    month: make(
      0.725,
      ['01–05', '06–10', '11–15 (Giữa tháng)', '16–20'],
      [118.0, 134.5, 215.0, 145.0],
      -0.4,
      2.2,
      { lsr: -0.1, nfr: -0.1, crr: 0.5, ctr: 0.08, cr: -0.05 }
    ),
  };
}

export const ECOM_SHOPS: ShopProfile[] = [
  // ================= SHOPEE (3 SHOPS) =================
  {
    id: '882410912',
    platform: 'shopee',
    name: 'AuraHome Official Store',
    tierLabel: 'Shopee Mall',
    category: 'Gia dụng thông minh',
    sellerUrl: 'https://banhang.shopee.vn/portal/sale/order?shop_id=882410912',
    timeframes: buildTimeframe({
      netRev: 845.2,
      netRevDeltaPct: 14.2,
      cogsPct: 48.0,
      feePct: 13.5,
      otherPct: 5.3,
      cir: 11.8,
      cirDeltaPct: -1.4,
      aovVnd: 318000,
      aovDeltaPct: 6.5,
      repeatRate: 28.4,
      repeatDeltaPct: 3.1,
      returnRate: 2.1,
      invDays: 24,
      invDeltaDays: -3,
      lsr: 0.8,
      nfr: 1.1,
      prepDays: 0.42,
      crr: 96.5,
      rating: 4.92,
      chatRespSeconds: 220,
      sqt: 0,
      violations: 0,
      preorderPct: 1.5,
      ctr: 4.12,
      atcRate: 14.8,
      cr: 3.45,
    }),
    skus: [
      {
        code: 'AH-DEN-01',
        name: 'Đèn Ngủ Cảm Biến Thông Minh AuraLite Pro',
        platform: 'shopee',
        shopId: '882410912',
        shopName: 'AuraHome Official Store',
        stock: 14,
        traffic: 18420,
        ctr: 5.4,
        cr: 4.8,
        rating: 4.96,
        revenue30d: 268.4,
      },
      {
        code: 'AH-THAM-04',
        name: 'Thảm Lau Chân Siêu Thấm Diatomite Cao Cấp',
        platform: 'shopee',
        shopId: '882410912',
        shopName: 'AuraHome Official Store',
        stock: 340,
        traffic: 14210,
        ctr: 1.85,
        cr: 3.9,
        rating: 4.88,
        revenue30d: 142.0,
      },
      {
        code: 'AH-KE-02',
        name: 'Kệ Gia Vị Nhà Bếp Thép Carbon Sơn Tĩnh Điện',
        platform: 'shopee',
        shopId: '882410912',
        shopName: 'AuraHome Official Store',
        stock: 185,
        traffic: 12950,
        ctr: 4.6,
        cr: 1.45,
        rating: 4.62,
        revenue30d: 94.6,
      },
      {
        code: 'AH-HOP-09',
        name: 'Set 6 Hộp Đựng Thực Phẩm Tủ Lạnh Kháng Khuẩn',
        platform: 'shopee',
        shopId: '882410912',
        shopName: 'AuraHome Official Store',
        stock: 420,
        traffic: 11340,
        ctr: 4.9,
        cr: 4.2,
        rating: 4.95,
        revenue30d: 189.2,
      },
      {
        code: 'AH-MAY-07',
        name: 'Máy Xay Tỏi Ớt Cầm Tay Sạc USB Inox 304',
        platform: 'shopee',
        shopId: '882410912',
        shopName: 'AuraHome Official Store',
        stock: 9,
        traffic: 8420,
        ctr: 4.3,
        cr: 4.1,
        rating: 4.91,
        revenue30d: 151.0,
      },
    ],
  },
  {
    id: '904155238',
    platform: 'shopee',
    name: 'Aurelia Beauty Mall',
    tierLabel: 'Shop Yêu Thích+',
    category: 'Mỹ phẩm & Chăm sóc da',
    sellerUrl: 'https://banhang.shopee.vn/portal/sale/order?shop_id=904155238',
    timeframes: buildTimeframe({
      netRev: 524.6,
      netRevDeltaPct: 19.4,
      cogsPct: 38.5,
      feePct: 14.2,
      otherPct: 5.1,
      cir: 14.6,
      cirDeltaPct: -0.9,
      aovVnd: 365000,
      aovDeltaPct: 8.2,
      repeatRate: 36.2,
      repeatDeltaPct: 4.5,
      returnRate: 1.5,
      invDays: 19,
      invDeltaDays: -4,
      lsr: 0.5,
      nfr: 0.8,
      prepDays: 0.36,
      crr: 97.8,
      rating: 4.95,
      chatRespSeconds: 165,
      sqt: 0,
      violations: 0,
      preorderPct: 0.8,
      ctr: 4.65,
      atcRate: 16.4,
      cr: 3.92,
    }),
    skus: [
      {
        code: 'AB-SER-15',
        name: 'Serum Vitamin C 15% Sáng Da 30ml (Combo 2 Chai)',
        platform: 'shopee',
        shopId: '904155238',
        shopName: 'Aurelia Beauty Mall',
        stock: 16,
        traffic: 16840,
        ctr: 5.1,
        cr: 4.6,
        rating: 4.97,
        revenue30d: 248.0,
      },
      {
        code: 'AB-KCN-50',
        name: 'Kem Chống Nắng Kiềm Dầu SPF50+ PA++++ 50ml',
        platform: 'shopee',
        shopId: '904155238',
        shopName: 'Aurelia Beauty Mall',
        stock: 290,
        traffic: 12180,
        ctr: 4.4,
        cr: 3.8,
        rating: 4.94,
        revenue30d: 164.6,
      },
      {
        code: 'AB-TON-20',
        name: 'Nước Cân Bằng Rau Má Làm Dịu Da Nhạy Cảm 200ml',
        platform: 'shopee',
        shopId: '904155238',
        shopName: 'Aurelia Beauty Mall',
        stock: 410,
        traffic: 7620,
        ctr: 2.15,
        cr: 3.2,
        rating: 4.89,
        revenue30d: 112.0,
      },
    ],
  },
  {
    id: '917640215',
    platform: 'shopee',
    name: 'TP Decor & Living',
    tierLabel: 'Shop Yêu Thích',
    category: 'Decor & Tinh dầu',
    sellerUrl: 'https://banhang.shopee.vn/portal/sale/order?shop_id=917640215',
    timeframes: buildTimeframe({
      netRev: 298.4,
      netRevDeltaPct: 9.6,
      cogsPct: 50.5,
      feePct: 13.0,
      otherPct: 5.8,
      cir: 13.4,
      cirDeltaPct: 0.6,
      aovVnd: 274000,
      aovDeltaPct: 3.1,
      repeatRate: 21.8,
      repeatDeltaPct: 1.7,
      returnRate: 2.8,
      invDays: 31,
      invDeltaDays: -1,
      lsr: 1.4,
      nfr: 1.6,
      prepDays: 0.64,
      crr: 93.4,
      rating: 4.86,
      chatRespSeconds: 340,
      sqt: 1,
      violations: 1,
      preorderPct: 3.2,
      ctr: 3.45,
      atcRate: 12.6,
      cr: 2.78,
    }),
    skus: [
      {
        code: 'TD-XONG-05',
        name: 'Máy Xông Tinh Dầu Vân Gỗ 500ml Bản Remote',
        platform: 'shopee',
        shopId: '917640215',
        shopName: 'TP Decor & Living',
        stock: 124,
        traffic: 9850,
        ctr: 4.2,
        cr: 3.6,
        rating: 4.91,
        revenue30d: 146.2,
      },
      {
        code: 'TD-BINH-11',
        name: 'Bình Giữ Nhiệt Hiển Thị Nhiệt Độ LED 500ml',
        platform: 'shopee',
        shopId: '917640215',
        shopName: 'TP Decor & Living',
        stock: 260,
        traffic: 5640,
        ctr: 2.05,
        cr: 3.5,
        rating: 4.85,
        revenue30d: 82.4,
      },
      {
        code: 'TD-MUC-03',
        name: 'Móc Treo Quần Áo Chống Trượt Nhung Cao Cấp (Set 10)',
        platform: 'shopee',
        shopId: '917640215',
        shopName: 'TP Decor & Living',
        stock: 510,
        traffic: 4890,
        ctr: 3.8,
        cr: 1.62,
        rating: 4.74,
        revenue30d: 69.8,
      },
    ],
  },

  // ================= TIKTOK SHOP (2 SHOPS) =================
  {
    id: 'tt-74910284',
    platform: 'tiktok',
    name: 'AuraHome TikTok Mall',
    tierLabel: 'TikTok Shop Mall',
    category: 'LIVE & Video Gia dụng',
    sellerUrl: 'https://seller-vn.tiktok.com/homepage',
    timeframes: buildTimeframe({
      netRev: 618.5,
      netRevDeltaPct: 24.6,
      cogsPct: 46.5,
      feePct: 11.2,
      otherPct: 6.4,
      cir: 16.2,
      cirDeltaPct: -1.8,
      aovVnd: 295000,
      aovDeltaPct: 7.4,
      repeatRate: 22.6,
      repeatDeltaPct: 2.9,
      returnRate: 3.4,
      invDays: 21,
      invDeltaDays: -4,
      lsr: 0.9,
      nfr: 1.5,
      prepDays: 0.48,
      crr: 95.4,
      rating: 4.89,
      chatRespSeconds: 245,
      sqt: 0,
      violations: 0,
      preorderPct: 0.5,
      ctr: 4.85,
      atcRate: 15.9,
      cr: 3.62,
    }),
    skus: [
      {
        code: 'TT-AH-DEN01',
        name: 'Combo 2 Đèn Ngủ Cảm Biến AuraLite (Deal LIVE)',
        platform: 'tiktok',
        shopId: 'tt-74910284',
        shopName: 'AuraHome TikTok Mall',
        stock: 18,
        traffic: 24100,
        ctr: 5.9,
        cr: 4.4,
        rating: 4.93,
        revenue30d: 312.5,
      },
      {
        code: 'TT-AH-GOI05',
        name: 'Gối Công Thái Học Memory Foam Bảo Vệ Đốt Sống Cổ',
        platform: 'tiktok',
        shopId: 'tt-74910284',
        shopName: 'AuraHome TikTok Mall',
        stock: 195,
        traffic: 15320,
        ctr: 4.5,
        cr: 3.9,
        rating: 4.94,
        revenue30d: 194.0,
      },
      {
        code: 'TT-AH-KE08',
        name: 'Kệ Đa Năng Gấp Gọn Bánh Xe 4 Tầng',
        platform: 'tiktok',
        shopId: 'tt-74910284',
        shopName: 'AuraHome TikTok Mall',
        stock: 220,
        traffic: 11240,
        ctr: 2.2,
        cr: 2.6,
        rating: 4.82,
        revenue30d: 112.0,
      },
    ],
  },
  {
    id: 'tt-74918819',
    platform: 'tiktok',
    name: 'Aurelia Skincare VN',
    tierLabel: 'TikTok Official',
    category: 'Affiliate & LIVE Mỹ phẩm',
    sellerUrl: 'https://seller-vn.tiktok.com/homepage',
    timeframes: buildTimeframe({
      netRev: 412.0,
      netRevDeltaPct: 28.2,
      cogsPct: 37.0,
      feePct: 11.8,
      otherPct: 6.0,
      cir: 18.5,
      cirDeltaPct: -2.1,
      aovVnd: 342000,
      aovDeltaPct: 9.1,
      repeatRate: 31.4,
      repeatDeltaPct: 4.2,
      returnRate: 2.6,
      invDays: 18,
      invDeltaDays: -3,
      lsr: 0.6,
      nfr: 1.1,
      prepDays: 0.39,
      crr: 96.8,
      rating: 4.94,
      chatRespSeconds: 180,
      sqt: 0,
      violations: 0,
      preorderPct: 0.4,
      ctr: 5.18,
      atcRate: 17.2,
      cr: 3.85,
    }),
    skus: [
      {
        code: 'TT-AB-SET01',
        name: 'Bộ Đôi Sáng Da Serum C + Kem Chống Nắng Aurelia',
        platform: 'tiktok',
        shopId: 'tt-74918819',
        shopName: 'Aurelia Skincare VN',
        stock: 265,
        traffic: 19450,
        ctr: 5.6,
        cr: 4.3,
        rating: 4.96,
        revenue30d: 264.0,
      },
      {
        code: 'TT-AB-SRM02',
        name: 'Sữa Rửa Mặt Tạo Bọt Dịu Nhẹ pH 5.5 150ml',
        platform: 'tiktok',
        shopId: 'tt-74918819',
        shopName: 'Aurelia Skincare VN',
        stock: 320,
        traffic: 12100,
        ctr: 4.1,
        cr: 1.78,
        rating: 4.86,
        revenue30d: 148.0,
      },
    ],
  },
];

export function classifySkuAction(sku: Pick<SkuItem, 'stock' | 'ctr' | 'cr'>): {
  category: SkuActionCategory;
  actionLabel: string;
} {
  if (sku.stock < 20) {
    return { category: 'stock', actionLabel: 'Sắp hết hàng — Nhập gấp' };
  }
  if (sku.ctr < 2.5) {
    return { category: 'ctr', actionLabel: 'Tối ưu ảnh bìa / Giá mồi' };
  }
  if (sku.cr < 2.0) {
    return { category: 'cr', actionLabel: 'Kiểm tra đánh giá / Voucher' };
  }
  return { category: 'good', actionLabel: 'Tăng trưởng tốt — Đẩy Ads' };
}

export function filterSkus(
  skus: Array<SkuItem & { category: SkuActionCategory; actionLabel: string }>,
  opts: { keyword?: string; status?: string }
): Array<SkuItem & { category: SkuActionCategory; actionLabel: string }> {
  const kw = String(opts.keyword || '').trim().toLowerCase();
  const status = String(opts.status || 'all').trim().toLowerCase();
  return skus.filter((sku) => {
    if (kw) {
      const hay = `${sku.name} ${sku.code} ${sku.shopName}`.toLowerCase();
      if (!hay.includes(kw)) return false;
    }
    if (status === 'all') return true;
    if (status === 'action') return sku.category !== 'good';
    return sku.category === status;
  });
}

export function aggregateShopMetrics(shops: ShopProfile[], timeframe: TimeframeId): AggregatedMetrics {
  const list = shops.length > 0 ? shops : [ECOM_SHOPS[0]];
  const frames = list.map((s) => s.timeframes[timeframe]);

  const totalRev = frames.reduce((sum, f) => sum + f.netRev, 0);
  const safeRev = totalRev > 0 ? totalRev : 1;
  const totalProfit = frames.reduce((sum, f) => sum + f.netProfit, 0);
  const totalOrders = frames.reduce((sum, f) => sum + f.orders, 0);

  const wAvg = (pick: (f: TimeframeMetrics) => number) =>
    frames.reduce((sum, f) => sum + pick(f) * f.netRev, 0) / safeRev;

  const cogsPct = round1(wAvg((f) => f.cogsPct));
  const feePct = round1(wAvg((f) => f.feePct));
  const otherPct = round1(wAvg((f) => f.otherPct));
  const cir = round1(wAvg((f) => f.cir));
  const netMargin = round1((totalProfit / safeRev) * 100);
  const roas = cir > 0 ? round2(100 / cir) : 0;
  const breakevenCir = round1(Math.max(0, 100 - cogsPct - feePct - otherPct));
  const aov = totalOrders > 0 ? Math.round((totalRev * 1_000_000) / totalOrders) : 0;

  const chartLabels = frames[0].chartLabels;
  const revSeries = chartLabels.map((_, idx) =>
    round1(frames.reduce((sum, f) => sum + (f.revSeries[idx] || 0), 0))
  );
  const profitSeries = chartLabels.map((_, idx) =>
    round1(frames.reduce((sum, f) => sum + (f.profitSeries[idx] || 0), 0))
  );
  const adsSeries = chartLabels.map((_, idx) =>
    round1(frames.reduce((sum, f) => sum + (f.adsSeries[idx] || 0), 0))
  );

  const impressions = frames.reduce((sum, f) => sum + f.ops.impressions, 0);
  const traffic = frames.reduce((sum, f) => sum + f.ops.traffic, 0);
  const atc = frames.reduce((sum, f) => sum + f.ops.atc, 0);
  const opsOrders = frames.reduce((sum, f) => sum + f.ops.orders, 0);

  return {
    netRev: round1(totalRev),
    netRevDeltaPct: round1(wAvg((f) => f.netRevDeltaPct)),
    netProfit: round1(totalProfit),
    netMargin,
    netProfitDeltaPct: round1(wAvg((f) => f.netProfitDeltaPct)),
    cir,
    roas,
    breakevenCir,
    cirDeltaPct: round1(wAvg((f) => f.cirDeltaPct)),
    cogsPct,
    feePct,
    otherPct,
    orders: totalOrders,
    aov,
    aovDeltaPct: round1(wAvg((f) => f.aovDeltaPct)),
    repeatRate: round1(wAvg((f) => f.repeatRate)),
    repeatDeltaPct: round1(wAvg((f) => f.repeatDeltaPct)),
    returnRate: round1(wAvg((f) => f.returnRate)),
    invDays: Math.round(wAvg((f) => f.invDays)),
    invDeltaDays: Math.round(wAvg((f) => f.invDeltaDays)),
    chartLabels,
    revSeries,
    profitSeries,
    adsSeries,
    ops: {
      lsr: round1(wAvg((f) => f.ops.lsr)),
      nfr: round1(wAvg((f) => f.ops.nfr)),
      prepDays: round2(wAvg((f) => f.ops.prepDays)),
      crr: round1(wAvg((f) => f.ops.crr)),
      rating: round2(wAvg((f) => f.ops.rating)),
      chatRespSeconds: Math.round(wAvg((f) => f.ops.chatRespSeconds)),
      sqt: Math.max(...frames.map((f) => f.ops.sqt)),
      violations: frames.reduce((sum, f) => sum + f.ops.violations, 0),
      preorderPct: round1(wAvg((f) => f.ops.preorderPct)),
      impressions,
      traffic,
      ctr: impressions > 0 ? round2((traffic / impressions) * 100) : 0,
      atc,
      atcRate: traffic > 0 ? round1((atc / traffic) * 100) : 0,
      orders: opsOrders,
      cr: traffic > 0 ? round2((opsOrders / traffic) * 100) : 0,
    },
  };
}

export function simulateProfit(input: SimulatorInput): SimulatorResult {
  const fixedOpsCost = typeof input.fixedOpsCost === 'number' ? input.fixedOpsCost : 5.3;
  const maxCir = round1(Math.max(0, 100 - input.cogsPct - input.feePct - fixedOpsCost));
  const netMargin = round1(100 - input.cogsPct - input.feePct - fixedOpsCost - input.cir);
  const netProfit = round1((input.baseRev * netMargin) / 100);
  const breakevenRoas = maxCir > 0 ? round2(100 / maxCir) : null;

  if (netMargin >= 15) {
    return {
      maxCir,
      netMargin,
      netProfit,
      breakevenRoas,
      status: 'safe',
      advice: `Biên an toàn cao (${netMargin.toFixed(1)}%): Có thể đẩy thêm ngân sách Ads để chiếm thị phần.`,
    };
  }
  if (netMargin > 0) {
    return {
      maxCir,
      netMargin,
      netProfit,
      breakevenRoas,
      status: 'thin',
      advice: `Biên lợi nhuận mỏng (${netMargin.toFixed(1)}%): Cần tối ưu giá thầu Ads hoặc tăng giá bán combo.`,
    };
  }
  return {
    maxCir,
    netMargin,
    netProfit,
    breakevenRoas,
    status: 'loss',
    advice: `Cảnh báo lỗ ròng (${netMargin.toFixed(1)}%): Chi phí Ads vượt ngưỡng hòa vốn, cần giảm thầu ngay.`,
  };
}

export function normalizePlatformId(raw: unknown): PlatformId {
  const s = String(raw || 'all').trim().toLowerCase();
  if (s === 'shopee' || s === 'tiktok') return s;
  return 'all';
}

export function normalizeTimeframeId(raw: unknown): TimeframeId {
  const s = String(raw || '30d').trim().toLowerCase();
  if (s === 'today' || s === '7d' || s === '30d' || s === 'month') return s;
  return '30d';
}

export function buildDashboardSnapshot(params?: {
  platform?: unknown;
  shopId?: unknown;
  timeframe?: unknown;
  updatedAt?: string;
}): DashboardSnapshot {
  const platform = normalizePlatformId(params?.platform);
  const timeframe = normalizeTimeframeId(params?.timeframe);
  const rawShopId = String(params?.shopId || 'all').trim();

  const shopeeShops = ECOM_SHOPS.filter((s) => s.platform === 'shopee');
  const tiktokShops = ECOM_SHOPS.filter((s) => s.platform === 'tiktok');

  const shopeeAgg = aggregateShopMetrics(shopeeShops, timeframe);
  const tiktokAgg = aggregateShopMetrics(tiktokShops, timeframe);

  const platformShops =
    platform === 'shopee' ? shopeeShops : platform === 'tiktok' ? tiktokShops : ECOM_SHOPS;

  const matchedShop =
    platform !== 'all' && rawShopId !== 'all'
      ? platformShops.find((s) => s.id === rawShopId) || null
      : null;

  const activeShopId = matchedShop ? matchedShop.id : 'all';
  const activeShops = matchedShop ? [matchedShop] : platformShops;
  const metrics = aggregateShopMetrics(activeShops, timeframe);

  const countAlerts = (shop: ShopProfile) =>
    shop.skus.filter((sku) => classifySkuAction(sku).category !== 'good').length;

  const platformsSummary = [
    {
      id: 'shopee' as const,
      name: 'Shopee',
      shopCount: shopeeShops.length,
      netRev: shopeeAgg.netRev,
      netProfit: shopeeAgg.netProfit,
      netMargin: shopeeAgg.netMargin,
      orders: shopeeAgg.orders,
    },
    {
      id: 'tiktok' as const,
      name: 'TikTok Shop',
      shopCount: tiktokShops.length,
      netRev: tiktokAgg.netRev,
      netProfit: tiktokAgg.netProfit,
      netMargin: tiktokAgg.netMargin,
      orders: tiktokAgg.orders,
    },
  ];

  const shopsInPlatform = (platform === 'all' ? [] : platformShops).map((s) => {
    const m = aggregateShopMetrics([s], timeframe);
    return {
      id: s.id,
      platform: s.platform,
      name: s.name,
      tierLabel: s.tierLabel,
      category: s.category,
      sellerUrl: s.sellerUrl,
      netRev: m.netRev,
      netMargin: m.netMargin,
      orders: m.orders,
      alertCount: countAlerts(s),
    };
  });

  const breakdownKind: 'platform' | 'shop' = platform === 'all' ? 'platform' : 'shop';
  const breakdownRows: BreakdownRow[] =
    platform === 'all'
      ? [
          {
            id: 'shopee',
            kind: 'platform',
            platform: 'shopee',
            shopId: 'all',
            name: 'Shopee',
            subtitle: `${shopeeShops.length} gian hàng đang hoạt động`,
            shopCount: shopeeShops.length,
            netRev: shopeeAgg.netRev,
            netProfit: shopeeAgg.netProfit,
            netMargin: shopeeAgg.netMargin,
            cir: shopeeAgg.cir,
            roas: shopeeAgg.roas,
            orders: shopeeAgg.orders,
            rating: shopeeAgg.ops.rating,
            sqt: shopeeAgg.ops.sqt,
            alertCount: shopeeShops.reduce((sum, s) => sum + countAlerts(s), 0),
          },
          {
            id: 'tiktok',
            kind: 'platform',
            platform: 'tiktok',
            shopId: 'all',
            name: 'TikTok Shop',
            subtitle: `${tiktokShops.length} gian hàng đang hoạt động`,
            shopCount: tiktokShops.length,
            netRev: tiktokAgg.netRev,
            netProfit: tiktokAgg.netProfit,
            netMargin: tiktokAgg.netMargin,
            cir: tiktokAgg.cir,
            roas: tiktokAgg.roas,
            orders: tiktokAgg.orders,
            rating: tiktokAgg.ops.rating,
            sqt: tiktokAgg.ops.sqt,
            alertCount: tiktokShops.reduce((sum, s) => sum + countAlerts(s), 0),
          },
        ]
      : platformShops.map((s) => {
          const m = aggregateShopMetrics([s], timeframe);
          return {
            id: s.id,
            kind: 'shop',
            platform: s.platform,
            shopId: s.id,
            name: s.name,
            subtitle: `${s.tierLabel} · ${s.category}`,
            netRev: m.netRev,
            netProfit: m.netProfit,
            netMargin: m.netMargin,
            cir: m.cir,
            roas: m.roas,
            orders: m.orders,
            rating: m.ops.rating,
            sqt: m.ops.sqt,
            alertCount: countAlerts(s),
          };
        });

  const skus = activeShops
    .flatMap((s) => s.skus)
    .map((sku) => ({
      ...sku,
      ...classifySkuAction(sku),
    }))
    .sort((a, b) => b.traffic - a.traffic);

  let scopeTitle = 'Tổng Quan Thương Mại Điện Tử';
  let scopeSubtitle = `Hợp nhất 2 nền tảng (Shopee & TikTok Shop) · ${ECOM_SHOPS.length} gian hàng đang đồng bộ`;
  let breadcrumb = ['Tổng quan TMĐT'];

  if (platform === 'shopee') {
    if (matchedShop) {
      scopeTitle = matchedShop.name;
      scopeSubtitle = `Shopee · ${matchedShop.tierLabel} · ${matchedShop.category} · ID ${matchedShop.id}`;
      breadcrumb = ['Tổng quan TMĐT', 'Shopee', matchedShop.name];
    } else {
      scopeTitle = 'Shopee Command Center';
      scopeSubtitle = `Tổng hợp ${shopeeShops.length} gian hàng Shopee (AuraHome, Aurelia Beauty, TP Decor)`;
      breadcrumb = ['Tổng quan TMĐT', 'Shopee', `Tất cả ${shopeeShops.length} shop`];
    }
  } else if (platform === 'tiktok') {
    if (matchedShop) {
      scopeTitle = matchedShop.name;
      scopeSubtitle = `TikTok Shop · ${matchedShop.tierLabel} · ${matchedShop.category} · ID ${matchedShop.id}`;
      breadcrumb = ['Tổng quan TMĐT', 'TikTok Shop', matchedShop.name];
    } else {
      scopeTitle = 'TikTok Shop Command Center';
      scopeSubtitle = `Tổng hợp ${tiktokShops.length} gian hàng TikTok Shop (LIVE & Affiliate)`;
      breadcrumb = ['Tổng quan TMĐT', 'TikTok Shop', `Tất cả ${tiktokShops.length} shop`];
    }
  }

  return {
    platform,
    shopId: activeShopId,
    timeframe,
    scopeTitle,
    scopeSubtitle,
    breadcrumb,
    platforms: platformsSummary,
    shopsInPlatform,
    metrics,
    breakdownKind,
    breakdownRows,
    skus,
    updatedAt: params?.updatedAt || '18/11/2026 18:55',
  };
}
