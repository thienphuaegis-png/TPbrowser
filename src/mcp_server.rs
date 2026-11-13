use std::{
    env,
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use crate::json_mini::{parse_json, JsonVal};

pub const DEFAULT_MCP_PORT: u16 = 17777;

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

fn handle_http_conn<F>(mut stream: TcpStream, dispatch: F) -> io::Result<()>
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
            let kl = k.trim().to_ascii_lowercase();
            let vl = v.trim();
            if kl == "content-length" {
                content_len = vl.parse::<usize>().unwrap_or(0);
            } else if kl == "transfer-encoding" && vl.to_ascii_lowercase().contains("chunked") {
                chunked = true;
            }
        }
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
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}; charset=utf-8\r\nContent-Length: {}\r\nMcp-Session-Id: tpbrowser-local\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: Content-Type, Authorization, Mcp-Session-Id, Accept\r\nAccess-Control-Allow-Methods: GET, POST, DELETE, OPTIONS\r\nConnection: close\r\n\r\n",
        status, reason, content_type, bytes.len()
    );
    stream.write_all(header.as_bytes())?;
    if !bytes.is_empty() {
        stream.write_all(bytes)?;
    }
    stream.flush()
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
            "List all open TPbrowser tabs (index, id, title, url, profile, active) and current layout/chat state.",
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

fn http_post_mcp(port: u16, json_body: &str) -> Result<Option<String>, String> {
    let mut stream = TcpStream::connect_timeout(
        &format!("127.0.0.1:{}", port).parse().unwrap(),
        Duration::from_secs(2),
    )
    .map_err(|e| format!("connect: {}", e))?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(25)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    let bytes = json_body.as_bytes();
    let req = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nAccept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        port,
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
    }
}
