#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod layout;

use std::{
    env, fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use layout::{
    compute_content_panes, compute_shell_layout, AiDisplayMode, ContentViewMode, TabOrientation,
    RIGHT_AI_DOCK_WIDTH,
};
use tao::{
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    keyboard::{KeyCode, ModifiersState},
    window::{Window, WindowBuilder},
};
use wry::{
    MemoryUsageLevel, NewWindowResponse, PageLoadEvent, PermissionKind, PermissionResponse, Rect,
    Theme, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows,
};

const CHROME_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

const TAB_SHELL_HTML: &str = include_str!("tab_shell.html");
const NEWTAB_HTML: &str = include_str!("newtab.html");
const AI_PANEL_HTML: &str = include_str!("ai_panel.html");
const CONTENT_AGENT_JS: &str = include_str!("content_agent.js");

static NEXT_TAB_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
enum AppCmd {
    ToggleTabOrientation,
    ToggleSidebarCollapse,
    CycleViewMode,
    ToggleAiMode,
    SetAiMode(AiDisplayMode),
    SyncAiPanelInit,
    OpenAiSettings,
    FocusAiPanel,
    OpenGoogleLogin,
    SetGoogleUser(String),
    SetUiTheme(String),
    NewTab {
        url: String,
        isolated: bool,
    },
    FocusIndex(usize),
    FocusTabById(u64),
    CloseIndex(usize),
    CloseActiveTab,
    NavigateActive(String),
    NavBack,
    NavForward,
    ReloadActive,
    GoHome,
    BookmarkActive,
    SaveBookmarks(String),
    SaveAiConfig {
        provider: String,
        api_key: String,
        model: String,
        base_url: String,
        auto_minimize_on_task: bool,
    },
    SaveSkillsJson(String),
    StartSkillRecording,
    StopSkillRecording,
    RecordedSkillStep(String),
    RunAiPrompt(String),
    RequestDomContextForAi,
    DomContextResult {
        tab_id: u64,
        title: String,
        url: String,
        visible_text: String,
        elements_json: String,
    },
    ExecuteDomAction(String),
    SyncBubbleStatus {
        working: bool,
        recording: bool,
        status_text: String,
        last_reply: String,
    },
    PageFinished {
        tab_id: u64,
        url: String,
    },
}

struct BrowserTab {
    id: u64,
    profile_name: String,
    url: String,
    title: String,
    webview: WebView,
}

struct TpBrowserApp {
    window: Window,
    proxy: EventLoopProxy<AppCmd>,
    web_context: WebContext,
    exe_dir: PathBuf,
    tab_shell: WebView,
    ai_shell: WebView,
    tabs: Vec<BrowserTab>,
    active_idx: usize,
    tab_orientation: TabOrientation,
    sidebar_collapsed: bool,
    view_mode: ContentViewMode,
    ai_mode: AiDisplayMode,
    ai_working: bool,
    ai_recording: bool,
    ai_status_text: String,
    ai_last_reply: String,
    master_ratio: f32,
    profile_counter: u32,
    // Config fields (persisted in config.json)
    ai_provider: String,
    ai_api_key: String,
    ai_model: String,
    ai_base_url: String,
    auto_minimize_on_task: bool,
    google_user: String,
    ui_theme: String,
    bookmarks_json: String,
    skills_json: String,
}

impl TpBrowserApp {
    fn new(window: Window, proxy: EventLoopProxy<AppCmd>, exe_dir: PathBuf) -> wry::Result<Self> {
        let data_dir = exe_dir.join("user_data");
        let _ = fs::create_dir_all(&data_dir);
        let skills_dir = exe_dir.join("skills");
        let _ = fs::create_dir_all(&skills_dir);
        let mut web_context = WebContext::new(Some(data_dir));

        let extensions_dir = exe_dir.join("extensions");
        let _ = fs::create_dir_all(&extensions_dir);

        let proxy_tab = proxy.clone();
        let tab_shell = WebViewBuilder::new_with_web_context(&mut web_context)
            .with_browser_extensions_enabled(true)
            .with_extensions_path(extensions_dir.clone())
            .with_profile_name("default".to_string())
            .with_theme(Theme::Light)
            .with_background_color((248, 250, 252, 255))
            .with_html(TAB_SHELL_HTML)
            .with_ipc_handler(move |req| {
                Self::handle_ipc(&proxy_tab, req.body());
            })
            .with_bounds(Rect {
                position: LogicalPosition::new(0, 0).into(),
                size: LogicalSize::new(228, 800).into(),
            })
            .build_as_child(&window)?;

        let shared_env = tab_shell.environment();

        let proxy_ai = proxy.clone();
        let ai_shell = WebViewBuilder::new_with_web_context(&mut web_context)
            .with_environment(shared_env)
            .with_profile_name("default".to_string())
            .with_theme(Theme::Light)
            .with_background_color((255, 255, 255, 255))
            .with_html(AI_PANEL_HTML)
            .with_ipc_handler(move |req| {
                Self::handle_ipc(&proxy_ai, req.body());
            })
            .with_bounds(Rect {
                position: LogicalPosition::new(1000, 0).into(),
                size: LogicalSize::new(RIGHT_AI_DOCK_WIDTH, 800).into(),
            })
            .build_as_child(&window)?;

        let _ = ai_shell.set_memory_usage_level(MemoryUsageLevel::Normal);

        let default_bookmarks = r#"[{"title":"Google Sheets","url":"https://docs.google.com/spreadsheets"},{"title":"Gmail Cá Nhân","url":"https://mail.google.com"},{"title":"Google Drive","url":"https://drive.google.com"},{"title":"FB Messenger","url":"https://www.messenger.com"},{"title":"Zalo Web","url":"https://chat.zalo.me"},{"title":"YouTube","url":"https://www.youtube.com"},{"title":"Google Gemini","url":"https://gemini.google.com"},{"title":"Tài Khoản Google","url":"https://myaccount.google.com"}]"#.to_string();
        let default_skills = r#"[{"name":"Kiểm tra & Tóm tắt nhanh trang","domain":"*","notes":"Đọc tiêu đề, các mục chính và số liệu quan trọng trên trang đang mở, trình bày ngắn gọn gạch đầu dòng.","steps":[]},{"name":"Chia đôi màn hình so sánh 2 Tab","domain":"*","notes":"Chuyển sang chế độ chia khung Xoắn ốc BSP để đối chiếu 2 trang web cạnh nhau.","steps":[{"type":"split_view"}]}]"#.to_string();

        let mut app = Self {
            window,
            proxy,
            web_context,
            exe_dir,
            tab_shell,
            ai_shell,
            tabs: Vec::new(),
            active_idx: 0,
            tab_orientation: TabOrientation::VerticalLeft,
            sidebar_collapsed: false,
            view_mode: ContentViewMode::SingleTab,
            ai_mode: AiDisplayMode::RightDock,
            ai_working: false,
            ai_recording: false,
            ai_status_text: "🤖 AI Cowork sẵn sàng".to_string(),
            ai_last_reply: String::new(),
            master_ratio: 0.54,
            profile_counter: 1,
            ai_provider: "openai".to_string(),
            ai_api_key: String::new(),
            ai_model: "gemini-3.8-flash".to_string(),
            ai_base_url: "http://127.0.0.1:8045/v1".to_string(),
            auto_minimize_on_task: true,
            google_user: String::new(),
            ui_theme: "obsidian-gold".to_string(),
            bookmarks_json: default_bookmarks,
            skills_json: default_skills,
        };

        app.load_config();
        app.load_skills();
        app.load_session_or_default();
        Ok(app)
    }

    fn normalize_url(input: &str) -> String {
        let trimmed = input.trim();
        if trimmed.is_empty()
            || trimmed.eq_ignore_ascii_case("tp://newtab")
            || trimmed.eq_ignore_ascii_case("about:blank")
        {
            return "tp://newtab".to_string();
        }
        if trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
            || trimmed.starts_with("chrome-extension://")
            || trimmed.starts_with("edge://")
        {
            return trimmed.to_string();
        }
        if trimmed.contains('.') && !trimmed.contains(' ') {
            return format!("https://{}", trimmed);
        }
        let encoded: String = trimmed
            .bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                    (b as char).to_string()
                }
                b' ' => "+".to_string(),
                _ => format!("%{:02X}", b),
            })
            .collect();
        format!("https://www.google.com/search?q={}", encoded)
    }

    fn title_from_url(url: &str) -> String {
        if url == "tp://newtab" {
            return "Không gian bắt đầu".to_string();
        }
        if url.starts_with("chrome-extension://") && url.contains("dashboard.html") {
            return "🛡️ uBlock Origin Dashboard".to_string();
        }
        let clean = url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("www.");
        clean.split('/').next().unwrap_or(url).to_string()
    }

    fn add_tab(&mut self, raw_url: &str, isolated: bool) {
        if self.tabs.len() >= 12 {
            return;
        }
        let tab_id = NEXT_TAB_ID.fetch_add(1, Ordering::SeqCst);
        let profile_name = if isolated {
            self.profile_counter += 1;
            format!("acc_{}", self.profile_counter)
        } else {
            "default".to_string()
        };

        let url = Self::normalize_url(raw_url);
        let init_js = CONTENT_AGENT_JS.replace("__TAB_ID__", &tab_id.to_string());

        let proxy_ipc = self.proxy.clone();
        let proxy_popup = self.proxy.clone();
        let proxy_load = self.proxy.clone();
        let shared_env = self.tab_shell.environment();

        let extensions_dir = self.exe_dir.join("extensions");
        let mut builder = WebViewBuilder::new_with_web_context(&mut self.web_context)
            .with_environment(shared_env)
            .with_browser_extensions_enabled(true)
            .with_user_agent(CHROME_USER_AGENT)
            .with_autoplay(true)
            .with_clipboard(true)
            .with_devtools(true)
            .with_theme(Theme::Light)
            .with_background_color((255, 255, 255, 255))
            .with_profile_name(profile_name.clone())
            .with_initialization_script(&init_js)
            .with_on_page_load_handler(move |event, loaded_url| {
                if matches!(event, PageLoadEvent::Finished) {
                    let _ = proxy_load.send_event(AppCmd::PageFinished {
                        tab_id,
                        url: loaded_url,
                    });
                }
            })
            .with_permission_handler(|kind| match kind {
                PermissionKind::Microphone
                | PermissionKind::Camera
                | PermissionKind::ClipboardRead
                | PermissionKind::Notifications
                | PermissionKind::Autoplay
                | PermissionKind::WindowManagement => PermissionResponse::Allow,
                _ => PermissionResponse::Default,
            })
            .with_new_window_req_handler(move |popup_url, features| {
                let lower = popup_url.to_lowercase();
                // Chặn đứng mọi cửa sổ/tab quảng cáo, cá cược, popunder tự nhảy ra
                let is_ad_or_popunder = lower.contains("chinhhang")
                    || lower.contains("adqc.")
                    || lower.contains("popunder")
                    || lower.contains("doubleclick")
                    || lower.contains("googlesyndication")
                    || lower.contains("adservice")
                    || lower.contains("popads")
                    || lower.contains("hitclub")
                    || lower.contains("sunwin")
                    || lower.contains("rikvip")
                    || lower.contains("gemwin")
                    || lower.contains("win79")
                    || lower.contains("yo88")
                    || lower.contains("uk88")
                    || lower.contains("bom88")
                    || lower.contains("tic88")
                    || lower.contains("haywin")
                    || lower.contains("zowin")
                    || lower.contains("zo88");

                if is_ad_or_popunder {
                    return NewWindowResponse::Deny;
                }

                let is_call_or_auth = features.size.is_some()
                    || popup_url.is_empty()
                    || popup_url == "about:blank"
                    || popup_url.contains("messenger.com")
                    || popup_url.contains("facebook.com")
                    || popup_url.contains("accounts.google.com")
                    || popup_url.contains("oauth")
                    || popup_url.contains("login");

                if is_call_or_auth {
                    NewWindowResponse::Allow
                } else {
                    let _ = proxy_popup.send_event(AppCmd::NewTab {
                        url: popup_url,
                        isolated: false,
                    });
                    NewWindowResponse::Deny
                }
            })
            .with_ipc_handler(move |req| {
                Self::handle_ipc(&proxy_ipc, req.body());
            })
            .with_bounds(Rect {
                position: LogicalPosition::new(228, 0).into(),
                size: LogicalSize::new(800, 800).into(),
            });

        if isolated {
            builder = builder.with_extensions_path(extensions_dir);
        }

        let is_ext_url = url.starts_with("chrome-extension://");
        builder = if url == "tp://newtab" {
            builder.with_html(NEWTAB_HTML)
        } else if is_ext_url {
            builder.with_html("<html><body style='background:#f8fafc;'></body></html>")
        } else {
            builder.with_url(&url)
        };

        if let Ok(webview) = builder.build_as_child(&self.window) {
            if is_ext_url {
                let ext_target = url.clone();
                let proxy_delayed = self.proxy.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(650));
                    let _ = proxy_delayed.send_event(AppCmd::NavigateActive(ext_target));
                });
            }
            let title = Self::title_from_url(&url);
            self.tabs.push(BrowserTab {
                id: tab_id,
                profile_name,
                url,
                title,
                webview,
            });
            self.active_idx = self.tabs.len() - 1;
            self.relayout();
            self.sync_all_ui();
            self.save_session();
        }
    }

    fn relayout(&self) {
        let scale = self.window.scale_factor();
        let logical = self.window.inner_size().to_logical::<u32>(scale);
        let shell = compute_shell_layout(
            logical.width,
            logical.height,
            self.tab_orientation,
            self.sidebar_collapsed,
            self.ai_mode,
            RIGHT_AI_DOCK_WIDTH,
        );

        let _ = self.tab_shell.set_visible(true);
        let _ = self.tab_shell.set_bounds(Rect {
            position: LogicalPosition::new(shell.tab_bar.x, shell.tab_bar.y).into(),
            size: LogicalSize::new(shell.tab_bar.width, shell.tab_bar.height).into(),
        });

        if let Some(ai_rect) = shell.ai_dock {
            let _ = self.ai_shell.set_visible(true);
            let _ = self.ai_shell.set_bounds(Rect {
                position: LogicalPosition::new(ai_rect.x, ai_rect.y).into(),
                size: LogicalSize::new(ai_rect.width, ai_rect.height).into(),
            });
        } else {
            // Khi thu nhỏ thành Bong bóng, ẩn khung Dock bên phải để trang web rộng 100%,
            // nhưng vẫn giữ MemoryUsageLevel::Normal để AI chạy ngầm liên tục!
            let _ = self.ai_shell.set_visible(false);
            let _ = self
                .ai_shell
                .set_memory_usage_level(MemoryUsageLevel::Normal);
        }

        let total = self.tabs.len();
        if total == 0 {
            return;
        }

        if self.view_mode == ContentViewMode::SingleTab || total == 1 {
            for (idx, tab) in self.tabs.iter().enumerate() {
                if idx == self.active_idx {
                    let _ = tab.webview.set_visible(true);
                    let _ = tab.webview.set_bounds(Rect {
                        position: LogicalPosition::new(
                            shell.content_area.x,
                            shell.content_area.y,
                        )
                        .into(),
                        size: LogicalSize::new(
                            shell.content_area.width,
                            shell.content_area.height,
                        )
                        .into(),
                    });
                    let _ = tab
                        .webview
                        .set_memory_usage_level(MemoryUsageLevel::Normal);
                } else {
                    let _ = tab.webview.set_visible(false);
                    let _ = tab.webview.set_memory_usage_level(MemoryUsageLevel::Low);
                }
            }
        } else {
            let rects = compute_content_panes(
                shell.content_area,
                self.view_mode,
                total,
                self.master_ratio,
                4,
            );
            for (idx, tab) in self.tabs.iter().enumerate() {
                if let Some(r) = rects.get(idx) {
                    let _ = tab.webview.set_visible(true);
                    let _ = tab.webview.set_bounds(Rect {
                        position: LogicalPosition::new(r.x, r.y).into(),
                        size: LogicalSize::new(r.width, r.height).into(),
                    });
                    let _ = tab.webview.set_memory_usage_level(if idx == self.active_idx {
                        MemoryUsageLevel::Normal
                    } else {
                        MemoryUsageLevel::Low
                    });
                }
            }
        }
    }

    fn sync_all_ui(&self) {
        // 1. Đồng bộ Thanh Quản Lý Tab (Dọc / Ngang)
        let mut tabs_json = Vec::new();
        for t in &self.tabs {
            tabs_json.push(format!(
                "{{\"id\":{},\"url\":\"{}\",\"title\":\"{}\",\"profile\":\"{}\"}}",
                t.id,
                Self::escape_json(&t.url),
                Self::escape_json(&t.title),
                Self::escape_json(&t.profile_name)
            ));
        }

        let shell_state = format!(
            "{{\"orientation\":\"{}\",\"sidebar_collapsed\":{},\"view_mode\":\"{}\",\"ai_mode\":\"{}\",\"ai_working\":{},\"active_idx\":{},\"google_user\":\"{}\",\"ui_theme\":\"{}\",\"tabs\":[{}]}}",
            self.tab_orientation.as_str(),
            self.sidebar_collapsed,
            self.view_mode.as_str(),
            self.ai_mode.as_str(),
            self.ai_working,
            self.active_idx,
            Self::escape_json(&self.google_user),
            Self::escape_json(&self.ui_theme),
            tabs_json.join(",")
        );
        let _ = self.tab_shell.evaluate_script(&format!(
            "if (window.__tpSyncShellState) window.__tpSyncShellState({});",
            shell_state
        ));

        // 2. Đồng bộ Bong bóng AI & Trang Bắt Đầu Trắng trên các Tab
        let is_split = self.view_mode != ContentViewMode::SingleTab && self.tabs.len() > 1;
        for (idx, t) in self.tabs.iter().enumerate() {
            let show_ring = is_split && idx == self.active_idx;
            let bubble_state = format!(
                "{{\"ai_mode\":\"{}\",\"working\":{},\"recording\":{},\"status_text\":\"{}\",\"last_reply\":\"{}\",\"show_split_ring\":{}}}",
                self.ai_mode.as_str(),
                self.ai_working,
                self.ai_recording,
                Self::escape_json(&self.ai_status_text),
                Self::escape_json(&self.ai_last_reply),
                show_ring
            );
            let _ = t.webview.evaluate_script(&format!(
                "if (window.__tpUpdateBubbleUi) window.__tpUpdateBubbleUi({});",
                bubble_state
            ));

            if t.url == "tp://newtab" {
                let nt_cfg = format!(
                    "{{\"google_user\":\"{}\",\"ui_theme\":\"{}\",\"bookmarks\":{}}}",
                    Self::escape_json(&self.google_user),
                    Self::escape_json(&self.ui_theme),
                    if self.bookmarks_json.trim().is_empty() {
                        "[]"
                    } else {
                        &self.bookmarks_json
                    }
                );
                let _ = t.webview.evaluate_script(&format!(
                    "if (window.__tpSyncNewTab) window.__tpSyncNewTab({});",
                    nt_cfg
                ));
            }
        }

        // 3. Cập nhật tiêu đề cửa sổ chính
        let tab_mode_lbl = match self.tab_orientation {
            TabOrientation::VerticalLeft => "Tab Dọc Trái",
            TabOrientation::HorizontalTop => "Tab Ngang Trên",
        };
        let ai_lbl = match self.ai_mode {
            AiDisplayMode::RightDock => "AI Ngăn Phải",
            AiDisplayMode::BubbleMinimized => {
                if self.ai_recording {
                    "🔴 Đang Ghi Mẫu Skill"
                } else if self.ai_working {
                    "AI Bong Bóng (Đang chạy...)"
                } else {
                    "AI Bong Bóng"
                }
            }
        };
        self.window.set_title(&format!(
            "TPbrowser [Tab {}/{} • {} • {}]",
            self.active_idx + 1,
            self.tabs.len().max(1),
            tab_mode_lbl,
            ai_lbl
        ));
    }

    fn sync_ai_config_to_panel(&self, open_drawer: bool) {
        let skills_raw = if self.skills_json.trim().is_empty() {
            "[]"
        } else {
            &self.skills_json
        };
        let cfg_js = format!(
            "if (window.__tpLoadAiConfigFromRust) window.__tpLoadAiConfigFromRust({{\"provider\":\"{}\",\"api_key\":\"{}\",\"model\":\"{}\",\"base_url\":\"{}\",\"auto_minimize_on_task\":{},\"open_drawer\":{},\"ui_theme\":\"{}\",\"skills\":{}}});",
            Self::escape_json(&self.ai_provider),
            Self::escape_json(&self.ai_api_key),
            Self::escape_json(&self.ai_model),
            Self::escape_json(&self.ai_base_url),
            self.auto_minimize_on_task,
            open_drawer,
            Self::escape_json(&self.ui_theme),
            skills_raw
        );
        let _ = self.ai_shell.evaluate_script(&cfg_js);
    }

    fn save_skills(&self) {
        let skills_dir = self.exe_dir.join("skills");
        let _ = fs::create_dir_all(&skills_dir);
        let path = skills_dir.join("skills.json");
        let _ = fs::write(path, &self.skills_json);
    }

    fn load_skills(&mut self) {
        let path = self.exe_dir.join("skills").join("skills.json");
        if let Ok(raw) = fs::read_to_string(path) {
            let trimmed = raw.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                self.skills_json = trimmed.to_string();
            }
        } else {
            self.save_skills();
        }
    }

    fn save_config(&self) {
        let cfg_path = self.exe_dir.join("config.json");
        let body = format!(
            "{{\n  \"tab_orientation\": \"{}\",\n  \"sidebar_collapsed\": {},\n  \"ai_mode\": \"{}\",\n  \"ai_provider\": \"{}\",\n  \"ai_api_key\": \"{}\",\n  \"ai_model\": \"{}\",\n  \"ai_base_url\": \"{}\",\n  \"auto_minimize_on_task\": {},\n  \"google_user\": \"{}\",\n  \"ui_theme\": \"{}\",\n  \"bookmarks\": {}\n}}",
            self.tab_orientation.as_str(),
            self.sidebar_collapsed,
            self.ai_mode.as_str(),
            Self::escape_json(&self.ai_provider),
            Self::escape_json(&self.ai_api_key),
            Self::escape_json(&self.ai_model),
            Self::escape_json(&self.ai_base_url),
            self.auto_minimize_on_task,
            Self::escape_json(&self.google_user),
            Self::escape_json(&self.ui_theme),
            if self.bookmarks_json.trim().is_empty() {
                "[]"
            } else {
                &self.bookmarks_json
            }
        );
        let _ = fs::write(cfg_path, body);
    }

    fn load_config(&mut self) {
        let cfg_path = self.exe_dir.join("config.json");
        let Ok(raw) = fs::read_to_string(cfg_path) else {
            return;
        };
        if let Some(v) = Self::extract_string(&raw, "\"tab_orientation\":\"")
            .or_else(|| Self::extract_string(&raw, "\"tab_orientation\": \""))
        {
            self.tab_orientation = TabOrientation::from_str(&v);
        }
        if raw.contains("\"sidebar_collapsed\": true") || raw.contains("\"sidebar_collapsed\":true")
        {
            self.sidebar_collapsed = true;
        }
        if let Some(v) = Self::extract_string(&raw, "\"ai_mode\":\"")
            .or_else(|| Self::extract_string(&raw, "\"ai_mode\": \""))
        {
            self.ai_mode = AiDisplayMode::from_str(&v);
        }
        if let Some(v) = Self::extract_string(&raw, "\"ai_provider\":\"")
            .or_else(|| Self::extract_string(&raw, "\"ai_provider\": \""))
        {
            self.ai_provider = v;
        }
        if let Some(v) = Self::extract_string(&raw, "\"ai_api_key\":\"")
            .or_else(|| Self::extract_string(&raw, "\"ai_api_key\": \""))
        {
            self.ai_api_key = v;
        }
        if let Some(v) = Self::extract_string(&raw, "\"ai_model\":\"")
            .or_else(|| Self::extract_string(&raw, "\"ai_model\": \""))
        {
            if !v.is_empty() {
                self.ai_model = v;
            }
        }
        if let Some(v) = Self::extract_string(&raw, "\"ai_base_url\":\"")
            .or_else(|| Self::extract_string(&raw, "\"ai_base_url\": \""))
        {
            if !v.is_empty() {
                self.ai_base_url = v;
            }
        }
        if raw.contains("\"auto_minimize_on_task\": false")
            || raw.contains("\"auto_minimize_on_task\":false")
        {
            self.auto_minimize_on_task = false;
        }
        if let Some(v) = Self::extract_string(&raw, "\"google_user\":\"")
            .or_else(|| Self::extract_string(&raw, "\"google_user\": \""))
        {
            self.google_user = v;
        }
        if let Some(v) = Self::extract_string(&raw, "\"ui_theme\":\"")
            .or_else(|| Self::extract_string(&raw, "\"ui_theme\": \""))
        {
            if !v.is_empty() {
                self.ui_theme = v;
            }
        }
        if let Some(bm_pos) = raw.find("\"bookmarks\":") {
            let after = raw[bm_pos + 12..].trim();
            if after.starts_with('[') {
                if let Some(end_pos) = after.rfind(']') {
                    self.bookmarks_json = after[..=end_pos].to_string();
                }
            }
        }
    }

    fn save_session(&self) {
        let session_path = self.exe_dir.join("session.json");
        let mut tabs_arr = Vec::new();
        for t in &self.tabs {
            tabs_arr.push(format!(
                "{{\"url\":\"{}\",\"profile\":\"{}\"}}",
                Self::escape_json(&t.url),
                Self::escape_json(&t.profile_name)
            ));
        }
        let body = format!(
            "{{\n  \"view_mode\": \"{}\",\n  \"active_idx\": {},\n  \"tabs\": [\n    {}\n  ]\n}}",
            self.view_mode.as_str(),
            self.active_idx,
            tabs_arr.join(",\n    ")
        );
        let _ = fs::write(session_path, body);
    }

    fn load_session_or_default(&mut self) {
        let session_path = self.exe_dir.join("session.json");
        if let Ok(raw) = fs::read_to_string(session_path) {
            if let Some(vm) = Self::extract_string(&raw, "\"view_mode\":\"")
                .or_else(|| Self::extract_string(&raw, "\"view_mode\": \""))
            {
                self.view_mode = ContentViewMode::from_str(&vm);
            }
            for line in raw.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("{\"url\":\"") {
                    if let Some(u) = Self::extract_string(trimmed, "\"url\":\"") {
                        self.add_tab(&u, false);
                    }
                }
            }
        }
        if self.tabs.is_empty() {
            self.add_tab("tp://newtab", false);
        }
    }

    fn escape_json(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\r', "")
            .replace('\n', "\\n")
    }

    fn extract_u64(json: &str, key: &str) -> Option<u64> {
        let start = json.find(key)? + key.len();
        let rest = json[start..].trim_start();
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse().ok()
    }

    fn extract_string(json: &str, key: &str) -> Option<String> {
        let start = json.find(key)? + key.len();
        let rest = &json[start..];
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(n) = chars.next() {
                    match n {
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        _ => out.push(n),
                    }
                }
            } else if ch == '"' {
                break;
            } else {
                out.push(ch);
            }
        }
        Some(out)
    }

    fn handle_ipc(proxy: &EventLoopProxy<AppCmd>, body: &str) {
        let tab_id = Self::extract_u64(body, "\"tab_id\":").unwrap_or(0);
        if body.contains("\"cmd\":\"toggle_tab_orientation\"") {
            let _ = proxy.send_event(AppCmd::ToggleTabOrientation);
        } else if body.contains("\"cmd\":\"toggle_sidebar_collapse\"") {
            let _ = proxy.send_event(AppCmd::ToggleSidebarCollapse);
        } else if body.contains("\"cmd\":\"cycle_view_mode\"") {
            let _ = proxy.send_event(AppCmd::CycleViewMode);
        } else if body.contains("\"cmd\":\"toggle_ai_mode\"") {
            let _ = proxy.send_event(AppCmd::ToggleAiMode);
        } else if body.contains("\"cmd\":\"set_ai_mode\"") {
            if let Some(m) = Self::extract_string(body, "\"mode\":\"") {
                let _ = proxy.send_event(AppCmd::SetAiMode(AiDisplayMode::from_str(&m)));
            }
        } else if body.contains("\"cmd\":\"ai_panel_ready\"") {
            let _ = proxy.send_event(AppCmd::SyncAiPanelInit);
        } else if body.contains("\"cmd\":\"open_ai_settings\"") {
            let _ = proxy.send_event(AppCmd::OpenAiSettings);
        } else if body.contains("\"cmd\":\"focus_ai_panel\"") {
            let _ = proxy.send_event(AppCmd::FocusAiPanel);
        } else if body.contains("\"cmd\":\"open_google_login\"") {
            let _ = proxy.send_event(AppCmd::OpenGoogleLogin);
        } else if body.contains("\"cmd\":\"set_google_user\"") {
            if let Some(u) = Self::extract_string(body, "\"google_user\":\"") {
                let _ = proxy.send_event(AppCmd::SetGoogleUser(u));
            }
        } else if body.contains("\"cmd\":\"set_ui_theme\"") {
            if let Some(th) = Self::extract_string(body, "\"theme\":\"") {
                let _ = proxy.send_event(AppCmd::SetUiTheme(th));
            }
        } else if body.contains("\"cmd\":\"new_tab\"") {
            let url = Self::extract_string(body, "\"url\":\"").unwrap_or_else(|| "tp://newtab".to_string());
            let isolated = body.contains("\"isolated\":true");
            let _ = proxy.send_event(AppCmd::NewTab { url, isolated });
        } else if body.contains("\"cmd\":\"focus_index\"") {
            if let Some(idx) = Self::extract_u64(body, "\"index\":") {
                let _ = proxy.send_event(AppCmd::FocusIndex(idx as usize));
            }
        } else if body.contains("\"cmd\":\"focus_this_tab\"") {
            let _ = proxy.send_event(AppCmd::FocusTabById(tab_id));
        } else if body.contains("\"cmd\":\"close_index\"") {
            if let Some(idx) = Self::extract_u64(body, "\"index\":") {
                let _ = proxy.send_event(AppCmd::CloseIndex(idx as usize));
            }
        } else if body.contains("\"cmd\":\"close_active_tab\"") {
            let _ = proxy.send_event(AppCmd::CloseActiveTab);
        } else if body.contains("\"cmd\":\"navigate_active\"") {
            if let Some(u) = Self::extract_string(body, "\"url\":\"") {
                let _ = proxy.send_event(AppCmd::NavigateActive(u));
            }
        } else if body.contains("\"cmd\":\"nav_back\"") {
            let _ = proxy.send_event(AppCmd::NavBack);
        } else if body.contains("\"cmd\":\"nav_forward\"") {
            let _ = proxy.send_event(AppCmd::NavForward);
        } else if body.contains("\"cmd\":\"reload\"") {
            let _ = proxy.send_event(AppCmd::ReloadActive);
        } else if body.contains("\"cmd\":\"go_home\"") {
            let _ = proxy.send_event(AppCmd::GoHome);
        } else if body.contains("\"cmd\":\"bookmark_active\"") {
            let _ = proxy.send_event(AppCmd::BookmarkActive);
        } else if body.contains("\"cmd\":\"save_bookmarks\"") {
            if let Some(bm) = Self::extract_string(body, "\"bookmarks_json\":\"") {
                let _ = proxy.send_event(AppCmd::SaveBookmarks(bm));
            }
        } else if body.contains("\"cmd\":\"save_ai_config\"") {
            let provider = Self::extract_string(body, "\"provider\":\"").unwrap_or_else(|| "gemini".to_string());
            let api_key = Self::extract_string(body, "\"api_key\":\"").unwrap_or_default();
            let model = Self::extract_string(body, "\"model\":\"").unwrap_or_else(|| "gemini-2.5-flash".to_string());
            let base_url = Self::extract_string(body, "\"base_url\":\"").unwrap_or_else(|| "http://127.0.0.1:20128/v1".to_string());
            let auto_min = !body.contains("\"auto_minimize_on_task\":false");
            let _ = proxy.send_event(AppCmd::SaveAiConfig {
                provider,
                api_key,
                model,
                base_url,
                auto_minimize_on_task: auto_min,
            });
        } else if body.contains("\"cmd\":\"save_skills_json\"") {
            if let Some(sk) = Self::extract_string(body, "\"skills_json\":\"") {
                let _ = proxy.send_event(AppCmd::SaveSkillsJson(sk));
            }
        } else if body.contains("\"cmd\":\"start_skill_recording\"") {
            let _ = proxy.send_event(AppCmd::StartSkillRecording);
        } else if body.contains("\"cmd\":\"stop_skill_recording\"") {
            let _ = proxy.send_event(AppCmd::StopSkillRecording);
        } else if body.contains("\"cmd\":\"recorded_skill_step\"") {
            if let Some(st) = Self::extract_string(body, "\"step_json\":\"") {
                let _ = proxy.send_event(AppCmd::RecordedSkillStep(st));
            }
        } else if body.contains("\"cmd\":\"run_ai_prompt\"") {
            if let Some(p) = Self::extract_string(body, "\"prompt\":\"") {
                let _ = proxy.send_event(AppCmd::RunAiPrompt(p));
            }
        } else if body.contains("\"cmd\":\"request_dom_context_for_ai\"") {
            let _ = proxy.send_event(AppCmd::RequestDomContextForAi);
        } else if body.contains("\"cmd\":\"dom_context_result\"") {
            let title = Self::extract_string(body, "\"title\":\"").unwrap_or_default();
            let url = Self::extract_string(body, "\"url\":\"").unwrap_or_default();
            let visible_text = Self::extract_string(body, "\"visible_text\":\"").unwrap_or_default();
            let elements_json = Self::extract_string(body, "\"elements_json\":\"").unwrap_or_else(|| "[]".to_string());
            let _ = proxy.send_event(AppCmd::DomContextResult {
                tab_id,
                title,
                url,
                visible_text,
                elements_json,
            });
        } else if body.contains("\"cmd\":\"execute_dom_action\"") {
            if let Some(act_json) = Self::extract_string(body, "\"action_json\":\"") {
                let _ = proxy.send_event(AppCmd::ExecuteDomAction(act_json));
            }
        } else if body.contains("\"cmd\":\"sync_bubble_status\"") {
            let working = body.contains("\"working\":true");
            let recording = body.contains("\"recording\":true");
            let status_text = Self::extract_string(body, "\"status_text\":\"").unwrap_or_default();
            let last_reply = Self::extract_string(body, "\"last_reply\":\"").unwrap_or_default();
            let _ = proxy.send_event(AppCmd::SyncBubbleStatus {
                working,
                recording,
                status_text,
                last_reply,
            });
        }
    }

    fn execute(&mut self, cmd: AppCmd) {
        match cmd {
            AppCmd::ToggleTabOrientation => {
                self.tab_orientation = self.tab_orientation.toggle();
                self.relayout();
                self.sync_all_ui();
                self.save_config();
            }
            AppCmd::ToggleSidebarCollapse => {
                self.sidebar_collapsed = !self.sidebar_collapsed;
                self.relayout();
                self.sync_all_ui();
                self.save_config();
            }
            AppCmd::CycleViewMode => {
                self.view_mode = self.view_mode.next();
                self.relayout();
                self.sync_all_ui();
                self.save_session();
            }
            AppCmd::ToggleAiMode => {
                self.ai_mode = match self.ai_mode {
                    AiDisplayMode::RightDock => AiDisplayMode::BubbleMinimized,
                    AiDisplayMode::BubbleMinimized => AiDisplayMode::RightDock,
                };
                self.relayout();
                self.sync_all_ui();
                self.save_config();
            }
            AppCmd::SetAiMode(mode) => {
                self.ai_mode = mode;
                self.relayout();
                self.sync_all_ui();
                self.save_config();
            }
            AppCmd::SyncAiPanelInit => {
                self.sync_ai_config_to_panel(false);
                self.sync_all_ui();
            }
            AppCmd::OpenAiSettings => {
                self.ai_mode = AiDisplayMode::RightDock;
                self.relayout();
                self.sync_all_ui();
                self.sync_ai_config_to_panel(true);
            }
            AppCmd::FocusAiPanel => {
                self.ai_mode = AiDisplayMode::RightDock;
                self.relayout();
                self.sync_all_ui();
                let _ = self.ai_shell.focus();
            }
            AppCmd::OpenGoogleLogin => {
                self.execute(AppCmd::NavigateActive(
                    "https://accounts.google.com".to_string(),
                ));
            }
            AppCmd::SetGoogleUser(user) => {
                self.google_user = user;
                self.save_config();
                self.sync_all_ui();
            }
            AppCmd::SetUiTheme(th) => {
                self.ui_theme = th;
                self.save_config();
                self.sync_all_ui();
                self.sync_ai_config_to_panel(false);
            }
            AppCmd::NewTab { url, isolated } => {
                self.add_tab(&url, isolated);
            }
            AppCmd::FocusIndex(idx) => {
                if idx < self.tabs.len() {
                    self.active_idx = idx;
                    self.relayout();
                    if let Some(t) = self.tabs.get(self.active_idx) {
                        let _ = t.webview.focus();
                    }
                    self.sync_all_ui();
                    self.save_session();
                }
            }
            AppCmd::FocusTabById(tab_id) => {
                if let Some(pos) = self.tabs.iter().position(|t| t.id == tab_id) {
                    if self.active_idx != pos {
                        self.active_idx = pos;
                        self.sync_all_ui();
                        self.save_session();
                    }
                }
            }
            AppCmd::CloseIndex(idx) => {
                if self.tabs.len() > 1 && idx < self.tabs.len() {
                    self.tabs.remove(idx);
                    if self.active_idx >= self.tabs.len() {
                        self.active_idx = self.tabs.len() - 1;
                    }
                    self.relayout();
                    self.sync_all_ui();
                    self.save_session();
                }
            }
            AppCmd::CloseActiveTab => {
                let idx = self.active_idx;
                self.execute(AppCmd::CloseIndex(idx));
            }
            AppCmd::NavigateActive(raw_url) => {
                let normalized = Self::normalize_url(&raw_url);
                if let Some(tab) = self.tabs.get_mut(self.active_idx) {
                    tab.url = normalized.clone();
                    tab.title = Self::title_from_url(&normalized);
                    if normalized == "tp://newtab" {
                        let _ = tab.webview.load_html(NEWTAB_HTML);
                    } else {
                        let _ = tab.webview.load_url(&normalized);
                    }
                }
                self.sync_all_ui();
                self.save_session();
            }
            AppCmd::NavBack => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    let _ = tab.webview.evaluate_script("window.history.back();");
                }
            }
            AppCmd::NavForward => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    let _ = tab.webview.evaluate_script("window.history.forward();");
                }
            }
            AppCmd::ReloadActive => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    if tab.url == "tp://newtab" {
                        let _ = tab.webview.load_html(NEWTAB_HTML);
                    } else {
                        let _ = tab.webview.reload();
                    }
                }
            }
            AppCmd::GoHome => {
                self.execute(AppCmd::NavigateActive("tp://newtab".to_string()));
            }
            AppCmd::BookmarkActive => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    if tab.url != "tp://newtab" {
                        let entry = format!(
                            "{{\"title\":\"{}\",\"url\":\"{}\"}}",
                            Self::escape_json(&tab.title),
                            Self::escape_json(&tab.url)
                        );
                        let trimmed = self.bookmarks_json.trim();
                        if trimmed.ends_with(']') && trimmed.len() > 2 {
                            self.bookmarks_json =
                                format!("{},{}]", &trimmed[..trimmed.len() - 1], entry);
                        } else {
                            self.bookmarks_json = format!("[{}]", entry);
                        }
                        self.save_config();
                        self.sync_all_ui();
                    }
                }
            }
            AppCmd::SaveBookmarks(bm_json) => {
                self.bookmarks_json = bm_json;
                self.save_config();
                self.sync_all_ui();
            }
            AppCmd::SaveAiConfig {
                provider,
                api_key,
                model,
                base_url,
                auto_minimize_on_task,
            } => {
                self.ai_provider = provider;
                self.ai_api_key = api_key;
                self.ai_model = model;
                self.ai_base_url = base_url;
                self.auto_minimize_on_task = auto_minimize_on_task;
                self.save_config();
            }
            AppCmd::SaveSkillsJson(skills_json) => {
                self.skills_json = skills_json;
                self.save_skills();
            }
            AppCmd::StartSkillRecording => {
                let _ = self.ai_shell.evaluate_script(
                    "if (window.__tpStartSkillRecordingFromRust) window.__tpStartSkillRecordingFromRust();",
                );
            }
            AppCmd::StopSkillRecording => {
                let _ = self.ai_shell.evaluate_script(
                    "if (window.__tpStopSkillRecordingFromRust) window.__tpStopSkillRecordingFromRust();",
                );
            }
            AppCmd::RecordedSkillStep(step_json) => {
                let js = format!(
                    "if (window.__tpAddRecordedStep) window.__tpAddRecordedStep({});",
                    step_json
                );
                let _ = self.ai_shell.evaluate_script(&js);
            }
            AppCmd::RunAiPrompt(prompt) => {
                let js = format!(
                    "if (window.triggerCoworkTask) window.triggerCoworkTask(\"{}\");",
                    Self::escape_json(&prompt)
                );
                let _ = self.ai_shell.evaluate_script(&js);
            }
            AppCmd::RequestDomContextForAi => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    let _ = tab.webview.evaluate_script(
                        "if (window.__tpCollectDomForAi) window.__tpCollectDomForAi();",
                    );
                }
            }
            AppCmd::DomContextResult {
                tab_id,
                title,
                url,
                visible_text,
                elements_json,
            } => {
                let target_idx = self
                    .tabs
                    .iter()
                    .position(|t| t.id == tab_id)
                    .unwrap_or(self.active_idx);
                if let Some(tab) = self.tabs.get_mut(target_idx) {
                    if !title.is_empty() {
                        tab.title = title.clone();
                    }
                }
                let mut all_tabs_json = Vec::new();
                for t in &self.tabs {
                    all_tabs_json.push(format!(
                        "{{\"title\":\"{}\",\"url\":\"{}\",\"profile\":\"{}\"}}",
                        Self::escape_json(&t.title),
                        Self::escape_json(&t.url),
                        Self::escape_json(&t.profile_name)
                    ));
                }
                let payload = format!(
                    "{{\"title\":\"{}\",\"url\":\"{}\",\"visible_text\":\"{}\",\"elements\":{},\"all_tabs\":[{}]}}",
                    Self::escape_json(&title),
                    Self::escape_json(&url),
                    Self::escape_json(&visible_text),
                    if elements_json.trim().is_empty() {
                        "[]"
                    } else {
                        &elements_json
                    },
                    all_tabs_json.join(",")
                );
                let _ = self.ai_shell.evaluate_script(&format!(
                    "if (window.__tpReceiveDomContext) window.__tpReceiveDomContext({});",
                    payload
                ));
                self.sync_all_ui();
            }
            AppCmd::ExecuteDomAction(action_json) => {
                if let Some(tab) = self.tabs.get(self.active_idx) {
                    let js = format!(
                        "if (window.__tpExecuteDomAction) window.__tpExecuteDomAction({});",
                        action_json
                    );
                    let _ = tab.webview.evaluate_script(&js);
                }
            }
            AppCmd::SyncBubbleStatus {
                working,
                recording,
                status_text,
                last_reply,
            } => {
                self.ai_working = working;
                self.ai_recording = recording;
                if !status_text.is_empty() {
                    self.ai_status_text = status_text;
                }
                if !last_reply.is_empty() {
                    self.ai_last_reply = last_reply;
                }
                self.sync_all_ui();
            }
            AppCmd::PageFinished { tab_id, url } => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                    if url.starts_with("http://") || url.starts_with("https://") {
                        tab.url = url.clone();
                        tab.title = Self::title_from_url(&url);
                    }
                }
                self.sync_ai_config_to_panel(false);
                self.sync_all_ui();
                self.save_session();
            }
        }
    }
}

fn main() -> wry::Result<()> {
    let event_loop = EventLoopBuilder::<AppCmd>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    let window = WindowBuilder::new()
        .with_title("TPbrowser — Trình Duyệt Cá Nhân & AI Cowork")
        .with_inner_size(LogicalSize::new(1480, 900))
        .with_min_inner_size(LogicalSize::new(860, 560))
        .build(&event_loop)
        .expect("Failed to create TPbrowser window");

    let mut app = TpBrowserApp::new(window, proxy, exe_dir)?;
    let mut modifiers = ModifiersState::empty();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    app.save_config();
                    app.save_skills();
                    app.save_session();
                    *control_flow = ControlFlow::Exit;
                }
                WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                    app.relayout();
                }
                WindowEvent::ModifiersChanged(new_mods) => {
                    modifiers = new_mods;
                }
                WindowEvent::KeyboardInput {
                    event:
                        KeyEvent {
                            physical_key,
                            state: ElementState::Pressed,
                            ..
                        },
                    ..
                } => {
                    if physical_key == KeyCode::F5 {
                        app.execute(AppCmd::ReloadActive);
                    } else if modifiers.alt_key() && !modifiers.control_key() {
                        match physical_key {
                            KeyCode::KeyT => app.execute(AppCmd::ToggleTabOrientation),
                            KeyCode::KeyA => app.execute(AppCmd::ToggleAiMode),
                            KeyCode::KeyN => app.execute(AppCmd::NewTab {
                                url: "tp://newtab".to_string(),
                                isolated: false,
                            }),
                            KeyCode::KeyP => app.execute(AppCmd::NewTab {
                                url: "tp://newtab".to_string(),
                                isolated: true,
                            }),
                            KeyCode::KeyW => app.execute(AppCmd::CloseActiveTab),
                            KeyCode::KeyM => app.execute(AppCmd::CycleViewMode),
                            _ => {}
                        }
                    }
                }
                _ => {}
            },
            Event::UserEvent(cmd) => {
                app.execute(cmd);
            }
            _ => {}
        }
    });
}
