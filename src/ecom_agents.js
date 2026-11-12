// Agent chạy ngầm trên Worker Tab Shopee (banhang.shopee.vn) — kế thừa từ stz-ai-hub / shopee-scraper.
// CHÚ Ý: Không nạp script này vào TikTok Seller để giữ 100% nguyên bản WebView2 chống CAPTCHA SecSDK.
(function () {
  if (window.__tpEcomAgentLoaded) return;
  window.__tpEcomAgentLoaded = true;

  var host = window.location.hostname || "";
  if (!host.includes("banhang.shopee.vn")) return;

  function sendRust(payload) {
    try {
      if (window.ipc && typeof window.ipc.postMessage === "function") {
        window.ipc.postMessage(JSON.stringify(payload));
      }
    } catch (_) {}
  }

  function stripVn(str) {
    return String(str || "")
      .replace(/\u00a0/g, " ")
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "")
      .replace(/đ/g, "d")
      .replace(/Đ/g, "D")
      .toLowerCase()
      .replace(/[^a-z0-9\s]/g, " ")
      .replace(/\s+/g, " ")
      .trim();
  }

  function parseVnNum(s) {
    if (typeof s === "number") return s;
    if (!s) return 0;
    var t = String(s).trim().toLowerCase();
    var mult = 1;
    if (t.includes("tỷ") || t.includes("ty")) mult = 1000000000;
    else if (t.includes("triệu") || t.includes("trieu") || /\d\s*tr\b/.test(t)) mult = 1000000;
    else if (/\d\s*k\b/.test(t)) mult = 1000;
    t = t.replace(/[^\d.,-]/g, "");
    if (!t) return 0;
    if (mult > 1) {
      t = t.replace(/,/g, ".");
    } else if (t.includes(".") && t.includes(",")) {
      t = t.lastIndexOf(",") > t.lastIndexOf(".")
        ? t.replace(/\./g, "").replace(",", ".")
        : t.replace(/,/g, "");
    } else if (t.includes(",")) {
      var p = t.split(",");
      t = p.length === 2 && p[1].length <= 2 ? t.replace(",", ".") : t.replace(/,/g, "");
    } else if (t.includes(".")) {
      var p2 = t.split(".");
      if ((p2.length === 2 && p2[1].length === 3) || p2.length > 2) t = t.replace(/\./g, "");
    }
    var v = parseFloat(t);
    return isNaN(v) ? 0 : v * mult;
  }

  function normalizeShopeeMicroVnd(n) {
    var val = Number(n || 0);
    return val >= 50000000 ? Math.round(val / 100000) : Math.round(val);
  }

  function fmtVnd(n) {
    return Math.round(Number(n || 0)).toLocaleString("vi-VN") + "đ";
  }

  function sleep(ms) {
    return new Promise(function (r) { setTimeout(r, ms); });
  }

  function getSpcCds() {
    var m = (document.cookie || "").match(/SPC_CDS=([^;]+)/);
    return m ? m[1] : "";
  }

  var orderStore = {
    orders: new Map(),
    tabCounts: {},
    extras: {
      rating_summary: null,
      bad_reviews: [],
      fake_reviews: [],
      live: null,
      discounts: [],
      vouchers: [],
      flash_sales: []
    }
  };

  function ingestShopeeOrderApi(url, data) {
    if (!data || typeof data !== "object") return;
    var d = data.data || data;
    if (url.includes("get_order_list_meta") || url.includes("get_package_list") || url.includes("search_order_list")) {
      var tabs = d.tab_info || d.tabs || d.order_count_info || d.status_counts;
      if (Array.isArray(tabs)) {
        tabs.forEach(function (t) {
          var name = t.label || t.name || t.tab_name || t.status;
          var cnt = t.count !== undefined ? t.count : (t.total !== undefined ? t.total : t.order_count);
          if (name !== undefined && cnt !== undefined) orderStore.tabCounts[String(name)] = Number(cnt);
        });
      }
    }

    var cardList =
      (d.card_list && Array.isArray(d.card_list) ? d.card_list : null) ||
      (d.orders && Array.isArray(d.orders) ? d.orders : null) ||
      (d.list && Array.isArray(d.list) ? d.list : null) ||
      [];

    var updated = false;
    cardList.forEach(function (card) {
      var c = card.order_card || card.package_level_order_card || card;
      var header = c.card_header || c.header || c;
      var orderSn = c.order_sn || header.order_sn || (c.order_info && c.order_info.order_sn) || c.package_number || "";
      if (!orderSn) return;

      var orderId =
        (c.order_ext_info && c.order_ext_info.order_id) ||
        header.order_id ||
        c.order_id ||
        "";

      var buyer =
        (header.buyer_info && (header.buyer_info.username || header.buyer_info.name)) ||
        header.buyer_name ||
        c.buyer_user_name ||
        "Khách Shopee";

      var statusText =
        (c.status_info && (c.status_info.status || c.status_info.status_text)) ||
        c.order_status_text ||
        "Đơn Shopee";

      var shippingMethod =
        (c.fulfilment_info && (c.fulfilment_info.masked_channel_name || c.fulfilment_info.channel_name)) ||
        (c.shipping_info && c.shipping_info.shipping_carrier) ||
        c.fulfillment_carrier_name ||
        c.logistics_channel_name ||
        "";

      var rawTotal = 0;
      if (c.payment_info && c.payment_info.total_price !== undefined) rawTotal = c.payment_info.total_price;
      else if (c.total_price !== undefined) rawTotal = c.total_price;
      else if (c.final_total !== undefined) rawTotal = c.final_total;
      else if (c.price_info && c.price_info.total_price !== undefined) rawTotal = c.price_info.total_price;
      var normTotal = normalizeShopeeMicroVnd(parseVnNum(rawTotal));

      var groups = [];
      if (c.item_info_group && Array.isArray(c.item_info_group.item_info_list)) {
        groups = c.item_info_group.item_info_list;
      } else if (Array.isArray(c.package_list)) {
        c.package_list.forEach(function (pkg) {
          if (pkg.item_info_group && Array.isArray(pkg.item_info_group.item_info_list)) {
            groups = groups.concat(pkg.item_info_group.item_info_list);
          }
        });
      } else if (c.item_info && Array.isArray(c.item_info.item_group_list)) {
        groups = c.item_info.item_group_list;
      } else if (Array.isArray(c.item_list)) {
        groups = c.item_list;
      }

      var flatItems = [];
      groups.forEach(function (g) {
        if (Array.isArray(g.item_list) && g.item_list.length > 0) flatItems = flatItems.concat(g.item_list);
        else if (g.name || g.item_name || g.product_name) flatItems.push(g);
      });

      flatItems.forEach(function (it, idx) {
        var itemName = it.item_name || it.name || it.product_name || "";
        var rawDesc = String(it.description || "").replace(/^Variation:\s*/i, "").trim();
        var modelName = it.model_name || it.variation_name || rawDesc || "";
        var qty = it.amount !== undefined ? it.amount : (it.quantity !== undefined ? it.quantity : 1);
        var rawItemPrice =
          it.item_price ||
          it.order_price ||
          it.model_discounted_price ||
          it.price ||
          (it.price_info && it.price_info.current_price) ||
          0;
        var normItemPrice = normalizeShopeeMicroVnd(parseVnNum(rawItemPrice));
        var key = String(orderSn) + "_" + idx;
        orderStore.orders.set(key, {
          order_id: String(orderId || ""),
          order_sn: String(orderSn),
          buyer: String(buyer),
          status: String(statusText),
          item_name: String(itemName),
          model_name: String(modelName),
          qty: Number(qty) || 1,
          item_price: normItemPrice > 0 ? String(normItemPrice) : "",
          total_price: normTotal > 0 ? String(normTotal) : "",
          shipping: String(shippingMethod)
        });
        updated = true;
      });
    });

    if (updated && !window.__tpBulkApiFetching) {
      clearTimeout(window.__tpOrderSummaryTimer);
      window.__tpOrderSummaryTimer = setTimeout(computeShopeeOrderSummary, 350);
    }
  }

  // Hook thụ động fetch + XHR khi ở trang đơn hàng
  try {
    var origFetch = window.fetch;
    window.fetch = async function () {
      var args = arguments;
      var res = await origFetch.apply(this, args);
      try {
        var url = typeof args[0] === "string" ? args[0] : (args[0] && args[0].url) || "";
        if (url.includes("/api/v3/order/") || url.includes("/api/v3/logistics/")) {
          res.clone().json().then(function (j) { ingestShopeeOrderApi(url, j); }).catch(function () {});
        }
      } catch (_) {}
      return res;
    };

    var origOpen = XMLHttpRequest.prototype.open;
    var origSend = XMLHttpRequest.prototype.send;
    XMLHttpRequest.prototype.open = function (method, url) {
      this.__tp_url = String(url || "");
      return origOpen.apply(this, arguments);
    };
    XMLHttpRequest.prototype.send = function () {
      this.addEventListener("load", function () {
        try {
          if (this.__tp_url && (this.__tp_url.includes("/api/v3/order/") || this.__tp_url.includes("/api/v3/logistics/"))) {
            ingestShopeeOrderApi(this.__tp_url, JSON.parse(this.responseText));
          }
        } catch (_) {}
      });
      return origSend.apply(this, arguments);
    };
  } catch (_) {}

  function computeShopeeOrderSummary() {
    var domCounts = {};
    document.querySelectorAll(".eds-tabs__nav-tab, [class*='tab-item'], [role='tab']").forEach(function (el) {
      var txt = (el.innerText || "").replace(/\s+/g, " ").trim();
      var m = txt.match(/^(.+?)\s*\((\d+)\)$/);
      if (m) domCounts[m[1].trim()] = parseInt(m[2], 10);
    });
    var counts = Object.assign({}, domCounts, orderStore.tabCounts);
    var rows = Array.from(orderStore.orders.values());

    var orderMap = new Map();
    var productPrices = new Map();
    var expressOrders = 0;

    rows.forEach(function (row) {
      if (!orderMap.has(row.order_sn)) {
        orderMap.set(row.order_sn, {
          order_id: row.order_id || "",
          order_sn: row.order_sn,
          buyer: row.buyer,
          status: row.status,
          shipping: row.shipping,
          order_total: normalizeShopeeMicroVnd(parseVnNum(row.total_price)),
          items: []
        });
        if (/hoa toc|instant|spx instant|grabexpress|bedelivery/i.test(stripVn(row.shipping + " " + row.status))) {
          expressOrders++;
        }
      }
      var ord = orderMap.get(row.order_sn);
      var rawItemPrice = normalizeShopeeMicroVnd(parseVnNum(row.item_price));
      var qty = Math.max(1, parseInt(row.qty || "1", 10));
      ord.items.push({
        item_name: row.item_name || "",
        model_name: row.model_name || "",
        qty: qty,
        item_price: rawItemPrice
      });
    });

    orderMap.forEach(function (ord) {
      var nonGift = ord.items.filter(function (it) {
        return !/qua tang|thu cam on|gift|sample|mau thu|tang kem/i.test(stripVn(it.item_name + " " + it.model_name));
      });
      var totalQty = nonGift.reduce(function (acc, it) { return acc + it.qty; }, 0) || 1;
      nonGift.forEach(function (it) {
        var effPrice = it.item_price > 0
          ? it.item_price
          : (nonGift.length === 1 && ord.order_total > 0 ? Math.round(ord.order_total / totalQty) : 0);
        it.effective_price = effPrice;
        if (effPrice > 0) {
          var key = stripVn(it.item_name).slice(0, 40) + "::" + stripVn(it.model_name).slice(0, 32);
          if (!productPrices.has(key)) productPrices.set(key, []);
          productPrices.get(key).push(effPrice);
        }
      });
    });

    var anomalies = [];
    orderMap.forEach(function (ord) {
      var nonGiftCount = ord.items.filter(function (it) {
        return !/qua tang|thu cam on|gift|sample|mau thu|tang kem/i.test(stripVn(it.item_name + " " + it.model_name));
      }).length;
      ord.items.forEach(function (it) {
        if (/qua tang|thu cam on|gift|sample|mau thu|tang kem/i.test(stripVn(it.item_name + " " + it.model_name))) return;
        var effPrice = it.effective_price || 0;
        var key = stripVn(it.item_name).slice(0, 40) + "::" + stripVn(it.model_name).slice(0, 32);
        var prices = (productPrices.get(key) || []).slice().sort(function (a, b) { return a - b; });
        var median = prices.length >= 2 ? prices[Math.floor(prices.length / 2)] : 0;
        var reasons = [];
        if (ord.order_total > 0 && ord.order_total < 15000 && nonGiftCount <= 1) {
          reasons.push("Đơn dưới 15.000đ (" + fmtVnd(ord.order_total) + ")");
        }
        if (median >= 20000 && effPrice > 0 && effPrice < median * 0.72) {
          var dropPct = Math.round((1 - effPrice / median) * 100);
          reasons.push("Tụt giá -" + dropPct + "% so với trung vị (" + fmtVnd(median) + ")");
        }
        if (reasons.length > 0) {
          anomalies.push({
            order_id: ord.order_id || "",
            order_sn: ord.order_sn,
            buyer: ord.buyer,
            status: ord.status,
            shipping: ord.shipping,
            item_name: it.item_name,
            model_name: it.model_name,
            qty: it.qty,
            effective_price: effPrice > 0 ? effPrice : ord.order_total,
            median_price: median,
            order_total: ord.order_total,
            reasons: reasons,
            url: ord.order_id
              ? ("https://banhang.shopee.vn/portal/sale/order/" + ord.order_id)
              : "https://banhang.shopee.vn/portal/sale/order"
          });
        }
      });
    });
    anomalies.sort(function (a, b) { return (a.effective_price || 0) - (b.effective_price || 0); });

    var payload = {
      ts: new Date().toLocaleTimeString("vi-VN"),
      logged_in: !/login|signin/i.test(window.location.pathname),
      to_ship: counts["Chờ lấy hàng"] !== undefined ? counts["Chờ lấy hàng"] : (counts["To Ship"] || 0),
      shipping: counts["Đang giao"] !== undefined ? counts["Đang giao"] : (counts["Shipping"] || 0),
      unpaid: counts["Chờ xác nhận"] !== undefined ? counts["Chờ xác nhận"] : (counts["Unpaid"] || 0),
      completed: counts["Đã giao"] !== undefined ? counts["Đã giao"] : (counts["Completed"] || 0),
      express_orders: expressOrders,
      scanned_orders: orderMap.size,
      scanned_lines: rows.length,
      anomalies: anomalies,
      orders: Array.from(orderMap.values()).slice(0, 200),
      raw_rows: rows.slice(0, 600),
      rating_summary: orderStore.extras.rating_summary,
      bad_reviews: orderStore.extras.bad_reviews,
      fake_reviews: orderStore.extras.fake_reviews,
      live: orderStore.extras.live,
      discounts: orderStore.extras.discounts,
      vouchers: orderStore.extras.vouchers,
      flash_sales: orderStore.extras.flash_sales
    };

    try {
      var s = JSON.stringify(payload);
      sendRust({ cmd: "update_platform_data", platform: "shopee_orders", data_json: s });
    } catch (_) {}
    return payload;
  }

  function detectFakeKeywords(text) {
    var norm = stripVn(text);
    if (!norm) return [];
    var rules = [
      { kw: "hang gia", label: "Hàng giả" },
      { kw: "hang nhai", label: "Hàng nhái" },
      { kw: "do gia", label: "Đồ giả" },
      { kw: "do nhai", label: "Đồ nhái" },
      { kw: "fake", label: "Fake" },
      { kw: "gia mao", label: "Giả mạo" },
      { kw: "khong chinh hang", label: "Không chính hãng" },
      { kw: "pha tron", label: "Nghi pha trộn" },
      { kw: "hang deu", label: "Hàng đểu" },
      { kw: "troi noi", label: "Trôi nổi" },
      { kw: "lua dao", label: "Nghi lừa đảo" },
      { kw: "het han", label: "Hết hạn" },
      { kw: "out date", label: "Out date" },
      { kw: "mui la", label: "Mùi lạ" },
      { kw: "mui ki", label: "Mùi kì" },
      { kw: "mui gat", label: "Mùi gắt" },
      { kw: "bien chat", label: "Biến chất" }
    ];
    var hits = [];
    for (var i = 0; i < rules.length; i++) {
      if (norm.indexOf(rules[i].kw) !== -1) hits.push(rules[i].label);
    }
    if (hits.length === 0 && /\b(hang gia|ban gia|do gia|gia mao|hang nhai|do nhai|nhai lai)\b/.test(norm)) {
      hits.push("Nghi hàng giả/nhái");
    }
    return hits;
  }

  async function safeJsonGet(url) {
    try {
      var r = await fetch(url, { credentials: "include" });
      return await r.json();
    } catch (_) {
      return null;
    }
  }

  async function safeJsonPost(url, bodyObj) {
    try {
      var r = await fetch(url, {
        method: "POST",
        credentials: "include",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(bodyObj)
      });
      return await r.json();
    } catch (_) {
      return null;
    }
  }

  async function fetchShopeeRatings(spcCds) {
    var q = "?SPC_CDS=" + encodeURIComponent(spcCds) + "&SPC_CDS_VER=2";
    var dashRes = await safeJsonGet("/api/v3/settings/get_rating_dashboard/" + q);
    var badReviews = [];
    var fakeReviews = [];
    var counts = null;
    var seenIds = {};

    var cursor = "0";
    for (var p = 1; p <= 5; p++) {
      var url = "/api/v3/settings/search_shop_rating_comments_new/" + q +
        "&rating_star=1,2,3&page_number=" + p +
        "&page_size=40&cursor=" + encodeURIComponent(cursor) +
        "&from_page_number=" + (p > 1 ? p - 1 : 1) + "&language=vi";
      var jr = await safeJsonGet(url);
      var d = jr && jr.data;
      if (!d) break;
      if (!counts && d.counts) counts = d.counts;
      var list = Array.isArray(d.list) ? d.list : [];
      if (list.length === 0) break;
      cursor = String(list[list.length - 1].comment_id || "0");
      for (var i = 0; i < list.length; i++) {
        var x = list[i];
        var cid = String(x.comment_id || (x.order_sn + "_" + i));
        if (seenIds[cid]) continue;
        seenIds[cid] = true;
        var commentText = String(x.comment || "").trim();
        var reasonsText = Array.isArray(x.low_rating_reasons) ? x.low_rating_reasons.join(" ") : "";
        var kwHits = detectFakeKeywords(commentText + " " + reasonsText);
        var isFake = kwHits.length > 0;
        var item = {
          comment_id: cid,
          rating_star: Number(x.rating_star || 0),
          comment: commentText,
          user_name: String(x.user_name || "Người mua"),
          order_id: x.order_id ? String(x.order_id) : "",
          order_sn: String(x.order_sn || ""),
          product_name: String(x.product_name || ""),
          model_name: String(x.model_name || ""),
          ctime: Number(x.ctime || x.submit_time || 0),
          replied: !!x.reply,
          has_images: !!(x.images && x.images.length),
          is_fake_alert: isFake,
          alert_keywords: kwHits,
          url: "https://banhang.shopee.vn/portal/settings/shop/rating",
          order_url: x.order_id ? ("https://banhang.shopee.vn/portal/sale/order/" + x.order_id) : ""
        };
        badReviews.push(item);
        if (isFake) fakeReviews.push(item);
      }
      if (list.length < 40) break;
    }

    var metric = (dashRes && dashRes.data && dashRes.data.metric) || {};
    var dashData = (dashRes && dashRes.data) || {};
    orderStore.extras.rating_summary = {
      good_rating_rate: metric.good_rating_rate ? Number((metric.good_rating_rate * 100).toFixed(2)) : 0,
      order_review_rate: metric.order_review_rate ? Number((metric.order_review_rate * 100).toFixed(2)) : 0,
      unreplied_bad_ratings: Number(dashData.unreplied_bad_ratings || (counts && counts.to_reply_count) || 0),
      one_star_count: Number((counts && counts.one_star_count) || 0),
      two_star_count: Number((counts && counts.two_star_count) || 0),
      three_star_count: Number((counts && counts.three_star_count) || 0),
      to_reply_count: Number((counts && counts.to_reply_count) || 0),
      scanned_count: badReviews.length,
      fake_alert_count: fakeReviews.length
    };
    orderStore.extras.bad_reviews = badReviews;
    orderStore.extras.fake_reviews = fakeReviews;
  }

  async function fetchShopeeLive() {
    var now = new Date();
    var endDate = now.getFullYear() + "-" + String(now.getMonth() + 1).padStart(2, "0") + "-" + String(now.getDate()).padStart(2, "0");
    var jr = await safeJsonGet("/api/supply/lm/sellercenter/liveList/v2?page=1&pageSize=10&timeDim=7d&endDate=" + endDate);
    if (!jr || !jr.data || !Array.isArray(jr.data.list) || jr.data.list.length === 0) {
      jr = await safeJsonGet("/api/supply/lm/sellercenter/liveList/v2?page=1&pageSize=10&timeDim=30d&endDate=" + endDate);
    }
    var list = (jr && jr.data && Array.isArray(jr.data.list)) ? jr.data.list : [];
    var sessions = list.map(function (s) {
      return {
        session_id: String(s.sessionId || ""),
        title: String(s.title || "Shopee Live"),
        status: Number(s.status || 2),
        is_live: Number(s.status) === 1,
        start_time_ms: Number(s.startTime || 0),
        duration_ms: Number(s.duration || 0),
        viewers: Number(s.viewers || 0),
        avg_ccu: Number(s.avgEngagedCcu || s.engagedUv || 0),
        atc: Number(s.atc || 0),
        comments: Number(s.comments || 0),
        placed_orders: Number(s.placedOrders || 0),
        confirmed_orders: Number(s.confirmedOrders || 0),
        confirmed_sales: Number(s.confirmedSales || 0),
        url: "https://banhang.shopee.vn/creator-center/insight/live"
      };
    });
    var isLiveNow = sessions.some(function (s) { return s.is_live; });
    orderStore.extras.live = {
      is_live_now: isLiveNow,
      total_sessions: Number((jr && jr.data && jr.data.total) || sessions.length),
      sessions: sessions,
      url: "https://banhang.shopee.vn/creator-center/insight/live"
    };
  }

  async function fetchShopeePromotions(spcCds) {
    var q = "?SPC_CDS=" + encodeURIComponent(spcCds) + "&SPC_CDS_VER=2";
    var nowSec = Math.floor(Date.now() / 1000);
    var next24h = nowSec + 86400;

    var dOngoing1 = await safeJsonPost("/api/marketing/v3/public/discount/list/" + q, { offset: 0, limit: 10, discount_type: 0, time_status: 2 });
    var dOngoing2 = await safeJsonPost("/api/marketing/v3/public/discount/list/" + q, { offset: 10, limit: 10, discount_type: 0, time_status: 2 });
    var dUpcoming = await safeJsonPost("/api/marketing/v3/public/discount/list/" + q, { offset: 0, limit: 10, discount_type: 0, time_status: 1 });
    var discItems = [];
    var seenDisc = {};
    [dOngoing1, dOngoing2, dUpcoming].forEach(function (res) {
      var arr = (res && res.data && Array.isArray(res.data.discounts)) ? res.data.discounts : [];
      arr.forEach(function (entry) {
        var dtype = Number(entry.discount_type || 1);
        var obj = entry.seller_discount || entry.add_on_deal || entry.bundle_deal;
        if (!obj) return;
        var id = String(obj.discount_id || obj.bundle_deal_id || obj.add_on_deal_id || "");
        if (!id || seenDisc[id]) return;
        seenDisc[id] = true;
        var kindLabel = dtype === 1 ? "Giảm giá SP" : (dtype === 2 ? "Mua kèm Deal Sốc / Quà tặng" : "Combo Khuyến Mãi");
        var url = dtype === 1
          ? ("https://banhang.shopee.vn/portal/marketing/discount/" + id)
          : (dtype === 2 ? ("https://banhang.shopee.vn/portal/marketing/add-on-deal/" + id) : ("https://banhang.shopee.vn/portal/marketing/bundle/" + id));
        var itemCount = (obj.item_preview && obj.item_preview.item_count) || (obj.main_item_preview && obj.main_item_preview.item_count) || 0;
        var st = Number(obj.time_status || 2);
        var endSec = Number(obj.end_time || 0);
        discItems.push({
          id: id,
          name: String(obj.name || kindLabel),
          kind_label: kindLabel,
          time_status: st,
          start_time: Number(obj.start_time || 0),
          end_time: endSec,
          item_count: Number(itemCount),
          expiring_24h: st === 2 && endSec > nowSec && endSec <= next24h,
          url: url
        });
      });
    });
    orderStore.extras.discounts = discItems;

    var vOngoing = await safeJsonGet("/api/marketing/v3/voucher/list/" + q + "&offset=0&limit=30&promotion_type=2");
    var vUpcoming = await safeJsonGet("/api/marketing/v3/voucher/list/" + q + "&offset=0&limit=20&promotion_type=1");
    var vouchers = [];
    var seenV = {};
    [[vOngoing, 2], [vUpcoming, 1]].forEach(function (pair) {
      var res = pair[0], st = pair[1];
      var arr = (res && res.data && Array.isArray(res.data.voucher_list)) ? res.data.voucher_list : [];
      arr.forEach(function (v) {
        var vid = String(v.voucher_id || "");
        if (!vid || seenV[vid]) return;
        seenV[vid] = true;
        var endSec = Number(v.end_time || 0);
        vouchers.push({
          voucher_id: vid,
          name: String(v.name || v.voucher_code || "Voucher"),
          voucher_code: String(v.voucher_code || ""),
          start_time: Number(v.start_time || 0),
          end_time: endSec,
          discount_pct: Number(v.discount || 0),
          value: Number(v.value || 0),
          min_price: Number(v.min_price || 0),
          max_value: Number(v.max_value || 0),
          current_usage: Number(v.current_usage || 0),
          usage_limit: Number(v.usage_limit || 0),
          time_status: st,
          expiring_24h: st === 2 && endSec > nowSec && endSec <= next24h,
          url: "https://banhang.shopee.vn/portal/marketing/vouchers/" + vid
        });
      });
    });
    orderStore.extras.vouchers = vouchers;

    var fsOngoing = await safeJsonGet("/api/marketing/v4/shop_flash_sale/get_shop_flash_sale_list/" + q + "&offset=0&limit=20&type=2");
    var fsUpcoming = await safeJsonGet("/api/marketing/v4/shop_flash_sale/get_shop_flash_sale_list/" + q + "&offset=0&limit=20&type=1");
    var flashSales = [];
    var seenFs = {};
    [[fsOngoing, 2], [fsUpcoming, 1]].forEach(function (pair) {
      var res = pair[0], st = pair[1];
      var arr = (res && res.data && Array.isArray(res.data.flash_sale_list)) ? res.data.flash_sale_list : [];
      arr.forEach(function (f) {
        var fid = String(f.flash_sale_id || "");
        if (!fid || seenFs[fid]) return;
        seenFs[fid] = true;
        var endSec = Number(f.end_time || 0);
        flashSales.push({
          flash_sale_id: fid,
          start_time: Number(f.start_time || 0),
          end_time: endSec,
          enabled_item_count: Number(f.enabled_item_count || 0),
          item_count: Number(f.item_count || 0),
          time_status: st,
          expiring_24h: st === 2 && endSec > nowSec && endSec <= next24h,
          url: "https://banhang.shopee.vn/portal/marketing/shop-flash-sale/" + fid
        });
      });
    });
    flashSales.sort(function (a, b) { return a.start_time - b.start_time; });
    orderStore.extras.flash_sales = flashSales;
  }

  // Gọi API trực tiếp 2 bước trong chính phiên cookie của Worker Tab (200 đơn + 200 đánh giá 1-3* + Live + Khuyến mãi)
  async function fetchShopeeOrdersViaApi() {
    var spcCds = getSpcCds();
    if (!spcCds) {
      return computeShopeeOrderSummary();
    }
    window.__tpBulkApiFetching = true;
    try {
      // Tab code chính xác trên API Shopee v3: 200=Chờ XN, 300=Chờ lấy, 400=Đang giao, 500=Đã giao
      var fetchPlan = [
        { tabCode: 200, key: "Chờ xác nhận", pages: 1 },
        { tabCode: 300, key: "Chờ lấy hàng", pages: 2 },
        { tabCode: 400, key: "Đang giao", pages: 2 },
        { tabCode: 500, key: "Đã giao", pages: 1 }
      ];

      var orderFetchPromise = (async function () {
        for (var fi = 0; fi < fetchPlan.length; fi++) {
          var plan = fetchPlan[fi];
          var nextSentinel = "";
          for (var pg = 1; pg <= plan.pages; pg++) {
            try {
              var pag = { from_page_number: Math.max(1, pg - 1), page_number: pg, page_size: 40 };
              if (pg > 1 && nextSentinel) pag.next_page_sentinel = nextSentinel;
              var r1 = await fetch("/api/v3/order/search_order_list_index?SPC_CDS=" + encodeURIComponent(spcCds) + "&SPC_CDS_VER=2", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                credentials: "include",
                body: JSON.stringify({
                  order_list_tab: plan.tabCode,
                  entity_type: 1,
                  pagination: pag,
                  filter: {}
                })
              });
              var j1 = await r1.json();
              var idxList = (j1 && j1.data && j1.data.index_list) || [];
              var pagOut = j1 && j1.data && j1.data.pagination;
              if (pagOut && pagOut.next_page_sentinel) nextSentinel = String(pagOut.next_page_sentinel);
              if (pg === 1 && pagOut && pagOut.total !== undefined) {
                orderStore.tabCounts[plan.key] = Number(pagOut.total);
              }
              if (idxList.length > 0) {
                for (var bi = 0; bi < idxList.length; bi += 20) {
                  var wave = [];
                  for (var sub = bi; sub < Math.min(idxList.length, bi + 20); sub += 5) {
                    var slice = idxList.slice(sub, sub + 5);
                    wave.push(
                      fetch("/api/v3/order/get_order_list_card_list?SPC_CDS=" + encodeURIComponent(spcCds) + "&SPC_CDS_VER=2", {
                        method: "POST",
                        headers: { "Content-Type": "application/json" },
                        credentials: "include",
                        body: JSON.stringify({
                          order_list_tab: plan.tabCode,
                          need_count_down_desc: false,
                          order_param_list: slice
                        })
                      }).then(function (r) { return r.json(); }).then(function (j2) {
                        ingestShopeeOrderApi("get_order_list_card_list", j2);
                      }).catch(function () {})
                    );
                  }
                  await Promise.all(wave);
                }
              }
              if (idxList.length < 40) break;
            } catch (_) { break; }
          }
        }
      })();

      await Promise.allSettled([
        orderFetchPromise,
        fetchShopeeRatings(spcCds),
        fetchShopeeLive(),
        fetchShopeePromotions(spcCds)
      ]);
    } finally {
      window.__tpBulkApiFetching = false;
    }
    return computeShopeeOrderSummary();
  }

  // Quét Quảng cáo Shopee PAS (bảng đôi nameT + numT)
  function extractShopeePasPageRows() {
    var tables = [].slice.call(document.querySelectorAll("table"));
    if (!tables.length) return [];

    var nameT = tables
      .filter(function (t) {
        var trs = [].slice.call(t.querySelectorAll("tr"));
        return trs.filter(function (r) {
          var x = (r.textContent || "").trim();
          return x.length > 12 && !/Chiến dịch Tối đa/.test(x.slice(0, 15));
        }).length >= 2;
      })
      .sort(function (a, b) { return b.querySelectorAll("tr").length - a.querySelectorAll("tr").length; })[0];

    if (nameT) {
      var nameRowCount = nameT.querySelectorAll("tr").length;
      var numT = tables
        .filter(function (t) { return t !== nameT && Math.abs(t.querySelectorAll("tr").length - nameRowCount) <= 2; })
        .find(function (t) {
          var tr = t.querySelector("tr");
          if (!tr || tr.children.length < 7) return false;
          var tds = [].slice.call(tr.children).map(function (td) { return td.textContent || ""; });
          return /₫|\d/.test(tds[3] || "") && /₫|\d/.test(tds[4] || "");
        });

      if (numT) {
        var nameAll = [].slice.call(nameT.querySelectorAll("tr"));
        var numAll = [].slice.call(numT.querySelectorAll("tr"));
        var fn = function (v) {
          var m = (v || "").match(/[₫\d][\d.,]*/);
          return m ? m[0] : "0";
        };
        var out = [];
        var n = Math.min(nameAll.length, numAll.length);
        for (var i = 0; i < n; i++) {
          var nr = nameAll[i];
          var rawName = (nr.textContent || "").replace(/\s+/g, " ").trim();
          if (rawName.length < 8 || /Tên Chiến dịch|Chiến dịch Quảng cáo/i.test(rawName.slice(0, 25))) continue;

          var tdsNr = [].slice.call(nr.querySelectorAll("td"));
          var nameTd = tdsNr.find(function (td) {
            var t = (td.textContent || "").trim();
            return t.length > 3 && !/Chiến dịch Tối đa/.test(t.slice(0, 15));
          }) || tdsNr[tdsNr.length - 1] || nr;

          var fd = nameTd.querySelector("div");
          var src = fd && fd.textContent && fd.textContent.trim() ? fd.textContent : (nameTd.textContent || "");
          var name = (src || "").split("\n").map(function (x) { return x.trim(); }).filter(Boolean).join(" ").replace(/\s+/g, " ").trim();
          var ci = name.indexOf("Chiến dịch Tối đa Doanh thu");
          if (ci > 3) name = name.slice(0, ci).trim();
          name = name.replace(/(ID:\s*\d+|Không có ngày kết thúc|Đang diễn ra|Tạm dừng|Đã kết thúc|Xem chi tiết).*$/i, "").trim();
          var half = Math.floor(name.length / 2);
          if (name.length > 10 && name.slice(0, half) === name.slice(half)) name = name.slice(0, half).trim();
          if (!name || name.startsWith("Sản phẩm này")) continue;

          var allRaw = (nameTd.innerText || nameTd.textContent || "").split("\n").map(function (x) { return x.trim(); }).filter(Boolean);
          var status = allRaw.find(function (t) { return /^Đang diễn ra|^Tạm dừng|^Kết thúc|^Đã đặt lịch/i.test(t); }) || "Đang diễn ra";
          var ctype = allRaw.find(function (t) { return /Chiến dịch|Nhóm|Đơn|Tự động|Thủ công/i.test(t) && t !== name; }) || (name.startsWith("...") ? "Chiến dịch Nhóm" : "Chiến dịch Đơn");

          var numTr = numAll[i];
          var tds = [].slice.call(numTr.children).map(function (td) { return (td.textContent || "").replace(/\s+/g, " ").trim(); });
          var g = function (idx) { return idx < tds.length ? fn(tds[idx]) : "0"; };

          var isCampTab = /₫|không giới hạn/i.test(tds[0] || "");
          var budget = isCampTab ? parseVnNum(g(0)) : 0;
          var bDigits = String(Math.round(budget));
          if (bDigits.length >= 10 && bDigits.length % 2 === 0) {
            var bh = bDigits.length / 2;
            if (bDigits.slice(0, bh) === bDigits.slice(bh)) budget = parseInt(bDigits.slice(0, bh), 10);
          }
          var cost = parseVnNum(g(3));
          var sales = parseVnNum(g(4));
          var roas = cost > 0 && sales > 0 ? Number((sales / cost).toFixed(2)) : parseVnNum(g(5));
          var acos = cost > 0 && sales > 0 ? Number(((cost / sales) * 100).toFixed(2)) : (cost > 0 ? 100 : 0);

          out.push({
            name: name,
            status: status,
            ctype: ctype,
            budget: budget,
            budget_raw: budget > 0 ? fmtVnd(budget) : "KGH",
            cost: cost,
            sales: sales,
            roas: roas,
            acos: acos,
            url: "https://banhang.shopee.vn/portal/marketing/pas/index"
          });
        }
        if (out.length > 0) return out;
      }
    }
    return [];
  }

  async function scanShopeePas() {
    if (!window.location.pathname.includes("/portal/marketing/pas")) return null;
    var overview = {};
    document.querySelectorAll(".statistic-item").forEach(function (el) {
      var lines = (el.innerText || "").split("\n").map(function (x) { return x.trim(); }).filter(Boolean);
      if (lines.length >= 2) overview[lines[0]] = lines[lines.length - 1];
    });

    if (!window.__tp_pas_tab_init) {
      var navTabs = [].slice.call(document.querySelectorAll(".eds-tabs__nav-tab, [class*='tabs__nav-tab'], [role='tab']"));
      var campTab = navTabs.find(function (el) { return /Chiến Dịch Nhóm/i.test(el.innerText || ""); });
      if (campTab) {
        window.__tp_pas_tab_init = true;
        var cls = (campTab.className || "") + " " + (campTab.parentElement ? campTab.parentElement.className || "" : "");
        if (!/active|selected|current/i.test(cls)) {
          campTab.scrollIntoView({ block: "center" });
          campTab.click();
          await sleep(1600);
        }
      }
      var ongoingRadio = [].slice.call(document.querySelectorAll("input[type='radio']")).find(function (i) { return i.value === "ongoing"; });
      if (ongoingRadio && !ongoingRadio.checked) {
        ongoingRadio.click();
        await sleep(1400);
      }
    }

    var firstTable = document.querySelector("table");
    if (firstTable && !window.__tp_pas_scrolled) {
      window.__tp_pas_scrolled = true;
      firstTable.scrollIntoView({ block: "center" });
      await sleep(500);
    }

    var campaigns = extractShopeePasPageRows();
    var pasPayload = {
      ts: new Date().toLocaleTimeString("vi-VN"),
      logged_in: !/login|signin/i.test(window.location.pathname),
      orders_ads: parseVnNum(overview["Đơn hàng"] || overview["Số đơn hàng"] || 0),
      sales_ads: parseVnNum(overview["Doanh Số"] || overview["GMV"] || 0),
      cost_ads: parseVnNum(overview["Chi phí Dịch vụ Hiển thị"] || overview["Chi phí"] || 0),
      roas_ads: parseVnNum(overview["ROAS"] || 0),
      campaigns: campaigns
    };
    if (pasPayload.roas_ads === 0 && pasPayload.cost_ads > 0 && pasPayload.sales_ads > 0) {
      pasPayload.roas_ads = Number((pasPayload.sales_ads / pasPayload.cost_ads).toFixed(2));
    }

    try {
      sendRust({ cmd: "update_platform_data", platform: "shopee_pas", data_json: JSON.stringify(pasPayload) });
    } catch (_) {}
    return pasPayload;
  }

  window.__tpRunShopeeScan = async function (mode) {
    if (/login|signin|account\/signin/i.test(window.location.pathname)) {
      sendRust({ cmd: "update_platform_data", platform: mode === "pas" ? "shopee_pas" : "shopee_orders", data_json: JSON.stringify({ logged_in: false, ts: new Date().toLocaleTimeString("vi-VN"), need_login: true }) });
      return;
    }
    if (mode === "pas" || window.location.pathname.includes("/portal/marketing/pas")) {
      return await scanShopeePas();
    }
    return await fetchShopeeOrdersViaApi();
  };
})();
