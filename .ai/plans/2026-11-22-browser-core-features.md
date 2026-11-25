# Browser Core Features Implementation Plan

**Goal:** Add the four highest-priority browser features while keeping MCP intact: downloads, private browsing, permission prompts, and print/PDF.

## Scope

| File | Responsibility |
|---|---|
| `src/main.rs` | Native download routing, private-session lifecycle, permission decisions, print command wiring |
| `src/titlebar.html` | Download indicator/menu and privacy affordance |
| `src/settings_modal.html` | Permission policy controls and browser-core status |
| `tests/test_browser_core_features.py` | Contract tests for the four features and MCP preservation |

## Execution order

1. Add contract tests for command names, download handlers, permission prompt state, private profile/session flags, and print command.
2. Implement download path selection and completion notifications with Wry handlers.
3. Implement private tabs using a temporary WebView2 profile and no session/history persistence.
4. Replace unconditional permission allow with a persisted per-origin decision flow and explicit default deny/prompt state.
5. Add print/PDF command and titlebar action using the WebView2 print path available through Wry/Windows.
6. Rebuild, run Cargo/Python tests, regenerate checkpoints, and rebuild `TPbrowser.exe`.

## Constraints

- Do not remove or change MCP commands/server behavior.
- Do not reintroduce Workspace or e-commerce scanning.
- Preserve existing tab/session/bookmark/history behavior for normal tabs.
- Do not read or modify secrets in `config.json`, `.env*`, or `user_data/*`.
- Verify the exact release executable and copied root executable hashes.
