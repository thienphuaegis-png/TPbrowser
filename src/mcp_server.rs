use std::{
    collections::hash_map::RandomState,
    env,
    fs,
    hash::{BuildHasher, Hasher},
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{mpsc, OnceLock},
    thread,
    time::{Duration, Instant},
};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use crate::json_mini::{parse_json, JsonVal};

pub const DEFAULT_MCP_PORT: u16 = 17777;
static MCP_AUTH_TOKEN: OnceLock<String> = OnceLock::new();

pub fn generate_auth_token() -> String {
    let hi = RandomState::new().build_hasher().finish();
    let lo = RandomState::new().build_hasher().finish();
    format!("tp_{:016x}{:016x}", hi, lo)
}

pub fn set_auth_token(token: String) {
    let trimmed = token.trim().to_string();
    if !trimmed.is_empty() {
        let _ = MCP_AUTH_TOKEN.set(trimmed);
    }
}

fn active_auth_token() -> Option<String> {
    if let Ok(env_tok) = env::var("TP_MCP_TOKEN") {
        let t = env_tok.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    MCP_AUTH_TOKEN.get().cloned().filter(|t| !t.is_empty())
}

fn require_local_auth_from_env() -> bool {
    env::var("TP_MCP_REQUIRE_LOCAL_AUTH")
        .ok()
        .map(|v| matches!(v.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.len() != bb.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in ab.iter().zip(bb.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn is_loopback_host(host: &str) -> bool {
    let h = host.trim();
    if h.is_empty() {
        return false;
    }
    let without_port = if h.starts_with('[') {
        h.split(']').next().unwrap_or("").trim_start_matches('[')
    } else {
        h.split(':').next().unwrap_or("")
    };
    without_port.eq_ignore_ascii_case("localhost")
        || without_port == "127.0.0.1"
        || without_port == "::1"
}

fn is_authorized_request(
    auth_header: Option<&str>,
    host_header: Option<&str>,
    has_proxy_header: bool,
    expected_token: Option<&str>,
    require_local_auth: bool,
) -> bool {
    let Some(expected) = expected_token.map(str::trim).filter(|t| !t.is_empty()) else {
        return !has_proxy_header;
    };

    if let Some(raw_auth) = auth_header {
        let mut parts = raw_auth.trim().splitn(2, char::is_whitespace);
        let scheme = parts.next().unwrap_or("");
        let token = parts.next().unwrap_or("").trim();
        return scheme.eq_ignore_ascii_case("Bearer") && constant_time_eq(token, expected);
    }

    !require_local_auth && !has_proxy_header && host_header.is_none_or(is_loopback_host)
}


#[derive(Debug, Clone)]
pub enum McpBridgeMsg {
    ToolCall {
        tool: String,
        args_json: String,
        reply: mpsc::Sender<Result<String, String>>,
    },
    ReadResource {
        uri: String,
        reply: mpsc::Sender<Result<String, String>>,
    },
    HealthCheck {
        reply: mpsc::Sender<String>,
    },
    WakeWindow,
}

pub fn mcp_port() -> u16 {
    env::var("TPBROWSER_MCP_PORT")
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok())
        .filter(|&p| p > 0)
        .unwrap_or(DEFAULT_MCP_PORT)
}

pub fn start_http_server<F>(dispatch: F)
where
    F: Fn(McpBridgeMsg) + Send + Sync + Clone + 'static,
{
    let port = mcp_port();
    thread::spawn(move || {
        let addr = format!("127.0.0.1:{}", port);
        let Ok(listener) = TcpListener::bind(&addr) else {
            return;
        };
        for stream in listener.incoming().flatten() {
            let d = dispatch.clone();
            thread::spawn(move || {
                let _ = handle_http_conn(stream, d);
            });
        }
    });
}

fn handle_http_conn<F>(stream: TcpStream, dispatch: F) -> io::Result<()>
where
    F: Fn(McpBridgeMsg) + Send + Sync + Clone + 'static,
{
    let tok = active_auth_token();
    handle_http_conn_with_auth(
        stream,
        dispatch,
        tok.as_deref(),
        require_local_auth_from_env(),
    )
}

fn handle_http_conn_with_auth<F>(
    mut stream: TcpStream,
    dispatch: F,
    expected_token: Option<&str>,
    require_local_auth: bool,
) -> io::Result<()>
where
    F: Fn(McpBridgeMsg) + Send + Sync + Clone + 'static,
{
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut req_line = String::new();
    if reader.read_line(&mut req_line)? == 0 {
        return Ok(());
    }
    let parts: Vec<&str> = req_line.trim_end_matches(&['\r', '\n'][..]).split_whitespace().collect();
    if parts.len() < 2 {
        return write_http_resp(&mut stream, 400, "Bad Request", "application/json", r#"{"error":"bad request"}"#);
    }
    let method = parts[0];
    let path = parts[1].split('?').next().unwrap_or("/");

    let mut content_len: usize = 0;
    let mut chunked = false;
    let mut has_origin = false;
    let mut has_proxy_header = false;
    let mut auth_header: Option<String> = None;
    let mut host_header: Option<String> = None;
    loop {
        let mut hline = String::new();
        if reader.read_line(&mut hline)? == 0 {
            break;
        }
        let trimmed = hline.trim_end_matches(&['\r', '\n'][..]);
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim();
            let vl = v.trim();
            if key.eq_ignore_ascii_case("origin") {
                has_origin = true;
            } else if key.eq_ignore_ascii_case("authorization") {
                auth_header = Some(vl.to_string());
            } else if key.eq_ignore_ascii_case("host") {
                host_header = Some(vl.to_string());
            } else if key.eq_ignore_ascii_case("cf-connecting-ip")
                || key.eq_ignore_ascii_case("x-forwarded-for")
                || key.eq_ignore_ascii_case("x-forwarded-host")
                || key.eq_ignore_ascii_case("x-real-ip")
                || key.eq_ignore_ascii_case("forwarded")
                || key.eq_ignore_ascii_case("cf-ray")
                || key.eq_ignore_ascii_case("cf-visitor")
            {
                has_proxy_header = true;
            }
            if key.eq_ignore_ascii_case("content-length") {
                content_len = vl.parse::<usize>().unwrap_or(0);
            } else if key.eq_ignore_ascii_case("transfer-encoding") && vl.to_ascii_lowercase().contains("chunked") {
                chunked = true;
            }
        }
    }

    // Loopback MCP is for native clients, not browser-origin requests. Reject
    // before reading bodies or dispatching any route, including preflights.
    if has_origin {
        return write_http_resp(&mut stream, 403, "Forbidden", "application/json", r#"{"error":"browser origin not permitted"}"#);
    }

    if method == "OPTIONS" {
        return write_http_resp(&mut stream, 204, "No Content", "text/plain", "");
    }

    if method == "GET" && path == "/health" {
        let (tx, rx) = mpsc::channel();
        dispatch(McpBridgeMsg::HealthCheck { reply: tx });
        let info = rx
            .recv_timeout(Duration::from_secs(3))
            .unwrap_or_else(|_| r#"{"ok":true,"app":"TPbrowser"}"#.to_string());
        return write_http_resp(&mut stream, 200, "OK", "application/json", &info);
    }

    if !is_authorized_request(
        auth_header.as_deref(),
        host_header.as_deref(),
        has_proxy_header,
        expected_token,
        require_local_auth,
    ) {
        return write_http_resp(
            &mut stream,
            401,
            "Unauthorized",
            "application/json",
            r#"{"error":"unauthorized"}"#,
        );
    }

    let body = if chunked {
        read_chunked_body(&mut reader)?
    } else if content_len > 0 {
        let capped = content_len.min(8 * 1024 * 1024);
        let mut buf = vec![0u8; capped];
        reader.read_exact(&mut buf)?;
        String::from_utf8_lossy(&buf).to_string()
    } else {
        String::new()
    };

    if path == "/wake" {
        dispatch(McpBridgeMsg::WakeWindow);
        return write_http_resp(&mut stream, 200, "OK", "application/json", r#"{"ok":true,"woken":true}"#);
    }

    if path == "/mcp" {
        if method == "DELETE" {
            return write_http_resp(&mut stream, 200, "OK", "application/json", "{}");
        }
        if method == "GET" {
            return write_http_resp(
                &mut stream,
                405,
                "Method Not Allowed",
                "application/json",
                r#"{"error":"Use POST /mcp for JSON-RPC requests"}"#,
            );
        }
        if method == "POST" {
            match handle_jsonrpc_message(&body, &dispatch) {
                Some(resp_json) => {
                    return write_http_resp(&mut stream, 200, "OK", "application/json", &resp_json);
                }
                None => {
                    return write_http_resp(&mut stream, 202, "Accepted", "application/json", "");
                }
            }
        }
    }

    write_http_resp(&mut stream, 404, "Not Found", "application/json", r#"{"error":"not found"}"#)
}

fn read_chunked_body<R: BufRead>(reader: &mut R) -> io::Result<String> {
    let mut out = Vec::new();
    loop {
        let mut size_line = String::new();
        if reader.read_line(&mut size_line)? == 0 {
            break;
        }
        let hex = size_line.trim().split(';').next().unwrap_or("0").trim();
        let size = usize::from_str_radix(hex, 16).unwrap_or(0);
        if size == 0 {
            let mut trailer = String::new();
            let _ = reader.read_line(&mut trailer);
            break;
        }
        let mut chunk = vec![0u8; size];
        reader.read_exact(&mut chunk)?;
        out.extend_from_slice(&chunk);
        let mut crlf = [0u8; 2];
        let _ = reader.read_exact(&mut crlf);
    }
    Ok(String::from_utf8_lossy(&out).to_string())
}

fn write_http_resp(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &str,
) -> io::Result<()> {
    let bytes = body.as_bytes();
    let extra_auth_hdr = if status == 401 {
        "WWW-Authenticate: Bearer\r\n"
    } else {
        ""
    };
    let header = format!(
        "HTTP/1.1 {} {}\r\n{}Content-Type: {}; charset=utf-8\r\nContent-Length: {}\r\nMcp-Session-Id: tpbrowser-local\r\nConnection: close\r\n\r\n",
        status, reason, extra_auth_hdr, content_type, bytes.len()
    );
    stream.write_all(header.as_bytes())?;
    if !bytes.is_empty() {
        stream.write_all(bytes)?;
    }
    stream.flush()?;
    let _ = stream.shutdown(std::net::Shutdown::Write);
    // Drain any already-buffered request body bytes non-blockingly so Windows
    // Winsock does not emit a TCP RST when closing after an early 401/403.
    let _ = stream.set_nonblocking(true);
    let mut drain = [0u8; 4096];
    while matches!(stream.read(&mut drain), Ok(n) if n > 0) {}
    Ok(())
}

pub fn handle_jsonrpc_message<F>(raw: &str, dispatch: &F) -> Option<String>
where
    F: Fn(McpBridgeMsg),
{
    let parsed = match parse_json(raw.trim()) {
        Ok(v) => v,
        Err(e) => {
            return Some(rpc_error(JsonVal::Null, -32700, &format!("Parse error: {}", e)).to_json());
        }
    };

    if let Some(arr) = parsed.as_arr() {
        let mut results = Vec::new();
        for item in arr {
            if let Some(res) = handle_single_rpc(item, dispatch) {
                results.push(res);
            }
        }
        if results.is_empty() {
            return None;
        }
        return Some(JsonVal::Arr(results).to_json());
    }

    handle_single_rpc(&parsed, dispatch).map(|v| v.to_json())
}

fn handle_single_rpc<F>(req: &JsonVal, dispatch: &F) -> Option<JsonVal>
where
    F: Fn(McpBridgeMsg),
{
    let id = req.get("id").cloned();
    let method = match req.get("method").and_then(|v| v.as_str()) {
        Some(m) => m,
        None => {
            return id.map(|i| rpc_error(i, -32600, "Invalid Request: missing method"));
        }
    };

    // Notifications have no id (or method starts with notifications/)
    if id.is_none() || method.starts_with("notifications/") {
        return None;
    }
    let req_id = id.unwrap_or(JsonVal::Null);
    let params = req.get("params");

    match method {
        "initialize" => {
            let client_ver = params
                .and_then(|p| p.get("protocolVersion"))
                .and_then(|v| v.as_str())
                .unwrap_or("2024-11-05");
            let chosen_ver = match client_ver {
                "2025-06-18" | "2025-03-26" | "2024-11-05" => client_ver,
                _ => "2024-11-05",
            };
            let result = JsonVal::obj(vec![
                ("protocolVersion", JsonVal::Str(chosen_ver.to_string())),
                (
                    "capabilities",
                    JsonVal::obj(vec![
                        ("tools", JsonVal::obj(vec![("listChanged", JsonVal::Bool(false))])),
                        (
                            "resources",
                            JsonVal::obj(vec![
                                ("subscribe", JsonVal::Bool(false)),
                                ("listChanged", JsonVal::Bool(false)),
                            ]),
                        ),
                    ]),
                ),
                (
                    "serverInfo",
                    JsonVal::obj(vec![
                        ("name", JsonVal::Str("tpbrowser".to_string())),
                        ("version", JsonVal::Str("0.4.0".to_string())),
                    ]),
                ),
            ]);
            Some(rpc_ok(req_id, result))
        }
        "ping" => Some(rpc_ok(req_id, JsonVal::obj(vec![]))),
        "tools/list" => Some(rpc_ok(
            req_id,
            JsonVal::obj(vec![("tools", build_tools_list())]),
        )),
        "tools/call" => {
            let Some(tool_name) = params.and_then(|p| p.get("name")).and_then(|v| v.as_str()) else {
                return Some(rpc_error(req_id, -32602, "Missing params.name"));
            };
            let args_json = params
                .and_then(|p| p.get("arguments"))
                .map(|v| v.to_json())
                .unwrap_or_else(|| "{}".to_string());

            let (tx, rx) = mpsc::channel();
            dispatch(McpBridgeMsg::ToolCall {
                tool: tool_name.to_string(),
                args_json,
                reply: tx,
            });

            let outcome = rx
                .recv_timeout(Duration::from_secs(20))
                .unwrap_or_else(|_| Err("TPbrowser timeout waiting for tool execution".to_string()));

            let (text, is_error) = match outcome {
                Ok(t) => (t, false),
                Err(e) => (e, true),
            };
            let result = JsonVal::obj(vec![
                (
                    "content",
                    JsonVal::Arr(vec![JsonVal::obj(vec![
                        ("type", JsonVal::Str("text".to_string())),
                        ("text", JsonVal::Str(text)),
                    ])]),
                ),
                ("isError", JsonVal::Bool(is_error)),
            ]);
            Some(rpc_ok(req_id, result))
        }
        "resources/list" => {
            let list = JsonVal::Arr(vec![
                resource_desc("tpbrowser://tabs", "Open Browser Tabs", "Live list of tabs, URLs, titles and active index", "application/json"),
                resource_desc("tpbrowser://windows", "Window Sync & Recovery State", "Live multi-window geometry, monitor clamping, and crash recovery snapshot", "application/json"),
                resource_desc("tpbrowser://tasks", "TPbrowser Cowork Tasks", "Tasks and reminders from tasks/tasks.json", "application/json"),
                resource_desc("tpbrowser://skills", "TPbrowser Recorded Skills", "Domain automation skills from skills/skills.json", "application/json"),
            ]);
            Some(rpc_ok(req_id, JsonVal::obj(vec![("resources", list)])))
        }
        "resources/read" => {
            let Some(uri) = params.and_then(|p| p.get("uri")).and_then(|v| v.as_str()) else {
                return Some(rpc_error(req_id, -32602, "Missing params.uri"));
            };
            let (tx, rx) = mpsc::channel();
            dispatch(McpBridgeMsg::ReadResource {
                uri: uri.to_string(),
                reply: tx,
            });
            match rx.recv_timeout(Duration::from_secs(10)) {
                Ok(Ok(text)) => {
                    let contents = JsonVal::Arr(vec![JsonVal::obj(vec![
                        ("uri", JsonVal::Str(uri.to_string())),
                        ("mimeType", JsonVal::Str("application/json".to_string())),
                        ("text", JsonVal::Str(text)),
                    ])]);
                    Some(rpc_ok(req_id, JsonVal::obj(vec![("contents", contents)])))
                }
                Ok(Err(e)) => Some(rpc_error(req_id, -32602, &e)),
                Err(_) => Some(rpc_error(req_id, -32603, "Timeout reading resource")),
            }
        }
        _ => Some(rpc_error(req_id, -32601, &format!("Method not found: {}", method))),
    }
}

fn rpc_ok(id: JsonVal, result: JsonVal) -> JsonVal {
    JsonVal::obj(vec![
        ("jsonrpc", JsonVal::Str("2.0".to_string())),
        ("id", id),
        ("result", result),
    ])
}

fn rpc_error(id: JsonVal, code: i64, message: &str) -> JsonVal {
    JsonVal::obj(vec![
        ("jsonrpc", JsonVal::Str("2.0".to_string())),
        ("id", id),
        (
            "error",
            JsonVal::obj(vec![
                ("code", JsonVal::Num(code as f64)),
                ("message", JsonVal::Str(message.to_string())),
            ]),
        ),
    ])
}

fn resource_desc(uri: &str, name: &str, desc: &str, mime: &str) -> JsonVal {
    JsonVal::obj(vec![
        ("uri", JsonVal::Str(uri.to_string())),
        ("name", JsonVal::Str(name.to_string())),
        ("description", JsonVal::Str(desc.to_string())),
        ("mimeType", JsonVal::Str(mime.to_string())),
    ])
}

fn prop(ty: &str, desc: &str) -> JsonVal {
    JsonVal::obj(vec![
        ("type", JsonVal::Str(ty.to_string())),
        ("description", JsonVal::Str(desc.to_string())),
    ])
}

fn tool_def(name: &str, desc: &str, props: Vec<(&str, JsonVal)>, required: Vec<&str>) -> JsonVal {
    let req_arr: Vec<JsonVal> = required.into_iter().map(|s| JsonVal::Str(s.to_string())).collect();
    JsonVal::obj(vec![
        ("name", JsonVal::Str(name.to_string())),
        ("description", JsonVal::Str(desc.to_string())),
        (
            "inputSchema",
            JsonVal::obj(vec![
                ("type", JsonVal::Str("object".to_string())),
                ("properties", JsonVal::obj(props)),
                ("required", JsonVal::Arr(req_arr)),
            ]),
        ),
    ])
}

fn build_tools_list() -> JsonVal {
    JsonVal::Arr(vec![
        tool_def(
            "tp_list_tabs",
            "List all open TPbrowser tabs (index, id, title, url, profile, active) and current layout/window recovery state.",
            vec![],
            vec![],
        ),
        tool_def(
            "tp_window_recovery_status",
            "Inspect live multi-window geometry, open windows/tab counts, DPI/monitor sync status, and crash recovery snapshot availability.",
            vec![],
            vec![],
        ),
        tool_def(
            "tp_recover_windows",
            "Recover windows/tabs from the last recovery snapshot and clamp all visible windows inside current monitor work areas.",
            vec![],
            vec![],
        ),
        tool_def(
            "tp_open_tab",
            "Open a new tab in TPbrowser with optional URL and isolated profile flag.",
            vec![
                ("url", prop("string", "Target URL (e.g. https://example.com or tp://newtab)")),
                ("isolated", prop("boolean", "Use isolated profile/session if true (default false)")),
            ],
            vec![],
        ),
        tool_def(
            "tp_focus_tab",
            "Switch focus to an open tab by 0-based index or tab_id.",
            vec![
                ("index", prop("integer", "0-based tab index")),
                ("tab_id", prop("integer", "Unique tab ID from tp_list_tabs")),
            ],
            vec![],
        ),
        tool_def(
            "tp_close_tab",
            "Close a tab by 0-based index or tab_id (defaults to the active tab).",
            vec![
                ("index", prop("integer", "0-based tab index")),
                ("tab_id", prop("integer", "Unique tab ID")),
            ],
            vec![],
        ),
        tool_def(
            "tp_navigate",
            "Navigate the active (or specified) tab to a URL or run back/forward/reload/home.",
            vec![
                ("url", prop("string", "Destination URL or search query")),
                ("action", prop("string", "Optional action: goto (default), back, forward, reload, home")),
                ("tab_id", prop("integer", "Optional target tab_id (defaults to active tab)")),
            ],
            vec![],
        ),
        tool_def(
            "tp_get_page_context",
            "Extract page title, URL, visible text, and interactive DOM elements (with target_id & selectors) from the active tab, a specific tab_id, or 'zalo'/'fb'.",
            vec![
                ("target", prop("string", "Target: 'active' (default), 'zalo', 'fb', or numeric tab_id")),
                ("max_chars", prop("integer", "Maximum visible_text characters to return (default 9000)")),
            ],
            vec![],
        ),
        tool_def(
            "tp_execute_action",
            "Perform a DOM action (click, fill, scroll, navigate, new_tab) on the active tab or target tab_id.",
            vec![
                ("type", prop("string", "Action type: click, fill, scroll, navigate, new_tab")),
                ("target_id", prop("integer", "Element id returned by tp_get_page_context")),
                ("selector", prop("string", "CSS selector fallback")),
                ("label", prop("string", "Semantic button/input label fallback")),
                ("value", prop("string", "Text to type when type='fill'")),
                ("submit", prop("boolean", "Submit parent form after fill")),
                ("direction", prop("string", "Scroll direction: up or down")),
                ("url", prop("string", "URL when type='navigate' or 'new_tab'")),
                ("tab_id", prop("integer", "Optional target tab_id (defaults to active tab)")),
            ],
            vec!["type"],
        ),
        tool_def(
            "tp_discover_shopee_shops",
            "Read the recognized Shopee shop table in an existing tab. Account identity is unavailable.",
            vec![("tab_id", prop("integer", "Optional existing tab ID; defaults to active tab"))],
            vec![],
        ),
        tool_def(
            "tp_get_scan_state",
            "Get current multi-shop e-commerce scan state, step, queue, and recent scanner logs directly from TPbrowser memory.",
            vec![],
            vec![],
        ),
        tool_def(
            "tp_eval_js",
            "Execute arbitrary JavaScript (supports async/await and Promises) on a TPbrowser WebView and return its JSON result.",
            vec![
                ("code", prop("string", "JavaScript function body or expression (e.g. 'return document.title;')")),
                ("target", prop("string", "Target WebView: 'active' (default), 'zalo', 'fb', or numeric tab_id")),
            ],
            vec!["code"],
        ),
        tool_def(
            "tp_tasks_list",
            "Read all tasks and reminders stored in TPbrowser (tasks/tasks.json).",
            vec![],
            vec![],
        ),
        tool_def(
            "tp_tasks_save",
            "Save the full tasks JSON array to tasks/tasks.json and sync all TPbrowser views.",
            vec![
                ("tasks_json", prop("string", "JSON array string of tasks")),
            ],
            vec!["tasks_json"],
        ),
        tool_def(
            "tp_task_complete",
            "Mark a TPbrowser task as completed by id.",
            vec![
                ("id", prop("string", "Task ID")),
            ],
            vec!["id"],
        ),
        tool_def(
            "tp_task_snooze",
            "Snooze a TPbrowser reminder by N minutes.",
            vec![
                ("id", prop("string", "Task ID")),
                ("minutes", prop("integer", "Snooze duration in minutes (default 10)")),
            ],
            vec!["id"],
        ),
        tool_def(
            "tp_chat_open",
            "Open or focus TPbrowser's built-in Zalo Web, Facebook Messenger, or AI Cowork panel.",
            vec![
                ("channel", prop("string", "Channel: 'zalo', 'fb', or 'ai'")),
            ],
            vec!["channel"],
        ),
        tool_def(
            "tp_chat_scan",
            "Read the active contact name and recent visible messages from TPbrowser's Zalo Web or Messenger panel.",
            vec![
                ("channel", prop("string", "Channel: 'zalo' (default) or 'fb'")),
                ("limit", prop("integer", "Max recent message items to return (default 15)")),
            ],
            vec![],
        ),
        tool_def(
            "tp_chat_draft",
            "Insert a draft message text into the Zalo Web or Messenger input box in TPbrowser (does not auto-send).",
            vec![
                ("channel", prop("string", "Channel: 'zalo' (default) or 'fb'")),
                ("text", prop("string", "Draft message text to insert")),
            ],
            vec!["text"],
        ),
    ])
}

/// Run as an MCP stdio server (`TPbrowser.exe --mcp`), forwarding JSON-RPC lines to
/// the running TPbrowser HTTP MCP endpoint (auto-launching TPbrowser GUI if needed).
pub fn run_stdio_bridge() -> i32 {
    let port = mcp_port();
    if !is_health_ok(port) {
        if let Err(e) = spawn_gui_and_wait(port) {
            let err = rpc_error(JsonVal::Null, -32000, &format!("Failed to start TPbrowser: {}", e));
            let _ = writeln!(io::stdout(), "{}", err.to_json());
            let _ = io::stdout().flush();
            return 1;
        }
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut out = stdout.lock();

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match http_post_mcp(port, trimmed) {
                    Ok(Some(resp)) => {
                        if writeln!(out, "{}", resp).is_err() {
                            break;
                        }
                        let _ = out.flush();
                    }
                    Ok(None) => {
                        // Notification (202 Accepted) -> no stdout line
                    }
                    Err(e) => {
                        // Try to preserve request id if parseable
                        let req_id = parse_json(trimmed)
                            .ok()
                            .and_then(|v| v.get("id").cloned())
                            .unwrap_or(JsonVal::Null);
                        let err = rpc_error(req_id, -32001, &format!("TPbrowser bridge error: {}", e));
                        let _ = writeln!(out, "{}", err.to_json());
                        let _ = out.flush();
                    }
                }
            }
            Err(_) => break,
        }
    }
    0
}

fn spawn_gui_and_wait(port: u16) -> Result<(), String> {
    let exe = env::current_exe().map_err(|e| format!("current_exe: {}", e))?;
    let mut cmd = Command::new(exe);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        const DETACHED_PROCESS: u32 = 0x00000008;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }
    cmd.spawn().map_err(|e| format!("spawn GUI: {}", e))?;

    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline {
        if is_health_ok(port) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(150));
    }
    Err("Timed out waiting for TPbrowser MCP port".to_string())
}

fn is_health_ok(port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(
        &format!("127.0.0.1:{}", port).parse().unwrap(),
        Duration::from_millis(350),
    ) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(600)));
    let req = format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n", port);
    if stream.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut buf = String::new();
    let _ = stream.read_to_string(&mut buf);
    buf.contains("200 OK")
}

fn resolve_bridge_auth_token() -> Option<String> {
    if let Some(tok) = active_auth_token() {
        return Some(tok);
    }
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let raw = fs::read_to_string(exe_dir.join("config.json")).ok()?;
    let val = parse_json(&raw).ok()?;
    val.get("mcp_auth_token")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

fn http_post_mcp(port: u16, json_body: &str) -> Result<Option<String>, String> {
    let mut stream = TcpStream::connect_timeout(
        &format!("127.0.0.1:{}", port).parse().unwrap(),
        Duration::from_secs(2),
    )
    .map_err(|e| format!("connect: {}", e))?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(25)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    let bytes = json_body.as_bytes();
    let auth_hdr = resolve_bridge_auth_token()
        .map(|tok| format!("Authorization: Bearer {}\r\n", tok))
        .unwrap_or_default();
    let req = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n{}Content-Type: application/json\r\nAccept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        port,
        auth_hdr,
        bytes.len()
    );
    stream.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    stream.write_all(bytes).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(|e| e.to_string())?;
    let resp_str = String::from_utf8_lossy(&raw);
    let Some((head, body)) = resp_str.split_once("\r\n\r\n") else {
        return Err("Malformed HTTP response".to_string());
    };
    if head.contains("202 Accepted") || body.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(body.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_origin_rejected_without_dispatch_and_native_client_works() {
        fn request(origin: &str, body: &str) -> String {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let worker = thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                handle_http_conn(stream, |msg| {
                    if let McpBridgeMsg::ToolCall { reply, .. } = msg { let _ = reply.send(Ok("native-only".into())); }
                }).unwrap();
            });
            let mut client = TcpStream::connect(addr).unwrap();
            write!(client, "POST /mcp HTTP/1.1\r\nHost: localhost\r\n{}Content-Length: {}\r\n\r\n{}", origin, body.len(), body).unwrap();
            let mut response = String::new(); let _ = client.read_to_string(&mut response);
            worker.join().unwrap(); response
        }
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"tp_discover_shopee_shops"}}"#;
        let rejected = request("Origin: https://evil.example\r\n", body);
        assert!(rejected.starts_with("HTTP/1.1 403"));
        assert!(!rejected.contains("native-only"));
        for tool in ["tp_eval_js", "tp_get_page_context"] {
            let payload = body.replace("tp_discover_shopee_shops", tool);
            let response = request("oRiGiN: null\r\n", &payload);
            assert!(response.starts_with("HTTP/1.1 403"), "{}", tool);
            assert!(!response.contains("Access-Control-Allow-Origin"));
        }
        let native = request("", body);
        assert!(native.starts_with("HTTP/1.1 200"));
        assert!(native.contains("native-only"));
    }

    #[test]
    fn proxied_mcp_requests_require_bearer_token_and_reject_bad_auth() {
        fn request_with_auth(headers: &str, path: &str, body: &str, expected: Option<&str>, require_local: bool) -> String {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let exp = expected.map(ToString::to_string);
            let worker = thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                handle_http_conn_with_auth(
                    stream,
                    |msg| match msg {
                        McpBridgeMsg::ToolCall { reply, .. } => {
                            let _ = reply.send(Ok("authed-ok".into()));
                        }
                        McpBridgeMsg::WakeWindow => {}
                        _ => {}
                    },
                    exp.as_deref(),
                    require_local,
                )
                .unwrap();
            });
            let mut client = TcpStream::connect(addr).unwrap();
            write!(
                client,
                "POST {} HTTP/1.1\r\n{}Content-Length: {}\r\n\r\n{}",
                path,
                headers,
                body.len(),
                body
            )
            .unwrap();
            let _ = client.flush();
            let _ = client.shutdown(std::net::Shutdown::Write);
            let mut raw = Vec::new();
            let mut buf = [0u8; 1024];
            loop {
                match client.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => raw.extend_from_slice(&buf[..n]),
                }
            }
            let response = String::from_utf8_lossy(&raw).to_string();
            worker.join().unwrap();
            response
        }

        let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"tp_eval_js","arguments":{"code":"1+1"}}}"#;
        let token = "tp_test_secret_token";

        // Proxied via Cloudflare tunnel without Bearer token -> 401
        let cf_no_auth = request_with_auth(
            "Host: demo.trycloudflare.com\r\nCF-Connecting-IP: 1.2.3.4\r\n",
            "/mcp",
            body,
            Some(token),
            false,
        );
        assert!(cf_no_auth.starts_with("HTTP/1.1 401"));
        assert!(cf_no_auth.contains("WWW-Authenticate: Bearer"));
        assert!(!cf_no_auth.contains("authed-ok"));

        // Proxied via X-Forwarded-For with wrong token -> 401
        let xff_bad_auth = request_with_auth(
            "Host: 127.0.0.1:17777\r\nX-Forwarded-For: 8.8.8.8\r\nAuthorization: Bearer wrong\r\n",
            "/mcp",
            body,
            Some(token),
            false,
        );
        assert!(xff_bad_auth.starts_with("HTTP/1.1 401"));
        assert!(!xff_bad_auth.contains("authed-ok"));

        // Proxied /wake without token -> 401
        let wake_no_auth = request_with_auth(
            "Host: demo.trycloudflare.com\r\nCF-Ray: abc\r\n",
            "/wake",
            "",
            Some(token),
            false,
        );
        assert!(wake_no_auth.starts_with("HTTP/1.1 401"));

        // Proxied with valid Bearer token -> 200 OK
        let cf_valid_auth = request_with_auth(
            "Host: demo.trycloudflare.com\r\nCF-Connecting-IP: 1.2.3.4\r\nAuthorization: Bearer tp_test_secret_token\r\n",
            "/mcp",
            body,
            Some(token),
            false,
        );
        assert!(cf_valid_auth.starts_with("HTTP/1.1 200"));
        assert!(cf_valid_auth.contains("authed-ok"));

        // Direct loopback without proxy headers -> allowed when require_local_auth=false
        let local_no_auth = request_with_auth(
            "Host: 127.0.0.1:17777\r\n",
            "/mcp",
            body,
            Some(token),
            false,
        );
        assert!(local_no_auth.starts_with("HTTP/1.1 200"));
        assert!(local_no_auth.contains("authed-ok"));

        // Direct loopback with wrong Authorization header -> still rejected 401
        let local_wrong_auth = request_with_auth(
            "Host: localhost:17777\r\nAuthorization: Bearer invalid\r\n",
            "/mcp",
            body,
            Some(token),
            false,
        );
        assert!(local_wrong_auth.starts_with("HTTP/1.1 401"));

        // Direct loopback with require_local_auth=true without token -> 401
        let strict_local = request_with_auth(
            "Host: 127.0.0.1:17777\r\n",
            "/mcp",
            body,
            Some(token),
            true,
        );
        assert!(strict_local.starts_with("HTTP/1.1 401"));
    }


    #[test]
    fn test_initialize_and_tools_list() {
        let dummy = |_msg: McpBridgeMsg| {};
        let init_req = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#;
        let resp = handle_jsonrpc_message(init_req, &dummy).unwrap();
        let v = parse_json(&resp).unwrap();
        assert_eq!(
            v.get("result").unwrap().get("protocolVersion").unwrap().as_str(),
            Some("2025-03-26")
        );
        assert_eq!(
            v.get("result").unwrap().get("serverInfo").unwrap().get("name").unwrap().as_str(),
            Some("tpbrowser")
        );

        let notif = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        assert!(handle_jsonrpc_message(notif, &dummy).is_none());

        let tools_req = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#;
        let tools_resp = handle_jsonrpc_message(tools_req, &dummy).unwrap();
        let tv = parse_json(&tools_resp).unwrap();
        let tools = tv.get("result").unwrap().get("tools").unwrap().as_arr().unwrap();
        assert!(tools.len() >= 15);
        assert!(tools.iter().any(|t| t.get("name").and_then(|n| n.as_str()) == Some("tp_eval_js")));
        assert!(tools.iter().any(|t| t.get("name").and_then(|n| n.as_str()) == Some("tp_recover_windows")));
        assert!(tools.iter().any(|t| t.get("name").and_then(|n| n.as_str()) == Some("tp_window_recovery_status")));

        let res_req = r#"{"jsonrpc":"2.0","id":3,"method":"resources/list","params":{}}"#;
        let res_resp = handle_jsonrpc_message(res_req, &dummy).unwrap();
        let rv = parse_json(&res_resp).unwrap();
        let resources = rv.get("result").unwrap().get("resources").unwrap().as_arr().unwrap();
        assert!(resources.iter().any(|r| r.get("uri").and_then(|u| u.as_str()) == Some("tpbrowser://windows")));
    }
}
