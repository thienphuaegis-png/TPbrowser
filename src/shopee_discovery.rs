use crate::json_mini::{parse_json, JsonVal};
use std::collections::HashMap;

pub const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const MAX_PAYLOAD: usize = 128 * 1024;
pub const SHOP_INDEX_URL: &str = "https://banhang.shopee.vn/portal/shop";
const URL: &str = SHOP_INDEX_URL;

pub const SCRIPT: &str = r#"(() => {
  try {
    const visible = e => !!(e.getClientRects().length && getComputedStyle(e).visibility !== 'hidden' && getComputedStyle(e).display !== 'none');
    const login = [...document.querySelectorAll('input[type=password], [autocomplete=current-password], form[action*=login]')].some(visible);
    const captcha = [...document.querySelectorAll('iframe[src*=captcha i], [class*=captcha i], [id*=captcha i], [data-captcha]')].some(visible)
      || /captcha|verify|xác minh/i.test(document.title || '');
    const loading = [...document.querySelectorAll('.eds-loading, .eds-react-table-loading, .eds-spinner, [aria-busy=true], .eds-loading-mask')].some(visible);
    const tables = [...document.querySelectorAll('.eds-react-table table')];
    const rows = [...document.querySelectorAll('tr.eds-react-table-row[data-row-key]')];
    const table = rows.length ? rows[0].closest('table') : (tables.length === 1 ? tables[0] : null);
    const root = table && table.closest('.eds-react-table');
    const empty = !!root && [...root.querySelectorAll('.eds-react-table-empty')].some(visible);
    const pagination = [...document.querySelectorAll('.eds-pagination, .eds-react-pagination, [class*="pagination" i], [aria-label*="pagination" i], [aria-label*="page" i], [class*="page-total" i], [class*="total-page" i]')].some(visible)
      || [...document.querySelectorAll('span, div, p')].some(e => !e.children.length && visible(e) && /(?:\bpage\s*\d+\s*(?:of|\/)\s*\d+|\btrang\s*\d+\s*(?:trên|\/)\s*\d+|\b\d+\s*[-–]\s*\d+\s*(?:of|trên)\s*\d+)/i.test(e.textContent));
    return { location: location.href, login, captcha, loading, table: !!table,
      empty, pagination,
      headers: table ? [...table.querySelectorAll('thead th')].map(e => e.textContent.trim()) : [],
      invalid_rows: table ? !!table.querySelector('tr.eds-react-table-row:not([data-row-key])') : false,
      rows: rows.map(e => ({id: e.getAttribute('data-row-key'), name: e.cells.length ? e.cells[0].textContent.trim() : null})) };
  } catch (_) { return null; }
})()"#;

pub fn parse_tab_id(value: Option<&JsonVal>) -> Result<Option<u64>, ()> {
    match value {
        None => Ok(None),
        Some(JsonVal::Num(n)) if n.is_finite() && *n >= 1.0 && *n <= 9_007_199_254_740_991.0 && n.fract() == 0.0 => {
            let id = *n as u64;
            if id as f64 == *n { Ok(Some(id)) } else { Err(()) }
        }
        Some(_) => Err(()),
    }
}

pub const MAX_PENDING: usize = 8;
type Pending<T> = HashMap<u64, (u64, u64, std::time::Instant, T)>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NavigationReadiness {
    current: Option<u64>,
    content_loaded: bool,
    completed: bool,
    invalidated: bool,
    ready: bool,
}

impl NavigationReadiness {
    pub fn start(&mut self, navigation_id: u64) {
        self.current = Some(navigation_id);
        self.content_loaded = false;
        self.completed = false;
        self.invalidated = false;
        self.ready = false;
    }

    pub fn content_loading(&mut self, navigation_id: u64) {
        if self.current == Some(navigation_id) && !self.completed && !self.invalidated {
            self.content_loaded = true;
        }
    }

    pub fn completed(&mut self, navigation_id: u64, success: bool) {
        if self.current != Some(navigation_id) || self.completed || self.invalidated {
            return;
        }
        self.completed = true;
        if success && self.content_loaded {
            self.ready = true;
        } else {
            self.invalidated = true;
            self.ready = false;
        }
    }

    pub fn close(&mut self) {
        *self = Self::default();
    }

    pub fn ready_navigation_id(&self) -> Option<u64> {
        self.ready.then_some(self.current).flatten()
    }

    pub fn is_ready_for(&self, navigation_id: u64) -> bool {
        self.ready_navigation_id() == Some(navigation_id)
    }
}

pub fn admit_navigation(state: &NavigationReadiness, saved_navigation_id: Option<u64>) -> bool {
    saved_navigation_id.is_some_and(|id| state.is_ready_for(id))
}

pub fn insert_pending<T>(pending: &mut Pending<T>, id: u64, entry: (u64, u64, std::time::Instant, T)) -> Result<(), T> {
    if pending.len() >= MAX_PENDING || pending.contains_key(&id) { return Err(entry.3); }
    pending.insert(id, entry);
    Ok(())
}

pub fn nearest_deadline<T>(pending: &Pending<T>) -> Option<std::time::Instant> {
    pending.values().map(|(_, _, deadline, _)| *deadline).min()
}

pub fn expire_pending(pending: &mut Pending<std::sync::mpsc::Sender<Result<String, String>>>, now: std::time::Instant) -> Vec<u64> {
    let expired: Vec<_> = pending.iter().filter_map(|(id, (_, _, deadline, _))| (*deadline <= now).then_some(*id)).collect();
    for id in &expired {
        if let Some((_, _, _, reply)) = pending.remove(id) { let _ = reply.send(Ok(unavailable())); }
    }
    expired
}

pub fn take_pending<T>(pending: &mut HashMap<u64, (u64, u64, std::time::Instant, T)>, req_id: u64, tab_id: u64, generation: Option<u64>, now: std::time::Instant) -> Option<(T, bool)> {
    if !pending.get(&req_id).is_some_and(|(id, _, _, _)| *id == tab_id) { return None; }
    pending.remove(&req_id).map(|(_, saved, deadline, reply)| (reply, now < deadline && generation == Some(saved)))
}

pub fn valid_url(url: &str) -> bool {
    url == URL
        || url.strip_prefix(URL).is_some_and(|s| {
            s.starts_with('?') || s.starts_with('#') || s == "/" || s.starts_with("/?") || s.starts_with("/#")
        })
}
fn result(status: &str, shops: Option<Vec<JsonVal>>, logged_in: bool) -> String {
    let mut fields = vec![
        ("status", JsonVal::Str(status.into())),
        ("logged_in", JsonVal::Bool(logged_in)),
        ("account_identity", JsonVal::Str("unavailable".into())),
    ];
    if let Some(shops) = shops { fields.push(("shops", JsonVal::Arr(shops))); }
    JsonVal::obj(fields).to_json()
}
pub fn unavailable() -> String { result("unavailable", None, false) }
pub fn result_loading() -> String { result("loading", None, false) }
pub fn normalize(current_url: &str, raw: &str) -> String {
    if !valid_url(current_url) || raw.len() > MAX_PAYLOAD { return unavailable(); }
    let Ok(data) = parse_json(raw) else { return unavailable() };
    let Some(location) = data.get("location").and_then(JsonVal::as_str) else { return unavailable() };
    if !valid_url(location) || location != current_url { return unavailable(); }
    let (Some(login), Some(captcha), Some(loading), Some(table)) = (data.get("login").and_then(JsonVal::as_bool), data.get("captcha").and_then(JsonVal::as_bool), data.get("loading").and_then(JsonVal::as_bool), data.get("table").and_then(JsonVal::as_bool)) else { return unavailable() };
    if loading { return result("loading", None, false); }
    if captcha { return result("captcha", None, false); }
    if login { return result("needs_login", None, false); }
    if !table || data.get("invalid_rows").and_then(JsonVal::as_bool) != Some(false) { return unavailable(); }
    let Some(rows) = data.get("rows").and_then(JsonVal::as_arr) else { return unavailable() };
    if rows.is_empty() { return if data.get("empty").and_then(JsonVal::as_bool) == Some(true) { result("no_data", None, false) } else { unavailable() }; }
    if data.get("empty").and_then(JsonVal::as_bool) != Some(false) { return unavailable(); }
    let mut seen = HashMap::new();
    let mut shops = Vec::new();
    for row in rows {
        let (Some(id), Some(name)) = (row.get("id").and_then(JsonVal::as_str), row.get("name").and_then(JsonVal::as_str)) else { return unavailable() };
        let (id, name) = (id.trim(), name.trim());
        if id.is_empty() || name.is_empty() { return unavailable(); }
        if let Some(previous) = seen.get(id) {
            if *previous != name { return unavailable(); }
            continue;
        }
        seen.insert(id, name);
        shops.push(JsonVal::obj(vec![("shop_id", JsonVal::Str(id.into())), ("display_name", JsonVal::Str(name.into()))]));
    }
    result("ok", Some(shops), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_cap_rejects_extra_and_recovers_after_completion() {
        let now = std::time::Instant::now();
        let mut pending = HashMap::new();
        for id in 0..MAX_PENDING as u64 {
            assert!(insert_pending(&mut pending, id, (1, 0, now + TIMEOUT, id)).is_ok());
        }
        assert!(insert_pending(&mut pending, 1000, (1, 0, now + TIMEOUT, 1000)).is_err());
        assert_eq!(pending.len(), MAX_PENDING);
        take_pending(&mut pending, 0, 1, Some(0), now);
        assert!(insert_pending(&mut pending, 1000, (1, 0, now + TIMEOUT, 1000)).is_ok());
    }

    #[test]
    fn deadlines_expire_once_and_callbacks_are_single_use() {
        use std::sync::mpsc;
        let now = std::time::Instant::now();
        let mut pending = HashMap::new();
        let (a, ar) = mpsc::channel();
        let (b, br) = mpsc::channel();
        let (c, cr) = mpsc::channel();
        insert_pending(&mut pending, 1, (10, 0, now + TIMEOUT, a)).unwrap();
        insert_pending(&mut pending, 2, (10, 0, now + TIMEOUT * 2, b)).unwrap();
        insert_pending(&mut pending, 3, (10, 0, now + TIMEOUT * 3, c)).unwrap();
        assert_eq!(nearest_deadline(&pending), Some(now + TIMEOUT));
        let (reply, valid) = take_pending(&mut pending, 2, 10, Some(0), now).unwrap();
        assert!(valid);
        reply.send(Ok("callback".to_string())).unwrap();
        expire_pending(&mut pending, now + TIMEOUT);
        assert_eq!(ar.try_recv().unwrap().unwrap(), unavailable());
        assert!(ar.try_recv().is_err());
        assert_eq!(br.try_recv().unwrap().unwrap(), "callback");
        assert!(take_pending(&mut pending, 1, 10, Some(0), now + TIMEOUT).is_none());
        assert!(take_pending(&mut pending, 2, 10, Some(0), now + TIMEOUT).is_none());
        assert_eq!(nearest_deadline(&pending), Some(now + TIMEOUT * 3));
        expire_pending(&mut pending, now + TIMEOUT * 3);
        expire_pending(&mut pending, now + TIMEOUT * 3);
        assert_eq!(cr.try_recv().unwrap().unwrap(), unavailable());
        assert!(cr.try_recv().is_err());
        assert_eq!(nearest_deadline(&pending), None);
    }

    #[test]
    fn discovery_tab_id_only_defaults_when_absent_and_rejects_invalid_values() {
        let parse = parse_tab_id;
        assert_eq!(parse(None), Ok(None));
        for (n, expected) in [(1.0, 1), (42.0, 42), (9_007_199_254_740_991.0, 9_007_199_254_740_991)] {
            assert_eq!(parse(Some(&JsonVal::Num(n))), Ok(Some(expected)));
        }
        for value in [JsonVal::Null, JsonVal::Str("1".into()), JsonVal::Bool(true), JsonVal::Arr(vec![]), JsonVal::Obj(vec![]), JsonVal::Num(-1.0), JsonVal::Num(0.0), JsonVal::Num(-0.0), JsonVal::Num(1.5), JsonVal::Num(f64::NAN), JsonVal::Num(f64::INFINITY), JsonVal::Num(f64::NEG_INFINITY), JsonVal::Num(9_007_199_254_740_992.0), JsonVal::Num(u64::MAX as f64)] {
            assert_eq!(parse(Some(&value)), Err(()), "accepted {value:?}");
        }
    }

    #[test]
    fn navigation_and_close_invalidate_pending() {
        let now = std::time::Instant::now();
        let mut pending = HashMap::new();
        pending.insert(1, (4, 8, now + TIMEOUT, 42));
        assert_eq!(take_pending(&mut pending, 1, 4, Some(9), now), Some((42, false)));
        pending.insert(2, (4, 8, now + TIMEOUT, 43));
        assert_eq!(take_pending(&mut pending, 2, 4, None, now), Some((43, false)));
    }
    #[test]
    fn empty_requires_visible_marker_and_pagination_does_not_block_rows() {
        let raw = r#"{"location":"https://banhang.shopee.vn/portal/shop","login":false,"captcha":false,"table":true,"loading":false,"invalid_rows":false,"empty":true,"pagination":false,"headers":["Tên Shop","Tên Đăng nhập của Shop","Khu vực","Đăng nhập lần cuối","Hoạt động"],"rows":[]}"#;
        assert!(normalize(URL, raw).contains("\"no_data\""));
        assert!(normalize(URL, &raw.replace("\"rows\":[]", r#""rows":[{"id":"1","name":"S"}]"#)).contains("\"status\":\"unavailable\""));
        assert!(normalize(URL, &raw.replace("\"empty\":true", "\"empty\":false")).contains("\"status\":\"unavailable\""));
        assert!(normalize(URL, &raw.replace("\"pagination\":false", "\"pagination\":true")).contains("\"status\":\"no_data\""));
        assert!(normalize(URL, &raw.replace("\"rows\":[]", r#""rows":[{"id":"1","name":"S"}]"#).replace("\"empty\":true", "\"empty\":false").replace("\"pagination\":false", "\"pagination\":true")).contains("\"status\":\"ok\""));
    }
    #[test]
    fn pending_is_bound_and_single_use_even_on_timeout() {
        let now = std::time::Instant::now();
        let mut pending = HashMap::new();
        pending.insert(7, (9, 0, now + TIMEOUT, 123));
        assert!(take_pending(&mut pending, 7, 8, Some(0), now).is_none());
        assert_eq!(take_pending(&mut pending, 7, 9, Some(0), now), Some((123, true)));
        assert!(take_pending(&mut pending, 7, 9, Some(0), now).is_none());
        pending.insert(7, (9, 0, now, 456));
        assert_eq!(take_pending(&mut pending, 7, 9, Some(0), now), Some((456, false)));
        assert!(take_pending(&mut pending, 7, 9, Some(0), now).is_none());
    }
    #[test]
    fn strict_contract_cases() {
        let raw = r#"{"location":"https://banhang.shopee.vn/portal/shop","login":false,"captcha":false,"table":true,"loading":false,"invalid_rows":false,"empty":true,"pagination":false,"headers":["Tên Shop","Tên Đăng nhập của Shop","Khu vực","Đăng nhập lần cuối","Hoạt động"],"rows":[]}"#;
        let status = |s: &str| parse_json(s).unwrap().get("status").unwrap().as_str().unwrap().to_string();
        assert_eq!(status(&normalize(URL, raw)), "no_data");
        assert_eq!(status(&normalize(URL, &raw.replace("\"loading\":false", "\"loading\":true"))), "loading");
        for changed in [raw.replace("\"invalid_rows\":false", "\"invalid_rows\":true"), raw.replace("\"rows\":[]", "\"rows\":[{}]"), raw.replace("\"table\":true", "\"table\":false"), "null".into(), "{".into(), "x".repeat(MAX_PAYLOAD + 1)] { assert_eq!(status(&normalize(URL, &changed)), "unavailable"); }
        assert_eq!(status(&normalize(URL, &raw.replace("\"login\":false", "\"login\":true"))), "needs_login");
        assert_eq!(status(&normalize(URL, &raw.replace("\"login\":false", "\"login\":true").replace("\"loading\":false", "\"loading\":true"))), "loading");
        assert_eq!(status(&normalize("https://evil.test/portal/shop", raw)), "unavailable");
        assert_eq!(status(&normalize(&format!("{}?x", URL), raw)), "unavailable");
        assert!(valid_url(&format!("{}?x#y", URL))); assert!(!valid_url(&format!("{}/x", URL)));
        let twice = raw.replace("\"empty\":true", "\"empty\":false").replace("\"rows\":[]", r#""rows":[{"id":"1","name":" A "},{"id":"1","name":"A"}]"#);
        let out = normalize(URL, &twice); assert_eq!(status(&out), "ok"); assert_eq!(out.matches("shop_id").count(), 1); assert!(out.contains("\"display_name\":\"A\""));
        let conflict = twice.replace("\"name\":\"A\"", "\"name\":\"B\""); assert_eq!(status(&normalize(URL, &conflict)), "unavailable");
    }
    #[test]
    fn recognized_table_returns_shops() {
        let raw = r#"{"location":"https://banhang.shopee.vn/portal/shop","login":false,"captcha":false,"table":true,"loading":false,"invalid_rows":false,"empty":false,"pagination":false,"headers":["Tên Shop","Tên Đăng nhập của Shop","Khu vực","Đăng nhập lần cuối","Hoạt động"],"rows":[{"id":"42","name":"  Shop A  "}]}"#;
        assert!(normalize("https://banhang.shopee.vn/portal/shop", raw).contains("\"status\":\"ok\""));
    }

    #[test]
    fn navigation_readiness_is_correlated_by_id_and_fails_closed() {
        let mut state = NavigationReadiness::default();
        assert_eq!(state.ready_navigation_id(), None);
        state.start(10);
        state.content_loading(9);
        state.completed(10, true);
        assert_eq!(state.ready_navigation_id(), None);
        state.content_loading(10);
        state.completed(9, true);
        assert_eq!(state.ready_navigation_id(), None);
        state.completed(10, true);
        assert_eq!(state.ready_navigation_id(), None);
        state.start(12);
        state.content_loading(12);
        state.completed(12, true);
        assert_eq!(state.ready_navigation_id(), Some(12));
        state.start(11);
        assert_eq!(state.ready_navigation_id(), None);
        state.completed(11, true);
        assert_eq!(state.ready_navigation_id(), None);
        state.content_loading(11);
        state.completed(11, false);
        assert_eq!(state.ready_navigation_id(), None);
    }

    #[test]
    fn same_url_reload_and_close_invalidate_saved_navigation() {
        let mut state = NavigationReadiness::default();
        state.start(20);
        state.content_loading(20);
        state.completed(20, true);
        assert!(state.is_ready_for(20));
        state.start(21);
        assert!(!state.is_ready_for(20));
        state.content_loading(21);
        state.completed(21, true);
        assert!(state.is_ready_for(21));
        state.close();
        assert_eq!(state.ready_navigation_id(), None);
        assert!(!state.is_ready_for(21));
    }

    #[test]
    fn discovery_admission_requires_current_ready_navigation() {
        let mut state = NavigationReadiness::default();
        state.start(30);
        assert!(!admit_navigation(&state, Some(30)));
        state.content_loading(30);
        assert!(!admit_navigation(&state, Some(30)));
        state.completed(30, true);
        assert!(admit_navigation(&state, Some(30)));
        assert!(!admit_navigation(&state, Some(29)));
        assert!(!admit_navigation(&state, None));
        state.start(31);
        state.content_loading(31);
        state.completed(31, false);
        assert!(!admit_navigation(&state, Some(31)));
    }

    #[test]
    fn duplicate_or_out_of_order_events_never_reestablish_readiness() {
        let mut state = NavigationReadiness::default();
        state.start(40);
        state.completed(40, true);
        state.content_loading(40);
        assert!(!state.is_ready_for(40));
        state.completed(40, true);
        assert!(!state.is_ready_for(40));

        state.start(41);
        state.content_loading(41);
        state.completed(41, true);
        assert!(state.is_ready_for(41));
        state.content_loading(40);
        state.completed(40, true);
        assert!(!state.is_ready_for(40));
        assert!(state.is_ready_for(41));
    }
}
