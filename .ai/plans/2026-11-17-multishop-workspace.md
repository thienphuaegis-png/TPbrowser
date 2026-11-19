# Multi-shop Workspace Implementation Plan

> **For agentic workers:** Execute one task at a time. Do not start a later task until the preceding task has fresh test evidence and review.

**Goal:** Transform Workspace from one platform-wide snapshot into a real multi-shop monitoring workspace with a combined dashboard, TikTok and Shopee tabs, selected-shop scanning, and actionable Shop Health alerts.

**Architecture:** Keep WebView2 as the only browser engine. Store snapshots by stable platform/account/shop identity rather than in the existing singleton platform JSON fields. Discover shops from the authenticated Seller Center pages or platform responses only after a real read-only probe; do not infer shops from cookies, names, or synthetic data.

**Confirmed product decisions:**
- Dashboard defaults to an aggregate of every shop the user selects for monitoring.
- Workspace has three top-level tabs: Dashboard, TikTok, and Shopee.
- Shop Health is first-class: account-health violations, store rating, operational-health metrics, expedited/same-day delivery, and a direct action list.
- All platform reads are read-only. No setting, listing, advertising-budget, order-status, appeal, or chat actions are automated in this work.

## Global Constraints

- Preserve existing WebView2 TikTok/Shopee scanners until their replacement is proven for the same platform/shop.
- Never inspect or print cookie values, `config.json`, `.env*`, or `user_data`.
- No speculative selectors/endpoints: capture and validate a real authenticated page/API payload before coding each platform discovery/health adapter.
- A platform shop is identified by an immutable platform-supplied ID. Display name is not a key.
- Persist only selected-shop configuration and redacted/normalized read-only snapshots; never persist credentials or cookie values.
- Keep the working tree’s unrelated modifications intact. Do not commit, push, reset, stash, or rewrite history.
- Every UI claim requires a runtime smoke test; unit tests do not prove Seller Center data extraction.

## Scope Boundary

- In scope: discovery, selection, snapshots, dashboards, health/violation/expedited-delivery visibility, and direct links to official Seller Center pages.
- Out of scope: bypassing authentication/CAPTCHA, submitting appeals, changing orders, updating products, changing ads, or automatically replying to buyers.

## Current Evidence

| Need | Current implementation | Gap |
|---|---|---|
| Workspace layout | `src/workspace.html` is one combined page; it currently holds `lastData` for 3 singleton platform payloads | No top-level Dashboard/TikTok/Shopee separation |
| Snapshot state | `src/main.rs` holds `shopee_orders_json`, `shopee_pas_json`, `tiktok_json` | A new scan overwrites data from another shop |
| Worker identity | `ensure_ecom_worker` reuses by `domain_hint` in `src/main.rs` | Cannot distinguish two shops on one platform |
| TikTok data | `TIKTOK_PASSIVE_SCAN_JS` reads Ads dashboard only | No shop health, violation, expedited/same-day delivery, orders or store identity |
| Shopee data | `src/ecom_agents.js` reads orders, ratings, promotions and PAS; detects candidate express orders | No official expedited rate/deadline, health score, or per-shop isolation |

## Task 1 — Multi-shop identity and selected-shop persistence

**Files:**
- Modify: `src/main.rs` around Workspace/config/session state and persistence helpers.
- Test: Rust tests in `src/main.rs`.

**Acceptance:** A normalized `PlatformShop` model has immutable `platform`, `account_id`, and `shop_id`, plus display fields and a `selected` flag. It serializes through the existing configuration path without secrets; unknown/missing saved data falls back safely. Existing single-shop scanning remains unchanged.

**Required checks:**
1. Add a failing Rust test covering round-trip persistence of two selected shops from the same platform and distinct IDs.
2. Add the minimal model/config implementation.
3. Run the focused test, `cargo test`, `cargo check --locked`, and `git diff --check`.

## Task 2 — Read-only authenticated shop discovery probe

**Files:**
- Modify: `src/main.rs` only after the real probe identifies stable platform data.
- Test: focused Rust/unit tests plus a live manual TPbrowser MCP smoke test.

**Acceptance:** From a user-authenticated TikTok Seller or Shopee Seller tab, TPbrowser exposes a normalized list of platform-provided accounts/shops. It visibly reports `needs_login`/CAPTCHA/no-data rather than inventing a list. No cookie values are returned or logged.

**Dependency:** Task 1.

## Task 3 — Shop selection UI and worker routing

**Files:**
- Modify: `src/workspace.html`, `src/main.rs`.
- Test: Rust routing/persistence tests and UI syntax check.

**Acceptance:** The user can sync shops, tick those monitored, and run scans only for selected shops. Worker identity includes platform+account+shop+scan-kind so two selected shops do not overwrite or reuse the wrong worker.

**Dependency:** Tasks 1–2.

## Task 4 — Per-shop snapshots and aggregate Dashboard

**Files:**
- Modify: `src/main.rs`, `src/workspace.html`.
- Test: aggregation tests and runtime smoke test.

**Acceptance:** The Dashboard defaults to all selected shops, supports filtering by platform/account/shop, labels every metric with its source shop, and never aggregates values when a shop snapshot is stale/needs-login/CAPTCHA.

**Dependency:** Tasks 1–3.

## Task 5 — TikTok Shop Health and expedited-delivery adapter

**Files:**
- Modify: `src/main.rs`, `src/workspace.html`.
- Test: parser tests plus authenticated TikTok live read-only verification.

**Acceptance:** For each selected TikTok shop, show separately: account-health/violation items, store rating and component metrics, standard late-dispatch metric, expedited/same-day late-dispatch metric, evaluation period/denominator when the platform exposes them, and direct links to the official source/action screen. Missing platform fields appear as unavailable, never zero.

**Dependency:** Tasks 2–4.

## Task 6 — Shopee Shop Health and expedited-delivery adapter

**Files:**
- Modify: `src/ecom_agents.js`, `src/main.rs`, `src/workspace.html`.
- Test: JS syntax/parser tests plus authenticated Shopee live read-only verification.

**Acceptance:** For each selected Shopee shop, display official health/penalty information and the platform-provided expedited/same-day delivery rate and affected order list where available. The existing heuristic `express_orders` is labeled as a count, never presented as the official rate.

**Dependency:** Tasks 2–4.

## Task 7 — Platform-specific views and action queue

**Files:**
- Modify: `src/workspace.html`.
- Test: UI runtime smoke test.

**Acceptance:** The TikTok and Shopee top-level tabs show only their selected shops, with a shared action queue ordered by severity/deadline. Every action opens the official tab/page; none performs a state-changing operation automatically.

**Dependency:** Tasks 4–6.

## Verification checklist

- [ ] Existing TikTok Ads scan still works for a single selected shop.
- [ ] Existing Shopee orders/PAS scan still works for a single selected shop.
- [ ] Two shops on one platform retain separate snapshots after restart.
- [ ] Default Dashboard combines every selected shop only.
- [ ] Health/violation and expedited metrics show their source period and never substitute missing data with 0.
- [ ] CAPTCHA/login blocks a shop only and does not erase other shops’ snapshots.
- [ ] `cargo test`, `cargo check --locked`, JS syntax checks, `git diff --check`, checkpoint integrity all pass.
