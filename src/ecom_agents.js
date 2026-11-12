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
    tabCounts: {}
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

      var buyer = header.buyer_name || (header.buyer_info && header.buyer_info.user_name) || c.buyer_user_name || "Khách Shopee";
      var statusText = c.status_info ? (c.status_info.status_text || "") : (c.order_status_text || "Đơn Shopee");
      var shippingMethod = (c.shipping_info && c.shipping_info.shipping_carrier) || c.fulfillment_carrier_name || c.logistics_channel_name || "";
      var totalPrice = "";
      if (c.total_price) totalPrice = String(c.total_price);
      else if (c.final_total) totalPrice = String(c.final_total);
      else if (c.price_info && c.price_info.total_price) totalPrice = String(c.price_info.total_price);

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
        var itemPrice = it.item_price || it.order_price || it.model_discounted_price || it.price || "";
        var key = String(orderSn) + "_" + idx;
        orderStore.orders.set(key, {
          order_sn: String(orderSn),
          buyer: String(buyer),
          status: String(statusText),
          item_name: String(itemName),
          model_name: String(modelName),
          qty: Number(qty) || 1,
          item_price: String(itemPrice),
          total_price: String(totalPrice),
          shipping: String(shippingMethod)
        });
        updated = true;
      });
    });

    if (updated) {
      setTimeout(computeShopeeOrderSummary, 350);
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
        var effPrice = it.item_price > 0 ? it.item_price : (ord.order_total > 0 ? Math.round(ord.order_total / totalQty) : 0);
        it.effective_price = effPrice;
        if (effPrice > 0) {
          var key = stripVn(it.item_name).slice(0, 45);
          if (!productPrices.has(key)) productPrices.set(key, []);
          productPrices.get(key).push(effPrice);
        }
      });
    });

    var anomalies = [];
    orderMap.forEach(function (ord) {
      ord.items.forEach(function (it) {
        if (/qua tang|thu cam on|gift|sample|mau thu|tang kem/i.test(stripVn(it.item_name + " " + it.model_name))) return;
        var effPrice = it.effective_price || 0;
        var key = stripVn(it.item_name).slice(0, 45);
        var prices = (productPrices.get(key) || []).slice().sort(function (a, b) { return a - b; });
        var median = prices.length ? prices[Math.floor(prices.length / 2)] : 0;
        var reasons = [];
        if (ord.order_total > 0 && ord.order_total < 15000 && ord.items.length === 1) {
          reasons.push("Đơn < 15.000đ (" + fmtVnd(ord.order_total) + ")");
        }
        if (median >= 20000 && effPrice > 0 && effPrice < median * 0.72) {
          var dropPct = Math.round((1 - effPrice / median) * 100);
          reasons.push("Tụt giá -" + dropPct + "% so với trung vị (" + fmtVnd(median) + ")");
        }
        if (reasons.length > 0) {
          anomalies.push({
            order_sn: ord.order_sn,
            item_name: it.item_name,
            model_name: it.model_name,
            qty: it.qty,
            effective_price: effPrice,
            order_total: ord.order_total,
            reasons: reasons
          });
        }
      });
    });

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
      orders: Array.from(orderMap.values()).slice(0, 120),
      raw_rows: rows.slice(0, 400)
    };

    try {
      var s = JSON.stringify(payload);
      sendRust({ cmd: "update_platform_data", platform: "shopee_orders", data_json: s });
    } catch (_) {}
    return payload;
  }

  // Gọi API trực tiếp 2 bước trong chính phiên cookie của Worker Tab (không cần bấm phân trang bằng chuột)
  async function fetchShopeeOrdersViaApi() {
    var spcCds = getSpcCds();
    if (!spcCds) {
      return computeShopeeOrderSummary();
    }
    var tabsToFetch = [100, 200, 300];
    for (var ti = 0; ti < tabsToFetch.length; ti++) {
      var tabCode = tabsToFetch[ti];
      try {
        var r1 = await fetch("/api/v3/order/search_order_list_index?SPC_CDS=" + encodeURIComponent(spcCds) + "&SPC_CDS_VER=2", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          credentials: "include",
          body: JSON.stringify({
            order_list_tab: tabCode,
            entity_type: 1,
            pagination: { from_page_number: 1, page_number: 1, page_size: 40 },
            filter: {}
          })
        });
        var j1 = await r1.json();
        var idxList = (j1 && j1.data && j1.data.index_list) || [];
        var total = (j1 && j1.data && j1.data.pagination && j1.data.pagination.total) || idxList.length;
        if (tabCode === 100) orderStore.tabCounts["Chờ lấy hàng"] = total;
        if (tabCode === 200) orderStore.tabCounts["Đang giao"] = total;
        if (tabCode === 300) orderStore.tabCounts["Đã giao"] = total;
        if (idxList.length > 0) {
          var r2 = await fetch("/api/v3/order/get_order_list_card_list?SPC_CDS=" + encodeURIComponent(spcCds) + "&SPC_CDS_VER=2", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            credentials: "include",
            body: JSON.stringify({
              order_list_tab: tabCode,
              need_count_down_desc: false,
              order_param_list: idxList
            })
          });
          var j2 = await r2.json();
          ingestShopeeOrderApi("get_order_list_card_list", j2);
        }
      } catch (_) {}
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
            acos: acos
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
