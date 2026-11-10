# AI Agent Onboarding Protocol (READ FIRST)

Before scanning files, answering questions about this codebase, or modifying code, **every AI agent MUST read these files in order**:

1. **`.ai/STATE.md`** — Hot-state handoff (<100 lines): latest commit, current session changes, test verification, and next tasks.
2. **`.ai/checkpoints/PROJECT_STATE.json`** — Compact architecture snapshot (~1.5k tokens), core file map, guided tour, and local `codegraph` AST commands (`codegraph callers/callees/impact <symbol>`). **Do NOT blindly read all repository files or large extension/runtime bundles (`extensions/uBOLite.chromium/rulesets/*`, `user_data/*`, `target/*`).**
3. **`.ai/MAP.html`** — Interactive feature checklist (`DEFAULT_TASKS` + `ROADMAP_VERSION`) and repository tree.

## Mandatory Repo Hygiene Rules
- **AI workspace isolation**: Store ALL plans, specs, mockups, and checkpoints strictly inside `.ai/` (`plans/`, `specs/`, `sketches/`, `checkpoints/`). Never drop AI markdown/HTML files in repo root or `docs/`.
- **Secrets & runtime data safety**: Never read, print, or commit `config.json` (contains user API keys), `.env*`, private keys, or `user_data/*`.
- **Checkpoint sync after changes**: Update `.ai/STATE.md` + `.ai/MAP.html`, run `CAP_NHAT_CHECKPOINT.bat` (or `python scripts/generate_checkpoint.py`), and verify with `pytest -q tests/test_checkpoint_integrity.py`.
