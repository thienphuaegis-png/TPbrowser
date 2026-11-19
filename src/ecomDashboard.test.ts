import test from 'node:test';
import assert from 'node:assert/strict';
import {
  ECOM_SHOPS,
  buildDashboardSnapshot,
  classifySkuAction,
  filterSkus,
  simulateProfit,
} from './ecomDashboard.js';

test('all-platform overview aggregates Shopee and TikTok revenue, profit, and orders accurately', () => {
  const allSnap = buildDashboardSnapshot({ platform: 'all', shopId: 'all', timeframe: '30d' });
  const shopeeSnap = buildDashboardSnapshot({ platform: 'shopee', shopId: 'all', timeframe: '30d' });
  const tiktokSnap = buildDashboardSnapshot({ platform: 'tiktok', shopId: 'all', timeframe: '30d' });

  assert.equal(allSnap.breakdownKind, 'platform');
  assert.equal(allSnap.breakdownRows.length, 2);
  assert.ok(Math.abs(allSnap.metrics.netRev - (shopeeSnap.metrics.netRev + tiktokSnap.metrics.netRev)) < 0.2);
  assert.ok(
    Math.abs(allSnap.metrics.netProfit - (shopeeSnap.metrics.netProfit + tiktokSnap.metrics.netProfit)) < 0.2
  );
  assert.equal(allSnap.metrics.orders, shopeeSnap.metrics.orders + tiktokSnap.metrics.orders);
  assert.equal(allSnap.skus.length, ECOM_SHOPS.reduce((sum, s) => sum + s.skus.length, 0));
});

test('Shopee platform drills down to 3 shops and single-shop selection isolates metrics and SKUs', () => {
  const shopeeAll = buildDashboardSnapshot({ platform: 'shopee', shopId: 'all', timeframe: '30d' });
  assert.equal(shopeeAll.shopsInPlatform.length, 3);
  assert.equal(shopeeAll.breakdownKind, 'shop');
  assert.equal(shopeeAll.breakdownRows.length, 3);

  const auraHome = buildDashboardSnapshot({ platform: 'shopee', shopId: '882410912', timeframe: '30d' });
  assert.equal(auraHome.shopId, '882410912');
  assert.equal(auraHome.scopeTitle, 'AuraHome Official Store');
  assert.equal(auraHome.metrics.netRev, 845.2);
  assert.equal(auraHome.metrics.cir, 11.8);
  assert.ok(auraHome.skus.every((sku) => sku.shopId === '882410912'));
  assert.ok(auraHome.metrics.netRev < shopeeAll.metrics.netRev);
});

test('timeframe switching updates revenue scale and chart label buckets', () => {
  const today = buildDashboardSnapshot({ platform: 'shopee', shopId: '882410912', timeframe: 'today' });
  const week = buildDashboardSnapshot({ platform: 'shopee', shopId: '882410912', timeframe: '7d' });
  const month30 = buildDashboardSnapshot({ platform: 'shopee', shopId: '882410912', timeframe: '30d' });

  assert.ok(today.metrics.netRev < week.metrics.netRev);
  assert.ok(week.metrics.netRev < month30.metrics.netRev);
  assert.equal(today.metrics.chartLabels.length, 7);
  assert.equal(month30.metrics.chartLabels.length, 4);
});

test('profit and CIR simulator computes break-even thresholds and classifies safe/thin/loss states', () => {
  const safe = simulateProfit({ baseRev: 845.2, cir: 11.8, feePct: 13.5, cogsPct: 48.0, fixedOpsCost: 5.3 });
  assert.equal(safe.status, 'safe');
  assert.equal(safe.maxCir, 33.2);
  assert.equal(safe.netMargin, 21.4);
  assert.equal(safe.breakevenRoas, 3.01);

  const thin = simulateProfit({ baseRev: 845.2, cir: 25.0, feePct: 13.5, cogsPct: 48.0, fixedOpsCost: 5.3 });
  assert.equal(thin.status, 'thin');
  assert.equal(thin.netMargin, 8.2);

  const loss = simulateProfit({ baseRev: 845.2, cir: 38.0, feePct: 13.5, cogsPct: 48.0, fixedOpsCost: 5.3 });
  assert.equal(loss.status, 'loss');
  assert.ok(loss.netMargin < 0);
  assert.ok(loss.netProfit < 0);
});

test('SKU action classifier and search/filter logic match operational alert rules', () => {
  assert.equal(classifySkuAction({ stock: 12, ctr: 5.0, cr: 4.5 }).category, 'stock');
  assert.equal(classifySkuAction({ stock: 100, ctr: 1.9, cr: 4.0 }).category, 'ctr');
  assert.equal(classifySkuAction({ stock: 100, ctr: 4.2, cr: 1.5 }).category, 'cr');
  assert.equal(classifySkuAction({ stock: 100, ctr: 4.5, cr: 3.9 }).category, 'good');

  const snap = buildDashboardSnapshot({ platform: 'all', shopId: 'all', timeframe: '30d' });
  const stockAlerts = filterSkus(snap.skus, { status: 'stock' });
  assert.ok(stockAlerts.length >= 3);
  assert.ok(stockAlerts.every((s) => s.stock < 20));

  const searchRes = filterSkus(snap.skus, { keyword: 'serum', status: 'all' });
  assert.ok(searchRes.length >= 2);
  assert.ok(searchRes.every((s) => s.name.toLowerCase().includes('serum')));
});
